//! Application service for read-only graph queries and the shared
//! lock→reload→mutate→persist→render transaction used by both the CLI and MCP.
//!
//! Per `docs/design/system/component-architecture.md`, the CLI and MCP adapters
//! call the **same** application services; this module owns the graph-level
//! read paths and the revision-checked mutation wrapper. Plan/plan-transition
//! logic lives in [`super::plan_service`]; design validation in
//! [`super::design_service`].
//!
//! All mutations follow the design's transaction pattern: lock, reload,
//! recheck revision, one domain transition, atomic TOML write, deterministic
//! Markdown render, release lock.

use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::PlanWorkspace;
use crate::domain::validation;
use crate::infrastructure::toml_store::{
    CompensationPredecessorRepairOutcome, ProvenanceRepairOutcome, TomlStore,
};

/// Application-level read-only graph service.
pub struct GraphService<'a> {
    store: &'a TomlStore,
    snapshot: std::cell::RefCell<
        Option<std::rc::Rc<crate::infrastructure::graph_snapshot::GraphSnapshot>>,
    >,
    observed_revision: std::cell::Cell<Option<u64>>,
    snapshot_loads: std::cell::Cell<usize>,
}

impl<'a> GraphService<'a> {
    /// Wraps a [`TomlStore`] for read-only graph access.
    #[must_use]
    pub fn new(store: &'a TomlStore) -> Self {
        Self {
            store,
            snapshot: std::cell::RefCell::new(None),
            observed_revision: std::cell::Cell::new(None),
            snapshot_loads: std::cell::Cell::new(0),
        }
    }

    /// This service is one request: callers share this snapshot, not a cache
    /// attached to a long-lived repository store or MCP server.
    pub fn snapshot(
        &self,
    ) -> MineResult<std::rc::Rc<crate::infrastructure::graph_snapshot::GraphSnapshot>> {
        if let Some(snapshot) = self.snapshot.borrow().as_ref() {
            return Ok(snapshot.clone());
        }
        let snapshot = std::rc::Rc::new(self.store.snapshot()?);
        self.snapshot_loads.set(self.snapshot_loads.get() + 1);
        self.observed_revision
            .set(Some(snapshot.workspace().revision));
        *self.snapshot.borrow_mut() = Some(snapshot.clone());
        Ok(snapshot)
    }

    pub fn snapshot_loads(&self) -> usize {
        self.snapshot_loads.get()
    }

    /// Result of this request's ordinary write, not a persistent admission cache.
    pub fn completed_checkpoint(
        &self,
    ) -> Option<crate::domain::lifecycle_operation::LifecycleOperation> {
        self.store.transaction_diagnostics().checkpoint
    }

    pub fn observed_revision(&self) -> MineResult<u64> {
        if let Some(revision) = self.observed_revision.get() {
            return Ok(revision);
        }
        let revision = self.store.observed_revision()?;
        self.observed_revision.set(Some(revision));
        Ok(revision)
    }

    pub fn worktrees(&self) -> MineResult<Vec<crate::infrastructure::git::WorktreeInfo>> {
        self.snapshot()?.worktrees()
    }

    pub fn refresh_git(&self) {
        if let Some(snapshot) = self.snapshot.borrow().as_ref() {
            snapshot.refresh_git();
        }
    }

    pub(crate) fn invalidate_observation(&self) {
        self.snapshot.borrow_mut().take();
        self.observed_revision.set(None);
    }

    /// Loads the workspace and runs full structural validation plus
    /// generated-view byte parity (`mine graph validate`).
    ///
    /// After the machine TOML validates, the on-disk generated view is
    /// compared byte-for-byte against a fresh deterministic render. A stale,
    /// hand-edited, or missing view is a validation failure whose detail
    /// names `mine graph render` as the non-destructive repair; validation
    /// itself never rewrites either file.
    ///
    /// # Errors
    /// Propagates [`crate::domain::error::MineError::GraphNotInitialized`] when no graph exists,
    /// `MineError::GraphInvalid`/`GraphCycle` on machine-source validation
    /// failure, and `MineError::GraphInvalid` on generated-view drift.
    pub fn validate(&self) -> MineResult<PlanWorkspace> {
        let ws = self.snapshot()?.workspace().clone();
        let expected = crate::infrastructure::toml_store::render_markdown(&ws)?;
        let view = self.store.md_path().display().to_string();
        match std::fs::read(self.store.md_path()) {
            Ok(bytes) if bytes == expected.as_bytes() => Ok(ws),
            Ok(_) => Err(MineError::GraphInvalid {
                detail: format!(
                    "generated view stale: {view}; preserve foreign bytes and classify drift; pending operations require exact recovery"
                ),
            }),
            Err(_) => Err(MineError::GraphInvalid {
                detail: format!(
                    "generated view missing: {view}; checkpoint-proven restoration may use `mine graph render --expected-revision {}`; pending operations require exact recovery",
                    ws.revision
                ),
            }),
        }
    }

    /// Returns the loaded workspace (revision, identity, frontier). Read-only.
    pub fn status(&self) -> MineResult<GraphStatus> {
        let ws = match self.snapshot() {
            Ok(snapshot) => snapshot.workspace().clone(),
            Err(MineError::GraphNotInitialized { .. }) => {
                use crate::infrastructure::system::SystemUuidSource;
                let declared = super::workspace_service::WorkspaceService::new(&SystemUuidSource)
                    .status(self.store)?;
                return Ok(GraphStatus {
                    workspace_id: declared.workspace_id,
                    revision: declared.revision,
                    stable_branch: declared.stable_branch,
                    integration_branch: declared.integration_branch,
                    plan_count: declared.plan_count,
                    ready: Vec::new(),
                    pending_operations: declared.pending_operations,
                });
            }
            Err(error) => return Err(error),
        };
        let ready = validation::ready_frontier(&ws);
        let pending_operations = self.pending_operations()?;
        Ok(GraphStatus {
            workspace_id: ws.workspace_id,
            revision: ws.revision,
            stable_branch: ws.stable_branch,
            integration_branch: ws.integration_branch,
            plan_count: ws.plans.len(),
            ready,
            pending_operations,
        })
    }

    /// Discover a lost begin response without starting a duplicate operation.
    /// Finish the complete compact journal scan before filtering terminal
    /// records; a corrupt unrelated record cannot become an empty summary.
    pub(crate) fn pending_operations(
        &self,
    ) -> MineResult<Vec<crate::infrastructure::operation_store::OperationObservation>> {
        use crate::domain::{graph::RepoPath, lifecycle_operation::terminal};
        use crate::infrastructure::operation_store::{OperationStore, regular_path};
        self.store.with_coordinator_lock(|guard| {
            let snapshot = self.snapshot()?;
            let workspace = snapshot.workspace();
            let actual = self.store.observed_revision()?;
            if workspace.revision != actual {
                return Err(MineError::RevisionConflict {
                    expected: workspace.revision,
                    actual,
                });
            }
            let directory = regular_path(
                self.repo_root(),
                &RepoPath::new(".mine/runtime/operations").expect("constant journal locator"),
            )?;
            if !directory.try_exists()? {
                return Ok(vec![]);
            }
            let config = super::plan_service::root_configuration(self.repo_root())?;
            let operations = OperationStore::under_lock(
                self.repo_root(),
                &config.repository_id,
                &workspace.workspace_id,
                guard,
            )
            .and_then(|store| store.observations())
            .map_err(|e| MineError::GraphInvalid {
                detail: e.to_string(),
            })?;
            Ok(operations
                .into_iter()
                .filter(|record| !terminal(&record.operation))
                .collect())
        })
    }

    /// Returns the READY frontier. Read-only.
    pub fn ready(&self) -> MineResult<Vec<String>> {
        Ok(validation::ready_frontier(self.snapshot()?.workspace()))
    }

    /// Returns a stable parallel wave (no write-scope overlap among READY
    /// plans). Read-only.
    pub fn wave(&self) -> MineResult<Vec<String>> {
        Ok(self.scheduling_snapshot()?.selected)
    }

    /// One complete advisory observation, not an execution grant. Invalid
    /// semantic evidence remains an error and never becomes conservative.
    pub fn scheduling_snapshot(&self) -> MineResult<crate::domain::scheduling::OwnershipWave> {
        self.store.with_coordinator_lock(|guard| {
            let snapshot = self.snapshot()?;
            let actual = self.store.observed_revision()?;
            if snapshot.workspace().revision != actual {
                return Err(MineError::RevisionConflict {
                    expected: snapshot.workspace().revision,
                    actual,
                });
            }
            super::plan_service::ownership_snapshot(&snapshot, guard)
        })
    }

    pub(crate) fn effective_authority(
        &self,
        id: &str,
    ) -> MineResult<crate::domain::ownership::EffectiveAuthority> {
        self.store.with_coordinator_lock(|_| {
            let snapshot = self.snapshot()?;
            let actual = self.store.observed_revision()?;
            if actual != snapshot.workspace().revision {
                return Err(MineError::RevisionConflict {
                    expected: snapshot.workspace().revision,
                    actual,
                });
            }
            super::plan_service::authority_observation(&snapshot, id)
        })
    }

    /// Read the exact durable operation identity even while its generated view
    /// or checkpoint is pending. Never expose private-index/runtime byte data.
    pub fn operation(
        &self,
        id: crate::domain::lifecycle_operation::OperationId,
    ) -> MineResult<(
        u64,
        crate::infrastructure::operation_store::OperationObservation,
    )> {
        use crate::infrastructure::operation_store::RepositoryJournal;
        self.store.with_coordinator_lock(|guard| {
            let config = super::plan_service::root_configuration(self.repo_root())?;
            let configured = TomlStore::from_configuration(self.repo_root(), &config)?;
            let revision = match configured.observed_revision() {
                Ok(revision) => revision,
                Err(MineError::GraphNotInitialized { .. }) => 0,
                Err(error) => return Err(error),
            };
            // Repository query is read-only and retains the operation's original
            // workspace even before initial graph publication or after release.
            let journal =
                RepositoryJournal::under_lock(self.repo_root(), &config.repository_id, guard)
                    .map_err(|e| MineError::GraphInvalid {
                        detail: e.to_string(),
                    })?;
            journal
                .observations()
                .map_err(|e| MineError::GraphInvalid {
                    detail: e.to_string(),
                })?;
            let result = journal.query(&id).map_err(|e| MineError::GraphInvalid {
                detail: e.to_string(),
            })?;
            let observation = crate::infrastructure::operation_store::OperationObservation {
                initial_predecessors: result.checkpoint.as_ref().and_then(|runtime| match &runtime
                    .before
                {
                    crate::infrastructure::graph_checkpoint::GraphBefore::Absent {
                        predecessors,
                    } => Some(predecessors.clone()),
                    _ => None,
                }),
                plan_git: result
                    .checkpoint
                    .as_ref()
                    .and_then(|r| r.plan_git.summary()),
                operation: result.operation,
                closure: result.closure,
            };
            Ok((revision, observation))
        })
    }

    /// Same coordinator and observer ordering for journal-only lifecycle writes.
    /// The callback may not reacquire the Root lock or silently refresh revision.
    pub(crate) fn observe_locked<T>(
        &self,
        expected_revision: u64,
        observe: impl FnOnce(
            &crate::infrastructure::graph_snapshot::GraphSnapshot,
            &crate::infrastructure::file_lock::FileLock,
        ) -> MineResult<T>,
    ) -> MineResult<T> {
        self.snapshot.borrow_mut().take();
        self.observed_revision.set(None);
        self.store.with_coordinator_lock(|guard| {
            let snapshot =
                std::rc::Rc::new(self.store.snapshot_at_revision(expected_revision, guard)?);
            self.snapshot_loads.set(self.snapshot_loads.get() + 1);
            self.observed_revision
                .set(Some(snapshot.workspace().revision));
            *self.snapshot.borrow_mut() = Some(snapshot.clone());
            observe(&snapshot, guard)
        })
    }

    /// An existing same-ID intent is inspected before applying the ordinary
    /// stale-observer gate. Only its typed adapter may reconcile it; this does
    /// not authorize unrelated writes at a newer revision.
    pub(crate) fn reconcile_operation_locked<T>(
        &self,
        expected_revision: u64,
        id: &crate::domain::lifecycle_operation::OperationId,
        reconcile: impl FnOnce(
            &crate::infrastructure::graph_snapshot::GraphSnapshot,
            &crate::infrastructure::file_lock::FileLock,
            bool,
        ) -> MineResult<T>,
    ) -> MineResult<T> {
        self.invalidate_observation();
        self.store.with_coordinator_lock(|guard| {
            let location = crate::infrastructure::operation_store::OperationLocation::new(
                self.repo_root(),
                id,
            )?
            .resolve()?;
            let existing = location.try_exists()?;
            let snapshot = std::rc::Rc::new(if existing {
                self.store.snapshot()?
            } else {
                self.store.snapshot_at_revision(expected_revision, guard)?
            });
            self.snapshot_loads.set(self.snapshot_loads.get() + 1);
            self.observed_revision
                .set(Some(snapshot.workspace().revision));
            *self.snapshot.borrow_mut() = Some(snapshot.clone());
            reconcile(&snapshot, guard, existing)
        })
    }

    /// Restore only an absent checkpoint-proven projection, or validate an
    /// unchanged canonical one. Never overwrites drift or recovers a journal.
    pub fn render(
        &self,
        expected_revision: u64,
    ) -> MineResult<crate::infrastructure::graph_checkpoint::GraphRenderOutcome> {
        use crate::{
            domain::lifecycle_operation::{NoInterruption, OperationError},
            infrastructure::graph_checkpoint::CheckpointKernel,
        };
        self.invalidate_observation();
        self.store.with_coordinator_lock(|guard| {
            CheckpointKernel::under_lock(self.repo_root(), guard)
                .and_then(|kernel| {
                    kernel.restore_view_under_lock(expected_revision, &mut NoInterruption)
                })
                .map_err(|error| match error {
                    OperationError::Mine(error) => error,
                    other => MineError::GraphInvalid {
                        detail: other.to_string(),
                    },
                })
        })
    }

    /// Performs the supported revision-checked schema-v1/v2 provenance
    /// migration, or returns an idempotent no-op for an already-valid v3 graph.
    pub fn repair_provenance(
        &self,
        expected_revision: u64,
        actor: &str,
    ) -> MineResult<ProvenanceRepairOutcome> {
        self.snapshot.borrow_mut().take();
        self.observed_revision.set(None);
        self.store
            .repair_provenance_with_revision(expected_revision, actor)
    }

    /// Repairs one invalid unstarted compensation predecessor list using the
    /// rejected predecessor as the sole edge authority.
    ///
    /// # Errors
    /// Propagates all revision, identity, lifecycle, validation, persistence,
    /// and Git-evidence failures from the bounded store transaction.
    pub fn repair_compensation_predecessors(
        &self,
        actor: &str,
        plan_id: &str,
        expected_revision: u64,
        reason: &str,
        updated_at: &str,
    ) -> MineResult<CompensationPredecessorRepairOutcome> {
        self.snapshot.borrow_mut().take();
        self.observed_revision.set(None);
        self.store.repair_compensation_predecessors_with_revision(
            actor,
            plan_id,
            expected_revision,
            reason,
            updated_at,
        )
    }

    /// Returns the authoritative repository root backing this graph service.
    #[must_use]
    pub fn repo_root(&self) -> &std::path::Path {
        self.store.repo_root()
    }

    /// Returns the configured repository-relative worktree root.
    pub fn worktrees_root(&self) -> MineResult<String> {
        self.store.worktrees_root()
    }

    /// The shared mutation transaction: lock → reload → recheck revision →
    /// one domain transition → atomic write → render → release lock. The CLI
    /// and MCP both call this for every graph mutation; neither duplicates the
    /// transaction. The CLI's `plan.*` handlers and the MCP mutating tools
    /// route through [`crate::application::plan_service::PlanService`], which calls this internally.
    ///
    /// `mutate` receives the reloaded workspace, applies one domain
    /// transition, and should set `revision = expected_revision + 1`. The store
    /// re-validates the result and renders Markdown.
    pub fn mutate<F>(
        &self,
        expected_revision: u64,
        actor: &str,
        mutate: F,
    ) -> MineResult<PlanWorkspace>
    where
        F: FnOnce(PlanWorkspace) -> MineResult<PlanWorkspace>,
    {
        self.mutate_locked(expected_revision, actor, |workspace, _, _| {
            mutate(workspace)
        })
    }

    pub(crate) fn mutate_locked<F>(
        &self,
        expected_revision: u64,
        actor: &str,
        mutate: F,
    ) -> MineResult<PlanWorkspace>
    where
        F: FnOnce(
            PlanWorkspace,
            &crate::infrastructure::file_lock::FileLock,
            &crate::domain::graph::Sha256,
        ) -> MineResult<PlanWorkspace>,
    {
        // Locked writes never trust an earlier read's mutable evidence.
        self.snapshot.borrow_mut().take();
        self.observed_revision.set(None);
        self.store.save_with_revision_locked(
            expected_revision,
            actor,
            |workspace, guard, source_sha256| {
                // Keep the existing generated-view gate, but only after the locked
                // revision check. It must not trigger another graph/Git load.
                let expected = crate::infrastructure::toml_store::render_markdown(&workspace)?;
                if std::fs::read(self.store.md_path()).ok().as_deref() != Some(expected.as_bytes())
                {
                    return Err(MineError::GraphInvalid {
                        detail:
                            "generated view stale or missing (run `mine graph render` to repair)"
                                .into(),
                    });
                }
                mutate(workspace, guard, source_sha256)
            },
        )
    }
}

/// Status summary returned by [`GraphService::status`]. Shared DTO shape for the
/// CLI `graph.status` envelope and the MCP `mine_graph_status` result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GraphStatus {
    pub workspace_id: String,
    pub revision: u64,
    pub stable_branch: String,
    pub integration_branch: String,
    pub plan_count: usize,
    pub ready: Vec<String>,
    pub pending_operations: Vec<crate::infrastructure::operation_store::OperationObservation>,
}

/// A request to start a plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanStartRequest {
    /// Graph revision observed by the caller, never refreshed internally.
    pub expected_revision: u64,
    pub id: String,
    pub owner: String,
    pub run_id: String,
    pub started_at: String,
}

/// A request to mark a plan implemented (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanImplementedRequest {
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    pub expected_revision: u64,
    pub id: String,
    pub report: String,
    pub commits: Vec<String>,
    pub updated_at: String,
}

/// A request to accept a plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanAcceptRequest {
    /// Graph revision observed by the caller, never refreshed internally.
    pub expected_revision: u64,
    pub id: String,
    pub operation_id: crate::domain::lifecycle_operation::OperationId,
    pub input: crate::domain::graph::RepoPath,
}

/// A request to reject a plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanRejectRequest {
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    pub expected_revision: u64,
    pub id: String,
    pub reason: String,
    pub review_report: String,
    pub compensating_plan: String,
    pub updated_at: String,
}

/// A request to add a plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanAddRequest {
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    pub expected_revision: u64,
    /// Contained JSON request selecting an approved policy and typed declarations.
    pub ownership_input: Option<String>,
    pub id: String,
    pub path: String,
    pub title: String,
    pub design_references: Vec<String>,
    pub exclusive_write_paths: Vec<String>,
    pub hard_predecessors: Vec<String>,
    /// Non-empty planning scope recorded on the node (ADR-0013). The adapter
    /// resolves the explicit value or the configured design-root default.
    pub planning_scope: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::toml_store::TomlStore;

    #[test]
    fn validate_returns_not_initialized_for_absent_graph() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlStore::new(dir.path());
        let svc = GraphService::new(&store);
        let err = svc.validate().unwrap_err();
        assert_eq!(err.code(), "MINE_GRAPH_NOT_INITIALIZED");
    }

    #[test]
    fn status_round_trips_a_seeded_graph() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlStore::new(dir.path());
        // Seed a minimal graph (reuse the store's own serialization shape).
        use crate::domain::graph::{PlanNode, PlanWorkspace};
        use crate::domain::status::PlanStatus;
        let ws = PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 5,
            project_id: "p".to_string(),
            workspace_id: "canonical-store-fixture".to_string(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: "c".to_string(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![PlanNode {
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                id: "plan01".to_string(),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                path: "docs/plan/plan01-test.md".to_string(),
                title: "First".to_string(),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                branch: "plan/plan01-test".into(),
                branch_anchor: String::new(),
                status: PlanStatus::Ready,
                hard_predecessors: vec![],
                soft_predecessors: vec![],
                design_references: vec!["docs/design/principles.md".to_string()],
                exclusive_write_paths: vec!["src/a/".to_string()],
                read_only_paths: vec![],
                reserved_shared_paths: vec![],
                implementation_report: String::new(),
                review_report: String::new(),
                implementation_commits: vec![],
                owner: String::new(),
                run_id: String::new(),
                started_at: String::new(),
                updated_at: String::new(),
                rejection_reason: String::new(),
                compensating_plan: String::new(),
                withdraw_reason: String::new(),
                withdrawn_at: String::new(),
                planning_scope: String::new(),
                readiness_checked_at: String::new(),
                adr_registry_revision: 0,
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                lineage_id: "plan01".into(),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                worktree_path: ".mine/worktrees/plan01".into(),
                body_sha256: None,
                ownership: None,
                ownership_policy: None,
                clarifications: vec![],
                acceptance: None,
                preparations: vec![],
                started_preparation_generation: None,
            }],
        };
        std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
        std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();
        let svc = GraphService::new(&store);
        let st = svc.status().unwrap();
        assert_eq!(st.revision, 5);
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        assert_eq!(st.ready, vec!["plan01".to_string()]);
    }
}
