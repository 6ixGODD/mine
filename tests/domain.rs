// Enforce no `unsafe` in MINE-owned test crates either (see src/lib.rs).
#![forbid(unsafe_code)]

//! Domain-level integration tests for the execution graph.
//!
//! These exercise the pure domain rules end to end: status transitions, path
//! safety, graph validation, topological sort, readiness derivation, parallel
//! wave, and write-scope conflict detection.

//! safety, graph validation, topological sort, readiness derivation, parallel
//! wave, and write-scope conflict detection. They construct workspaces
//! directly without touching the filesystem.

use mine::domain::error::MineError;
use mine::domain::graph::{PlanNode, PlanWorkspace};
use mine::domain::status::PlanStatus;
use mine::domain::validation;

fn node(id: &str, hard: &[&str], excl: &[&str]) -> PlanNode {
    PlanNode {
        id: id.to_string(),
        path: format!("docs/plan/{id}-test.md"),
        title: format!("Plan {id}"),
        branch: mine::domain::plan_id::registered_plan_branch(
            "validation-test",
            id,
            &format!("docs/plan/{id}-test.md"),
        )
        .unwrap(),
        branch_anchor: String::new(),
        status: PlanStatus::Blocked,
        hard_predecessors: hard.iter().map(|s| s.to_string()).collect(),
        soft_predecessors: vec![],
        design_references: vec!["docs/design/principles.md".to_string()],
        exclusive_write_paths: excl.iter().map(|s| s.to_string()).collect(),
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
        lineage_id: mine::domain::plan_id::CanonicalPlanId::parse(id)
            .unwrap()
            .lineage_id(),
        worktree_path: mine::domain::plan_id::CanonicalPlanId::parse(id)
            .unwrap()
            .worktree_path(),
        body_sha256: None,
        ownership: None,
        ownership_policy: None,
        clarifications: vec![],
        acceptance: None,
        preparations: vec![],
        started_preparation_generation: None,
    }
}

fn workspace(plans: Vec<PlanNode>) -> PlanWorkspace {
    PlanWorkspace {
        schema_version: mine::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
        revision: 0,
        project_id: "test".to_string(),
        workspace_id: "validation-test".to_string(),
        stable_branch: "master".to_string(),
        integration_branch: "dev".to_string(),
        stable_baseline_commit: String::new(),
        design_root: "docs/design/index.md".to_string(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans,
    }
}

// ---------- state machine ----------

#[test]
fn full_lifecycle_transitions_allowed() -> Result<(), MineError> {
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut n = node("plan01", &[], &["src/a/"]);
    // READY -> IN_PROGRESS -> IMPLEMENTED -> ACCEPTED
    n.status = PlanStatus::Ready;
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    PlanStatus::Ready.validate_transition("plan01", PlanStatus::InProgress)?;
    n.status = PlanStatus::InProgress;
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    PlanStatus::InProgress.validate_transition("plan01", PlanStatus::Implemented)?;
    n.status = PlanStatus::Implemented;
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    PlanStatus::Implemented.validate_transition("plan01", PlanStatus::Accepted)?;
    Ok(())
}

#[test]
fn reject_path_requires_review_then_compensation() -> Result<(), MineError> {
    // IMPLEMENTED -> REJECTED. REJECTED is terminal: compensation is closed
    // by registering a compensating plan (mine plan add) and rewiring
    // downstream successors (mine plan rewire-compensation), NOT by a status
    // transition. The earlier REJECTED -> BLOCKED edge is removed as dead
    // historical baggage (no operation performs it).
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    PlanStatus::Implemented.validate_transition("plan01", PlanStatus::Rejected)?;
    // REJECTED may NOT transition to BLOCKED, READY, or any other state.
    for target in [
        PlanStatus::Blocked,
        PlanStatus::Ready,
        PlanStatus::InProgress,
        PlanStatus::Implemented,
        PlanStatus::Accepted,
        PlanStatus::Draft,
    ] {
        assert!(
            PlanStatus::Rejected
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                .validate_transition("plan01", target)
                .is_err(),
            "REJECTED -> {target:?} must be rejected (terminal)"
        );
    }
    Ok(())
}

#[test]
fn accepted_is_terminal_no_back_transition() -> Result<(), MineError> {
    for target in [
        PlanStatus::Ready,
        PlanStatus::InProgress,
        PlanStatus::Implemented,
        PlanStatus::Rejected,
        PlanStatus::Draft,
    ] {
        assert!(
            PlanStatus::Accepted
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                .validate_transition("plan01", target)
                .is_err(),
            "ACCEPTED -> {target:?} must be rejected"
        );
    }
    Ok(())
}

// ---------- validation ----------

#[test]
fn duplicate_plan_path_rejected() -> Result<(), MineError> {
    // Same canonical path as the first node: exercise the duplicate-path gate,
    // not an unrelated obsolete filename refusal.
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut dup = node("plan02", &[], &["src/b/"]);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let first = node("plan01", &[], &["src/a/"]);
    dup.path.clone_from(&first.path);
    let g = workspace(vec![first, dup]);
    let err = validation::validate(&g).unwrap_err();
    assert_eq!(err.code(), "MINE_GRAPH_INVALID");
    Ok(())
}

#[test]
fn diamond_dependency_validates_and_topologizes() -> Result<(), MineError> {
    let g = workspace(vec![
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan01", &[], &["src/a/"]),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan02", &["plan01"], &["src/b/"]),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan03", &["plan01"], &["src/c/"]),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan04", &["plan02", "plan03"], &["src/d/"]),
    ]);
    validation::validate(&g)?;
    let order = validation::topological_sort(&g)?;
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert_eq!(order.first(), Some(&"plan01".to_string()));
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert_eq!(order.last(), Some(&"plan04".to_string()));
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert!(order.iter().position(|x| x == "plan02") < order.iter().position(|x| x == "plan04"));
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert!(order.iter().position(|x| x == "plan03") < order.iter().position(|x| x == "plan04"));
    Ok(())
}

#[test]
fn parallel_wave_picks_disjoint_ready_set() -> Result<(), MineError> {
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut p01 = node("plan01", &[], &["src/a/"]);
    p01.status = PlanStatus::Accepted;
    p01.branch_anchor = "pure-domain-anchor".into();
    let mut g = workspace(vec![
        p01,
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan02", &["plan01"], &["src/b/"]),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan03", &["plan01"], &["src/c/"]),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan04", &["plan01"], &["src/d/"]),
    ]);
    // All hard predecessors (01) accepted; mark 02/03/04 READY.
    g.plans
        .iter_mut()
        .skip(1)
        .for_each(|p| p.status = PlanStatus::Ready);
    let wave = validation::parallel_wave(&g);
    // 02, 03, 04 have disjoint write scopes and no ancestor relationships
    // among themselves, so all three can run in parallel.
    assert_eq!(wave.len(), 3);
    Ok(())
}

#[test]
fn parallel_wave_excludes_write_overlap() -> Result<(), MineError> {
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut p01 = node("plan01", &[], &["src/a/"]);
    p01.status = PlanStatus::Accepted;
    p01.branch_anchor = "pure-domain-anchor".into();
    let mut g = workspace(vec![
        p01,
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan02", &["plan01"], &["src/shared/"]),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan03", &["plan01"], &["src/shared/x.rs"]),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node("plan04", &["plan01"], &["src/other/"]),
    ]);
    // Make exclusive scopes disjoint for validation, then force an overlap
    // for the wave via reserved shared paths on 02 and 03.
    g.plans[2].exclusive_write_paths = vec!["src/other2/".to_string()];
    g.plans[1].reserved_shared_paths = vec!["docs/plan/execution-graph.toml".to_string()];
    g.plans[2].reserved_shared_paths = vec!["docs/plan/execution-graph.toml".to_string()];
    validation::validate(&g)?;
    g.plans
        .iter_mut()
        .skip(1)
        .for_each(|p| p.status = PlanStatus::Ready);
    let wave = validation::parallel_wave(&g);
    // 02 and 03 share the reserved path; only one of them plus 04 can be in
    // the wave.
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert!(wave.contains(&"plan04".to_string()));
    assert!(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        wave.contains(&"plan02".to_string()) ^ wave.contains(&"plan03".to_string()),
        "wave was {wave:?}"
    );
    Ok(())
}

#[test]
fn empty_design_references_rejected() -> Result<(), MineError> {
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut n = node("plan01", &[], &["src/a/"]);
    n.design_references = vec![];
    let g = workspace(vec![n]);
    let err = validation::validate(&g).unwrap_err();
    assert_eq!(err.code(), "MINE_GRAPH_INVALID");
    Ok(())
}

#[test]
fn unsafe_owned_path_rejected() -> Result<(), MineError> {
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let n = node("plan01", &[], &["../escape/"]);
    let g = workspace(vec![n]);
    let err = validation::validate(&g).unwrap_err();
    assert_eq!(err.code(), "MINE_GRAPH_INVALID");
    Ok(())
}
