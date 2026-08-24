// Enforce no `unsafe` in shared test helpers.
#![forbid(unsafe_code)]

//! Shared helpers for `mine plan release` and
//! `mine plan rewire-compensation`) integration tests.
//!
//! Each test builds an ISOLATED TEMPORARY repository seeded with a controlled
//! execution graph and drives `mine::cli::dispatch` over stdio JSON. The live
//! repository graph is never mutated; tests snapshot its bytes before/after and
//! assert they are unchanged.

use std::path::{Path, PathBuf};

use mine::cli;
use mine::domain::graph::{PlanNode, PlanWorkspace};
use mine::domain::status::PlanStatus;
use mine::infrastructure::toml_store::TomlStore;

/// Renders an outcome as JSON and parses the envelope. Successful JSON is on
/// stdout; error JSON on stderr (kept off stdout for pipeline purity).
pub fn envelope_json(outcome: &cli::Outcome) -> serde_json::Value {
    let (stdout, stderr) = cli::render(outcome, true, false);
    let body = if outcome.exit_code == 0 {
        stdout
    } else {
        stderr
    };
    serde_json::from_str(&body).expect("envelope must be valid JSON")
}

/// Builds an `argv` vector with `--repo`.
pub fn run(repo: &str, rest: &[&str]) -> Vec<String> {
    let mut v = vec!["mine".to_string(), "--repo".to_string(), repo.to_string()];
    v.extend(rest.iter().map(|s| s.to_string()));
    v
}

/// A minimal plan node builder for test fixtures.
pub fn node(id: &str, status: PlanStatus, hard: &[&str], soft: &[&str]) -> PlanNode {
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
        exclusive_write_paths: vec![format!("tests/{id}/")],
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

/// A rejected plan node with a registered compensating plan.
pub fn rejected_node(id: &str, comp: &str, hard: &[&str]) -> PlanNode {
    let mut n = node(id, PlanStatus::Rejected, hard, &[]);
    n.compensating_plan = comp.to_string();
    n
}

/// Builds an isolated temp repo: real `.mine/config.toml` (validated) plus a
/// seeded execution graph whose header fields come from the real graph and
/// whose plans are `plans`. The graph TOML and Markdown view are written
/// deterministically via the accepted renderer. Returns the temp dir and the
/// repo root.
pub fn seeded_repo(plans: Vec<PlanNode>) -> (tempfile::TempDir, PathBuf) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_path_buf();
    std::fs::create_dir_all(repo.join(".mine")).unwrap();
    std::fs::create_dir_all(repo.join("docs/plan")).unwrap();
    let cfg = mine::cli::context::load_config(&manifest).expect("real config exists");
    std::fs::write(repo.join(".mine/config.toml"), cfg.to_toml()).unwrap();

    let real_graph: PlanWorkspace =
        toml::from_str(include_str!("../fixtures/development-execution-graph.toml"))
            .expect("development graph fixture parses");
    let mut ws = real_graph;
    ws.plans = plans;
    std::fs::write(
        repo.join("docs/plan/execution-graph.toml"),
        toml::to_string(&ws).unwrap(),
    )
    .unwrap();
    TomlStore::new(&repo).render().unwrap();
    (tmp, repo)
}

/// Loads the seeded graph from a temp repo.
pub fn load_graph(repo: &Path) -> PlanWorkspace {
    toml::from_str(&std::fs::read_to_string(repo.join("docs/plan/execution-graph.toml")).unwrap())
        .unwrap()
}

/// Development graph fixture bytes, asserted unchanged by tests that operate
/// on isolated copies. Stable release trees intentionally carry no live graph.
pub fn live_graph_bytes() -> Vec<u8> {
    include_bytes!("../fixtures/development-execution-graph.toml").to_vec()
}

/// Dispatches a CLI call against `repo` and returns the outcome + its JSON envelope.
pub fn dispatch_json(repo: &Path, rest: &[&str]) -> (cli::Outcome, serde_json::Value) {
    let outcome = cli::dispatch(&run(repo.to_str().unwrap(), rest), "mine");
    let env = envelope_json(&outcome);
    (outcome, env)
}
