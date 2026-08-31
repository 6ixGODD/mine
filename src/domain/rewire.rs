//! Compensation rewiring: replace a rejected plan's id with its registered
//! compensating plan's id in every still-mutable downstream successor.
//!
//! Implements `docs/design/execution-graph/state-machine-and-algorithms.md#compensation-rewiring`.
//! The replacement id is **derived** from the rejected plan's `compensating_plan`
//! field (the single source of truth), never supplied by the caller, so
//! dependency substitution can never be triggered by a similar-looking id.
//!
//! The operation is pure: it mutates the supplied [`PlanWorkspace`] in place and
//! does not touch the filesystem or bump the revision (the caller's
//! persistence transaction does those). It never touches the rejected plan
//! node and never weakens the immutability of active/accepted/terminal
//! successors: a successor that references the rejected id in
//! `hard_predecessors`/`soft_predecessors` is rewired only when its status is
//! `DRAFT`/`BLOCKED`/`READY`; any other status fails with
//! [`MineError::RewireSuccessorLocked`] and leaves the workspace unmutated.

use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::PlanWorkspace;
use crate::domain::status::PlanStatus;
use crate::domain::validation;

/// Returns `true` iff the status is a still-mutable frontier status: the only
/// successors whose predecessor entries may be rewired.
fn is_mutable(status: PlanStatus) -> bool {
    matches!(
        status,
        PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready
    )
}

/// Rewires downstream dependencies from `rejected_id` onto its registered
/// compensating plan.
///
/// For every successor whose `hard_predecessors` or `soft_predecessors` list
/// contains `rejected_id` exactly:
/// - if the successor is mutable (`DRAFT`/`BLOCKED`/`READY`), each exact
///   occurrence of `rejected_id` is replaced in place by the compensating id
///   (order preserved) and `updated_at` is refreshed;
/// - otherwise the operation fails with [`MineError::RewireSuccessorLocked`]
///   and the workspace is left unchanged.
///
/// Returns the affected successor ids in stable plan-insertion order. On any
/// error the workspace is left unmutated (validation precedes mutation, and
/// mutation is staged on a clone swapped back only after a post-rewire cycle
/// check passes).
///
/// # Errors
/// - [`MineError::PlanNotFound`] — `rejected_id` or its compensating plan
///   missing, or a referenced predecessor missing.
/// - [`MineError::InvalidTransition`] — `rejected_id` is not `REJECTED`.
/// - [`MineError::GraphInvalid`] — `compensating_plan` empty, or the
///   compensating plan is itself `REJECTED`.
/// - [`MineError::RewireSuccessorLocked`] — a referencing successor is in an
///   active/accepted/terminal status.
/// - [`MineError::GraphCycle`] — rewiring would create a hard-dependency cycle.
pub fn rewire_compensation(
    ws: &mut PlanWorkspace,
    rejected_id: &str,
    now: &str,
) -> MineResult<Vec<String>> {
    let rejected = ws.get(rejected_id).ok_or_else(|| MineError::PlanNotFound {
        plan_id: rejected_id.to_string(),
    })?;
    if rejected.status != PlanStatus::Rejected {
        return Err(MineError::InvalidTransition {
            plan_id: rejected_id.to_string(),
            from: rejected.status.as_str().to_string(),
            to: "REWIRED".to_string(),
        });
    }
    let comp = rejected.compensating_plan.clone();
    if comp.is_empty() {
        return Err(MineError::GraphInvalid {
            detail: format!("rejected plan {rejected_id} has no compensating_plan"),
        });
    }
    let comp_node = ws.get(&comp).ok_or_else(|| MineError::PlanNotFound {
        plan_id: comp.clone(),
    })?;
    if comp_node.status == PlanStatus::Rejected {
        return Err(MineError::GraphInvalid {
            detail: format!("compensating plan {comp} is itself REJECTED"),
        });
    }

    // Identify affected successors (in stable insertion order) and lock-check
    // them before mutating.
    let mut affected: Vec<String> = Vec::new();
    for p in &ws.plans {
        if p.id == rejected_id || p.id == comp {
            continue;
        }
        if references(p, rejected_id) {
            if !is_mutable(p.status) {
                return Err(MineError::RewireSuccessorLocked {
                    plan_id: rejected_id.to_string(),
                    successor_id: p.id.clone(),
                    successor_status: p.status.as_str().to_string(),
                });
            }
            affected.push(p.id.clone());
        }
    }

    if affected.is_empty() {
        // Idempotent no-op: nothing to rewire. Leave the workspace untouched.
        return Ok(Vec::new());
    }

    // Stage the mutation on a clone so a post-rewire cycle check can discard it.
    let mut staged = ws.clone();
    for id in &affected {
        let node = staged.get_mut(id).expect("affected ids present");
        replace_id(&mut node.hard_predecessors, rejected_id, &comp);
        replace_id(&mut node.soft_predecessors, rejected_id, &comp);
        node.updated_at = now.to_string();
    }

    // Post-rewire cycle check on the staged graph.
    validation::topological_sort(&staged)?;

    // Commit the staged mutation.
    *ws = staged;
    Ok(affected)
}

/// Returns `true` iff `node` references `id` in its hard or soft predecessors.
fn references(node: &crate::domain::graph::PlanNode, id: &str) -> bool {
    node.hard_predecessors.iter().any(|p| p == id) || node.soft_predecessors.iter().any(|p| p == id)
}

/// Replaces every exact occurrence of `from` with `to` in `list`, in place,
/// preserving the order of all other entries.
fn replace_id(list: &mut [String], from: &str, to: &str) {
    for entry in list.iter_mut() {
        if entry == from {
            *entry = to.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::graph::{PlanNode, PlanWorkspace};

    fn node(id: &str, status: PlanStatus, hard: &[&str], soft: &[&str]) -> PlanNode {
        PlanNode {
            id: id.to_string(),
            path: format!("docs/plan/{id}.md"),
            title: id.to_string(),
            branch: String::new(),
            branch_anchor: String::new(),
            status,
            hard_predecessors: hard.iter().map(|s| s.to_string()).collect(),
            soft_predecessors: soft.iter().map(|s| s.to_string()).collect(),
            design_references: vec!["docs/design/principles.md".to_string()],
            exclusive_write_paths: vec![format!("src/{id}/")],
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
        }
    }

    fn rejected_node(id: &str, comp: &str, hard: &[&str]) -> PlanNode {
        let mut n = node(id, PlanStatus::Rejected, hard, &[]);
        n.compensating_plan = comp.to_string();
        n
    }

    fn ws(plans: Vec<PlanNode>) -> PlanWorkspace {
        PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 5,
            project_id: "p".to_string(),
            workspace_id: "w".to_string(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: String::new(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans,
        }
    }

    #[test]
    fn rewires_blocked_successor_hard_predecessors() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            node("04", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            node("05-1", PlanStatus::Ready, &["03"], &[]),
            node("06", PlanStatus::Blocked, &["04", "05"], &[]),
        ]);
        let affected = rewire_compensation(&mut w, "05", "now").unwrap();
        assert_eq!(affected, vec!["06".to_string()]);
        assert_eq!(w.get("06").unwrap().hard_predecessors, vec!["04", "05-1"]);
        assert_eq!(w.get("05").unwrap().status, PlanStatus::Rejected); // unchanged
    }

    #[test]
    fn rewire_draft_and_ready_successors_in_insertion_order() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            node("04", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            node("05-1", PlanStatus::Ready, &["03"], &[]),
            node("07", PlanStatus::Draft, &["05"], &[]),
            node("08", PlanStatus::Ready, &["05"], &[]),
        ]);
        let affected = rewire_compensation(&mut w, "05", "now").unwrap();
        assert_eq!(affected, vec!["07".to_string(), "08".to_string()]);
        for id in ["07", "08"] {
            assert_eq!(
                w.get(id).unwrap().hard_predecessors,
                vec!["05-1".to_string()]
            );
        }
    }

    #[test]
    fn rewire_soft_predecessors() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            node("05-1", PlanStatus::Ready, &["03"], &[]),
            node("09", PlanStatus::Draft, &[], &["05"]),
        ]);
        let affected = rewire_compensation(&mut w, "05", "now").unwrap();
        assert_eq!(affected, vec!["09".to_string()]);
        assert_eq!(
            w.get("09").unwrap().soft_predecessors,
            vec!["05-1".to_string()]
        );
    }

    #[test]
    fn preserves_unrelated_predecessor_order_and_entries() {
        let mut w = ws(vec![
            node("01", PlanStatus::Accepted, &[], &[]),
            node("02", PlanStatus::Accepted, &[], &[]),
            node("03", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            node("05-1", PlanStatus::Ready, &["03"], &[]),
            node("06", PlanStatus::Blocked, &["01", "05", "02"], &[]),
        ]);
        rewire_compensation(&mut w, "05", "now").unwrap();
        assert_eq!(
            w.get("06").unwrap().hard_predecessors,
            vec!["01", "05-1", "02"]
        );
    }

    #[test]
    fn sibling_id_not_rewired() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            node("05-1", PlanStatus::Ready, &["03"], &[]),
            node("050", PlanStatus::Draft, &["050"], &[]),
        ]);
        // 050 references itself "050", not "05" -> unaffected.
        let affected = rewire_compensation(&mut w, "05", "now").unwrap();
        assert!(affected.is_empty(), "sibling id 050 must not be rewired");
        assert_eq!(
            w.get("050").unwrap().hard_predecessors,
            vec!["050".to_string()]
        );
    }

    #[test]
    fn not_rejected_original_errors() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            node("05", PlanStatus::Implemented, &["03"], &[]),
            node("05-1", PlanStatus::Ready, &["03"], &[]),
        ]);
        let err = rewire_compensation(&mut w, "05", "now").unwrap_err();
        assert_eq!(err.code(), "MINE_INVALID_TRANSITION");
    }

    #[test]
    fn empty_compensating_plan_errors() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "", &["03"]),
            node("06", PlanStatus::Draft, &["05"], &[]),
        ]);
        let err = rewire_compensation(&mut w, "05", "now").unwrap_err();
        assert_eq!(err.code(), "MINE_GRAPH_INVALID");
        assert_eq!(
            w.get("06").unwrap().hard_predecessors,
            vec!["05".to_string()],
            "unchanged"
        );
    }

    #[test]
    fn missing_replacement_errors() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            node("06", PlanStatus::Draft, &["05"], &[]),
        ]);
        let err = rewire_compensation(&mut w, "05", "now").unwrap_err();
        assert_eq!(err.code(), "MINE_PLAN_NOT_FOUND");
    }

    #[test]
    fn rejected_replacement_errors() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            rejected_node("05-1", "", &["03"]),
            node("06", PlanStatus::Draft, &["05"], &[]),
        ]);
        let err = rewire_compensation(&mut w, "05", "now").unwrap_err();
        assert_eq!(err.code(), "MINE_GRAPH_INVALID");
    }

    #[test]
    fn locked_successor_errors_and_leaves_ws_unchanged() {
        for status in [
            PlanStatus::InProgress,
            PlanStatus::Implemented,
            PlanStatus::Accepted,
            PlanStatus::Rejected,
        ] {
            let mut w = ws(vec![
                node("03", PlanStatus::Accepted, &[], &[]),
                node("04", PlanStatus::Accepted, &[], &[]),
                rejected_node("05", "05-1", &["03"]),
                node("05-1", PlanStatus::Ready, &["03"], &[]),
                node("06", status, &["04", "05"], &[]),
            ]);
            let before = w.clone();
            let err = rewire_compensation(&mut w, "05", "now").unwrap_err();
            assert_eq!(err.code(), "MINE_REWIRE_SUCCESSOR_LOCKED");
            assert_eq!(w, before, "ws unchanged for locked successor ({status:?})");
        }
    }

    #[test]
    fn cycle_errors_and_leaves_ws_unchanged() {
        // 05-1 hard-depends on 06; rewiring 06: 06->05-1 and 05-1->06 => cycle.
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            node("04", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            node("05-1", PlanStatus::Draft, &["06"], &[]),
            node("06", PlanStatus::Draft, &["04", "05"], &[]),
        ]);
        let before = w.clone();
        let err = rewire_compensation(&mut w, "05", "now").unwrap_err();
        assert_eq!(err.code(), "MINE_GRAPH_CYCLE");
        assert_eq!(w, before, "ws unchanged on cycle");
    }

    #[test]
    fn idempotent_no_op_returns_empty_and_leaves_ws_unchanged() {
        let mut w = ws(vec![
            node("03", PlanStatus::Accepted, &[], &[]),
            node("04", PlanStatus::Accepted, &[], &[]),
            rejected_node("05", "05-1", &["03"]),
            node("05-1", PlanStatus::Accepted, &["03"], &[]),
            node("06", PlanStatus::Blocked, &["04", "05-1"], &[]), // already rewired
        ]);
        let before = w.clone();
        let affected = rewire_compensation(&mut w, "05", "now").unwrap();
        assert!(affected.is_empty());
        assert_eq!(w, before);
    }
}
