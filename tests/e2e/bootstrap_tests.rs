// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Bootstrap tests: `mine init` determinism, clean repository skeleton, and
//! self-hosted plan lifecycle in an isolated temp repo.

use mine::cli;
use std::path::PathBuf;
use std::process::Command;

#[allow(dead_code)]
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn init_git_repo(dir: &std::path::Path) {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["init", "--quiet", "-b", "master"])
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "user.email", "test@example.com"])
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "user.name", "test"])
        .status()
        .unwrap();
    std::fs::write(dir.join("README.md"), "test\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["add", "."])
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["commit", "--quiet", "-m", "init"])
        .status()
        .unwrap();
}

fn run(args: &[&str], repo: &std::path::Path) -> (i32, serde_json::Value) {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        repo.to_string_lossy().to_string(),
    ];
    argv.extend(args.iter().map(|s| s.to_string()));
    let outcome = cli::dispatch(&argv, "mine");
    let (stdout, stderr) = cli::render(&outcome, true, false);
    let body = if outcome.exit_code == 0 {
        stdout
    } else {
        stderr
    };
    (
        outcome.exit_code,
        serde_json::from_str(&body).unwrap_or(serde_json::Value::Null),
    )
}

#[test]
fn mine_init_creates_only_approved_skeleton() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let (exit, env) = run(&["init", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "mine init must succeed: {env}");
    assert_eq!(env["ok"], true);
    // Created: .mine/config.toml, .mine/design/.mine-design.toml, .mine/design/index.md, AGENTS.md
    assert!(
        tmp.path().join(".mine/config.toml").exists(),
        "config created"
    );
    assert!(
        tmp.path().join(".mine/design/.mine-design.toml").exists(),
        "marker created"
    );
    assert!(
        tmp.path().join(".mine/design/index.md").exists(),
        "index created"
    );
    // Does NOT create: .mine/plan/, business code, branches, commits
    assert!(!tmp.path().join(".mine/plan").exists(), "no plan workspace");
    assert!(
        !tmp.path().join(".mine/plan/execution-graph.toml").exists(),
        "no graph"
    );
    // No git branches created (only the initial commit)
    let branches = Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["branch", "--list"])
        .output()
        .unwrap();
    let branch_list = String::from_utf8_lossy(&branches.stdout);
    assert!(
        !branch_list.contains("dev"),
        "mine init does not create dev branch"
    );
    assert!(
        !branch_list.contains("plan/"),
        "mine init does not create plan branches"
    );
    // No commits beyond the initial
    let commits = Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    let count: u32 = String::from_utf8_lossy(&commits.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    assert_eq!(count, 1, "mine init does not commit");
}

#[test]
fn mine_init_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    run(&["init", "--format", "json"], tmp.path());
    let (exit2, env2) = run(&["init", "--format", "json"], tmp.path());
    assert_eq!(exit2, 0, "second init succeeds: {env2}");
    assert_eq!(env2["ok"], true);
}

#[test]
fn mine_init_refuses_unmarked_owned_design_without_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // Pre-place foreign content in the configured owned namespace.
    std::fs::create_dir_all(tmp.path().join(".mine/design")).unwrap();
    std::fs::write(tmp.path().join(".mine/design/old.md"), "legacy").unwrap();
    let (exit, env) = run(&["init", "--format", "json"], tmp.path());
    assert_ne!(exit, 0, "foreign ownership must be refused: {env}");
    assert_eq!(env["error"]["code"], "MINE_DESIGN_NAMESPACE_CONFLICT");
    assert_eq!(
        std::fs::read(tmp.path().join(".mine/design/old.md")).unwrap(),
        b"legacy"
    );
    assert!(!tmp.path().join(".mine/design/.mine-design.toml").exists());
    assert!(!tmp.path().join(".mine/config.toml").exists());
    assert!(!tmp.path().join(".mine/runtime/design-backups").exists());
}

#[test]
fn self_hosted_plan_lifecycle_in_temp_repo() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    run(&["init", "--format", "json"], tmp.path());
    super::managed_private_dev(tmp.path());
    // Native open checkpoints the initially absent graph pair at revision1.
    let (exit, env) = run(
        &[
            "workspace",
            "open",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            "0",
            "--format",
            "json",
        ],
        tmp.path(),
    );
    assert_eq!(exit, 0, "workspace open: {env}");
    // mine-release-allow-plan-reference: private canonical Plan body exists before registration
    let plan_path = ".mine/plan/plan01-test.md";
    std::fs::write(tmp.path().join(plan_path), "# Explicit private Plan\n").unwrap();
    super::fixture_git(tmp.path(), &["add", "--", plan_path]);
    super::fixture_git(
        tmp.path(),
        &["commit", "--quiet", "-m", "private Plan body"],
    );
    // Add a plan.
    let (exit, env) = run(
        &[
            "plan",
            "add",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            "1",
            "--id",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan01",
            "--path",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            ".mine/plan/plan01-test.md",
            "--title",
            "Test",
            "--design-ref",
            ".mine/design/index.md",
            "--write",
            "tests/",
            "--format",
            "json",
        ],
        tmp.path(),
    );
    assert_eq!(exit, 0, "plan add: {env}");
    assert_eq!(env["data"]["plan"]["status"], "DRAFT");
    // Release.
    let (exit, env) = run(
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        &[
            "plan",
            "release",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan01",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            "2",
            "--format",
            "json",
        ],
        tmp.path(),
    );
    assert_eq!(exit, 0, "plan release: {env}");
    assert_eq!(env["data"]["status_after"], "READY");
    // Graph validate.
    let (exit, env) = run(&["graph", "validate", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "graph validate: {env}");
    // Design validate.
    let (exit, env) = run(&["design", "validate", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "design validate: {env}");
}
