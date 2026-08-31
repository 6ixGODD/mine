// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! `mine plan withdraw` integration tests (ADR-0012).
//!
//! Each test builds an isolated temporary Git repository and drives the real
//! CLI dispatch. The lossless-cleanup matrix follows
//! `docs/design/execution-graph/state-machine-and-algorithms.md#plan-withdrawal`:
//! ancestor/equality tip proofs and a clean worktree succeed; divergence,
//! dirt, and missing evidence refuse (exit 6) with nothing written.

#![allow(dead_code)]

mod common;

use std::path::{Path, PathBuf};

use mine::cli;
use mine::domain::graph::PlanWorkspace;
use mine::infrastructure::toml_store::TomlStore;

fn dispatch(repo: &Path, rest: &[&str]) -> cli::Outcome {
    let argv = common::run(repo.to_str().unwrap(), rest);
    cli::dispatch(&argv, "mine")
}

fn envelope(outcome: &cli::Outcome) -> serde_json::Value {
    common::envelope_json(outcome)
}

fn git(repo: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A repository on the managed `dev` branch with an open plan workspace.
fn managed_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_path_buf();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("README.md"), "fixture\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "/.mine/\n").unwrap();
    git(&repo, &["add", "README.md", ".gitignore"]);
    git(&repo, &["commit", "--quiet", "-m", "initial"]);
    assert_eq!(dispatch(&repo, &["init"]).exit_code, 0);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "mine init"]);
    git(&repo, &["switch", "-c", "dev"]);
    assert_eq!(dispatch(&repo, &["workspace", "open"]).exit_code, 0);
    (tmp, repo)
}

/// Registers one canonical plan as DRAFT (`register` only) and returns its id.
fn register_plan(repo: &Path, id: &str) {
    let path = format!("docs/plan/{id}-fixture.md");
    std::fs::write(repo.join(&path), format!("# {id}\n")).unwrap();
    let outcome = dispatch(
        repo,
        &[
            "plan",
            "add",
            "--id",
            id,
            "--path",
            &path,
            "--title",
            "fixture",
            "--design-ref",
            "docs/design/index.md",
            "--write",
            "src/",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "plan add {id} failed");
}

fn release_plan(repo: &Path, id: &str) {
    let outcome = dispatch(repo, &["plan", "release", "--id", id]);
    assert_eq!(outcome.exit_code, 0, "plan release {id} failed");
}

fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "--quiet", "-m", message]);
}

fn graph(repo: &Path) -> PlanWorkspace {
    TomlStore::new(repo).load().unwrap()
}

fn worktree_of(repo: &Path, id: &str) -> PathBuf {
    repo.join(&graph(repo).get(id).unwrap().worktree_path)
}

#[test]
fn draft_withdraw_is_graph_only_with_deterministic_envelope() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    let before = graph(&repo);
    let outcome = dispatch(
        &repo,
        &[
            "plan",
            "withdraw",
            "--id",
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            "plan02",
            "--reason",
            "superseded design",
        ],
    );
    assert_eq!(outcome.exit_code, 0);
    let env = envelope(&outcome);
    assert_eq!(env["command"], "plan.withdraw");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    assert_eq!(env["data"]["plan"], "plan02");
    assert_eq!(env["data"]["status_before"], "DRAFT");
    assert_eq!(env["data"]["status_after"], "WITHDRAWN");
    assert_eq!(env["data"]["cleaned_git_artifacts"], false);
    assert!(
        env["data"]["withdrawn_at"]
            .as_str()
            .is_some_and(|t| !t.is_empty())
    );
    assert_eq!(env["revision_after"], json_u64(&env["revision_before"]) + 1);
    let after = graph(&repo);
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let node = after.get("plan02").unwrap();
    assert_eq!(node.status.as_str(), "WITHDRAWN");
    assert_eq!(node.withdraw_reason, "superseded design");
    assert!(!node.withdrawn_at.is_empty());
    assert_eq!(after.revision, before.revision + 1, "revision bumped once");
    // Not idempotent-success: re-running refuses and writes nothing.
    let rerun = dispatch(
        &repo,
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        &["plan", "withdraw", "--id", "plan02", "--reason", "again"],
    );
    assert_eq!(rerun.exit_code, 4);
    let env = envelope(&rerun);
    assert_eq!(env["error"]["code"], "MINE_INVALID_TRANSITION");
    assert_eq!(graph(&repo).revision, after.revision);
    drop(tmp);
}

fn json_u64(v: &serde_json::Value) -> u64 {
    v.as_u64().unwrap()
}

#[test]
fn empty_reason_is_refused_without_writes() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    let before = std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap();
    let outcome = dispatch(
        &repo,
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        &["plan", "withdraw", "--id", "plan02", "--reason", "  "],
    );
    assert_eq!(outcome.exit_code, 4);
    let env = envelope(&outcome);
    assert_eq!(env["error"]["code"], "MINE_GRAPH_INVALID");
    assert_eq!(
        std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap(),
        before
    );
    drop(tmp);
}

#[test]
fn started_plan_cannot_be_withdrawn() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    release_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    commit_all(&repo, "register plan02");
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        dispatch(&repo, &["plan", "prepare", "--id", "plan02"]).exit_code,
        0
    );
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        dispatch(&repo, &["plan", "start", "--id", "plan02"]).exit_code,
        0
    );
    let outcome = dispatch(
        &repo,
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        &["plan", "withdraw", "--id", "plan02", "--reason", "too late"],
    );
    assert_eq!(outcome.exit_code, 4);
    let env = envelope(&outcome);
    assert_eq!(env["error"]["code"], "MINE_INVALID_TRANSITION");
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        graph(&repo).get("plan02").unwrap().status.as_str(),
        "IN_PROGRESS"
    );
    drop(tmp);
}

#[test]
fn ready_unprepared_plan_withdraws_graph_only() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    release_plan(&repo, "plan02");
    // Never prepared: the registered branch was never created in Git.
    let outcome = dispatch(
        &repo,
        &[
            "plan",
            "withdraw",
            "--id",
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            "plan02",
            "--reason",
            "stale registration",
        ],
    );
    assert_eq!(outcome.exit_code, 0);
    let env = envelope(&outcome);
    assert_eq!(env["data"]["status_before"], "READY");
    assert_eq!(env["data"]["cleaned_git_artifacts"], false);
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        graph(&repo).get("plan02").unwrap().status.as_str(),
        "WITHDRAWN"
    );
    drop(tmp);
}

#[test]
fn ready_prepared_lane_cleanup_is_lossless() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    release_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    commit_all(&repo, "register plan02");
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        dispatch(&repo, &["plan", "prepare", "--id", "plan02"]).exit_code,
        0
    );
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let worktree = worktree_of(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let branch = graph(&repo).get("plan02").unwrap().branch.clone();
    assert!(worktree.exists());

    let outcome = dispatch(
        &repo,
        &[
            "plan",
            "withdraw",
            "--id",
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            "plan02",
            "--reason",
            "architecture replaced",
        ],
    );
    assert_eq!(outcome.exit_code, 0);
    let env = envelope(&outcome);
    assert_eq!(env["data"]["cleaned_git_artifacts"], true);
    assert!(!worktree.exists(), "linked worktree removed");
    assert!(
        !git_branch_exists(&repo, &branch),
        "never-started branch deleted"
    );
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        graph(&repo).get("plan02").unwrap().status.as_str(),
        "WITHDRAWN"
    );
    drop(tmp);
}

fn git_branch_exists(repo: &Path, branch: &str) -> bool {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .unwrap();
    out.status.success() && !out.stdout.is_empty()
}

#[test]
fn diverged_tip_refuses_exit_6_and_preserves_artifacts() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    release_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    commit_all(&repo, "register plan02");
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        dispatch(&repo, &["plan", "prepare", "--id", "plan02"]).exit_code,
        0
    );
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let worktree = worktree_of(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let branch = graph(&repo).get("plan02").unwrap().branch.clone();
    // Unique work on the plan branch: its tip diverges from integration.
    std::fs::write(worktree.join("unique.txt"), "never committed to dev\n").unwrap();
    git(&worktree, &["add", "unique.txt"]);
    git(&worktree, &["commit", "--quiet", "-m", "unique work"]);
    let before = std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap();

    let outcome = dispatch(
        &repo,
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        &["plan", "withdraw", "--id", "plan02", "--reason", "stale"],
    );
    assert_eq!(
        outcome.exit_code, 6,
        "diverged tip is a Git-evidence refusal"
    );
    let env = envelope(&outcome);
    assert_eq!(env["error"]["code"], "MINE_GIT_EVIDENCE");
    assert_eq!(
        std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap(),
        before,
        "nothing written"
    );
    assert!(worktree.exists(), "worktree preserved");
    assert!(git_branch_exists(&repo, &branch), "branch preserved");
    drop(tmp);
}

#[test]
fn dirty_worktree_refuses_exit_6_and_preserves_artifacts() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    release_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    commit_all(&repo, "register plan02");
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        dispatch(&repo, &["plan", "prepare", "--id", "plan02"]).exit_code,
        0
    );
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let worktree = worktree_of(&repo, "plan02");
    std::fs::write(worktree.join("scratch.txt"), "uncommitted\n").unwrap();
    let before = std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap();

    let outcome = dispatch(
        &repo,
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        &["plan", "withdraw", "--id", "plan02", "--reason", "stale"],
    );
    assert_eq!(outcome.exit_code, 6);
    let env = envelope(&outcome);
    assert_eq!(env["error"]["code"], "MINE_GIT_EVIDENCE");
    assert_eq!(
        std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap(),
        before
    );
    drop(tmp);
}

#[test]
fn draft_with_stray_branch_artifacts_refuses() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let branch = graph(&repo).get("plan02").unwrap().branch.clone();
    // Stray Git artifact on an unstarted registration: inconsistent state.
    git(&repo, &["branch", &branch, "dev"]);
    let before = std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap();

    let outcome = dispatch(
        &repo,
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        &["plan", "withdraw", "--id", "plan02", "--reason", "stale"],
    );
    assert_eq!(outcome.exit_code, 4);
    let env = envelope(&outcome);
    assert_eq!(env["error"]["code"], "MINE_GRAPH_INVALID");
    assert_eq!(
        std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap(),
        before
    );
    drop(tmp);
}

#[test]
fn withdrawn_predecessor_keeps_successor_unsatisfied() {
    let (tmp, repo) = managed_repo();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    register_plan(&repo, "plan02");
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    // plan04 depends on plan02.
    let outcome = dispatch(
        &repo,
        &[
            "plan",
            "add",
            "--id",
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            "plan04",
            "--path",
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            "docs/plan/plan04-fixture.md",
            "--title",
            "successor",
            "--design-ref",
            "docs/design/index.md",
            "--write",
            "tests/",
            "--hard",
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            "plan02",
        ],
    );
    assert_eq!(outcome.exit_code, 0);
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    std::fs::write(repo.join("docs/plan/plan04-fixture.md"), "# plan04\n").unwrap();
    assert_eq!(
        dispatch(
            &repo,
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            &["plan", "withdraw", "--id", "plan02", "--reason", "retired"]
        )
        .exit_code,
        0
    );
    // The successor releases to BLOCKED: a withdrawn predecessor is never
    // satisfied.
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    release_plan(&repo, "plan04");
    assert_eq!(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        graph(&repo).get("plan04").unwrap().status.as_str(),
        "BLOCKED"
    );
    drop(tmp);
}

#[test]
fn closure_accepts_withdrawn_and_rejected_but_not_unresolved() {
    use mine::domain::status::PlanStatus;
    // A graph whose only non-ACCEPTED node is WITHDRAWN closes cleanly.
    let (tmp, repo) = common::seeded_repo(vec![
        common::node("01", PlanStatus::Accepted, &[], &[]),
        common::node("02", PlanStatus::Withdrawn, &[], &[]),
    ]);
    let outcome = dispatch(&repo, &["workspace", "close"]);
    assert_eq!(outcome.exit_code, 0, "withdrawn-only remainder closes");
    assert_eq!(envelope(&outcome)["data"]["closable"], true);
    drop(tmp);

    // A READY node still blocks closure and the message names its status.
    let (tmp, repo) = common::seeded_repo(vec![
        common::node("01", PlanStatus::Accepted, &[], &[]),
        common::node("02", PlanStatus::Withdrawn, &[], &[]),
        common::node("03", PlanStatus::Ready, &[], &[]),
    ]);
    let outcome = dispatch(&repo, &["workspace", "close"]);
    assert_eq!(outcome.exit_code, 4);
    let env = envelope(&outcome);
    let detail = env["error"]["message"].as_str().unwrap();
    assert!(
        detail.contains("READY"),
        "unresolved statuses listed: {detail}"
    );
    assert!(
        !detail.contains("WITHDRAWN"),
        "withdrawn not listed: {detail}"
    );
    drop(tmp);
}

#[test]
fn ready_frontier_and_wave_exclude_withdrawn_nodes() {
    use mine::domain::status::PlanStatus;
    let (tmp, repo) = common::seeded_repo(vec![
        common::node("01", PlanStatus::Withdrawn, &[], &[]),
        common::node("02", PlanStatus::Ready, &[], &[]),
    ]);
    let outcome = dispatch(&repo, &["graph", "ready", "--format", "json"]);
    assert_eq!(outcome.exit_code, 0);
    let env = envelope(&outcome);
    let ready = env["data"]["ready"].to_string();
    assert!(
        ready.contains("02") && !ready.contains("01"),
        "ready: {ready}"
    );
    let outcome = dispatch(&repo, &["graph", "wave", "--format", "json"]);
    assert_eq!(outcome.exit_code, 0);
    let env = envelope(&outcome);
    let wave = env["data"]["wave"].to_string();
    assert!(wave.contains("02") && !wave.contains("01"), "wave: {wave}");
    drop(tmp);
}
