//! TOML persistence for the execution graph.
//!
//! Implements `docs/design/execution-graph/persistence-and-concurrency.md`.
//! The machine fact source is `docs/plan/execution-graph.toml`; the generated
//! view is `docs/plan/execution-graph.md` and is never a mutation target.
//!
//! Write sequence for [`TomlStore::save_with_revision`]:
//! 1. acquire the exclusive lock on `.mine/locks/execution-graph.lock`;
//! 2. reload the on-disk TOML (state may have changed while waiting);
//! 3. recheck `expected_revision` against the reloaded revision;
//! 4. run a caller-provided mutation that produces the new workspace;
//! 5. write the TOML atomically;
//! 6. render the Markdown view from the committed TOML;
//! 7. release the lock.
//!
//! If the TOML write succeeds but Markdown rendering fails, the TOML remains
//! the fact source and a partial-success error is returned advising
//! `mine graph render` repair.

use super::git_evidence::{GitDiagnostics, GitLimits, GitRequest, GitSpawner, SystemGit};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Per-transaction observations, never authority or a persistent evidence cache.
#[derive(Debug, Clone, Default)]
pub struct TransactionDiagnostics {
    pub operation_id: String,
    pub checkpoint: Option<crate::domain::lifecycle_operation::LifecycleOperation>,
    pub phase: &'static str,
    pub graph_size: usize,
    pub graph_loads: usize,
    pub validation_time: Duration,
    pub lock_wait: Duration,
    pub lock_hold: Duration,
    pub git: GitDiagnostics,
}

struct TransactionObservation<'a> {
    target: &'a Mutex<TransactionDiagnostics>,
    began: std::time::Instant,
    git: GitRequest,
}

impl Drop for TransactionObservation<'_> {
    fn drop(&mut self) {
        if let Ok(mut diagnostics) = self.target.lock() {
            diagnostics.lock_hold = self.began.elapsed();
            diagnostics.git = self.git.diagnostics();
        }
    }
}

use crate::domain::config::MineConfig;
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{
    ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION, CURRENT_GRAPH_SCHEMA_VERSION, LEGACY_GRAPH_SCHEMA_VERSION,
    PREVIOUS_GRAPH_SCHEMA_VERSION, PlanWorkspace, is_supported_graph_schema,
};
use crate::domain::path::normalize_repo_relative;
use crate::domain::plan_id::{CanonicalPlanId, is_exact_bootstrap_node, registered_plan_branch};
use crate::domain::status::PlanStatus;
use crate::domain::validation;

use super::atomic_write;
use super::file_lock;
use super::git;

/// Default lock timeout in milliseconds.
const DEFAULT_LOCK_TIMEOUT_MS: u64 = 5000;

/// The execution-graph lock file name.
pub const GRAPH_LOCK_NAME: &str = "execution-graph.lock";

/// Evidence returned by the revision-checked schema-v1/v2 provenance migration.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProvenanceRepairOutcome {
    pub workspace: PlanWorkspace,
    pub revision_before: u64,
    pub revision_after: u64,
    pub changed: bool,
    pub repaired_plans: Vec<String>,
}

/// Evidence returned by the bounded repair of one invalid, unstarted
/// compensation predecessor list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CompensationPredecessorRepairOutcome {
    /// Workspace after the operation, or the unchanged valid workspace for an
    /// idempotent no-op.
    pub workspace: PlanWorkspace,
    /// Repaired compensation ID.
    pub plan_id: String,
    /// Rejected predecessor from which the authoritative list was derived.
    pub rejected_predecessor: String,
    /// Predecessor list observed before repair.
    pub predecessors_before: Vec<String>,
    /// Exact predecessor list after repair.
    pub predecessors_after: Vec<String>,
    /// Status observed before repair.
    pub status_before: PlanStatus,
    /// Status after repair.
    pub status_after: PlanStatus,
    /// Operator-supplied audit reason returned to the caller.
    pub reason: String,
    /// Graph revision observed under the lock.
    pub revision_before: u64,
    /// Graph revision after mutation, equal to `revision_before` for a no-op.
    pub revision_after: u64,
    /// Whether graph bytes were rewritten.
    pub changed: bool,
}

/// Persistence adapter for the execution-graph TOML and generated Markdown.
pub struct TomlStore {
    git_spawner: Arc<dyn GitSpawner>,
    diagnostics: Mutex<TransactionDiagnostics>,
    /// Repository root.
    repo_root: PathBuf,
    /// Path to `docs/plan/execution-graph.toml`.
    toml_path: PathBuf,
    /// Path to `docs/plan/execution-graph.md`.
    md_path: PathBuf,
    /// Path to `.mine/locks/execution-graph.lock`.
    lock_path: PathBuf,
    /// Lock wait timeout.
    lock_timeout: Duration,
}

impl TomlStore {
    /// Creates a store rooted at `repo_root` using conventional paths.
    #[must_use]
    pub fn new(repo_root: &Path) -> Self {
        let plan = repo_root.join(crate::domain::config::GovernanceLayout::PLAN_DIR);
        let toml_path = plan.join("execution-graph.toml");
        let md_path = plan.join("execution-graph.md");
        let lock_path = repo_root.join(".mine").join("locks").join(GRAPH_LOCK_NAME);
        Self {
            git_spawner: Arc::new(SystemGit),
            diagnostics: Mutex::new(TransactionDiagnostics::default()),
            repo_root: repo_root.to_path_buf(),
            toml_path,
            md_path,
            lock_path,
            lock_timeout: Duration::from_millis(DEFAULT_LOCK_TIMEOUT_MS),
        }
    }

    /// Compose the fact source/view from Root configuration, not a caller
    /// whitelist or a conventional-path fallback after configuration failure.
    pub fn from_configuration(repo_root: &Path, config: &MineConfig) -> MineResult<Self> {
        let paths = super::graph_checkpoint::GraphCheckpointPaths::from_config(repo_root, config)?;
        let (toml_path, md_path) = paths.resolve()?;
        let mut store = Self::new(&repo_root.canonicalize()?);
        store.toml_path = toml_path;
        store.md_path = md_path;
        store.lock_timeout = Duration::from_millis(u64::from(config.graph.lock_timeout_ms));
        Ok(store)
    }

    /// Returns a store with a custom lock timeout (mainly for tests).
    #[must_use]
    pub fn with_lock_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
    }

    /// Uses the same owned-child boundary as production, without changing PATH.
    pub fn with_git_spawner(mut self, spawner: Arc<dyn GitSpawner>) -> Self {
        self.git_spawner = spawner;
        self
    }

    pub fn transaction_diagnostics(&self) -> TransactionDiagnostics {
        self.diagnostics
            .lock()
            .expect("diagnostic mutex poisoned")
            .clone()
    }

    fn git_request(&self) -> GitRequest {
        GitRequest::with_spawner(
            &self.repo_root,
            GitLimits::default(),
            self.git_spawner.clone(),
        )
    }

    /// Returns the TOML fact-source path.
    #[must_use]
    pub fn toml_path(&self) -> &Path {
        &self.toml_path
    }

    /// Returns the generated Markdown view path.
    #[must_use]
    pub fn md_path(&self) -> &Path {
        &self.md_path
    }

    /// Read journals and graph observations under the same Root coordinator.
    /// The borrowed guard also prevents recursive operation-store acquisition.
    pub(crate) fn with_coordinator_lock<T>(
        &self,
        observe: impl FnOnce(&file_lock::FileLock) -> MineResult<T>,
    ) -> MineResult<T> {
        let _request = super::git_evidence::GitRequestScope::join_or_new();
        let guard = self.acquire_coordinator_lock()?;
        observe(&guard)
    }

    fn acquire_coordinator_lock(&self) -> MineResult<file_lock::FileLock> {
        let relative =
            crate::domain::graph::RepoPath::new(format!(".mine/locks/{GRAPH_LOCK_NAME}"))
                .expect("constant coordinator locator");
        // Canonical equality after opening is too late: both the expected
        // path and guard could follow the same junction outside Root. Refuse
        // existing links/reparse points before creating any directory/file.
        let target = super::operation_store::regular_path(&self.repo_root, &relative)?;
        let guard = file_lock::acquire_exclusive(&target, self.lock_timeout)?;
        super::operation_store::regular_path(&self.repo_root, &relative)?;
        Ok(guard)
    }

    /// Loads the workspace from the TOML fact source.
    ///
    /// # Errors
    /// - [`MineError::GraphNotInitialized`] if the file is absent.
    /// - [`MineError::GraphInvalid`] if the TOML is unparseable or fails
    ///   structural validation.
    pub fn load(&self) -> MineResult<PlanWorkspace> {
        Ok(self.snapshot()?.workspace().clone())
    }

    /// Parse and acquire one deduplicated evidence snapshot for a request.
    pub fn snapshot(&self) -> MineResult<super::graph_snapshot::GraphSnapshot> {
        self.snapshot_observed(None)
    }

    /// Journal writes borrow this guard, compare the caller revision before
    /// Git, and consume the same freshly parsed graph as their evidence provider.
    pub(crate) fn snapshot_at_revision(
        &self,
        expected: u64,
        guard: &file_lock::FileLock,
    ) -> MineResult<super::graph_snapshot::GraphSnapshot> {
        if guard.path().canonicalize()? != self.lock_path.canonicalize()? {
            return Err(MineError::GraphInvalid {
                detail: "caller does not hold this Root coordinator".into(),
            });
        }
        self.snapshot_observed(Some(expected))
    }

    fn snapshot_observed(
        &self,
        expected: Option<u64>,
    ) -> MineResult<super::graph_snapshot::GraphSnapshot> {
        let _request = super::git_evidence::GitRequestScope::join_or_new();
        if !self.toml_path.exists() {
            return Err(MineError::GraphNotInitialized {
                path: self.toml_path.clone(),
            });
        }
        let content = std::fs::read_to_string(&self.toml_path)?;
        let ws = parse_workspace(&content)?;
        if let Some(expected) = expected {
            if expected != ws.revision {
                return Err(MineError::RevisionConflict {
                    expected,
                    actual: ws.revision,
                });
            }
        }
        let began = std::time::Instant::now();
        let mut git = self.git_request();
        validate_workspace_evidence(&ws, &self.worktrees_root()?, &mut git, &self.repo_root)?;
        super::graph_snapshot::GraphSnapshot::new(
            ws,
            content.as_bytes(),
            &self.repo_root,
            git,
            began.elapsed(),
        )
    }

    /// Observer-only read: no object, ref, status or ancestry query. Validation
    /// belongs under the mutation lock, after the revision comparison.
    pub fn observed_revision(&self) -> MineResult<u64> {
        let content = std::fs::read_to_string(&self.toml_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                MineError::GraphNotInitialized {
                    path: self.toml_path.clone(),
                }
            } else {
                e.into()
            }
        })?;
        Ok(parse_workspace(&content)?.revision)
    }

    /// Persists a workspace under the lock with revision checking.
    ///
    /// `expected_revision` is checked against the on-disk revision *after* the
    /// lock is acquired and the file reloaded. The caller's `mutate` closure
    /// receives the reloaded workspace and returns the new one to persist; it
    /// must set `revision = expected_revision + 1` and update timestamps.
    ///
    /// # Errors
    /// - [`MineError::RevisionConflict`] if the on-disk revision does not match
    ///   `expected_revision`.
    /// - [`MineError::LockTimeout`] if the lock cannot be acquired.
    /// - [`MineError::GraphInvalid`] if the mutated workspace fails validation.
    /// - Partial success ([`MineError::GraphCommitted`] with actual revisions)
    ///   if the TOML writes but Markdown rendering fails.
    pub fn save_with_revision(
        &self,
        expected_revision: u64,
        actor: &str,
        mutate: impl FnOnce(PlanWorkspace) -> MineResult<PlanWorkspace>,
    ) -> MineResult<PlanWorkspace> {
        self.save_with_revision_locked(expected_revision, actor, |workspace, _, _| {
            mutate(workspace)
        })
    }

    /// Native admission consumers borrow the already-held guard and exact
    /// parsed source identity; they must never reacquire the coordinator or
    /// substitute a reserialized/project-label-modified graph for that source.
    pub(crate) fn save_with_revision_locked(
        &self,
        expected_revision: u64,
        actor: &str,
        mutate: impl FnOnce(
            PlanWorkspace,
            &file_lock::FileLock,
            &crate::domain::graph::Sha256,
        ) -> MineResult<PlanWorkspace>,
    ) -> MineResult<PlanWorkspace> {
        let _request = super::git_evidence::GitRequestScope::join_or_new();
        crate::domain::error::contract_text("operation.actor", actor).map_err(|e| {
            MineError::GraphInvalid {
                detail: e.to_string(),
            }
        })?;
        let waiting = std::time::Instant::now();
        *self.diagnostics.lock().expect("diagnostic mutex poisoned") = TransactionDiagnostics {
            operation_id: uuid::Uuid::new_v4().to_string(),
            phase: "lock_wait",
            ..TransactionDiagnostics::default()
        };
        let acquired = self.acquire_coordinator_lock();
        {
            let mut diagnostics = self.diagnostics.lock().expect("diagnostic mutex poisoned");
            diagnostics.lock_wait = waiting.elapsed();
        }
        let guard = acquired?;
        {
            let mut diagnostics = self.diagnostics.lock().expect("diagnostic mutex poisoned");
            diagnostics.graph_loads = 1;
            diagnostics.phase = "locked_reload";
        }
        let mut observation = TransactionObservation {
            target: &self.diagnostics,
            began: std::time::Instant::now(),
            git: self.git_request(),
        };

        // Reload under the lock.
        let content = if self.toml_path.exists() {
            std::fs::read_to_string(&self.toml_path)?
        } else {
            return Err(MineError::GraphNotInitialized {
                path: self.toml_path.clone(),
            });
        };
        let reloaded = parse_workspace(&content)?;

        if reloaded.revision != expected_revision {
            return Err(MineError::RevisionConflict {
                expected: expected_revision,
                actual: reloaded.revision,
            });
        }
        if expected_revision == u64::MAX {
            return Err(MineError::GraphInvalid {
                detail: "graph revision is exhausted; mutation refused".into(),
            });
        }

        // A journaled Git effect can be pending while graph revision is still
        // unchanged. Ordinary graph writers have no same-operation recovery
        // authority and must stop before acquiring further Git evidence.
        self.refuse_pending_publication(&reloaded, &guard)?;
        let original_head = self.checkpoint_root_head()?;

        self.diagnostics
            .lock()
            .expect("diagnostic mutex poisoned")
            .graph_size = reloaded.plans.len();

        let worktrees_root = self.worktrees_root()?;
        let validation_began = std::time::Instant::now();
        validate_workspace_evidence(
            &reloaded,
            &worktrees_root,
            &mut observation.git,
            &self.repo_root,
        )?;
        self.diagnostics
            .lock()
            .expect("diagnostic mutex poisoned")
            .validation_time += validation_began.elapsed();

        let original = reloaded.clone();
        let source_sha256 = crate::infrastructure::graph_checkpoint::digest(content.as_bytes());
        let mut new_ws = mutate(reloaded, &guard, &source_sha256)?;
        // An idempotent logical no-op is not a schema migration. In particular,
        // rewiring an already-closed relationship must preserve older bytes and
        // the observer revision rather than silently publish v7 at the old rev.
        if new_ws == original {
            self.diagnostics
                .lock()
                .expect("diagnostic mutex poisoned")
                .phase = "complete";
            return Ok(original);
        }
        if expected_revision.checked_add(1) != Some(new_ws.revision) {
            return Err(MineError::GraphInvalid {
                detail: "a changed graph must increment the observed revision exactly once".into(),
            });
        }
        // The migration shares the mutation's one revision increment. Missing
        // historical body/acceptance/preparation evidence stays missing.
        new_ws.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
        validation::validate_native_successor(&original, &new_ws)?;
        let validation_began = std::time::Instant::now();
        validate_workspace_evidence(
            &new_ws,
            &worktrees_root,
            &mut observation.git,
            &self.repo_root,
        )?;
        self.diagnostics
            .lock()
            .expect("diagnostic mutex poisoned")
            .validation_time += validation_began.elapsed();

        if std::fs::read(&self.toml_path)?.as_slice() != content.as_bytes() {
            return Err(MineError::GraphInvalid {
                detail: "graph source changed after locked admission".into(),
            });
        }
        self.checkpoint_candidate_locked(
            expected_revision,
            actor,
            &new_ws,
            &guard,
            &original_head,
        )?;
        Ok(new_ws)
    }

    fn checkpoint_candidate_locked(
        &self,
        expected_revision: u64,
        actor: &str,
        candidate: &PlanWorkspace,
        guard: &file_lock::FileLock,
        original_head: &crate::domain::graph::CommitId,
    ) -> MineResult<()> {
        use crate::{
            domain::lifecycle_operation::*, infrastructure::graph_checkpoint::CheckpointKernel,
        };
        let kernel = CheckpointKernel::under_lock(&self.repo_root, guard).map_err(|e| match e {
            OperationError::Mine(e) => e,
            other => MineError::GraphInvalid {
                detail: other.to_string(),
            },
        })?;
        let id = OperationId::generate();
        self.diagnostics
            .lock()
            .expect("diagnostic mutex poisoned")
            .operation_id = id.as_str().into();
        let result = kernel
            .begin_observed(
                id.clone(),
                expected_revision,
                actor,
                candidate,
                original_head,
                &mut NoInterruption,
            )
            .and_then(|_| kernel.resume(&id, &mut NoInterruption));
        match result {
            Ok(record) => {
                let mut diagnostics = self.diagnostics.lock().expect("diagnostic mutex poisoned");
                diagnostics.checkpoint = Some(record.operation);
                diagnostics.phase = "complete";
                Ok(())
            }
            Err(error) => {
                let location = crate::infrastructure::operation_store::OperationLocation::new(
                    &self.repo_root,
                    &id,
                )?
                .resolve()?;
                if location.try_exists()? {
                    let record = kernel.store().query(&id).map_err(|e|MineError::GraphInvalid {detail:format!("checkpoint outcome unknown for {}: {error}; journal unavailable: {e}",id.as_str())})?;
                    self.diagnostics
                        .lock()
                        .expect("diagnostic mutex poisoned")
                        .checkpoint = Some(record.operation.clone());
                    Err(MineError::GraphCheckpointPending {
                        operation: Box::new(record.operation),
                        revision_after: self.observed_revision().ok(),
                        detail: error.to_string(),
                    })
                } else {
                    Err(match error {
                        OperationError::Mine(error) => error,
                        other => MineError::GraphInvalid {
                            detail: other.to_string(),
                        },
                    })
                }
            }
        }
    }

    fn checkpoint_root_head(&self) -> MineResult<crate::domain::graph::CommitId> {
        let mut request =
            crate::infrastructure::graph_checkpoint::checkpoint_git(&self.repo_root, None)
                .map_err(|e| MineError::GraphInvalid {
                    detail: e.to_string(),
                })?;
        let head = request.text(&["rev-parse", "--verify", "HEAD^{commit}"])?;
        crate::domain::graph::CommitId::new(head).map_err(|e| MineError::GraphInvalid {
            detail: e.to_string(),
        })
    }

    /// Repairs a supported legacy schema graph under the graph lock and
    /// upgrades it to the fail-closed current wire format. This is the only
    /// write path that may parse a legacy graph without first passing current
    /// validation.
    ///
    /// v1/v2 graphs run the provenance migration; a v3 graph needs no field
    /// derivation and upgrades to the current schema as a version-only
    /// migration. Repeating the operation against an already-current graph is
    /// an idempotent success with no revision bump and no file rewrite.
    pub fn repair_provenance_with_revision(
        &self,
        expected_revision: u64,
        actor: &str,
    ) -> MineResult<ProvenanceRepairOutcome> {
        let _request = super::git_evidence::GitRequestScope::join_or_new();
        crate::domain::error::contract_text("operation.actor", actor).map_err(|e| {
            MineError::GraphInvalid {
                detail: e.to_string(),
            }
        })?;
        let guard = self.acquire_coordinator_lock()?;
        if !self.toml_path.exists() {
            return Err(MineError::GraphNotInitialized {
                path: self.toml_path.clone(),
            });
        }
        let content = std::fs::read_to_string(&self.toml_path)?;
        let mut workspace = parse_workspace(&content)?;
        if workspace.revision != expected_revision {
            return Err(MineError::RevisionConflict {
                expected: expected_revision,
                actual: workspace.revision,
            });
        }
        self.refuse_pending_publication(&workspace, &guard)?;
        let worktrees_root = self.worktrees_root()?;
        let mut request = self.git_request();
        if workspace.schema_version == CURRENT_GRAPH_SCHEMA_VERSION {
            validate_workspace_evidence(
                &workspace,
                &worktrees_root,
                &mut request,
                &self.repo_root,
            )?;
            return Ok(ProvenanceRepairOutcome {
                revision_before: workspace.revision,
                revision_after: workspace.revision,
                workspace,
                changed: false,
                repaired_plans: Vec::new(),
            });
        }
        let revision_before = workspace.revision;
        let original_head = self.checkpoint_root_head()?;
        let repaired_plans = if is_supported_graph_schema(workspace.schema_version) {
            // Version-only upgrade: the v4 fields are optional with defaults,
            // so nothing is derived.
            validate_workspace_evidence(
                &workspace,
                &worktrees_root,
                &mut request,
                &self.repo_root,
            )?;
            Vec::new()
        } else if matches!(
            workspace.schema_version,
            LEGACY_GRAPH_SCHEMA_VERSION | ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION
        ) {
            migrate_provenance(&mut workspace, &worktrees_root, &self.repo_root)?
        } else {
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "unsupported execution-graph schema version {}; no migration is registered",
                    workspace.schema_version
                ),
            });
        };
        workspace.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
        workspace.revision =
            revision_before
                .checked_add(1)
                .ok_or_else(|| MineError::GraphInvalid {
                    detail: "graph revision exhausted".into(),
                })?;
        validate_workspace_evidence(&workspace, &worktrees_root, &mut request, &self.repo_root)?;
        if std::fs::read(&self.toml_path)?.as_slice() != content.as_bytes() {
            return Err(MineError::GraphInvalid {
                detail: "repair source changed after admission".into(),
            });
        }
        self.checkpoint_candidate_locked(
            revision_before,
            actor,
            &workspace,
            &guard,
            &original_head,
        )?;

        Ok(ProvenanceRepairOutcome {
            revision_before,
            revision_after: workspace.revision,
            workspace,
            changed: true,
            repaired_plans,
        })
    }

    /// Repairs one invalid compensation predecessor list under the graph lock
    /// without accepting caller-supplied edges.
    ///
    /// The rejected predecessor recorded in the graph is the sole authority.
    /// The operation is limited to a compensation with no branch or execution,
    /// implementation, review, child-compensation, or withdrawal evidence.
    /// Every refusal happens before persistence; an already-exact valid target
    /// is an idempotent no-op with no revision bump or file rewrite.
    ///
    /// # Errors
    /// Returns a revision, graph, identity, evidence, or Git-evidence error
    /// when any ADR-0014 precondition is not proven.
    pub fn repair_compensation_predecessors_with_revision(
        &self,
        actor: &str,
        plan_id: &str,
        expected_revision: u64,
        reason: &str,
        updated_at: &str,
    ) -> MineResult<CompensationPredecessorRepairOutcome> {
        let _request = super::git_evidence::GitRequestScope::join_or_new();
        crate::domain::error::contract_text("operation.actor", actor).map_err(|e| {
            MineError::GraphInvalid {
                detail: e.to_string(),
            }
        })?;
        let guard = self.acquire_coordinator_lock()?;
        if !self.toml_path.exists() {
            return Err(MineError::GraphNotInitialized {
                path: self.toml_path.clone(),
            });
        }
        let content = std::fs::read_to_string(&self.toml_path)?;
        let mut workspace = parse_workspace(&content)?;
        // The immediately previous schema is a supported read format whose
        // optional v4 fields default safely. As this operation is itself a
        // lifecycle mutation, its candidate must perform the ordinary
        // next-mutation schema upgrade under the same lock and revision bump.
        let upgrades_previous_schema = match workspace.schema_version {
            CURRENT_GRAPH_SCHEMA_VERSION => false,
            version if is_supported_graph_schema(version) => true,
            version => {
                return Err(MineError::GraphInvalid {
                    detail: format!(
                        "compensation predecessor repair requires schema version {CURRENT_GRAPH_SCHEMA_VERSION} or immediately previous version {PREVIOUS_GRAPH_SCHEMA_VERSION}; found {version}"
                    ),
                });
            }
        };
        if workspace.revision != expected_revision {
            return Err(MineError::RevisionConflict {
                expected: expected_revision,
                actual: workspace.revision,
            });
        }
        let reason = reason.trim();
        self.refuse_pending_publication(&workspace, &guard)?;
        let original_head = self.checkpoint_root_head()?;
        if reason.is_empty() {
            return Err(MineError::GraphInvalid {
                detail: "compensation predecessor repair requires a non-empty reason".to_string(),
            });
        }
        if updated_at.trim().is_empty() {
            return Err(MineError::GraphInvalid {
                detail: "compensation predecessor repair requires a non-empty timestamp"
                    .to_string(),
            });
        }

        let target = workspace
            .get(plan_id)
            .ok_or_else(|| MineError::PlanNotFound {
                plan_id: plan_id.to_string(),
            })?
            .clone();
        let parsed = CanonicalPlanId::parse(plan_id)?;
        if !parsed.is_compensation() {
            return Err(MineError::GraphInvalid {
                detail: format!("plan {plan_id} is not a compensation identity"),
            });
        }
        if !matches!(
            target.status,
            PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready
        ) {
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "compensation {plan_id} status {} is not repairable",
                    target.status
                ),
            });
        }
        ensure_compensation_repair_has_no_execution_evidence(&target)?;
        if git::branch_exists(&self.repo_root, &target.branch) {
            return Err(MineError::GitEvidence {
                plan_id: plan_id.to_string(),
                detail: format!("compensation branch {} already exists", target.branch),
            });
        }
        if git::list_worktrees(&self.repo_root)?
            .iter()
            .any(|worktree| worktree.branch.as_deref() == Some(target.branch.as_str()))
        {
            return Err(MineError::GitEvidence {
                plan_id: plan_id.to_string(),
                detail: "a linked worktree is already checked out on the compensation branch"
                    .to_string(),
            });
        }

        let rejected = validation::declaring_rejected_predecessor(&workspace, plan_id)?;
        let rejected_id = rejected.id.clone();
        let authoritative = rejected.hard_predecessors.clone();
        let predecessors_before = target.hard_predecessors.clone();
        let status_before = target.status;
        if predecessors_before == authoritative && !upgrades_previous_schema {
            let worktrees_root = self.worktrees_root()?;
            validate_persisted_workspace(&workspace, &worktrees_root, &self.repo_root)?;
            return Ok(CompensationPredecessorRepairOutcome {
                workspace,
                plan_id: plan_id.to_string(),
                rejected_predecessor: rejected_id,
                predecessors_before: predecessors_before.clone(),
                predecessors_after: predecessors_before,
                status_before,
                status_after: status_before,
                reason: reason.to_string(),
                revision_before: expected_revision,
                revision_after: expected_revision,
                changed: false,
            });
        }

        if predecessors_before != authoritative {
            let target = workspace
                .get_mut(plan_id)
                .expect("target was resolved before mutation");
            target.hard_predecessors = authoritative.clone();
            target.status = match status_before {
                PlanStatus::Draft => PlanStatus::Draft,
                PlanStatus::Blocked | PlanStatus::Ready => PlanStatus::Ready,
                _ => unreachable!("repairable status checked above"),
            };
            target.updated_at = updated_at.to_string();
        }
        let status_after = workspace
            .get(plan_id)
            .expect("target remains registered after bounded mutation")
            .status;
        workspace.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
        workspace.revision =
            expected_revision
                .checked_add(1)
                .ok_or_else(|| MineError::GraphInvalid {
                    detail: "graph revision exhausted".into(),
                })?;
        let worktrees_root = self.worktrees_root()?;
        validate_persisted_workspace(&workspace, &worktrees_root, &self.repo_root)?;
        if std::fs::read(&self.toml_path)?.as_slice() != content.as_bytes() {
            return Err(MineError::GraphInvalid {
                detail: "repair source changed after admission".into(),
            });
        }
        self.checkpoint_candidate_locked(
            expected_revision,
            actor,
            &workspace,
            &guard,
            &original_head,
        )?;

        Ok(CompensationPredecessorRepairOutcome {
            revision_before: expected_revision,
            revision_after: workspace.revision,
            workspace,
            plan_id: plan_id.to_string(),
            rejected_predecessor: rejected_id,
            predecessors_before,
            predecessors_after: authoritative,
            status_before,
            status_after,
            reason: reason.to_string(),
            changed: true,
        })
    }

    /// Re-renders the Markdown view from the current TOML without mutating
    /// the TOML. Used by `mine graph render` repair.
    ///
    /// # Errors
    /// Returns [`MineError::GraphInvalid`] if the TOML cannot be loaded or
    /// rendered.
    pub fn render(&self) -> MineResult<()> {
        let _request = super::git_evidence::GitRequestScope::join_or_new();
        let ws = self.load()?;
        let md = render_markdown(&ws)?;
        atomic_write::write(&self.md_path, md.as_bytes())?;
        Ok(())
    }

    /// Returns the repository root.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    fn refuse_pending_publication(
        &self,
        workspace: &PlanWorkspace,
        guard: &file_lock::FileLock,
    ) -> MineResult<()> {
        use super::operation_store::{OperationStore, read_bounded, regular_path};
        let refused =
            |error: crate::domain::lifecycle_operation::OperationError| MineError::GraphInvalid {
                detail: error.to_string(),
            };
        let directory = regular_path(
            &self.repo_root,
            &crate::domain::graph::RepoPath::new(".mine/runtime/operations")
                .expect("constant operation directory"),
        )?;
        if !directory.try_exists()? {
            return Ok(());
        }
        let config_path = regular_path(
            &self.repo_root,
            &crate::domain::graph::RepoPath::new(".mine/config.toml")
                .expect("constant configuration locator"),
        )?;
        let config_bytes = read_bounded(&config_path).map_err(refused)?;
        let config = MineConfig::parse(
            &config_path,
            std::str::from_utf8(&config_bytes).map_err(|_| MineError::GraphInvalid {
                detail: "configuration encoding".into(),
            })?,
        )?;
        let store = OperationStore::under_lock(
            &self.repo_root,
            &config.repository_id,
            &workspace.workspace_id,
            guard,
        )
        .map_err(refused)?;
        if let Some(pending) = store.barriers().map_err(refused)?.global.first() {
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "pending workspace operation {}; query and recover that same operation before a graph write",
                    pending.operation_id.as_str()
                ),
            });
        }
        Ok(())
    }

    /// Returns the configured repository-relative linked-worktree root.
    pub fn worktrees_root(&self) -> MineResult<String> {
        let path = self.repo_root.join(".mine").join("config.toml");
        if !path.exists() {
            return Ok(".mine/worktrees".to_string());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(MineConfig::parse(&path, &content)?.plan.worktrees_root)
    }
}

fn ensure_compensation_repair_has_no_execution_evidence(
    target: &crate::domain::graph::PlanNode,
) -> MineResult<()> {
    let has_evidence = !target.branch_anchor.is_empty()
        || !target.preparations.is_empty()
        || target.started_preparation_generation.is_some()
        || !target.owner.is_empty()
        || !target.run_id.is_empty()
        || !target.started_at.is_empty()
        || !target.implementation_report.is_empty()
        || !target.review_report.is_empty()
        || !target.implementation_commits.is_empty()
        || !target.rejection_reason.is_empty()
        || !target.compensating_plan.is_empty()
        || !target.withdraw_reason.is_empty()
        || !target.withdrawn_at.is_empty();
    if has_evidence {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "compensation {} has lifecycle evidence and cannot be repaired",
                target.id
            ),
        });
    }
    Ok(())
}

fn parse_workspace(content: &str) -> MineResult<PlanWorkspace> {
    toml::from_str(content).map_err(|e| MineError::GraphInvalid {
        detail: format!("could not parse execution-graph TOML: {e}"),
    })
}

fn validate_persisted_workspace(
    workspace: &PlanWorkspace,
    worktrees_root: &str,
    repo_root: &Path,
) -> MineResult<()> {
    validate_workspace_evidence(
        workspace,
        worktrees_root,
        &mut super::git_evidence::GitRequest::new(repo_root),
        repo_root,
    )
}

fn validate_workspace_evidence(
    workspace: &PlanWorkspace,
    worktrees_root: &str,
    git: &mut super::git_evidence::GitRequest,
    root: &Path,
) -> MineResult<()> {
    validation::validate_with_worktrees_root(workspace, worktrees_root)?;
    let config_path = super::graph_snapshot::resolve_contained(
        root,
        &crate::domain::graph::RepoPath::new(".mine/config.toml").expect("constant path"),
    )?;
    if config_path.try_exists()? {
        let config = MineConfig::parse(&config_path, &std::fs::read_to_string(&config_path)?)?;
        let layout = config.layout()?;
        for node in &workspace.plans {
            if !is_exact_bootstrap_node(&workspace.workspace_id, &node.id, &node.path) {
                layout.validate_plan_path(&node.id, &node.path)?;
            }
        }
    }
    let anchors: Vec<_> = workspace
        .plans
        .iter()
        .filter(|node| !node.branch_anchor.is_empty())
        .map(|node| node.branch_anchor.clone())
        .collect();
    git.validate_commits(&anchors).map_err(|_| {
            MineError::GraphInvalid {
                detail: "branch_anchor evidence is not an exact full native commit object ID in the repository".to_string(),
            }
        })
}

fn migrate_provenance(
    workspace: &mut PlanWorkspace,
    worktrees_root: &str,
    repo_root: &Path,
) -> MineResult<Vec<String>> {
    let worktrees_root = normalize_repo_relative(worktrees_root)?;
    let workspace_id = workspace.workspace_id.clone();
    let mut repaired = Vec::new();
    for node in &mut workspace.plans {
        let (lineage_id, worktree_path) =
            if is_exact_bootstrap_node(&workspace_id, &node.id, &node.path) {
                // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                ("plan01".to_string(), format!("{worktrees_root}/plan01"))
            } else {
                let parsed = CanonicalPlanId::parse(&node.id)?;
                (
                    parsed.lineage_id(),
                    parsed.worktree_path_under(&worktrees_root),
                )
            };
        let branch = registered_plan_branch(&workspace_id, &node.id, &node.path)?;
        let changed = fill_or_verify(&mut node.lineage_id, &lineage_id, &node.id, "lineage_id")?
            | fill_or_verify(
                &mut node.worktree_path,
                &worktree_path,
                &node.id,
                "worktree_path",
            )?
            | fill_or_verify(&mut node.branch, &branch, &node.id, "branch")?;
        if changed {
            repaired.push(node.id.clone());
        }
    }
    for index in 0..workspace.plans.len() {
        let node = &workspace.plans[index];
        if matches!(
            node.status,
            crate::domain::status::PlanStatus::Draft
                | crate::domain::status::PlanStatus::Blocked
                | crate::domain::status::PlanStatus::Ready
                | crate::domain::status::PlanStatus::Withdrawn
        ) {
            if !node.branch_anchor.is_empty() {
                return Err(MineError::GraphInvalid {
                    detail: format!(
                        "pre-start plan {} has a branch_anchor; refusing repair",
                        node.id
                    ),
                });
            }
            continue;
        }
        let anchor = derive_branch_anchor(workspace, index, repo_root)?;
        let node = &mut workspace.plans[index];
        if fill_or_verify(&mut node.branch_anchor, &anchor, &node.id, "branch_anchor")?
            && !repaired.contains(&node.id)
        {
            repaired.push(node.id.clone());
        }
    }
    Ok(repaired)
}

fn derive_branch_anchor(
    workspace: &PlanWorkspace,
    index: usize,
    repo_root: &Path,
) -> MineResult<String> {
    let node = &workspace.plans[index];
    if let Some(first) = node.implementation_commits.first() {
        let mut resolved = Vec::with_capacity(node.implementation_commits.len());
        for token in &node.implementation_commits {
            resolved.push(git::resolve_commit(repo_root, token).map_err(|_| {
                MineError::GraphInvalid {
                    detail: format!(
                        "plan {} implementation commit {token} does not resolve to a commit",
                        node.id
                    ),
                }
            })?);
        }
        for pair in resolved.windows(2) {
            if !git::is_ancestor_checked(repo_root, &pair[0], &pair[1])? {
                return Err(MineError::GraphInvalid {
                    detail: format!(
                        "plan {} implementation commits are not in ancestor order",
                        node.id
                    ),
                });
            }
        }
        let first = git::resolve_commit(repo_root, first).map_err(|_| MineError::GraphInvalid {
            detail: format!("plan {} first implementation commit is invalid", node.id),
        })?;
        return git::first_parent(repo_root, &first).map_err(|_| MineError::GraphInvalid {
            detail: format!(
                "plan {} first implementation commit has no resolvable parent",
                node.id
            ),
        });
    }

    let parsed = CanonicalPlanId::parse(&node.id).map_err(|_| MineError::GraphInvalid {
        detail: format!(
            "started plan {} has no implementation commits and is not an unambiguous compensation",
            node.id
        ),
    })?;
    if !parsed.is_compensation() {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "active base plan {} has no implementation commits; refusing repair",
                node.id
            ),
        });
    }
    let predecessors: Vec<_> = workspace
        .plans
        .iter()
        .filter(|candidate| {
            candidate.status == crate::domain::status::PlanStatus::Rejected
                && candidate.compensating_plan == node.id
                && candidate.lineage_id == node.lineage_id
        })
        .collect();
    if predecessors.len() != 1 {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "plan {} does not have exactly one declared rejected predecessor",
                node.id
            ),
        });
    }
    let predecessor = predecessors[0];
    crate::domain::plan_id::validate_compensation_replacement(&predecessor.id, &node.id)?;
    if predecessor.hard_predecessors != node.hard_predecessors {
        return Err(MineError::GraphInvalid {
            detail: format!("plan {} does not inherit predecessor dependencies", node.id),
        });
    }
    let anchor = git::head_of_branch(repo_root, &predecessor.branch).map_err(|_| {
        MineError::GraphInvalid {
            detail: format!("plan {} rejected predecessor branch is missing", node.id),
        }
    })?;
    let tip =
        git::head_of_branch(repo_root, &node.branch).map_err(|_| MineError::GraphInvalid {
            detail: format!("plan {} active branch is missing", node.id),
        })?;
    let target = repo_root.join(&node.worktree_path);
    let target = target.canonicalize().map_err(|_| MineError::GraphInvalid {
        detail: format!("plan {} active worktree is missing", node.id),
    })?;
    let inventory = git::list_worktrees(repo_root)?;
    let observed = inventory
        .iter()
        .find(|worktree| {
            PathBuf::from(&worktree.path)
                .canonicalize()
                .is_ok_and(|path| path == target)
        })
        .ok_or_else(|| MineError::GraphInvalid {
            detail: format!("plan {} active worktree is not registered", node.id),
        })?;
    if observed.branch.as_deref() != Some(node.branch.as_str()) || observed.head != tip {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "plan {} observed worktree does not match its branch tip",
                node.id
            ),
        });
    }
    if !git::is_ancestor_checked(repo_root, &anchor, &tip)? {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "plan {} rejected predecessor tip is not an ancestor of active tip",
                node.id
            ),
        });
    }
    Ok(anchor)
}

fn fill_or_verify(
    field: &mut String,
    expected: &str,
    plan_id: &str,
    field_name: &str,
) -> MineResult<bool> {
    if field.is_empty() {
        *field = expected.to_string();
        return Ok(true);
    }
    if field != expected {
        return Err(MineError::GraphInvalid {
            detail: format!("legacy plan {plan_id} has inconsistent {field_name}; refusing repair"),
        });
    }
    Ok(false)
}

/// Renders the Markdown view for a workspace.
///
/// The view is a stable, human- and agent-readable summary. The format is
/// deterministic: the same TOML always renders to the same Markdown.
pub fn render_markdown(ws: &PlanWorkspace) -> MineResult<String> {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        out,
        "# Execution Graph\n\n> GENERATED VIEW. Do not edit directly. The machine fact source is `execution-graph.toml`.\n>\n> This directory is ephemeral and must be purged before stable release integration.\n\n- Workspace: `{}`\n- Stable branch: `{}`\n- Integration branch: `{}`\n- Revision: `{}`\n",
        ws.workspace_id, ws.stable_branch, ws.integration_branch, ws.revision
    )
    .map_err(|e| MineError::GraphInvalid {
        detail: format!("markdown write failed: {e}"),
    })?;

    writeln!(out, "| Plan | Title | Status | Hard predecessors |").map_err(|e| {
        MineError::GraphInvalid {
            detail: format!("markdown write failed: {e}"),
        }
    })?;
    writeln!(out, "|---|---|---|---|").map_err(|e| MineError::GraphInvalid {
        detail: format!("markdown write failed: {e}"),
    })?;
    for p in &ws.plans {
        let preds = if p.hard_predecessors.is_empty() {
            "-".to_string()
        } else {
            p.hard_predecessors.join(", ")
        };
        writeln!(out, "| {} | {} | {} | {} |", p.id, p.title, p.status, preds).map_err(|e| {
            MineError::GraphInvalid {
                detail: format!("markdown write failed: {e}"),
            }
        })?;
    }

    // Dependencies section: every rendered edge corresponds to an actual
    // predecessor entry on a node. Arrows denote dependency edges ONLY; they
    // never join plans that merely happen to be adjacent in a topological
    // sort. Lines are sorted by successor id, then predecessor list order
    // (hard entries before soft entries, each in list order).
    out.push_str(
        "
## Dependencies

```text
",
    );
    let mut successors: Vec<&crate::domain::graph::PlanNode> = ws.plans.iter().collect();
    successors.sort_by(|a, b| a.id.cmp(&b.id));
    let mut edge_count = 0usize;
    for successor in successors {
        let hard: Vec<String> = successor
            .hard_predecessors
            .iter()
            .map(|pred| format!("{pred} -> {} (hard)", successor.id))
            .collect();
        let soft: Vec<String> = successor
            .soft_predecessors
            .iter()
            .map(|pred| format!("{pred} -> {} (soft)", successor.id))
            .collect();
        for line in hard.iter().chain(soft.iter()) {
            writeln!(out, "{line}").map_err(|e| MineError::GraphInvalid {
                detail: format!("markdown write failed: {e}"),
            })?;
            edge_count += 1;
        }
    }
    if edge_count == 0 {
        out.push_str(
            "(no dependencies)
",
        );
    }
    out.push_str(
        "```
",
    );

    // Execution order: the topological sort rendered as a numbered
    // enumeration explicitly labeled as one legal execution order.
    let order = validation::topological_sort(ws)?;
    out.push_str(
        "
## Execution order

One legal execution order:

```text
",
    );
    if order.is_empty() {
        out.push_str(
            "(no plans)
",
        );
    } else {
        for (index, id) in order.iter().enumerate() {
            writeln!(out, "{}. {id}", index + 1).map_err(|e| MineError::GraphInvalid {
                detail: format!("markdown write failed: {e}"),
            })?;
        }
    }
    out.push_str(
        "```
",
    );

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::graph::PlanNode;
    use crate::domain::status::PlanStatus;

    fn sample_workspace(rev: u64) -> PlanWorkspace {
        PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: rev,
            project_id: "mine-is-not-everyones".to_string(),
            workspace_id: "canonical-store-fixture".to_string(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: "abc".to_string(),
            design_root: ".mine/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![PlanNode {
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                id: "plan01".to_string(),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                path: ".mine/plan/plan01-test.md".to_string(),
                title: "First".to_string(),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                branch: "plan/plan01-test".into(),
                branch_anchor: String::new(),
                status: PlanStatus::Ready,
                hard_predecessors: vec![],
                soft_predecessors: vec![],
                design_references: vec![".mine/design/principles.md".to_string()],
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
        }
    }

    #[test]
    fn load_returns_not_initialized_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlStore::new(dir.path());
        let err = store.load().unwrap_err();
        assert_eq!(err.code(), "MINE_GRAPH_NOT_INITIALIZED");
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlStore::new(dir.path());
        // Seed the TOML directly (simulating an existing graph).
        let ws = sample_workspace(0);
        std::fs::create_dir_all(store.toml_path.parent().unwrap()).unwrap();
        std::fs::write(&store.toml_path, toml::to_string(&ws).unwrap()).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded, ws);
        assert!(!store.md_path.exists()); // save_with_revision renders; load does not.
    }

    #[test]
    fn save_with_revision_increments_and_renders() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlStore::new(dir.path());
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .current_dir(dir.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        };
        git(&["init", "-b", "dev"]);
        for (key, value) in [
            ("user.name", "Store fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
        ] {
            git(&["config", key, value]);
        }
        std::fs::write(dir.path().join("README.md"), "# Native store fixture\n").unwrap();
        git(&["add", "README.md"]);
        git(&["commit", "-m", "stable fixture baseline"]);
        let baseline = git(&["rev-parse", "HEAD"]);
        git(&["branch", "master"]);
        let source_config = Path::new(env!("CARGO_MANIFEST_DIR")).join(".mine/config.toml");
        let mut config = MineConfig::parse(
            &source_config,
            &std::fs::read_to_string(&source_config)
                .unwrap()
                .replace("docs/design", ".mine/design")
                .replace("docs/plan", ".mine/plan"),
        )
        .unwrap();
        config.repository_id = uuid::Uuid::new_v4().to_string();
        for directory in [".mine", ".mine/design", ".mine/plan"] {
            std::fs::create_dir_all(dir.path().join(directory)).unwrap();
        }
        std::fs::write(dir.path().join(".mine/config.toml"), config.to_toml()).unwrap();
        std::fs::write(dir.path().join(&config.design.root), "# Fixture Design\n").unwrap();
        std::fs::write(
            dir.path().join(&config.design.marker),
            crate::domain::design_marker::DesignMarker::new(
                config.repository_id,
                "2026-10-17T00:00:00Z".into(),
            )
            .to_toml(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join(".gitignore"),
            ".mine/runtime/\n.mine/locks/\n",
        )
        .unwrap();
        let mut ws = sample_workspace(0);
        ws.workspace_id = uuid::Uuid::new_v4().to_string();
        ws.stable_baseline_commit = baseline;
        std::fs::write(&store.toml_path, toml::to_string(&ws).unwrap()).unwrap();
        std::fs::write(&store.md_path, render_markdown(&ws).unwrap()).unwrap();
        std::fs::write(dir.path().join(&ws.plans[0].path), "# Store fixture Plan\n").unwrap();
        git(&[
            "add",
            ".gitignore",
            ".mine/config.toml",
            ".mine/design",
            ".mine/plan",
        ]);
        git(&["commit", "-m", "committed source governance"]);

        let result = store
            .save_with_revision(0, "fixture-actor", |mut w| {
                w.revision = 1;
                w.plans[0].title = "Updated fixture title".into();
                Ok(w)
            })
            .unwrap();
        assert_eq!(result.revision, 1);
        assert_eq!(result.plans[0].title, "Updated fixture title");

        // Markdown rendered with the new revision.
        let md = std::fs::read_to_string(store.md_path).unwrap();
        assert!(md.contains("Revision: `1`"));
        assert!(md.contains("Updated fixture title"));
        assert_ne!(git(&["rev-parse", "HEAD"]), git(&["rev-parse", "master"]));
        assert!(git(&["status", "--porcelain"]).is_empty());
    }

    #[test]
    fn save_with_revision_conflict_detected() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlStore::new(dir.path());
        let ws = sample_workspace(5);
        std::fs::create_dir_all(store.toml_path.parent().unwrap()).unwrap();
        std::fs::write(&store.toml_path, toml::to_string(&ws).unwrap()).unwrap();

        let err = store
            .save_with_revision(4, "fixture-actor", |w| {
                // caller expected revision 4 but on disk it is 5
                Ok(w)
            })
            .unwrap_err();
        assert_eq!(err.code(), "MINE_REVISION_CONFLICT");
        // TOML unchanged.
        assert_eq!(store.load().unwrap().revision, 5);
    }

    #[test]
    fn render_repair_regenerates_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlStore::new(dir.path());
        let ws = sample_workspace(7);
        std::fs::create_dir_all(store.toml_path.parent().unwrap()).unwrap();
        std::fs::write(&store.toml_path, toml::to_string(&ws).unwrap()).unwrap();
        // Stale / absent markdown.
        store.render().unwrap();
        let md = std::fs::read_to_string(store.md_path).unwrap();
        assert!(md.contains("Revision: `7`"));
    }

    #[test]
    fn render_markdown_is_deterministic() {
        let ws = sample_workspace(3);
        let a = render_markdown(&ws).unwrap();
        let b = render_markdown(&ws).unwrap();
        assert_eq!(a, b);
    }
}
