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
use crate::infrastructure::toml_store::{ProvenanceRepairOutcome, TomlStore};

/// Application-level read-only graph service.
pub struct GraphService<'a> {
    store: &'a TomlStore,
}

impl<'a> GraphService<'a> {
    /// Wraps a [`TomlStore`] for read-only graph access.
    #[must_use]
    pub fn new(store: &'a TomlStore) -> Self {
        Self { store }
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
        let ws = self.store.load()?;
        let expected = crate::infrastructure::toml_store::render_markdown(&ws)?;
        let view = self.store.md_path().display().to_string();
        match std::fs::read(self.store.md_path()) {
            Ok(bytes) if bytes == expected.as_bytes() => Ok(ws),
            Ok(_) => Err(MineError::GraphInvalid {
                detail: format!(
                    "generated view stale: {view} does not match a fresh render (run `mine graph render` to repair)"
                ),
            }),
            Err(_) => Err(MineError::GraphInvalid {
                detail: format!(
                    "generated view missing: {view} (run `mine graph render` to repair)"
                ),
            }),
        }
    }

    /// Returns the loaded workspace (revision, identity, frontier). Read-only.
    pub fn status(&self) -> MineResult<GraphStatus> {
        let ws = self.store.load()?;
        let ready = validation::ready_frontier(&ws);
        Ok(GraphStatus {
            workspace_id: ws.workspace_id,
            revision: ws.revision,
            stable_branch: ws.stable_branch,
            integration_branch: ws.integration_branch,
            plan_count: ws.plans.len(),
            ready,
        })
    }

    /// Returns the READY frontier. Read-only.
    pub fn ready(&self) -> MineResult<Vec<String>> {
        Ok(validation::ready_frontier(&self.store.load()?))
    }

    /// Returns a stable parallel wave (no write-scope overlap among READY
    /// plans). Read-only.
    pub fn wave(&self) -> MineResult<Vec<String>> {
        Ok(validation::parallel_wave(&self.store.load()?))
    }

    /// Re-renders the Markdown view from the committed TOML (`mine graph
    /// render`). No revision change.
    pub fn render(&self) -> MineResult<()> {
        self.store.render()
    }

    /// Performs the supported revision-checked schema-v1/v2 provenance
    /// migration, or returns an idempotent no-op for an already-valid v3 graph.
    pub fn repair_provenance(&self, expected_revision: u64) -> MineResult<ProvenanceRepairOutcome> {
        self.store
            .repair_provenance_with_revision(expected_revision)
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
    pub fn mutate<F>(&self, expected_revision: u64, mutate: F) -> MineResult<PlanWorkspace>
    where
        F: FnOnce(PlanWorkspace) -> MineResult<PlanWorkspace>,
    {
        self.store.save_with_revision(expected_revision, mutate)
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
}

/// A request to start a plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanStartRequest {
    pub id: String,
    pub owner: String,
    pub run_id: String,
    pub started_at: String,
}

/// A request to mark a plan implemented (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanImplementedRequest {
    pub id: String,
    pub report: String,
    pub commits: Vec<String>,
    pub updated_at: String,
}

/// A request to accept a plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanAcceptRequest {
    pub id: String,
    pub review_report: String,
    pub updated_at: String,
}

/// A request to reject a plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanRejectRequest {
    pub id: String,
    pub reason: String,
    pub review_report: String,
    pub compensating_plan: String,
    pub updated_at: String,
}

/// A request to add a plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanAddRequest {
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
            workspace_id: "bootstrap-mine-v1".to_string(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: "c".to_string(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![PlanNode {
                id: "01".to_string(),
                path: "docs/plan/01.md".to_string(),
                title: "First".to_string(),
                branch: String::new(),
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
                lineage_id: String::new(),
                worktree_path: String::new(),
            }],
        };
        std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
        std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();
        let svc = GraphService::new(&store);
        let st = svc.status().unwrap();
        assert_eq!(st.revision, 5);
        assert_eq!(st.ready, vec!["01".to_string()]);
    }
}
