//! Application service for plan lifecycle transitions (`add`/`show`/`start`/
//! `implemented`/`accept`/`reject`).
//!
//! Per `docs/design/system/component-architecture.md`, the CLI and MCP
//! adapters call the **same** application services. This module owns the
//! state-machine transition checks, predecessor validation, successor
//! release, and evidence recording. It uses [`GraphService::mutate`] for the
//! lock→reload→recheck-revision→atomic-write→render transaction; it does
//! **not** reimplement persistence, locking, or revision handling.
//!
//! State-machine rules come from `docs/design/execution-graph/
//! state-machine-and-algorithms.md` and the domain
//! `PlanStatus::validate_transition` / `validation::hard_predecessors_accepted`.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::application::graph_service::{
    GraphService, PlanAcceptRequest, PlanAddRequest, PlanImplementedRequest, PlanRejectRequest,
    PlanStartRequest,
};
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{PlanNode, PlanWorkspace};
use crate::domain::plan_id::{
    CanonicalPlanId, registered_plan_branch, validate_compensation_replacement, validate_plan_path,
};
use crate::domain::status::PlanStatus;
use crate::domain::validation;
use crate::infrastructure::git::{self, WorktreeInfo};

fn refusal(detail: impl Into<String>) -> MineError {
    MineError::GraphInvalid {
        detail: detail.into(),
    }
}

fn same_path(observed: &str, target: &Path) -> bool {
    PathBuf::from(observed)
        .canonicalize()
        .is_ok_and(|path| path == target)
}

fn resolve_with_missing_tail(path: &Path) -> MineResult<PathBuf> {
    let mut cursor = path;
    let mut missing = Vec::<OsString>::new();
    loop {
        match cursor.symlink_metadata() {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = cursor
                    .file_name()
                    .ok_or_else(|| refusal("registered worktree path has no existing ancestor"))?;
                missing.push(name.to_os_string());
                cursor = cursor
                    .parent()
                    .ok_or_else(|| refusal("registered worktree path has no existing ancestor"))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut resolved = cursor.canonicalize()?;
    if !resolved.is_dir() {
        return Err(refusal(
            "registered worktree path has a non-directory ancestor",
        ));
    }
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn contained_target(root: &Path, registered: &Path) -> MineResult<PathBuf> {
    let root = root.canonicalize()?;
    let target = resolve_with_missing_tail(registered)?;
    if !target.starts_with(&root) {
        return Err(refusal("registered worktree path escapes Root"));
    }
    Ok(target)
}

fn initial_branch_anchor(
    workspace: &PlanWorkspace,
    node: &PlanNode,
    parsed: &CanonicalPlanId,
    root: &Path,
) -> MineResult<String> {
    let candidate = if parsed.is_compensation() {
        let predecessor = workspace
            .plans
            .iter()
            .find(|candidate| {
                candidate.compensating_plan == node.id
                    && candidate.status == PlanStatus::Rejected
                    && candidate.lineage_id == node.lineage_id
            })
            .ok_or_else(|| refusal("compensation is not declared by a rejected predecessor"))?;
        validate_compensation_replacement(&predecessor.id, &node.id)?;
        if node.hard_predecessors != predecessor.hard_predecessors {
            return Err(refusal("compensation must inherit hard predecessors"));
        }
        git::head_of_branch(root, &predecessor.branch)?
    } else {
        let head = git::head_commit(root).ok_or_else(|| refusal("Root HEAD is unavailable"))?;
        git::resolve_commit(root, &head)?
    };
    git::resolve_commit(root, &candidate)
}

fn validate_registered_worktree(
    node: &PlanNode,
    target: &Path,
    inventory: &[WorktreeInfo],
    root: &Path,
) -> MineResult<String> {
    let observed = inventory
        .iter()
        .find(|worktree| same_path(&worktree.path, target))
        .ok_or_else(|| refusal("registered Plan worktree is not attached"))?;
    if observed.locked || observed.prunable || observed.detached || observed.bare {
        return Err(refusal("registered Plan worktree is not usable"));
    }
    if observed.branch.as_deref() != Some(node.branch.as_str()) {
        return Err(refusal("registered Plan worktree is on the wrong branch"));
    }
    let tip = git::head_of_branch(root, &node.branch)?;
    if observed.head != tip {
        return Err(refusal(
            "observed Plan worktree HEAD does not match registered branch tip",
        ));
    }
    Ok(tip)
}

fn validate_review_report(root: &Path, node: &PlanNode, requested: &str) -> MineResult<String> {
    let requested = crate::domain::path::normalize_repo_relative(requested)?;
    let stem = Path::new(&node.path)
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| refusal("registered Plan path has no UTF-8 file stem"))?;
    let expected = format!("docs/plan/reports/{stem}-review.md");
    if requested != expected {
        return Err(refusal(format!(
            "rejection review report must be the canonical path {expected}"
        )));
    }
    if requested == node.implementation_report {
        return Err(refusal(
            "rejection review report must not be the implementation report",
        ));
    }
    let report_path = root.join(&requested);
    let metadata = report_path.symlink_metadata().map_err(|_| {
        refusal(format!(
            "rejection review report does not exist at {expected}"
        ))
    })?;
    if !metadata.file_type().is_file() {
        return Err(refusal("rejection review report must be a regular file"));
    }
    let canonical_root = root.canonicalize()?;
    let canonical_report = report_path.canonicalize()?;
    if !canonical_report.starts_with(&canonical_root) {
        return Err(refusal("rejection review report escapes repository Root"));
    }
    Ok(expected)
}

/// Shared plan lifecycle service. Constructed over a [`GraphService`] and used
/// by both the CLI `plan.*` handlers and the MCP `mine_plan_*` tools.
pub struct PlanService<'a> {
    graph: &'a GraphService<'a>,
}

/// Runtime-enriched Plan lookup DTO. Persisted fields remain flattened for
/// compatibility; absolute/caller/observed locations are read-only evidence.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanLookup {
    #[serde(flatten)]
    pub node: PlanNode,
    pub registered_worktree_path: String,
    pub resolved_worktree_path: String,
    pub observed_worktree: Option<WorktreeInfo>,
    pub caller_location: String,
    pub authoritative_root: String,
    pub authoritative_revision: u64,
}

/// Result of deterministic linked-worktree preparation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanPreparation {
    #[serde(flatten)]
    pub lookup: PlanLookup,
    pub action: String,
}

impl<'a> PlanService<'a> {
    #[must_use]
    pub fn new(graph: &'a GraphService<'a>) -> Self {
        Self { graph }
    }

    /// Looks up a plan by id. Read-only.
    pub fn show(&self, id: &str) -> MineResult<(u64, PlanNode)> {
        let ws = self.graph.validate()?;
        let node = ws.get(id).cloned().ok_or_else(|| MineError::PlanNotFound {
            plan_id: id.to_string(),
        })?;
        Ok((ws.revision, node))
    }

    /// Looks up a Plan and enriches it with authoritative Root and linked
    /// worktree evidence gathered from Git's stable porcelain inventory.
    pub fn show_with_locations(
        &self,
        id: &str,
        caller_root: &std::path::Path,
    ) -> MineResult<PlanLookup> {
        let ws = self.graph.validate()?;
        let node = ws.get(id).cloned().ok_or_else(|| MineError::PlanNotFound {
            plan_id: id.to_string(),
        })?;
        let lineage_branches: HashSet<&str> = ws
            .plans
            .iter()
            .filter(|candidate| candidate.lineage_id == node.lineage_id)
            .map(|candidate| candidate.branch.as_str())
            .collect();
        let observed_worktree = git::list_worktrees(self.graph.repo_root())
            .unwrap_or_default()
            .into_iter()
            .skip(1)
            .find(|worktree| {
                worktree
                    .branch
                    .as_deref()
                    .is_some_and(|branch| lineage_branches.contains(branch))
            });
        let resolved_worktree_path = observed_worktree.as_ref().map_or_else(
            || {
                self.graph
                    .repo_root()
                    .join(&node.worktree_path)
                    .display()
                    .to_string()
            },
            |worktree| worktree.path.clone(),
        );
        Ok(PlanLookup {
            registered_worktree_path: node.worktree_path.clone(),
            resolved_worktree_path,
            observed_worktree,
            caller_location: caller_root.display().to_string(),
            authoritative_root: self.graph.repo_root().display().to_string(),
            authoritative_revision: ws.revision,
            node,
        })
    }

    /// Creates, switches, or exactly reuses the registered lineage worktree.
    pub fn prepare(&self, id: &str, caller_root: &Path) -> MineResult<PlanPreparation> {
        let ws = self.graph.validate()?;
        let node = ws.get(id).cloned().ok_or_else(|| MineError::PlanNotFound {
            plan_id: id.to_string(),
        })?;
        let parsed = CanonicalPlanId::parse(id)?;
        if !matches!(
            node.status,
            PlanStatus::Ready | PlanStatus::InProgress | PlanStatus::Implemented
        ) {
            return Err(MineError::InvalidTransition {
                plan_id: id.to_string(),
                from: node.status.as_str().to_string(),
                to: "PREPARED".to_string(),
            });
        }
        let root = self.graph.repo_root();
        if git::current_branch(root)?.as_deref() != Some(ws.integration_branch.as_str()) {
            return Err(refusal(format!(
                "Root must remain on integration branch {}",
                ws.integration_branch
            )));
        }
        if !git::is_clean(root) {
            return Err(refusal("Root integration worktree is dirty"));
        }
        let registered = root.join(&node.worktree_path);
        let target = contained_target(root, &registered)?;
        let inventory = git::list_worktrees(root)?;
        if inventory.iter().any(|w| w.locked || w.prunable) {
            return Err(refusal(
                "locked or prunable worktree registration requires manual recovery",
            ));
        }
        let at_path = inventory.iter().find(|w| same_path(&w.path, &target));
        let branch_owner = inventory
            .iter()
            .find(|w| w.branch.as_deref() == Some(node.branch.as_str()));
        let initial_head = initial_branch_anchor(&ws, &node, &parsed, root)?;
        let branch_tip = git::branch_exists(root, &node.branch)
            .then(|| git::head_of_branch(root, &node.branch))
            .transpose()?;
        match node.status {
            PlanStatus::Ready => {
                if branch_tip.as_deref().is_some_and(|tip| tip != initial_head) {
                    return Err(refusal(format!(
                        "registered branch {} is not at required HEAD {initial_head}",
                        node.branch
                    )));
                }
            }
            PlanStatus::InProgress | PlanStatus::Implemented => {
                let tip = branch_tip
                    .as_deref()
                    .ok_or_else(|| refusal("active Plan branch is missing"))?;
                let anchor = git::resolve_commit(root, &node.branch_anchor)
                    .map_err(|_| refusal("stored branch_anchor is not a valid commit"))?;
                if anchor != node.branch_anchor {
                    return Err(refusal(
                        "stored branch_anchor is not a full commit object ID",
                    ));
                }
                if !git::is_ancestor_checked(root, &anchor, tip)? {
                    return Err(refusal(
                        "stored branch_anchor is not an ancestor of the Plan branch tip",
                    ));
                }
            }
            _ => unreachable!("status gate above restricts preparation states"),
        }
        let action = if let Some(observed) = at_path {
            if observed.branch.as_deref() == Some(node.branch.as_str()) {
                let tip = branch_tip
                    .as_deref()
                    .ok_or_else(|| refusal("registered Plan branch is missing"))?;
                if observed.head != tip {
                    return Err(refusal(
                        "observed Plan worktree HEAD does not match registered branch tip",
                    ));
                }
                "reused"
            } else {
                if node.status != PlanStatus::Ready || !parsed.is_compensation() {
                    return Err(refusal("registered lineage path is on the wrong branch"));
                }
                let worktree = Path::new(&observed.path);
                if !git::is_clean(worktree) {
                    return Err(refusal("compensation lineage worktree is dirty"));
                }
                if branch_owner.is_some_and(|owner| owner.path != observed.path) {
                    return Err(refusal("registered branch is checked out elsewhere"));
                }
                if !git::branch_exists(root, &node.branch) {
                    git::create_branch(root, &node.branch, &initial_head)?;
                }
                git::switch_branch(worktree, &node.branch)?;
                "switched"
            }
        } else {
            if node.status != PlanStatus::Ready {
                return Err(refusal("active Plan worktree is missing"));
            }
            if branch_owner.is_some() {
                return Err(refusal("registered branch is checked out elsewhere"));
            }
            if target.exists() {
                return Err(refusal(
                    "registered worktree directory is occupied but unregistered",
                ));
            }
            if parsed.is_compensation() {
                return Err(refusal(
                    "compensation requires its existing lineage worktree",
                ));
            }
            if !git::branch_exists(root, &node.branch) {
                git::create_branch(root, &node.branch, &initial_head)?;
            }
            let revalidated_target = contained_target(root, &registered)?;
            if revalidated_target != target {
                return Err(refusal(
                    "registered worktree path changed during preparation",
                ));
            }
            if let Err(add_error) = git::add_worktree(root, &revalidated_target, &node.branch) {
                // Git versions differ in whether a failed `worktree add`
                // preserves an existing branch. Keep the failure evidence
                // deterministic and recoverable on every platform.
                if !git::branch_exists(root, &node.branch) {
                    git::create_branch(root, &node.branch, &initial_head).map_err(
                        |recovery_error| {
                            refusal(format!(
                                "worktree creation failed ({add_error}); branch recovery failed ({recovery_error})"
                            ))
                        },
                    )?;
                }
                return Err(add_error);
            }
            "created"
        };
        let lookup = self.show_with_locations(id, caller_root)?;
        let observed = lookup
            .observed_worktree
            .as_ref()
            .ok_or_else(|| refusal("prepared worktree was not observed after mutation"))?;
        if observed.branch.as_deref() != Some(node.branch.as_str()) {
            return Err(refusal(
                "prepared worktree branch does not match registration",
            ));
        }
        let prepared_tip = git::head_of_branch(root, &node.branch)?;
        if observed.head != prepared_tip {
            return Err(refusal(
                "prepared worktree HEAD does not match registered branch tip",
            ));
        }
        Ok(PlanPreparation {
            lookup,
            action: action.to_string(),
        })
    }

    /// Adds a new plan node (status `DRAFT`).
    pub fn add(&self, req: PlanAddRequest) -> MineResult<PlanWorkspace> {
        let expected = self.graph.validate()?.revision;
        let worktrees_root = self.graph.worktrees_root()?;
        self.graph.mutate(expected, move |mut w| {
            if w.get(&req.id).is_some() {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan id {} already exists", req.id),
                });
            }
            let (branch, lineage_id, worktree_path) =
                if w.workspace_id == "bootstrap-mine-v1" && !req.id.starts_with("plan") {
                    crate::domain::path::normalize_repo_relative(&req.path)?;
                    (String::new(), String::new(), String::new())
                } else {
                    let parsed = CanonicalPlanId::parse(&req.id)?;
                    let _slug = validate_plan_path(&req.id, &req.path)?;
                    (
                        registered_plan_branch(&w.workspace_id, &req.id, &req.path)?,
                        parsed.lineage_id(),
                        parsed.worktree_path_under(&worktrees_root),
                    )
                };
            if req.design_references.is_empty() {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} has no design references", req.id),
                });
            }
            for dr in &req.design_references {
                if dr.is_empty() {
                    return Err(MineError::GraphInvalid {
                        detail: "design-reference must not be empty".to_string(),
                    });
                }
                crate::domain::path::normalize_repo_relative(dr)?;
            }
            for hp in &req.hard_predecessors {
                if !w.ids().contains(hp.as_str()) {
                    return Err(MineError::GraphInvalid {
                        detail: format!("plan {} hard_predecessor {} not found", req.id, hp),
                    });
                }
            }
            let node = PlanNode {
                id: req.id,
                path: req.path,
                title: req.title,
                branch,
                branch_anchor: String::new(),
                status: PlanStatus::Draft,
                hard_predecessors: req.hard_predecessors,
                soft_predecessors: vec![],
                design_references: req.design_references,
                exclusive_write_paths: req.exclusive_write_paths,
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
                lineage_id,
                worktree_path,
            };
            w.plans.push(node);
            w.revision = expected + 1;
            Ok(w)
        })
    }

    /// Starts a plan: requires `READY` + hard predecessors `ACCEPTED`.
    pub fn start(&self, req: PlanStartRequest) -> MineResult<PlanWorkspace> {
        let expected = self.graph.validate()?.revision;
        let root = self.graph.repo_root().to_path_buf();
        self.graph.mutate(expected, move |mut w| {
            let start_node = w
                .get(&req.id)
                .cloned()
                .ok_or_else(|| MineError::PlanNotFound {
                    plan_id: req.id.clone(),
                })?;
            let current = start_node.status;
            if current != PlanStatus::Ready {
                return Err(MineError::InvalidTransition {
                    plan_id: req.id.clone(),
                    from: current.as_str().to_string(),
                    to: PlanStatus::InProgress.as_str().to_string(),
                });
            }
            if !validation::hard_predecessors_accepted(&w, &req.id)? {
                let preds = w
                    .get(&req.id)
                    .map(|n| n.hard_predecessors.clone())
                    .unwrap_or_default();
                let unaccepted = preds
                    .into_iter()
                    .find(|p| w.get(p).is_some_and(|n| n.status != PlanStatus::Accepted))
                    .unwrap_or_default();
                return Err(MineError::PredecessorNotAccepted {
                    plan_id: req.id.clone(),
                    predecessor_id: unaccepted,
                    predecessor_status: "not accepted".to_string(),
                });
            }
            if git::current_branch(&root)?.as_deref() != Some(w.integration_branch.as_str()) {
                return Err(refusal(format!(
                    "Root must remain on integration branch {}",
                    w.integration_branch
                )));
            }
            if !git::is_clean(&root) {
                return Err(refusal("Root integration worktree is dirty"));
            }
            let parsed = CanonicalPlanId::parse(&req.id)?;
            let expected_anchor = initial_branch_anchor(&w, &start_node, &parsed, &root)?;
            let target = contained_target(&root, &root.join(&start_node.worktree_path))?;
            let inventory = git::list_worktrees(&root)?;
            let observed_tip =
                validate_registered_worktree(&start_node, &target, &inventory, &root)?;
            if observed_tip != expected_anchor {
                return Err(refusal(format!(
                    "prepared Plan branch is not at required start anchor {expected_anchor}"
                )));
            }
            let node = w.get_mut(&req.id).expect("checked present above");
            node.status
                .validate_transition(&req.id, PlanStatus::InProgress)?;
            node.status = PlanStatus::InProgress;
            node.branch_anchor = expected_anchor;
            node.owner = req.owner;
            node.run_id = req.run_id;
            node.started_at = req.started_at.clone();
            node.updated_at = req.started_at;
            w.revision = expected + 1;
            Ok(w)
        })
    }

    /// Marks a plan implemented: records the report + commits.
    pub fn mark_implemented(&self, req: PlanImplementedRequest) -> MineResult<PlanWorkspace> {
        let expected = self.graph.validate()?.revision;
        self.graph.mutate(expected, move |mut w| {
            let node = w.get_mut(&req.id).ok_or_else(|| MineError::PlanNotFound {
                plan_id: req.id.clone(),
            })?;
            node.status
                .validate_transition(&req.id, PlanStatus::Implemented)?;
            node.status = PlanStatus::Implemented;
            node.implementation_report = req.report;
            node.implementation_commits = req.commits;
            node.updated_at = req.updated_at;
            w.revision = expected + 1;
            Ok(w)
        })
    }

    /// Accepts a plan: requires `IMPLEMENTED`; releases eligible BLOCKED
    /// successors whose hard predecessors are all now accepted.
    pub fn accept(&self, req: PlanAcceptRequest) -> MineResult<PlanWorkspace> {
        let expected = self.graph.validate()?.revision;
        self.graph.mutate(expected, move |mut w| {
            let current =
                w.get(&req.id)
                    .map(|n| n.status)
                    .ok_or_else(|| MineError::PlanNotFound {
                        plan_id: req.id.clone(),
                    })?;
            if current != PlanStatus::Implemented {
                return Err(MineError::InvalidTransition {
                    plan_id: req.id.clone(),
                    from: current.as_str().to_string(),
                    to: PlanStatus::Accepted.as_str().to_string(),
                });
            }
            PlanStatus::Implemented.validate_transition(&req.id, PlanStatus::Accepted)?;
            // Accept the target first, then release newly-ready successors.
            let accepted_ancestors: HashSet<String> = w
                .plans
                .iter()
                .filter(|p| p.status == PlanStatus::Accepted)
                .map(|p| p.id.clone())
                .collect();
            for p in w.plans.iter_mut() {
                if p.status == PlanStatus::Blocked
                    && !p.hard_predecessors.is_empty()
                    && p.hard_predecessors
                        .iter()
                        .all(|hp| accepted_ancestors.contains(hp) || hp == &req.id)
                {
                    p.status = PlanStatus::Ready;
                    p.updated_at = req.updated_at.clone();
                }
            }
            let node = w.get_mut(&req.id).expect("checked present above");
            node.status = PlanStatus::Accepted;
            node.review_report = req.review_report;
            node.updated_at = req.updated_at;
            w.revision = expected + 1;
            Ok(w)
        })
    }

    /// Rejects a plan: records reason + compensating plan; downstream stays
    /// blocked.
    pub fn reject(&self, req: PlanRejectRequest) -> MineResult<PlanWorkspace> {
        let expected = self.graph.validate()?.revision;
        let root = self.graph.repo_root().to_path_buf();
        self.graph.mutate(expected, move |mut w| {
            let current = w
                .get(&req.id)
                .cloned()
                .ok_or_else(|| MineError::PlanNotFound {
                    plan_id: req.id.clone(),
                })?;
            if !matches!(
                current.status,
                PlanStatus::InProgress | PlanStatus::Implemented
            ) {
                return Err(MineError::InvalidTransition {
                    plan_id: req.id.clone(),
                    from: current.status.as_str().to_string(),
                    to: PlanStatus::Rejected.as_str().to_string(),
                });
            }
            current
                .status
                .validate_transition(&req.id, PlanStatus::Rejected)?;
            if req.reason.trim().is_empty() {
                return Err(MineError::GraphInvalid {
                    detail: "rejection requires a non-empty reason".to_string(),
                });
            }
            if req.compensating_plan.is_empty() {
                return Err(MineError::GraphInvalid {
                    detail: "rejection requires a compensating plan".to_string(),
                });
            }
            if req.id.starts_with("plan") {
                validate_compensation_replacement(&req.id, &req.compensating_plan)?;
            }
            let review_report = validate_review_report(&root, &current, &req.review_report)?;
            let node = w.get_mut(&req.id).expect("checked present above");
            node.status = PlanStatus::Rejected;
            node.rejection_reason = req.reason;
            node.review_report = review_report;
            node.compensating_plan = req.compensating_plan;
            node.updated_at = req.updated_at;
            w.revision = expected + 1;
            Ok(w)
        })
    }
}

/// A request to release a DRAFT plan to the startable frontier (shared by CLI
/// and MCP).
#[derive(Debug, Clone)]
pub struct PlanReleaseRequest {
    pub id: String,
    pub updated_at: String,
}

/// A request to rewire downstream successors off a REJECTED plan onto its
/// registered compensating plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanRewireRequest {
    pub id: String,
    pub updated_at: String,
}

impl<'a> PlanService<'a> {
    /// Releases a `DRAFT` plan: `DRAFT -> READY` when every hard predecessor is
    /// `ACCEPTED` (incl. no preds), else `DRAFT -> BLOCKED`. Returns the saved
    /// workspace and the unsatisfied predecessors (in stable order) for
    /// deterministic reporting.
    pub fn release(&self, req: PlanReleaseRequest) -> MineResult<PlanWorkspace> {
        let expected = self.graph.validate()?.revision;
        self.graph.mutate(expected, move |mut w| {
            crate::domain::plan_release::release_plan(&mut w, &req.id, &req.updated_at)?;
            w.revision = expected + 1;
            Ok(w)
        })
    }

    /// Rewires a REJECTED plan's downstream successors onto its registered
    /// compensating plan. The replacement id is derived from the rejected plan's
    /// `compensating_plan` field (the single source of truth). Returns the saved
    /// workspace and the affected successor ids (in stable insertion order); on
    /// the idempotent no-op (no successor still references the rejected id), the
    /// revision is NOT bumped and the affected list is empty.
    pub fn rewire_compensation(
        &self,
        req: PlanRewireRequest,
    ) -> MineResult<(PlanWorkspace, Vec<String>)> {
        let expected = self.graph.validate()?.revision;
        let affected_cell = std::cell::RefCell::new(Vec::<String>::new());
        let saved = self.graph.mutate(expected, |mut w| {
            let affected =
                crate::domain::rewire::rewire_compensation(&mut w, &req.id, &req.updated_at)?;
            if !affected.is_empty() {
                w.revision = expected + 1;
            }
            *affected_cell.borrow_mut() = affected;
            Ok(w)
        })?;
        Ok((saved, affected_cell.into_inner()))
    }
}
