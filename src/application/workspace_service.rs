//! Internal workspace lifecycle: `mine workspace open|status|close`.
//!
//! Implements the workspace contract from
//! `docs/design/governance/branch-and-plan-lifecycle.md` ("Plan workspace
//! creation" / "Release closure") and `docs/design/interfaces/cli-contract.md`
//! ("Workspace commands"):
//!
//! - `open` generates an internal `workspace_id` (a UUID), records the stable
//!   baseline commit, integration branch, and ownership on the execution
//!   graph's `PlanWorkspace` aggregate. It takes **no** user-supplied release
//!   version; workspace identity is independent of repository version.
//! - `status` reports the current workspace identity and revision.
//! - `close` validates closure (no unresolved plans) and, when configured and
//!   requested, may purge only the ownership-marked `docs/plan/` tree with an
//!   explicit expected workspace identity. Version determination is separate.
//!
//! Initial graph publication is a journalled native Root checkpoint under one
//! coordinator lock. Product staging/merges/branch switching stay with Skills;
//! the binary checkpoints only its exact configured graph source/view pair.

use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::PlanWorkspace;
use crate::domain::ports::UuidSource;
use crate::infrastructure::toml_store::TomlStore;

/// The outcome of `mine workspace open`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOpenOutcome {
    pub workspace_id: String,
    pub stable_baseline_commit: String,
    pub integration_branch: String,
    pub revision_before: u64,
    pub revision_after: u64,
}

/// The outcome of `mine workspace status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceStatusOutcome {
    pub workspace_id: String,
    pub revision: u64,
    pub integration_branch: String,
    pub stable_branch: String,
    pub stable_baseline_commit: String,
    pub plan_count: usize,
    pub has_unresolved: bool,
    pub pending_operations: Vec<crate::infrastructure::operation_store::OperationObservation>,
}

/// The workspace service. Constructed with injected ports for
/// deterministic tests.
pub struct WorkspaceService<'a> {
    uuid_source: &'a dyn UuidSource,
}

impl<'a> WorkspaceService<'a> {
    /// Creates the service with its actual identifier source.
    #[must_use]
    pub fn new(uuid_source: &'a dyn UuidSource) -> Self {
        Self { uuid_source }
    }

    /// Opens (initializes) a workspace on the graph. Idempotent: if the graph
    /// already has a workspace_id, it is preserved and only a no-op re-run is
    /// reported (no state change, no revision increment).
    pub fn open(
        &self,
        store: &TomlStore,
        expected_revision: u64,
        actor: &str,
    ) -> MineResult<WorkspaceOpenOutcome> {
        use crate::domain::lifecycle_operation::{BarrierFacts, NoInterruption};
        use crate::infrastructure::{
            graph_checkpoint::CheckpointKernel, operation_store::RepositoryJournal,
        };
        crate::domain::error::contract_text("operation.actor", actor).map_err(|e| {
            MineError::GraphInvalid {
                detail: e.to_string(),
            }
        })?;
        let config = super::plan_service::root_configuration(store.repo_root())?;
        let configured = TomlStore::from_configuration(store.repo_root(), &config)?;
        configured.with_coordinator_lock(|guard| {
            let actual = match configured.observed_revision() {
                Ok(revision) => Some(revision),
                Err(MineError::GraphNotInitialized { .. }) => None,
                Err(error) => return Err(error),
            };
            if actual.unwrap_or(0) != expected_revision {
                return Err(MineError::RevisionConflict {
                    expected: expected_revision, actual: actual.unwrap_or(0),
                });
            }
            let history = RepositoryJournal::under_lock(configured.repo_root(), &config.repository_id, guard)
                .and_then(|journal| journal.observations())
                .map_err(|e| MineError::GraphInvalid { detail: e.to_string() })?;
            let mut barriers = BarrierFacts::default();
            for record in &history { barriers.observe(&record.operation); }
            if let Some(barrier) = barriers.global.first() {
                let pending = history.iter().find(|record| record.operation.fields().operation_id == barrier.operation_id)
                    .ok_or_else(|| MineError::GraphInvalid { detail: "pending operation observation lost".into() })?;
                return Err(MineError::GraphCheckpointPending {
                    operation: Box::new(pending.operation.clone()), revision_after: actual,
                    detail: "workspace open cannot bypass pending publication or an unknown workspace write; query/finalize the same operation".into(),
                });
            }
            if actual.is_some() {
                let snapshot = configured.snapshot_at_revision(expected_revision, guard)?;
                let ws = snapshot.workspace();
                crate::infrastructure::operation_store::history::predecessors(
                    &RepositoryJournal::under_lock(configured.repo_root(), &config.repository_id, guard)
                        .map_err(|e| MineError::GraphInvalid { detail:e.to_string() })?,
                    &history, Some(&ws.workspace_id),
                ).map_err(|e| MineError::GraphInvalid { detail:e.to_string() })?;
                if ws.workspace_id.is_empty() {
                    return Err(MineError::GraphInvalid { detail: "existing graph lacks workspace identity; explicit legacy recovery required".into() });
                }
                if std::fs::read(configured.md_path())? != crate::infrastructure::toml_store::render_markdown(ws)?.as_bytes() {
                    return Err(MineError::GraphInvalid { detail: "existing workspace projection is missing or changed".into() });
                }
                return Ok(WorkspaceOpenOutcome {
                    workspace_id: ws.workspace_id.clone(),
                    stable_baseline_commit: ws.stable_baseline_commit.clone(),
                    integration_branch: ws.integration_branch.clone(),
                    revision_before: ws.revision, revision_after: ws.revision,
                });
            }
            let (kernel, intent) = CheckpointKernel::initial_under_lock(
                configured.repo_root(), guard, expected_revision, actor, self.uuid_source, &mut NoInterruption,
            ).map_err(|e| MineError::GraphInvalid { detail: e.to_string() })?;
            let id = &intent.operation.fields().operation_id;
            let completed = kernel.resume(id, &mut NoInterruption).map_err(|error| {
                match kernel.store().query(id) {
                    Ok(current) => MineError::GraphCheckpointPending {
                        operation: Box::new(current.operation),
                        revision_after: configured.observed_revision().ok(),
                        detail: error.to_string(),
                    },
                    Err(query) => MineError::GraphInvalid {
                        detail: format!("initial checkpoint outcome unknown: {error}; journal unavailable: {query}"),
                    },
                }
            })?;
            let runtime = completed.checkpoint.as_ref().ok_or_else(|| MineError::GraphInvalid {
                detail: "completed initial checkpoint lacks runtime".into(),
            })?;
            let ws: PlanWorkspace = toml::from_str(&runtime.source_after).map_err(|e| MineError::GraphInvalid {
                detail: e.to_string(),
            })?;
            Ok(WorkspaceOpenOutcome {
                workspace_id: ws.workspace_id, stable_baseline_commit: ws.stable_baseline_commit,
                integration_branch: ws.integration_branch, revision_before: 0, revision_after: ws.revision,
            })
        })
    }

    /// Reports the current workspace status. Read-only (no mutation, no
    /// revision change). Withdrawn nodes are retired and do not count as
    /// unresolved (ADR-0012).
    pub fn status(&self, store: &TomlStore) -> MineResult<WorkspaceStatusOutcome> {
        use crate::domain::lifecycle_operation::terminal;
        use crate::infrastructure::{
            graph_checkpoint::CheckpointKernel, operation_store::RepositoryJournal,
        };
        let config = super::plan_service::root_configuration(store.repo_root())?;
        let configured = TomlStore::from_configuration(store.repo_root(), &config)?;
        configured.with_coordinator_lock(|guard| {
            let journal =
                RepositoryJournal::under_lock(configured.repo_root(), &config.repository_id, guard)
                    .map_err(|e| MineError::GraphInvalid {
                        detail: e.to_string(),
                    })?;
            let history = journal
                .observations()
                .map_err(|e| MineError::GraphInvalid {
                    detail: e.to_string(),
                })?;
            let pending_operations: Vec<_> = history
                .iter()
                .filter(|record| !terminal(&record.operation))
                .cloned()
                .collect();
            let (ws, revision) = match configured.load() {
                Ok(ws) => {
                    let revision = ws.revision;
                    (ws, revision)
                }
                Err(error @ MineError::GraphNotInitialized { .. }) => {
                    if pending_operations.len() != 1 {
                        return Err(error);
                    }
                    let id = &pending_operations[0].operation.fields().operation_id;
                    let kernel = CheckpointKernel::initial_intent_under_lock(
                        configured.repo_root(),
                        guard,
                        id,
                    )
                    .map_err(|e| MineError::GraphInvalid {
                        detail: e.to_string(),
                    })?;
                    let record = kernel
                        .store()
                        .query(id)
                        .map_err(|e| MineError::GraphInvalid {
                            detail: e.to_string(),
                        })?;
                    let ws = toml::from_str(
                        &record
                            .checkpoint
                            .ok_or_else(|| MineError::GraphInvalid {
                                detail: "initial runtime missing".into(),
                            })?
                            .source_after,
                    )
                    .map_err(|e| MineError::GraphInvalid {
                        detail: format!("initial candidate: {e}"),
                    })?;
                    // Metadata is declared by the validated intent, not a live
                    // revision-zero graph. The actual source is still absent.
                    (ws, 0)
                }
                Err(error) => return Err(error),
            };
            if revision > 0 {
                crate::infrastructure::operation_store::history::predecessors(
                    &journal,
                    &history,
                    Some(&ws.workspace_id),
                )
                .map_err(|e| MineError::GraphInvalid {
                    detail: e.to_string(),
                })?;
            }
            let has_unresolved = !pending_operations.is_empty()
                || crate::domain::validation::validate_terminal_workspace(&ws).is_err();
            Ok(WorkspaceStatusOutcome {
                workspace_id: ws.workspace_id,
                revision,
                integration_branch: ws.integration_branch,
                stable_branch: ws.stable_branch,
                stable_baseline_commit: ws.stable_baseline_commit,
                plan_count: ws.plans.len(),
                has_unresolved,
                pending_operations,
            })
        })
    }

    /// Validates closure readiness. A workspace is closable when no node is in
    /// an unresolved status (`ACCEPTED` nodes are terminal, `REJECTED` nodes
    /// close through their compensation chain, and `WITHDRAWN` nodes are
    /// retired). Read-only.
    ///
    /// # Errors
    /// Returns [`MineError::GraphInvalid`] when unresolved plans remain,
    /// listing the unresolved statuses.
    pub fn close(&self, store: &TomlStore) -> MineResult<WorkspaceStatusOutcome> {
        let status = self.status(store)?;
        if let Some(pending) = status.pending_operations.first() {
            if pending.operation.fields().kind
                != crate::domain::lifecycle_operation::OperationKind::GraphCheckpoint
            {
                return Err(MineError::GraphInvalid {
                    detail: format!(
                        "pending operation {} blocks workspace closure",
                        pending.operation.fields().operation_id.as_str()
                    ),
                });
            }
            return Err(MineError::GraphCheckpointPending {
                operation: Box::new(pending.operation.clone()),
                revision_after: (status.revision != 0).then_some(status.revision),
                detail: "pending lifecycle operation blocks workspace closure".into(),
            });
        }
        if status.has_unresolved {
            let ws = store.load()?;
            let mut unresolved: Vec<&str> = ws
                .plans
                .iter()
                .filter(|p| crate::domain::validation::validate_terminal_node(&ws, p).is_err())
                .map(|p| p.status.as_str())
                .collect();
            unresolved.sort_unstable();
            unresolved.dedup();
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "workspace {} cannot close: unresolved plans remain ({}; revision {})",
                    status.workspace_id,
                    unresolved.join(", "),
                    status.revision
                ),
            });
        }
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::system::SystemUuidSource;
    use std::{fs, path::Path, process::Command};

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().into()
    }

    fn fresh_root() -> (tempfile::TempDir, String) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        git(root, &["init", "-b", "dev"]);
        for (key, value) in [
            ("user.name", "Workspace Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
        ] {
            git(root, &["config", key, value]);
        }
        fs::create_dir_all(root.join(".mine")).unwrap();
        fs::create_dir_all(root.join(".mine/design")).unwrap();
        fs::write(root.join(".gitignore"), ".mine/runtime/\n.mine/locks/\n").unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(".mine/config.toml"),
            root.join(".mine/config.toml"),
        )
        .unwrap();
        let config = crate::domain::config::MineConfig::parse(
            &root.join(".mine/config.toml"),
            &fs::read_to_string(root.join(".mine/config.toml"))
                .unwrap()
                .replace("docs/design", ".mine/design")
                .replace("docs/plan", ".mine/plan"),
        )
        .unwrap();
        fs::write(root.join(".mine/config.toml"), config.to_toml()).unwrap();
        let marker = crate::domain::design_marker::DesignMarker::new(
            config.repository_id,
            "2026-10-16T00:00:00Z".into(),
        );
        fs::write(
            root.join(".mine/design/.mine-design.toml"),
            marker.to_toml(),
        )
        .unwrap();
        fs::write(
            root.join(".mine/design/index.md"),
            "<!-- mine-managed-design -->\n# Fixture\n",
        )
        .unwrap();
        git(
            root,
            &[
                "add",
                ".gitignore",
                ".mine/config.toml",
                ".mine/design/.mine-design.toml",
                ".mine/design/index.md",
            ],
        );
        git(root, &["commit", "-m", "initial committed setup"]);
        let baseline = git(root, &["rev-parse", "HEAD"]);
        git(root, &["branch", "master"]);
        (directory, baseline)
    }

    #[test]
    fn open_initializes_workspace_with_generated_id_and_increments_revision() {
        let (root, baseline) = fresh_root();
        let store = TomlStore::new(root.path());
        let svc = WorkspaceService::new(&SystemUuidSource);
        let outcome = svc.open(&store, 0, "fixture-actor").unwrap();
        uuid::Uuid::parse_str(&outcome.workspace_id).unwrap();
        assert_eq!(outcome.revision_before, 0);
        assert_eq!(outcome.revision_after, 1);
        assert_eq!(outcome.stable_baseline_commit, baseline);
        assert_ne!(outcome.workspace_id, "0.1.0");
        assert_eq!(git(root.path(), &["rev-parse", "HEAD^"]), baseline);
        assert!(git(root.path(), &["status", "--porcelain"]).is_empty());
    }

    #[test]
    fn open_refuses_unknown_closure_before_idempotent_success() {
        use crate::domain::{graph::*, lifecycle_operation::*};
        use crate::infrastructure::{graph_checkpoint::digest, operation_store::JournalRecord};
        // mine-release-allow-plan-reference: isolated unknown-outcome journal fixture
        const ID: &str = "plan82";
        let (root, _) = fresh_root();
        let store = TomlStore::new(root.path());
        let service = WorkspaceService::new(&SystemUuidSource);
        let opened = service.open(&store, 0, "fixture-actor").unwrap();
        let config = super::super::plan_service::root_configuration(root.path()).unwrap();
        let head = git(root.path(), &["rev-parse", "HEAD"]);
        let source = fs::read(store.toml_path()).unwrap();
        let unknown = JournalRecord {
            schema_version: 1,
            operation: LifecycleOperation::new(OperationFields {
                schema_version: 1,
                operation_id: OperationId::generate(),
                stage_revision: 0,
                kind: OperationKind::Acceptance,
                stage: OperationStage::Acceptance(AcceptanceStage::Reviewed),
                outcome: OperationOutcome::Unknown,
                repository_id: config.repository_id.clone(),
                workspace_id: opened.workspace_id.clone(),
                actor: "independent-fixture".into(),
                observed_graph_revision: 1,
                plan: Some(
                    PlanIdentity::new(PlanIdentityFields {
                        repository_id: config.repository_id,
                        workspace_id: opened.workspace_id,
                        plan_id: ID.into(),
                        plan_path: RepoPath::new(format!(".mine/plan/{ID}-fixture.md")).unwrap(),
                        body_sha256: Some(digest(b"fixture body")),
                        lineage_id: ID.into(),
                        branch: format!("plan/{ID}-fixture"),
                    })
                    .unwrap(),
                ),
                registered_worktree: Some(RepoPath::new(format!(".mine/worktrees/{ID}")).unwrap()),
                review: Some(ReviewEvidence {
                    reviewed_tip: CommitId::new(head.clone()).unwrap(),
                    report: FileIdentity {
                        path: RepoPath::new(format!(".mine/plan/reports/{ID}-fixture-review.md"))
                            .unwrap(),
                        bytes: 4,
                        sha256: digest(b"test"),
                    },
                }),
                integrated_commit: None,
                verification_evidence: vec![],
                cleanup: None,
                checkpoint: None,
                diagnostics: vec![],
            })
            .unwrap(),
            closure: Some(ClosureBinding::Acceptance),
            checkpoint: None,
        };
        // Storage fixture only: unknown evidence is not claimed to have completed
        // a real review. Its very uncertainty must block workspace authorization.
        unknown.validate().unwrap();
        let directory = root.path().join(".mine/runtime/operations");
        let catalog_path = directory.join(".catalog");
        let mut catalog: serde_json::Value =
            serde_json::from_slice(&fs::read(&catalog_path).unwrap()).unwrap();
        let ids = catalog["operations"].as_array_mut().unwrap();
        ids.push(serde_json::json!(unknown.operation.fields().operation_id));
        ids.sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
        fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
        fs::write(
            directory.join(format!(
                "{}.json",
                unknown.operation.fields().operation_id.as_str()
            )),
            serde_json::to_vec(&unknown).unwrap(),
        )
        .unwrap();
        let result = service.open(&store, 1, "noop-fixture");
        assert!(
            matches!(result, Err(MineError::GraphCheckpointPending { operation, .. }) if *operation == unknown.operation)
        );
        assert_eq!(fs::read(store.toml_path()).unwrap(), source);
        assert_eq!(git(root.path(), &["rev-parse", "HEAD"]), head);
        assert!(git(root.path(), &["status", "--porcelain"]).is_empty());
    }

    #[test]
    fn open_is_idempotent_on_existing_workspace() {
        let (root, _) = fresh_root();
        let store = TomlStore::new(root.path());
        let svc = WorkspaceService::new(&SystemUuidSource);
        let first = svc.open(&store, 0, "fixture-actor").unwrap();
        let tip = git(root.path(), &["rev-parse", "HEAD"]);
        let second = svc.open(&store, 1, "fixture-actor").unwrap();
        assert_eq!(first.workspace_id, second.workspace_id);
        assert_eq!(first.revision_after, second.revision_after);
        assert_eq!(second.revision_before, second.revision_after);
        assert_eq!(git(root.path(), &["rev-parse", "HEAD"]), tip);
        assert!(matches!(
            svc.open(&store, 0, "fixture-actor"),
            Err(MineError::RevisionConflict {
                expected: 0,
                actual: 1
            })
        ));
    }
}
