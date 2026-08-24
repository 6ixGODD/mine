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
//! The service delegates all graph mutations to [`TomlStore::save_with_revision`]
//! (lock → reload → recheck revision → mutate → atomic write → render),
//! preserving revision and optimistic-concurrency semantics. It performs no
//! Git mutation (commit/merge/reset/clean/stash/push/branch-delete) — branch
//! and commit actions belong to the Skills/agent layer, not this service.

use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::PlanWorkspace;
use crate::domain::ports::{Clock, UuidSource};
use crate::domain::status::PlanStatus;
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
}

/// The internal workspace service. Constructed with injected ports for
/// deterministic tests.
pub struct WorkspaceService<'a> {
    uuid_source: &'a dyn UuidSource,
    /// Reserved for timestamp-stamping workspace events in later plans; kept
    /// so the injected port shape stays stable.
    #[allow(dead_code)]
    clock: &'a dyn Clock,
}

impl<'a> WorkspaceService<'a> {
    /// Creates the service with the given identifier/time sources.
    #[must_use]
    pub fn new(uuid_source: &'a dyn UuidSource, clock: &'a dyn Clock) -> Self {
        Self { uuid_source, clock }
    }

    /// Opens (initializes) a workspace on the graph. Idempotent: if the graph
    /// already has a workspace_id, it is preserved and only a no-op re-run is
    /// reported (no state change, no revision increment).
    pub fn open(
        &self,
        store: &TomlStore,
        stable_baseline_commit: &str,
        stable_branch: &str,
        integration_branch: &str,
    ) -> MineResult<WorkspaceOpenOutcome> {
        match store.load() {
            Ok(ws) if !ws.workspace_id.is_empty() => {
                // Workspace already initialized. Preserve identity; no mutation.
                Ok(WorkspaceOpenOutcome {
                    workspace_id: ws.workspace_id,
                    stable_baseline_commit: ws.stable_baseline_commit,
                    integration_branch: ws.integration_branch,
                    revision_before: ws.revision,
                    revision_after: ws.revision,
                })
            }
            Ok(ws) => {
                // Graph present but no workspace identity yet: initialize in
                // place via the locked, revision-checked mutation path.
                let expected = ws.revision;
                let workspace_id = self.uuid_source.new_repository_id();
                let baseline = stable_baseline_commit.to_string();
                let saved = store.save_with_revision(expected, move |mut reloaded| {
                    if reloaded.revision != expected {
                        return Err(MineError::RevisionConflict {
                            expected,
                            actual: reloaded.revision,
                        });
                    }
                    reloaded.workspace_id = workspace_id;
                    reloaded.stable_baseline_commit = baseline;
                    reloaded.integration_branch = integration_branch.to_string();
                    reloaded.stable_branch = stable_branch.to_string();
                    reloaded.revision = expected + 1;
                    Ok(reloaded)
                })?;
                Ok(WorkspaceOpenOutcome {
                    workspace_id: saved.workspace_id,
                    stable_baseline_commit: saved.stable_baseline_commit,
                    integration_branch: saved.integration_branch,
                    revision_before: expected,
                    revision_after: saved.revision,
                })
            }
            Err(MineError::GraphNotInitialized { .. }) => {
                // No graph at all: create a fresh workspace at revision 1.
                let workspace_id = self.uuid_source.new_repository_id();
                let new_ws = self.build_initial_workspace(
                    workspace_id.clone(),
                    stable_baseline_commit,
                    stable_branch,
                    integration_branch,
                );
                let toml_content =
                    toml::to_string(&new_ws).map_err(|e| MineError::GraphInvalid {
                        detail: format!("could not serialize execution-graph TOML: {e}"),
                    })?;
                if let Some(parent) = store.toml_path().parent() {
                    std::fs::create_dir_all(parent)?;
                }
                crate::infrastructure::atomic_write::write(
                    store.toml_path(),
                    toml_content.as_bytes(),
                )?;
                // Render the generated Markdown view for parity.
                store.render()?;
                Ok(WorkspaceOpenOutcome {
                    workspace_id: new_ws.workspace_id,
                    stable_baseline_commit: new_ws.stable_baseline_commit,
                    integration_branch: new_ws.integration_branch,
                    revision_before: 0,
                    revision_after: new_ws.revision,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Reports the current workspace status. Read-only (no mutation, no
    /// revision change).
    pub fn status(&self, store: &TomlStore) -> MineResult<WorkspaceStatusOutcome> {
        let ws = store.load()?;
        let has_unresolved = ws
            .plans
            .iter()
            .any(|p| !matches!(p.status, PlanStatus::Accepted | PlanStatus::Rejected));
        Ok(WorkspaceStatusOutcome {
            workspace_id: ws.workspace_id,
            revision: ws.revision,
            integration_branch: ws.integration_branch,
            stable_branch: ws.stable_branch,
            stable_baseline_commit: ws.stable_baseline_commit,
            plan_count: ws.plans.len(),
            has_unresolved,
        })
    }

    /// Validates closure readiness. A workspace is closable when every plan is
    /// terminal (`ACCEPTED` or `REJECTED`-with-compensation). Read-only.
    ///
    /// # Errors
    /// Returns [`MineError::GraphInvalid`] when unresolved plans remain.
    pub fn close(&self, store: &TomlStore) -> MineResult<WorkspaceStatusOutcome> {
        let status = self.status(store)?;
        if status.has_unresolved {
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "workspace {} cannot close: unresolved plans remain (revision {})",
                    status.workspace_id, status.revision
                ),
            });
        }
        Ok(status)
    }

    fn build_initial_workspace(
        &self,
        workspace_id: String,
        stable_baseline_commit: &str,
        stable_branch: &str,
        integration_branch: &str,
    ) -> PlanWorkspace {
        PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 1,
            project_id: "mine-is-not-everyones".to_string(),
            workspace_id,
            stable_branch: stable_branch.to_string(),
            integration_branch: integration_branch.to_string(),
            stable_baseline_commit: stable_baseline_commit.to_string(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::infrastructure::system::{SystemClock, SystemUuidSource};

    fn seed_empty_graph(repo_root: &Path) {
        let store = TomlStore::new(repo_root);
        std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
        // Write a minimal graph at revision 0 with no plans.
        let ws = PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 0,
            project_id: "p".to_string(),
            workspace_id: String::new(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: String::new(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![],
        };
        std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();
    }

    #[test]
    fn open_initializes_workspace_with_generated_id_and_increments_revision() {
        let root = tempfile::tempdir().unwrap();
        let store = TomlStore::new(root.path());
        let ws = PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 0,
            project_id: "p".to_string(),
            workspace_id: String::new(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: String::new(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![],
        };
        std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
        std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();

        let svc = WorkspaceService::new(&SystemUuidSource, &SystemClock);
        let outcome = svc.open(&store, "deadbeef", "master", "dev").unwrap();
        assert!(!outcome.workspace_id.is_empty());
        assert_eq!(outcome.revision_before, 0);
        assert_eq!(outcome.revision_after, 1);
        assert_eq!(outcome.stable_baseline_commit, "deadbeef");
        // Identity is distinct from a release version: it is a UUID-shaped id.
        assert_ne!(outcome.workspace_id, "0.1.0");
    }

    #[test]
    fn open_is_idempotent_on_existing_workspace() {
        let root = tempfile::tempdir().unwrap();
        let store = TomlStore::new(root.path());
        seed_empty_graph(root.path());
        let svc = WorkspaceService::new(&SystemUuidSource, &SystemClock);
        let first = svc.open(&store, "deadbeef", "master", "dev").unwrap();
        let second = svc.open(&store, "deadbeef", "master", "dev").unwrap();
        assert_eq!(first.workspace_id, second.workspace_id);
        assert_eq!(first.revision_after, second.revision_after);
        assert_eq!(second.revision_before, second.revision_after);
    }
}
