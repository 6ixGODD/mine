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

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::domain::config::MineConfig;
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{
    ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION, CURRENT_GRAPH_SCHEMA_VERSION, LEGACY_GRAPH_SCHEMA_VERSION,
    PREVIOUS_GRAPH_SCHEMA_VERSION, PlanWorkspace,
};
use crate::domain::path::normalize_repo_relative;
use crate::domain::plan_id::{CanonicalPlanId, is_exact_bootstrap_node, registered_plan_branch};
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

/// Persistence adapter for the execution-graph TOML and generated Markdown.
pub struct TomlStore {
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
        let toml_path = repo_root
            .join("docs")
            .join("plan")
            .join("execution-graph.toml");
        let md_path = repo_root
            .join("docs")
            .join("plan")
            .join("execution-graph.md");
        let lock_path = repo_root.join(".mine").join("locks").join(GRAPH_LOCK_NAME);
        Self {
            repo_root: repo_root.to_path_buf(),
            toml_path,
            md_path,
            lock_path,
            lock_timeout: Duration::from_millis(DEFAULT_LOCK_TIMEOUT_MS),
        }
    }

    /// Returns a store with a custom lock timeout (mainly for tests).
    #[must_use]
    pub fn with_lock_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
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

    /// Loads the workspace from the TOML fact source.
    ///
    /// # Errors
    /// - [`MineError::GraphNotInitialized`] if the file is absent.
    /// - [`MineError::GraphInvalid`] if the TOML is unparseable or fails
    ///   structural validation.
    pub fn load(&self) -> MineResult<PlanWorkspace> {
        if !self.toml_path.exists() {
            return Err(MineError::GraphNotInitialized {
                path: self.toml_path.clone(),
            });
        }
        let content = std::fs::read_to_string(&self.toml_path)?;
        let ws = parse_workspace(&content)?;
        validate_persisted_workspace(&ws, &self.worktrees_root()?, &self.repo_root)?;
        Ok(ws)
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
    /// - Partial success ([`MineError::GraphInvalid`] with a render-repair hint)
    ///   if the TOML writes but Markdown rendering fails.
    pub fn save_with_revision(
        &self,
        expected_revision: u64,
        mutate: impl FnOnce(PlanWorkspace) -> MineResult<PlanWorkspace>,
    ) -> MineResult<PlanWorkspace> {
        let _lock = file_lock::acquire_exclusive(&self.lock_path, self.lock_timeout)?;

        // Reload under the lock.
        let reloaded = if self.toml_path.exists() {
            let content = std::fs::read_to_string(&self.toml_path)?;
            parse_workspace(&content)?
        } else {
            return Err(MineError::GraphNotInitialized {
                path: self.toml_path.clone(),
            });
        };

        let worktrees_root = self.worktrees_root()?;
        validate_persisted_workspace(&reloaded, &worktrees_root, &self.repo_root)?;

        if reloaded.revision != expected_revision {
            return Err(MineError::RevisionConflict {
                expected: expected_revision,
                actual: reloaded.revision,
            });
        }

        let mut new_ws = mutate(reloaded)?;
        // The writer always emits the current schema: a v3 graph upgrades
        // implicitly here (the v4 fields are optional with defaults), so the
        // schema bump rides the mutation's own revision increment.
        new_ws.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
        validate_persisted_workspace(&new_ws, &worktrees_root, &self.repo_root)?;

        let toml_content = toml::to_string(&new_ws).map_err(|e| MineError::GraphInvalid {
            detail: format!("could not serialize execution-graph TOML: {e}"),
        })?;
        atomic_write::write(&self.toml_path, toml_content.as_bytes())?;

        // Render the Markdown view from the committed TOML. If this fails, the
        // TOML is still the fact source.
        let reloaded_for_render = self.load()?;
        match render_markdown(&reloaded_for_render) {
            Ok(md) => {
                atomic_write::write(&self.md_path, md.as_bytes())?;
            }
            Err(e) => {
                return Err(MineError::GraphInvalid {
                    detail: format!(
                        "TOML written but Markdown render failed (run `mine graph render` to repair): {e}"
                    ),
                });
            }
        }

        Ok(reloaded_for_render)
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
    ) -> MineResult<ProvenanceRepairOutcome> {
        let _lock = file_lock::acquire_exclusive(&self.lock_path, self.lock_timeout)?;
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
        let worktrees_root = self.worktrees_root()?;
        if workspace.schema_version == CURRENT_GRAPH_SCHEMA_VERSION {
            validate_persisted_workspace(&workspace, &worktrees_root, &self.repo_root)?;
            return Ok(ProvenanceRepairOutcome {
                revision_before: workspace.revision,
                revision_after: workspace.revision,
                workspace,
                changed: false,
                repaired_plans: Vec::new(),
            });
        }
        let revision_before = workspace.revision;
        let repaired_plans = if workspace.schema_version == PREVIOUS_GRAPH_SCHEMA_VERSION {
            // Version-only upgrade: the v4 fields are optional with defaults,
            // so nothing is derived.
            validate_persisted_workspace(&workspace, &worktrees_root, &self.repo_root)?;
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
        workspace.revision = revision_before + 1;
        validate_persisted_workspace(&workspace, &worktrees_root, &self.repo_root)?;
        self.persist_and_render(&workspace)?;

        Ok(ProvenanceRepairOutcome {
            revision_before,
            revision_after: workspace.revision,
            workspace,
            changed: true,
            repaired_plans,
        })
    }

    /// Re-renders the Markdown view from the current TOML without mutating
    /// the TOML. Used by `mine graph render` repair.
    ///
    /// # Errors
    /// Returns [`MineError::GraphInvalid`] if the TOML cannot be loaded or
    /// rendered.
    pub fn render(&self) -> MineResult<()> {
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

    /// Returns the configured repository-relative linked-worktree root.
    pub fn worktrees_root(&self) -> MineResult<String> {
        let path = self.repo_root.join(".mine").join("config.toml");
        if !path.exists() {
            return Ok(".mine/worktrees".to_string());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(MineConfig::parse(&path, &content)?.plan.worktrees_root)
    }

    fn persist_and_render(&self, workspace: &PlanWorkspace) -> MineResult<()> {
        let toml_content = toml::to_string(workspace).map_err(|e| MineError::GraphInvalid {
            detail: format!("could not serialize execution-graph TOML: {e}"),
        })?;
        atomic_write::write(&self.toml_path, toml_content.as_bytes())?;
        let md = render_markdown(workspace)?;
        atomic_write::write(&self.md_path, md.as_bytes())?;
        Ok(())
    }
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
    validation::validate_with_worktrees_root(workspace, worktrees_root)?;
    for node in &workspace.plans {
        if node.branch_anchor.is_empty() {
            continue;
        }
        let resolved = git::resolve_commit(repo_root, &node.branch_anchor).map_err(|_| {
            MineError::GraphInvalid {
                detail: format!(
                    "plan {} branch_anchor does not resolve to a commit in the repository",
                    node.id
                ),
            }
        })?;
        if resolved != node.branch_anchor {
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "plan {} branch_anchor is not the exact full commit object ID",
                    node.id
                ),
            });
        }
    }
    Ok(())
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
            workspace_id: "bootstrap-mine-v1".to_string(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: "abc".to_string(),
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
        let ws = sample_workspace(0);
        std::fs::create_dir_all(store.toml_path.parent().unwrap()).unwrap();
        std::fs::write(&store.toml_path, toml::to_string(&ws).unwrap()).unwrap();

        let result = store
            .save_with_revision(0, |mut w| {
                w.revision = 1;
                w.plans[0].status = PlanStatus::InProgress;
                w.plans[0].owner = "tester".to_string();
                Ok(w)
            })
            .unwrap();
        assert_eq!(result.revision, 1);
        assert_eq!(result.plans[0].status, PlanStatus::InProgress);

        // Markdown rendered with the new revision.
        let md = std::fs::read_to_string(store.md_path).unwrap();
        assert!(md.contains("Revision: `1`"));
        assert!(md.contains("IN_PROGRESS"));
    }

    #[test]
    fn save_with_revision_conflict_detected() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlStore::new(dir.path());
        let ws = sample_workspace(5);
        std::fs::create_dir_all(store.toml_path.parent().unwrap()).unwrap();
        std::fs::write(&store.toml_path, toml::to_string(&ws).unwrap()).unwrap();

        let err = store
            .save_with_revision(4, |w| {
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
