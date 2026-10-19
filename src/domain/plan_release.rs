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

/// Planning-readiness audit evidence recorded by `mine plan release`
/// (ADR-0013): the scope the re-check ran for, when, and which ADR registry
/// revision satisfied it (0 when the repository has no bootstrapped
/// registry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseReadinessAudit {
    pub planning_scope: String,
    pub checked_at: String,
    pub adr_registry_revision: u64,
}

/// Releases a `DRAFT` plan into the startable frontier.
///
/// Computes the unsatisfied hard predecessors (those not `ACCEPTED`), in
/// stable predecessor-list order, then transitions the node:
/// - `DRAFT -> READY` when `unsatisfied` is empty (includes plans with no
///   hard predecessors);
/// - `DRAFT -> BLOCKED` otherwise.
///
/// When `audit` is supplied, its scope is persisted on the node (a legacy
/// node without a recorded scope adopts the resolved value; an existing scope
/// is immutable and must match) together with `readiness_checked_at` and
/// `adr_registry_revision` as release-time audit evidence.
///
/// The node's `updated_at` is refreshed to `now`. No other node is touched and
/// the workspace `revision` is not modified here.
///
/// # Errors
/// - [`MineError::PlanNotFound`] if `plan_id` does not exist.
/// - [`MineError::InvalidTransition`] if the node is not currently `DRAFT`.
/// - [`MineError::GraphInvalid`] if the audit scope conflicts with the node's
///   already-recorded immutable planning scope.
///
/// `ws` is left unmutated on error.
pub fn release_plan(
    ws: &mut PlanWorkspace,
    plan_id: &str,
    now: &str,
    audit: Option<&ReleaseReadinessAudit>,
) -> MineResult<()> {
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
    if let Some(audit) = audit {
        if audit.planning_scope.trim().is_empty() {
            return Err(MineError::GraphInvalid {
                detail: format!("plan {plan_id} release audit has an empty planning scope"),
            });
        }
        if !node.planning_scope.is_empty() && node.planning_scope != audit.planning_scope {
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "plan {plan_id} planning scope {} is immutable and cannot become {}",
                    node.planning_scope, audit.planning_scope
                ),
            });
        }
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
    if let Some(audit) = audit {
        // A legacy node without a recorded scope adopts the resolved value;
        // an existing scope was verified equal above.
        node.planning_scope = audit.planning_scope.clone();
        node.readiness_checked_at = audit.checked_at.clone();
        node.adr_registry_revision = audit.adr_registry_revision;
    }
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
            withdraw_reason: String::new(),
            withdrawn_at: String::new(),
            planning_scope: String::new(),
            readiness_checked_at: String::new(),
            adr_registry_revision: 0,
            lineage_id: String::new(),
            worktree_path: String::new(),
            body_sha256: None,
            ownership: None,
            ownership_policy: None,
            clarifications: vec![],
            acceptance: None,
            preparations: vec![],
            started_preparation_generation: None,
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
        release_plan(&mut w, "09", "now", None).unwrap();
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
        release_plan(&mut w, "09", "now", None).unwrap();
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
        release_plan(&mut w, "09", "now", None).unwrap();
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
            PlanStatus::Withdrawn,
        ] {
            let mut w = ws(vec![node("09", status, &[])]);
            let before = w.clone();
            let err = release_plan(&mut w, "09", "now", None).unwrap_err();
            assert_eq!(err.code(), "MINE_INVALID_TRANSITION");
            assert_eq!(w, before, "ws unchanged on rejected release ({status:?})");
        }
    }

    #[test]
    fn missing_plan_is_not_found() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        let err = release_plan(&mut w, "missing", "now", None).unwrap_err();
        assert_eq!(err.code(), "MINE_PLAN_NOT_FOUND");
    }

    #[test]
    fn missing_predecessor_is_not_found() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &["ghost"])]);
        let err = release_plan(&mut w, "09", "now", None).unwrap_err();
        assert_eq!(err.code(), "MINE_PLAN_NOT_FOUND");
    }

    #[test]
    fn release_refreshes_updated_at() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        release_plan(&mut w, "09", "2026-07-24T00:00:00Z", None).unwrap();
        assert_eq!(w.get("09").unwrap().updated_at, "2026-07-24T00:00:00Z");
    }

    #[test]
    fn release_does_not_bump_revision() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        release_plan(&mut w, "09", "now", None).unwrap();
        assert_eq!(w.revision, 5, "release itself does not bump revision");
    }

    fn audit(scope: &str) -> ReleaseReadinessAudit {
        ReleaseReadinessAudit {
            planning_scope: scope.to_string(),
            checked_at: "2026-08-31T00:00:00Z".to_string(),
            adr_registry_revision: 13,
        }
    }

    #[test]
    fn release_records_readiness_audit_evidence() {
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        release_plan(
            &mut w,
            "09",
            "now",
            Some(&audit("docs/design/execution-graph")),
        )
        .unwrap();
        let node = w.get("09").unwrap();
        assert_eq!(node.planning_scope, "docs/design/execution-graph");
        assert_eq!(node.readiness_checked_at, "2026-08-31T00:00:00Z");
        assert_eq!(node.adr_registry_revision, 13);
    }

    #[test]
    fn release_persists_resolved_scope_on_legacy_node() {
        // A legacy pre-v4 node has no recorded scope; the release mutation
        // persists the resolved design-root value.
        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        assert!(w.get("09").unwrap().planning_scope.is_empty());
        release_plan(&mut w, "09", "now", Some(&audit("docs/design"))).unwrap();
        assert_eq!(w.get("09").unwrap().planning_scope, "docs/design");
    }

    #[test]
    fn release_refuses_conflicting_scope_and_empty_audit() {
        let mut scoped = node("09", PlanStatus::Draft, &[]);
        scoped.planning_scope = "docs/design/execution-graph".to_string();
        let mut w = ws(vec![scoped]);
        let before = w.clone();
        let err = release_plan(&mut w, "09", "now", Some(&audit("docs/design"))).unwrap_err();
        assert_eq!(err.code(), "MINE_GRAPH_INVALID");
        assert_eq!(w, before, "ws unchanged on scope conflict");

        let mut w = ws(vec![node("09", PlanStatus::Draft, &[])]);
        let empty = ReleaseReadinessAudit {
            planning_scope: String::new(),
            checked_at: "now".to_string(),
            adr_registry_revision: 1,
        };
        assert!(release_plan(&mut w, "09", "now", Some(&empty)).is_err());
    }
}
