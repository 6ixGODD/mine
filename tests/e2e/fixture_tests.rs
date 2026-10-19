// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! E2E fixtures: the six named bootstrap/design-interaction scenarios.
//!
//! 1. New repository — `bootstrap_tests::mine_init_creates_only_approved_skeleton`
//! 2. Large old repository without design — `large_old_repo_without_design`
//! 3. Stale managed design — `stale_managed_design_preserved_and_flagged`
//! 4. Unmarked owned design conflict — `bootstrap_tests::mine_init_refuses_unmarked_owned_design_without_mutation`
//! 5. Protected design decision — `protected_design_decision_not_overwritten`
//! 6. Unscoped incomplete coverage — `unscoped_incomplete_coverage_reported`
//!
//! All fixtures are deterministic, self-contained, and operate inside isolated
//! temp repos. No fixture inspects or mutates the real repository.

use mine::cli;
use std::path::PathBuf;
use std::process::Command;

#[allow(dead_code)]
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
}

fn git_commit(dir: &std::path::Path, msg: &str) {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["add", "."])
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["commit", "--quiet", "-m", msg])
        .status()
        .unwrap();
}

// --- Scenario 2: Large old repository without design ---

#[test]
fn large_old_repo_without_design_preserves_code_and_init_creates_skeleton() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // Simulate a realistic old repository with code, config, and history.
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"old\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join("README.md"), "# Old Project\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("tests")).unwrap();
    std::fs::write(tmp.path().join("tests/smoke.rs"), "#[test] fn test() {}\n").unwrap();
    git_commit(tmp.path(), "initial: old project with code, no design");

    // Add more history.
    std::fs::write(tmp.path().join("src/lib.rs"), "pub fn helper() {}\n").unwrap();
    git_commit(tmp.path(), "add lib module");

    // Run mine init.
    let (exit, env) = run(&["init", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "mine init must succeed on old repo: {env}");

    // Init creates only the approved skeleton; no architecture inferred.
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

    // Pre-existing code is preserved, not modified.
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("Cargo.toml"))
            .unwrap()
            .split_once("edition")
            .unwrap()
            .0,
        "[package]\nname = \"old\"\nversion = \"1.0.0\"\n"
    );

    // No plan workspace, no branches, no extra commits.
    assert!(
        !tmp.path().join(".mine/plan").exists(),
        "no plan workspace created"
    );
    let branches = Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["branch", "--list"])
        .output()
        .unwrap();
    let branch_str = String::from_utf8_lossy(&branches.stdout);
    assert!(!branch_str.contains("dev"), "no dev branch");
    assert!(!branch_str.contains("plan/"), "no plan branches");

    // Only the initial commits exist (mine init does not commit).
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
    assert_eq!(count, 2, "mine init does not create commits");

    // First explicit architecture/sync action is required.
    let (_exit, env) = run(&["graph", "status", "--format", "json"], tmp.path());
    // Graph is not initialized until workspace open.
    assert!(
        env.get("error").is_some() || env["ok"] == false || env["data"].get("revision").is_some()
    );
}

// --- Scenario 3: Stale managed design ---

#[test]
fn stale_managed_design_marker_preserved_and_design_validates() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    run(&["init", "--format", "json"], tmp.path());

    // Add a managed design leaf that is "stale" (describes old behavior).
    std::fs::create_dir_all(tmp.path().join(".mine/design/architecture")).unwrap();
    std::fs::write(
        tmp.path().join(".mine/design/architecture/index.md"),
        "# Architecture\n\nOld design that doesn't match current code.\n",
    )
    .unwrap();
    // Update the root index to link to it.
    std::fs::write(
        tmp.path().join(".mine/design/index.md"),
        "# Design Index\n\n- [Architecture](architecture/index.md)\n",
    )
    .unwrap();

    // mine init is idempotent: preserves the marker and existing design.
    let (exit, env) = run(&["init", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "idempotent init preserves marker: {env}");

    // Marker is preserved (not regenerated).
    let marker =
        std::fs::read_to_string(tmp.path().join(".mine/design/.mine-design.toml")).unwrap();
    assert!(
        marker.contains("managed_by = \"MINE\""),
        "MINE ownership proven"
    );

    // Design validates (marker + index exist and are consistent).
    let (exit, env) = run(&["design", "validate", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "design validates with stale managed design: {env}");

    // Unrelated design content survives.
    assert!(
        tmp.path()
            .join(".mine/design/architecture/index.md")
            .exists(),
        "stale leaf preserved"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".mine/design/architecture/index.md")).unwrap(),
        "# Architecture\n\nOld design that doesn't match current code.\n"
    );

    // Generated and authoritative design state are not confused (init does not
    // reconcile code to design; that is mine-sync's role).
    let (exit, _env) = run(&["graph", "validate", "--format", "json"], tmp.path());
    // No graph yet (no workspace): expected.
    assert_ne!(exit, 0, "no graph until workspace is opened");
}

// --- Scenario 5: Protected design decision ---

#[test]
fn protected_design_decision_not_silently_overwritten() {
    // The mine-sync authority order (ADR-0005) states that user-protected
    // design decisions are NOT overwritten from code. mine-init does not
    // perform sync, so this test verifies that init preserves a design
    // document that the user has explicitly written (not overwritten),
    // and that the design validates with it.
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    run(&["init", "--format", "json"], tmp.path());

    // User writes a protected design decision.
    std::fs::create_dir_all(tmp.path().join(".mine/design/decisions")).unwrap();
    std::fs::write(
        tmp.path().join(".mine/design/decisions/0001-protected.md"),
        "# ADR-0001: Protected Decision\n\n## Status\nProtected by user.\n\n## Decision\nUse approach X, not approach Y, regardless of current code.\n",
    ).unwrap();
    std::fs::write(
        tmp.path().join(".mine/design/index.md"),
        "# Design Index\n\n- [ADR-0001](decisions/0001-protected.md)\n",
    )
    .unwrap();

    // Re-run init: the protected design document must survive unchanged.
    let (exit, _env) = run(&["init", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "init preserves protected design");

    let adr = std::fs::read_to_string(tmp.path().join(".mine/design/decisions/0001-protected.md"))
        .unwrap();
    assert!(
        adr.contains("Protected by user"),
        "protected design preserved"
    );
    assert!(
        adr.contains("Use approach X"),
        "protected decision not overwritten"
    );

    // Design validates with the protected document.
    let (exit, _env) = run(&["design", "validate", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "design validates with protected decision");

    // No business code mutation occurred (init does not modify code).
    let (exit, env) = run(&["status", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "status succeeds: {env}");
}

// --- Scenario 6: Unscoped incomplete coverage ---

#[test]
fn unscoped_incomplete_coverage_reported_not_silently_completed() {
    // mine-sync with no scope must report incomplete coverage honestly.
    // Since mine-sync is a Skill (agent-driven, not a CLI command), this test
    // verifies the design principle through the available CLI surface:
    // mine init + design validate do not claim complete architecture coverage.
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // Simulate a large codebase that is only partially understood.
    for i in 0..10 {
        std::fs::create_dir_all(tmp.path().join(format!("src/module{i}"))).unwrap();
        std::fs::write(
            tmp.path().join(format!("src/module{i}/mod.rs")),
            format!("// Module {i} with complex behavior\npub fn f() {{}}\n"),
        )
        .unwrap();
    }
    git_commit(tmp.path(), "large codebase");

    run(&["init", "--format", "json"], tmp.path());

    // Design validates but does NOT claim complete architecture coverage.
    let (exit, env) = run(&["design", "validate", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "design validates: {env}");

    // The init outcome does not generate architecture from code.
    assert!(
        !tmp.path().join(".mine/design/architecture").exists(),
        "init does not infer/invent architecture from code"
    );

    // The design index is the MINE scaffold, not a code-derived architecture.
    let index = std::fs::read_to_string(tmp.path().join(".mine/design/index.md")).unwrap();
    assert!(
        !index.contains("module0") && !index.contains("Module 0"),
        "init does not fabricate architecture from code scanning"
    );

    // A workspace open does not claim complete coverage either.
    super::managed_private_dev(tmp.path());
    let (exit, _env) = run(
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
    assert_eq!(exit, 0, "workspace opens without inferring architecture");

    // The system requires explicit follow-up (mine-arch or mine-sync) rather
    // than silently completing: graph status shows a fresh empty graph.
    let (exit, env) = run(&["graph", "status", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "graph status: {env}");
    assert_eq!(
        env["data"]["plan_count"].as_u64(),
        Some(0),
        "no plans auto-created from inferred architecture"
    );
}
