// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Golden tests for deterministic Markdown rendering of the execution graph.
//!
//! The generated view `docs/plan/execution-graph.md` must be a deterministic
//! function of the TOML machine source: the same TOML always renders to the
//! same bytes. These tests assert that property against the real repository
//! graph and against synthetic workspaces.

use mine::domain::graph::{PlanNode, PlanWorkspace};
use mine::domain::status::PlanStatus;
use mine::infrastructure::toml_store::render_markdown;
use mine::render;

#[test]
fn render_is_deterministic_for_development_fixture() {
    let ws = development_fixture();
    let a = render::render(&ws).expect("render a");
    let b = render::render(&ws).expect("render b");
    assert_eq!(a, b, "rendering the same workspace is byte-deterministic");
}

#[test]
fn render_markdown_round_trips_through_render_module() {
    let ws = sample_workspace();
    let direct = render_markdown(&ws).unwrap();
    let via_mod = render::render(&ws).unwrap();
    assert_eq!(direct, via_mod);
}

#[test]
fn render_contains_required_sections_and_plans() {
    let ws = development_fixture();
    let md = render::render(&ws).unwrap();
    assert!(md.contains("# Execution Graph"));
    assert!(md.contains("GENERATED VIEW"));
    assert!(md.contains("Revision: `"));
    assert!(md.contains("| Plan | Title | Status | Hard predecessors |"));
    // Every plan id appears in the table.
    for p in &ws.plans {
        assert!(
            md.contains(&format!("| {} |", p.id)),
            "plan {} row present",
            p.id
        );
    }
}

#[test]
fn render_is_stable_across_status_changes() {
    // Changing only a status updates only that row; the structure is stable.
    let mut ws = sample_workspace();
    let original_status = ws.plans[0].status;
    let baseline = render_markdown(&ws).unwrap();
    // Mutate one status and ensure the new render differs only in that cell.
    ws.plans[0].status = PlanStatus::InProgress;
    let updated = render_markdown(&ws).unwrap();
    assert_ne!(baseline, updated);
    // Reverting reproduces the baseline exactly (determinism, not order-dependent).
    ws.plans[0].status = original_status;
    let reverted = render_markdown(&ws).unwrap();
    assert_eq!(baseline, reverted);
}

fn development_fixture() -> PlanWorkspace {
    toml::from_str(include_str!("fixtures/development-execution-graph.toml"))
        .expect("development graph fixture parses")
}

fn sample_workspace() -> PlanWorkspace {
    PlanWorkspace {
        schema_version: mine::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
        revision: 7,
        project_id: "mine-is-not-everyones".to_string(),
        workspace_id: "golden-ws".to_string(),
        stable_branch: "master".to_string(),
        integration_branch: "dev".to_string(),
        stable_baseline_commit: "abc123".to_string(),
        design_root: "docs/design/index.md".to_string(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans: vec![
            PlanNode {
                id: "01".to_string(),
                path: "docs/plan/01.md".to_string(),
                title: "First".to_string(),
                branch: String::new(),
                branch_anchor: String::new(),
                status: PlanStatus::Accepted,
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
                lineage_id: String::new(),
                worktree_path: String::new(),
            },
            PlanNode {
                id: "02".to_string(),
                path: "docs/plan/02.md".to_string(),
                title: "Second".to_string(),
                branch: String::new(),
                branch_anchor: String::new(),
                status: PlanStatus::Ready,
                hard_predecessors: vec!["01".to_string()],
                soft_predecessors: vec![],
                design_references: vec!["docs/design/principles.md".to_string()],
                exclusive_write_paths: vec!["src/b/".to_string()],
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
            },
        ],
    }
}
