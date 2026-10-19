// Enforce no `unsafe` in shared test helpers.
#![forbid(unsafe_code)]

//! Shared helpers for `mine plan release` and
//! `mine plan rewire-compensation`) integration tests.
//!
//! Each test builds an ISOLATED TEMPORARY repository seeded with a controlled
//! execution graph and drives `mine::cli::dispatch` over stdio JSON. The real
//! Root graph is never a fixture target. Development-fixture byte guards below
//! check that artifact only; they do not claim a live Root graph audit.

use std::path::{Path, PathBuf};
use std::process::Command;

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
    let canonical = mine::domain::plan_id::CanonicalPlanId::parse(id).unwrap();
    PlanNode {
        id: id.to_string(),
        path: format!(".mine/plan/{id}-fixture.md"),
        title: id.to_string(),
        branch: format!("plan/{id}-fixture"),
        branch_anchor: String::new(),
        status,
        hard_predecessors: hard.iter().map(|s| s.to_string()).collect(),
        soft_predecessors: soft.iter().map(|s| s.to_string()).collect(),
        design_references: vec![".mine/design/index.md".to_string()],
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
        withdraw_reason: String::new(),
        withdrawn_at: String::new(),
        planning_scope: String::new(),
        readiness_checked_at: String::new(),
        adr_registry_revision: 0,
        lineage_id: canonical.lineage_id(),
        worktree_path: canonical.worktree_path(),
        body_sha256: None,
        ownership: None,
        ownership_policy: None,
        clarifications: vec![],
        acceptance: None,
        preparations: vec![],
        started_preparation_generation: None,
    }
}

/// A rejected plan node with a registered compensating plan.
pub fn rejected_node(id: &str, comp: &str, hard: &[&str]) -> PlanNode {
    let mut n = node(id, PlanStatus::Rejected, hard, &[]);
    n.compensating_plan = comp.to_string();
    n
}

/// Builds a private real-Git Root dev with committed configuration, Design,
/// canonical Plan bodies, graph and view. Seeded historical states are explicit
/// fixture facts, not fabricated native preparation or acceptance receipts.
pub fn seeded_repo(plans: Vec<PlanNode>) -> (tempfile::TempDir, PathBuf) {
    seeded_repo_with_lock_timeout(plans, 5_000)
}

/// A revision-race fixture must wait for the actual native checkpoint to finish
/// before asserting stale-revision refusal. This changes only that fixture's
/// configured contention wait, not production defaults or performance budgets.
pub fn seeded_repo_with_lock_timeout(
    plans: Vec<PlanNode>,
    lock_timeout_ms: u32,
) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_path_buf();
    git(&repo, &["init", "-b", "master"]);
    for (key, value) in [
        ("user.name", "Public Contract Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(&repo, &["config", key, value]);
    }
    std::fs::write(repo.join("README.md"), "# Private fixture\n").unwrap();
    std::fs::write(
        repo.join(".gitignore"),
        "/.mine/runtime/\n/.mine/locks/\n/.mine/build/\n/.mine/worktrees/\n",
    )
    .unwrap();
    git(&repo, &["add", "README.md", ".gitignore"]);
    git(&repo, &["commit", "-m", "private initial source"]);
    let (outcome, envelope) = dispatch_json(&repo, &["init", "--format", "json"]);
    assert_eq!(outcome.exit_code, 0, "{envelope}");
    let mut config = mine::cli::context::load_config(&repo).unwrap();
    config.graph.lock_timeout_ms = lock_timeout_ms;
    std::fs::write(repo.join(".mine/config.toml"), config.to_toml()).unwrap();
    git(
        &repo,
        &[
            "add",
            ".mine/config.toml",
            ".mine/.gitignore",
            ".mine/.gitattributes",
            "AGENTS.md",
            ".mine/design",
        ],
    );
    git(&repo, &["commit", "-m", "private managed baseline"]);
    let baseline = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["switch", "-c", "dev"]);
    let mut ws = PlanWorkspace {
        schema_version: 4,
        revision: 72,
        project_id: "public-contract-fixture".into(),
        workspace_id: uuid::Uuid::new_v4().to_string(),
        stable_branch: "master".into(),
        integration_branch: "dev".into(),
        stable_baseline_commit: baseline.clone(),
        design_root: ".mine/design/index.md".into(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans: Vec::new(),
    };
    ws.plans = plans;
    std::fs::create_dir_all(repo.join(".mine/plan")).unwrap();
    for node in &mut ws.plans {
        if node.branch_anchor.is_empty()
            && matches!(
                node.status,
                PlanStatus::InProgress
                    | PlanStatus::Implemented
                    | PlanStatus::Accepted
                    | PlanStatus::Rejected
            )
        {
            node.branch_anchor = baseline.clone();
        }
        std::fs::write(
            repo.join(&node.path),
            format!("# Private {} fixture\n", node.id),
        )
        .unwrap();
    }
    std::fs::write(
        repo.join(".mine/plan/execution-graph.toml"),
        toml::to_string(&ws).unwrap(),
    )
    .unwrap();
    TomlStore::new(&repo).render().unwrap();
    git(&repo, &["add", ".mine/plan"]);
    git(
        &repo,
        &[
            "commit",
            "-m",
            "explicit seeded lifecycle-state graph fixture",
        ],
    );
    assert_eq!(git(&repo, &["status", "--porcelain=v1", "-uall"]), "");
    (tmp, repo)
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Loads the seeded graph from a temp repo.
pub fn load_graph(repo: &Path) -> PlanWorkspace {
    toml::from_str(&std::fs::read_to_string(repo.join(".mine/plan/execution-graph.toml")).unwrap())
        .unwrap()
}

/// Read the on-disk development fixture, not a compile-time byte constant.
/// This detects accidental fixture edits; it is NOT a live Root graph audit.
pub fn live_graph_bytes() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/development-execution-graph.toml"),
    )
    .unwrap()
}

/// Dispatches a CLI call against `repo` and returns the outcome + its JSON envelope.
pub fn dispatch_json(repo: &Path, rest: &[&str]) -> (cli::Outcome, serde_json::Value) {
    let outcome = cli::dispatch(&run(repo.to_str().unwrap(), rest), "mine");
    let env = envelope_json(&outcome);
    (outcome, env)
}
