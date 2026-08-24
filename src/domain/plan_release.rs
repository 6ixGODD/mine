//! Plan release: the explicit, deterministic gate that moves a newly
//! registered `DRAFT` plan into the startable frontier.
//!
//! Implements `docs/design/execution-graph/state-machine-and-algorithms.md#plan-release`.
//! `mine plan add` registers plans as `DRAFT`; this operation transitions a
//! `DRAFT` node to `READY` (every hard predecessor `ACCEPTED`, including the
//! no-predecessor case) or to `BLOCKED` (one or more hard predecessors not yet
//! `ACCEPTED`). It is pure: it mutates the supplied [`PlanWorkspace`] in place
//! and does not touch the filesystem or bump the revision (the caller's
//! persistence transaction does those). It never alters a non-`DRAFT` node and
//! exposes no arbitrary state-editing capability.

use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::PlanWorkspace;
use crate::domain::status::PlanStatus;

/// Releases a `DRAFT` plan into the startable frontier.
///
/// Computes the unsatisfied hard predecessors (those not `ACCEPTED`), in
/// stable predecessor-list order, then transitions the node:
/// - `DRAFT -> READY` when `unsatisfied` is empty (includes plans with no
///   hard predecessors);
/// - `DRAFT -> BLOCKED` otherwise.
///
/// The node's `updated_at` is refreshed to `now`. No other node is touched and
/// the workspace `revision` is not modified here.
///
/// # Errors
/// - [`MineError::PlanNotFound`] if `plan_id` does not exist.
/// - [`MineError::InvalidTransition`] if the node is not currently `DRAFT`.
///
/// `ws` is left unmutated on error.
pub fn release_plan(ws: &mut PlanWorkspace, plan_id: &str, now: &str) -> MineResult<()> {
    let node = ws.get(plan_id).ok_or_else(|| MineError::PlanNotFound {
        plan_id: plan_id.to_string(),
    })?;
    if node.status != PlanStatus::Draft {
        return Err(MineError::InvalidTransition {
            plan_id: plan_id.to_string(),
            from: node.status.as_str().to_string(),
            to: PlanStatus::Ready.as_str().to_string(),
        });
    }
    let hard = node.hard_predecessors.clone();

    // Validate every predecessor id exists (structural validation also checks
    // this, but we surface a precise error before mutating).
    for pred in &hard {
        if ws.get(pred).is_none() {
            return Err(MineError::PlanNotFound {
                plan_id: pred.clone(),
            });
        }
    }

    let any_unsatisfied = hard.iter().any(|pred| {
        ws.get(pred)
            .is_some_and(|n| n.status != PlanStatus::Accepted)
    });

    let target = if any_unsatisfied {
        PlanStatus::Blocked
    } else {
        PlanStatus::Ready
    };

    let node = ws.get_mut(plan_id).expect("checked present above");
    node.status = target;
    node.updated_at = now.to_string();
    Ok(())
}

/// Returns the unsatisfied hard predecessors (status != `ACCEPTED`) of
/// `plan_id`, in stable predecessor-list order. Used by the CLI handler to
/// report `data.unsatisfied_predecessors` deterministically when the node has
/// just been released; the caller passes the *post-release* workspace (the
/// predecessors' statuses are unaffected by the release itself).
///
/// # Errors
/// Returns [`MineError::PlanNotFound`] if `plan_id` or any predecessor is
/// absent.
pub fn unsatisfied_predecessors(ws: &PlanWorkspace, plan_id: &str) -> MineResult<Vec<String>> {
    let node = ws.get(plan_id).ok_or_else(|| MineError::PlanNotFound {
        plan_id: plan_id.to_string(),
    })?;
    let mut out = Vec::new();
    for pred in &node.hard_predecessors {
        let pred_node = ws.get(pred).ok_or_else(|| MineError::PlanNotFound {
            plan_id: pred.clone(),
        })?;
        if pred_node.status != PlanStatus::Accepted {
            out.push(pred.clone());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::graph::{PlanNode, PlanWorkspace};
    use crate::domain::status::PlanStatus;

    fn node(id: &str, status: PlanStatus, hard: &[&str]) -> PlanNode {
        PlanNode {
            id: id.to_string(),
            path: format!("docs/plan/{id}.md"),
            title: id.to_string(),
            branch: String::new(),
            branch_anchor: String::new(),
            status,
            hard_predecessors: hard.iter().map(|s| s.to_string()).collect(),
            soft_predecessors: vec![],
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
            lineage_id: String::new(),
            worktree_path: String::new(),
        }
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
    fn no_predecessors_draft_becomes_ready() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        release_plan(&mut w, "09", "now").unwrap();
        assert_eq!(w.get("09").unwrap().status, PlanStatus::Ready);
        assert_eq!(
            unsatisfied_predecessors(&w, "09").unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn all_accepted_predecessors_becomes_ready() {
        let mut w = ws(vec![
            node("01", PlanStatus::Accepted, &[]),
            node("03", PlanStatus::Accepted, &[]),
            node("09", PlanStatus::Draft, &["01", "03"]),
        ]);
        release_plan(&mut w, "09", "now").unwrap();
        assert_eq!(w.get("09").unwrap().status, PlanStatus::Ready);
        assert!(unsatisfied_predecessors(&w, "09").unwrap().is_empty());
    }

    #[test]
    fn one_unaccepted_predecessor_becomes_blocked() {
        let mut w = ws(vec![
            node("01", PlanStatus::Accepted, &[]),
            node("02", PlanStatus::Blocked, &["01"]),
            node("09", PlanStatus::Draft, &["01", "02"]),
        ]);
        release_plan(&mut w, "09", "now").unwrap();
        assert_eq!(w.get("09").unwrap().status, PlanStatus::Blocked);
        assert_eq!(
            unsatisfied_predecessors(&w, "09").unwrap(),
            vec!["02".to_string()]
        );
    }

    #[test]
    fn non_draft_rejects_and_leaves_ws_unchanged() {
        for status in [
            PlanStatus::Blocked,
            PlanStatus::Ready,
            PlanStatus::InProgress,
            PlanStatus::Implemented,
            PlanStatus::Accepted,
            PlanStatus::Rejected,
        ] {
            let mut w = ws(vec![node("09", status, &[])]);
            let before = w.clone();
            let err = release_plan(&mut w, "09", "now").unwrap_err();
            assert_eq!(err.code(), "MINE_INVALID_TRANSITION");
            assert_eq!(w, before, "ws unchanged on rejected release ({status:?})");
        }
    }

    #[test]
    fn missing_plan_is_not_found() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        let err = release_plan(&mut w, "missing", "now").unwrap_err();
        assert_eq!(err.code(), "MINE_PLAN_NOT_FOUND");
    }

    #[test]
    fn missing_predecessor_is_not_found() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &["ghost"])]);
        let err = release_plan(&mut w, "09", "now").unwrap_err();
        assert_eq!(err.code(), "MINE_PLAN_NOT_FOUND");
    }

    #[test]
    fn release_refreshes_updated_at() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        release_plan(&mut w, "09", "2026-07-24T00:00:00Z").unwrap();
        assert_eq!(w.get("09").unwrap().updated_at, "2026-07-24T00:00:00Z");
    }

    #[test]
    fn release_does_not_bump_revision() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        release_plan(&mut w, "09", "now").unwrap();
        assert_eq!(w.revision, 5, "release itself does not bump revision");
    }
}
