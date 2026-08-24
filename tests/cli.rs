// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! End-to-end CLI integration tests.
//!
//! These drive `mine::cli::dispatch` against temporary repositories (and the
//! real repository graph for read-only validation) and assert the stable JSON
//! envelope contract, exit codes, human output, optimistic-concurrency
//! behavior, and that no Git mutation occurs.

use std::path::PathBuf;

use mine::cli;
use mine::domain::graph::PlanWorkspace;
use mine::infrastructure::git::GitEvidence;
use mine::infrastructure::toml_store::TomlStore;

use serde_json::Value;

/// Renders an outcome as JSON and parses the envelope. On success the JSON is on
/// stdout; on error it is on stderr (`render` routes error output to stderr so
/// stdout stays clean for pipelines consuming only successful JSON).
fn envelope_json(outcome: &cli::Outcome) -> Value {
    let (stdout, stderr) = cli::render(outcome, true, false);
    let body = if outcome.exit_code == 0 {
        stdout
    } else {
        stderr
    };
    serde_json::from_str(&body).expect("envelope must be valid JSON")
}

fn run(repo: &str, rest: &[&str]) -> Vec<String> {
    let mut v = vec!["mine".to_string(), "--repo".to_string(), repo.to_string()];
    v.extend(rest.iter().map(|s| s.to_string()));
    v
}

fn run_no_repo(rest: &[&str]) -> Vec<String> {
    let mut v = vec!["mine".to_string()];
    v.extend(rest.iter().map(|s| s.to_string()));
    v
}

fn development_graph_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/development-execution-graph.toml")
}

fn init_git_repo(root: &std::path::Path) {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["init", "--quiet"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.email", "t@t"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.name", "t"])
        .status()
        .unwrap();
    std::fs::write(root.join("README.md"), "x\n").unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["add", "README.md"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["commit", "--quiet", "-m", "init"])
        .status()
        .unwrap();
}

#[test]
fn no_subcommand_is_usage_exit_2() {
    let outcome = cli::dispatch(&run_no_repo(&[]), "mine");
    assert_eq!(outcome.exit_code, 2);
    assert_eq!(outcome.command, "usage");
    let env = envelope_json(&outcome);
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "MINE_USAGE");
    let (stdout, _stderr) = cli::render(&outcome, false, false);
    assert!(stdout.is_empty());
}

#[test]
fn unknown_command_is_usage_exit_2() {
    let outcome = cli::dispatch(&run_no_repo(&["bogus"]), "mine");
    assert_eq!(outcome.exit_code, 2);
    let env = envelope_json(&outcome);
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "MINE_USAGE");
}

#[test]
fn init_in_absent_repo_is_idempotent_and_json_stable() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let repo = root.path().to_str().unwrap();

    let outcome1 = cli::dispatch(&run(repo, &["init", "--format", "json"]), "mine");
    assert_eq!(outcome1.exit_code, 0, "init exit 0");
    assert_eq!(outcome1.command, "init");
    let env1 = envelope_json(&outcome1);
    assert_eq!(env1["ok"], true);
    assert_eq!(env1["command"], "init");
    let id = env1["data"]["repository_id"].as_str().unwrap().to_string();
    assert!(!id.is_empty());

    let outcome2 = cli::dispatch(&run(repo, &["init", "--format", "json"]), "mine");
    assert_eq!(outcome2.exit_code, 0);
    let env2 = envelope_json(&outcome2);
    assert_eq!(env2["data"]["repository_id"], env1["data"]["repository_id"]);
    // Deterministic JSON: re-rendering the first outcome is byte-identical.
    assert_eq!(
        cli::render(&outcome1, true, false).0,
        cli::render(&outcome1, true, false).0
    );
}

#[test]
fn init_backs_up_legacy_unmarked_design_root() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let repo = root.path().to_str().unwrap();
    std::fs::create_dir_all(root.path().join("docs").join("design")).unwrap();
    std::fs::write(
        root.path().join("docs").join("design").join("legacy.md"),
        "legacy",
    )
    .unwrap();

    let outcome = cli::dispatch(&run(repo, &["init", "--format", "json"]), "mine");
    assert_eq!(
        outcome.exit_code, 0,
        "init backs up and continues -> exit 0"
    );
    let env = envelope_json(&outcome);
    assert_eq!(env["ok"], true);
    // A backed-up-design action was recorded.
    let actions = env["data"]["actions"].as_array().unwrap();
    let backed_up = actions.iter().any(|a| a["kind"] == "backed-up-design");
    assert!(backed_up, "init recorded a backed-up-design action");
    // The legacy content was moved into a backup directory.
    let docs_dir = root.path().join("docs");
    let backups: Vec<_> = std::fs::read_dir(&docs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("design-backup-")
        })
        .collect();
    assert_eq!(backups.len(), 1, "exactly one design backup created");
    let legacy_preserved = std::fs::read_to_string(backups[0].path().join("legacy.md")).unwrap();
    assert_eq!(legacy_preserved, "legacy");
    // A fresh MINE-managed design root was created.
    assert!(
        root.path()
            .join("docs")
            .join("design")
            .join(".mine-design.toml")
            .exists()
    );
}

#[test]
fn status_reports_graph_and_git_evidence() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let repo = root.path().to_str().unwrap();
    let _ = cli::dispatch(&run(repo, &["init", "--quiet"]), "mine");
    let outcome = cli::dispatch(&run(repo, &["status", "--format", "json"]), "mine");
    assert_eq!(outcome.exit_code, 0);
    let env = envelope_json(&outcome);
    assert_eq!(env["command"], "status");
    assert!(env["data"]["git"].is_object(), "git evidence present");
}

#[test]
fn graph_validate_on_development_fixture_parses_and_validates() {
    let tmp = temp_copy_of_real_graph();
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &["graph", "validate", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(outcome.exit_code, 0, "real graph must validate");
    let env = envelope_json(&outcome);
    assert_eq!(env["ok"], true);
    assert!(env["data"]["plans"].as_u64().unwrap() >= 9);
}

#[test]
fn graph_render_is_deterministic_and_idempotent() {
    // Operate on a temporary development-graph fixture so the test never
    // mutates the stable tree, which intentionally has no graph workspace.
    let tmp = tempfile::tempdir().unwrap();
    let dst = tmp.path();
    std::fs::create_dir_all(dst.join("docs/plan")).unwrap();
    std::fs::copy(
        development_graph_fixture(),
        dst.join("docs/plan/execution-graph.toml"),
    )
    .unwrap();
    let outcome1 = cli::dispatch(
        &run(
            dst.to_str().unwrap(),
            &["graph", "render", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(outcome1.exit_code, 0);
    let store = TomlStore::new(dst);
    let first = std::fs::read_to_string(store.md_path()).unwrap();
    // Second render: byte-identical (deterministic + idempotent).
    let _ = cli::dispatch(
        &run(dst.to_str().unwrap(), &["graph", "render", "--quiet"]),
        "mine",
    );
    let second = std::fs::read_to_string(store.md_path()).unwrap();
    assert_eq!(
        first, second,
        "graph render is deterministic and idempotent"
    );
}

/// Copies the real repository graph to a temp repo so write-path plan tests
/// never touch the live `docs/plan/execution-graph.toml`.
fn temp_copy_of_real_graph() -> tempfile::TempDir {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tmp = tempfile::tempdir().unwrap();
    let clone = std::process::Command::new("git")
        .args(["clone", "--quiet", "--no-checkout"])
        .arg(&repo_root)
        .arg(tmp.path())
        .status()
        .unwrap();
    assert!(clone.success(), "temporary graph repo clone must succeed");
    std::fs::create_dir_all(tmp.path().join("docs/plan")).unwrap();
    std::fs::create_dir_all(tmp.path().join(".mine")).unwrap();
    let cfg = mine::cli::context::load_config(&repo_root).expect("real config exists");
    std::fs::write(tmp.path().join(".mine/config.toml"), cfg.to_toml()).unwrap();
    std::fs::copy(
        development_graph_fixture(),
        tmp.path().join("docs/plan/execution-graph.toml"),
    )
    .unwrap();
    tmp
}

#[test]
fn cli_reads_writes_and_v3_repair_fail_closed_on_a_forged_anchor() {
    let tmp = temp_copy_of_real_graph();
    let toml_path = tmp.path().join("docs/plan/execution-graph.toml");
    let mut workspace =
        toml::from_str::<PlanWorkspace>(&std::fs::read_to_string(&toml_path).unwrap()).unwrap();
    workspace.plans[0].branch_anchor = "HEAD".to_string();
    std::fs::write(&toml_path, toml::to_string(&workspace).unwrap()).unwrap();
    let before = std::fs::read(&toml_path).unwrap();

    let revision = workspace.revision.to_string();
    let commands = [
        vec!["graph", "validate", "--format", "json"],
        vec!["graph", "status", "--format", "json"],
        vec!["plan", "start", "--id", "02-1", "--format", "json"],
        vec![
            "graph",
            "repair-provenance",
            "--expected-revision",
            revision.as_str(),
            "--format",
            "json",
        ],
    ];
    for command in commands {
        let outcome = cli::dispatch(&run(tmp.path().to_str().unwrap(), &command), "mine");
        let env = envelope_json(&outcome);
        assert_eq!(outcome.exit_code, 4, "command {command:?}: {env}");
        assert_eq!(env["error"]["code"], "MINE_GRAPH_INVALID");
        assert_eq!(std::fs::read(&toml_path).unwrap(), before);
    }
}

#[test]
fn plan_start_refuses_non_ready_plan() {
    // Node is ACCEPTED (terminal): `plan start` must be refused. Operate
    // on a TEMP COPY so the write path is never exercised against the live
    // repository graph. We snapshot the live graph bytes before/after and
    // assert they are unchanged (independent of the current revision number).
    let live_path = development_graph_fixture();
    let live_before = std::fs::read_to_string(&live_path).unwrap();
    let tmp = temp_copy_of_real_graph();
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &["plan", "start", "--id", "02-1", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(
        outcome.exit_code, 4,
        "non-ready transition -> validation exit 4"
    );
    let env = envelope_json(&outcome);
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "MINE_INVALID_TRANSITION");
    // Live graph byte-unchanged.
    let live_after = std::fs::read_to_string(&live_path).unwrap();
    assert_eq!(
        live_before, live_after,
        "live graph unchanged by the temp-copy test"
    );
}

#[test]
fn plan_accept_requires_implemented_state() {
    // `plan accept` must be refused for a plan that is NOT `IMPLEMENTED`. We
    // operate on a TEMP COPY of the real graph and inject an IN_PROGRESS
    // synthetic plan node so the accept write path is exercised against the
    // copy only, never the live repository graph. Snapshot the live graph
    // bytes before/after to assert the live graph is byte-unchanged
    // (independent of the bootstrap revision number).
    let live_path = development_graph_fixture();
    let live_before = std::fs::read_to_string(&live_path).unwrap();
    let tmp = temp_copy_of_real_graph();
    let toml_path = tmp.path().join("docs/plan/execution-graph.toml");
    let mut ws = toml::from_str::<mine::domain::graph::PlanWorkspace>(
        &std::fs::read_to_string(&toml_path).unwrap(),
    )
    .unwrap();
    use mine::domain::graph::PlanNode;
    use mine::domain::status::PlanStatus;
    let injected_revision = ws.revision;
    // Node is ACCEPTED; reuse it as a resolved hard predecessor. The
    // synthetic plan is IN_PROGRESS (not IMPLEMENTED); accepting it must fail.
    ws.plans.push(PlanNode {
        id: "99-test".to_string(),
        path: "docs/plan/99-test.md".to_string(),
        title: "99-test".to_string(),
        branch: String::new(),
        branch_anchor: String::new(),
        status: PlanStatus::InProgress,
        hard_predecessors: vec!["02-1".to_string()],
        soft_predecessors: vec![],
        design_references: vec!["docs/design/principles.md".to_string()],
        exclusive_write_paths: vec!["tests/noop/".to_string()],
        read_only_paths: vec![],
        reserved_shared_paths: vec![],
        implementation_report: String::new(),
        review_report: String::new(),
        implementation_commits: vec![],
        owner: "tester".to_string(),
        run_id: "test".to_string(),
        started_at: "2026-07-23T00:00:00Z".to_string(),
        updated_at: "2026-07-23T00:00:00Z".to_string(),
        rejection_reason: String::new(),
        compensating_plan: String::new(),
        lineage_id: String::new(),
        worktree_path: String::new(),
    });
    std::fs::write(&toml_path, toml::to_string(&ws).unwrap()).unwrap();

    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &[
                "plan", "accept", "--id", "99-test", "--review", "none.md", "--format", "json",
            ],
        ),
        "mine",
    );
    assert_eq!(outcome.exit_code, 4);
    let env = envelope_json(&outcome);
    assert_eq!(env["error"]["code"], "MINE_INVALID_TRANSITION");
    // Temp copy unchanged (rejected transition must not mutate): the injected
    // 99-test plan is still present, and the revision string is unchanged
    // (compare the bytes captured before dispatching).
    let after = std::fs::read_to_string(&toml_path).unwrap();
    let after_ws = toml::from_str::<mine::domain::graph::PlanWorkspace>(&after).unwrap();
    assert_eq!(
        after_ws.revision, injected_revision,
        "temp graph revision unchanged by rejected accept"
    );
    assert!(after.contains("99-test"));
    // Live graph fully untouched (byte snapshot before/after), and the injected
    // test plan did not leak to it.
    let live_after = std::fs::read_to_string(&live_path).unwrap();
    assert_eq!(live_before, live_after, "live graph byte-unchanged");
    assert!(!live_after.contains("99-test"));
}

#[test]
fn plan_lifecycle_implemented_accept_releases_successor() {
    // Independent-review addition (reviewer-owned; tests/cli.rs is a
    // exclusive path). The implementation's own suite covered only the
    // *negative* plan-lifecycle CLI paths (refused start/accept). This test
    // exercises the post-start positive path end to end through the real CLI
    // dispatcher: implemented -> accept, asserting that acceptance
    // releases a BLOCKED successor to READY only once its complete
    // hard-predecessor set is satisfied. Operates entirely on a TEMP COPY of
    // the real graph with two synthetic nodes; the live repository graph is
    // byte-snapshotted before and after and asserted unchanged, and the
    // injected node ids are asserted never to leak into it.
    let live_path = development_graph_fixture();
    let live_before = std::fs::read_to_string(&live_path).unwrap();
    let tmp = temp_copy_of_real_graph();
    let toml_path = tmp.path().join("docs/plan/execution-graph.toml");

    // Inject an IN_PROGRESS node whose sole hard predecessor ("02-1") is
    // already ACCEPTED in the real graph, and a BLOCKED successor gated on it.
    // Real-Git start/anchor capture is covered in tests/plan_prepare.rs.
    let mut ws =
        toml::from_str::<PlanWorkspace>(&std::fs::read_to_string(&toml_path).unwrap()).unwrap();
    use mine::domain::graph::PlanNode;
    use mine::domain::status::PlanStatus;
    let blank_node = |id: &str, status: PlanStatus, hard: Vec<&str>| PlanNode {
        id: id.to_string(),
        path: format!("docs/plan/{id}-test.md"),
        title: format!("{id}-test"),
        branch: String::new(),
        branch_anchor: String::new(),
        status,
        hard_predecessors: hard.into_iter().map(str::to_string).collect(),
        soft_predecessors: vec![],
        design_references: vec!["docs/design/principles.md".to_string()],
        exclusive_write_paths: vec![format!("tests/noop-{id}/")],
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
    };
    ws.plans.push(blank_node(
        "99-lifecycle",
        PlanStatus::InProgress,
        vec!["02-1"],
    ));
    ws.plans.push(blank_node(
        "99-successor",
        PlanStatus::Blocked,
        vec!["99-lifecycle"],
    ));
    std::fs::write(&toml_path, toml::to_string(&ws).unwrap()).unwrap();

    // 1) implemented: IN_PROGRESS -> IMPLEMENTED.
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &[
                "plan",
                "implemented",
                "--id",
                "99-lifecycle",
                "--report",
                "docs/plan/reports/99-lifecycle-implementation.md",
                "--commit",
                "deadbeefcafebabe0000000000000000000000",
                "--format",
                "json",
            ],
        ),
        "mine",
    );
    assert_eq!(
        outcome.exit_code, 0,
        "implemented must succeed: {outcome:?}"
    );
    let after_impl =
        toml::from_str::<PlanWorkspace>(&std::fs::read_to_string(&toml_path).unwrap()).unwrap();
    assert_eq!(
        after_impl.get("99-lifecycle").unwrap().status,
        PlanStatus::Implemented
    );

    // 2) accept: IMPLEMENTED -> ACCEPTED, releasing the BLOCKED successor to
    //    READY (its sole hard predecessor is now accepted).
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &[
                "plan",
                "accept",
                "--id",
                "99-lifecycle",
                "--review",
                "docs/plan/reports/99-lifecycle-review.md",
                "--format",
                "json",
            ],
        ),
        "mine",
    );
    assert_eq!(outcome.exit_code, 0, "accept must succeed: {outcome:?}");
    let env = envelope_json(&outcome);
    assert_eq!(env["ok"], true);
    let after_accept =
        toml::from_str::<PlanWorkspace>(&std::fs::read_to_string(&toml_path).unwrap()).unwrap();
    assert_eq!(
        after_accept.get("99-lifecycle").unwrap().status,
        PlanStatus::Accepted,
        "target reaches ACCEPTED"
    );
    assert_eq!(
        after_accept.get("99-successor").unwrap().status,
        PlanStatus::Ready,
        "successor released to READY once its full hard-predecessor set is ACCEPTED"
    );

    // Live graph fully untouched (byte snapshot before/after); the injected
    // synthetic ids never leak into it.
    let live_after = std::fs::read_to_string(&live_path).unwrap();
    assert_eq!(live_before, live_after, "live graph byte-unchanged");
    assert!(!live_after.contains("99-lifecycle"));
    assert!(!live_after.contains("99-successor"));
}

#[test]
fn workspace_status_on_development_fixture_is_idempotent() {
    let tmp = temp_copy_of_real_graph();
    let repo_root = tmp.path().to_path_buf();
    let store = TomlStore::new(&repo_root);
    let before = store.load().unwrap();
    let outcome = cli::dispatch(
        &run(
            repo_root.to_str().unwrap(),
            &["workspace", "status", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(outcome.exit_code, 0);
    let env = envelope_json(&outcome);
    assert_eq!(env["data"]["workspace_id"], before.workspace_id);
    assert_eq!(env["data"]["revision"], before.revision);
}

#[test]
fn repository_version_show_round_trips() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cfg = mine::cli::context::load_config(&repo_root).expect("config present");
    let outcome = cli::dispatch(
        &run(
            repo_root.to_str().unwrap(),
            &["repository", "version", "show", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(outcome.exit_code, 0);
    let env = envelope_json(&outcome);
    assert_eq!(env["data"]["version"], cfg.mine_code_version);
}

#[test]
fn cli_performs_no_git_mutation() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let before_clean = GitEvidence::collect(&repo_root).clean;
    for cmd in [
        ["status", "--format", "json"].as_slice(),
        ["design", "status", "--format", "json"].as_slice(),
    ] {
        let outcome = cli::dispatch(&run(repo_root.to_str().unwrap(), cmd), "mine");
        assert_eq!(outcome.exit_code, 0, "cmd {:?}", cmd);
    }
    let after_clean = GitEvidence::collect(&repo_root).clean;
    assert_eq!(before_clean, after_clean, "CLI made no Git mutation");
}

#[test]
fn revision_conflict_surfaces_exit_5() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let repo = root.path().to_str().unwrap();
    let _ = cli::dispatch(&run(repo, &["init", "--quiet"]), "mine");
    let store = TomlStore::new(root.path());
    let ws = PlanWorkspace {
        schema_version: mine::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
        revision: 3,
        project_id: "p".to_string(),
        workspace_id: "ws".to_string(),
        stable_branch: "master".to_string(),
        integration_branch: "dev".to_string(),
        stable_baseline_commit: "c".to_string(),
        design_root: "docs/design/index.md".to_string(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans: vec![],
    };
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();

    let err = store.save_with_revision(2, Ok).unwrap_err();
    let he = mine::cli::HandlerError::from_mine(&err);
    assert_eq!(he.exit_code, mine::output::exit_code::CONFLICT);
    assert_eq!(he.code, "MINE_REVISION_CONFLICT");
}

#[test]
fn design_backup_round_trip_and_gitignore() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let repo = root.path().to_str().unwrap();
    let _ = cli::dispatch(&run(repo, &["init", "--quiet"]), "mine");
    std::fs::write(root.path().join("docs/design/leaf.md"), "# Leaf\n").unwrap();

    let outcome = cli::dispatch(
        &run(repo, &["design", "backup", "--format", "json"]),
        "mine",
    );
    assert_eq!(outcome.exit_code, 0);
    let env = envelope_json(&outcome);
    let backup_rel = env["data"]["backup_path"].as_str().unwrap().to_string();
    assert!(backup_rel.starts_with("docs/design-backup-"));
    let backup = root.path().join(&backup_rel);
    assert!(backup.join("index.md").exists());
    assert!(backup.join("leaf.md").exists());
    assert_eq!(
        std::fs::read_to_string(backup.join(".gitignore")).unwrap(),
        "*\n"
    );
}

#[test]
fn json_envelope_has_stable_sorted_keys() {
    let tmp = temp_copy_of_real_graph();
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &["graph", "ready", "--format", "json"],
        ),
        "mine",
    );
    let body = cli::render(&outcome, true, false).0;
    // The envelope serializes with sorted keys: "command" is alphabetically
    // first, so a stable serializer emits it as the first object key.
    assert!(body.starts_with(r#"{"command":"#));
}

// ---------------------------------------------------------------------------
// Generic release portability and branch handling regression tests.
// ---------------------------------------------------------------------------

/// Initializes a git repo whose default branch is `branch_name`.
fn init_git_repo_with_branch(root: &std::path::Path, branch_name: &str) {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["init", "--quiet", "-b", branch_name])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.email", "t@t"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.name", "t"])
        .status()
        .unwrap();
    std::fs::write(root.join("README.md"), "x\n").unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["add", "README.md"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["commit", "--quiet", "-m", "init"])
        .status()
        .unwrap();
}

#[test]
fn init_persists_detected_main_branch() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo_with_branch(tmp.path(), "main");
    let outcome = cli::dispatch(
        &run(tmp.path().to_str().unwrap(), &["init", "--format", "json"]),
        "mine",
    );
    assert_eq!(
        outcome.exit_code,
        0,
        "init must succeed: {}",
        envelope_json(&outcome)
    );
    let env = envelope_json(&outcome);
    assert_eq!(
        env["data"]["stable_branch"], "main",
        "init JSON must report detected main branch"
    );
    // The persisted config must record main, not master.
    let config_path = tmp.path().join(".mine/config.toml");
    let config_text = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        config_text.contains("stable = \"main\""),
        "config must persist main: {config_text}"
    );
    assert!(
        !config_text.contains("stable = \"master\""),
        "config must not hardcode master: {config_text}"
    );
}

#[test]
fn init_persists_master_when_default_branch_is_master() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo_with_branch(tmp.path(), "master");
    let outcome = cli::dispatch(
        &run(tmp.path().to_str().unwrap(), &["init", "--format", "json"]),
        "mine",
    );
    assert_eq!(
        outcome.exit_code,
        0,
        "init must succeed: {}",
        envelope_json(&outcome)
    );
    let config_path = tmp.path().join(".mine/config.toml");
    let config_text = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        config_text.contains("stable = \"master\""),
        "config must persist master: {config_text}"
    );
}

#[test]
fn init_persists_explicit_custom_stable_branch() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo_with_branch(tmp.path(), "trunk");
    let outcome = cli::dispatch(
        &run(tmp.path().to_str().unwrap(), &["init", "--format", "json"]),
        "mine",
    );
    assert_eq!(
        outcome.exit_code,
        0,
        "init must succeed: {}",
        envelope_json(&outcome)
    );
    let env = envelope_json(&outcome);
    assert_eq!(
        env["data"]["stable_branch"], "trunk",
        "custom branch must be reported"
    );
    let config_path = tmp.path().join(".mine/config.toml");
    let config_text = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        config_text.contains("stable = \"trunk\""),
        "config must persist trunk: {config_text}"
    );
}

#[test]
fn external_repo_without_skills_passes_distribution_gate() {
    // A minimal non-MINE repository: git repo, no skills/, no plugins/.
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // Initialize MINE so config + design exist.
    let outcome = cli::dispatch(
        &run(tmp.path().to_str().unwrap(), &["init", "--format", "json"]),
        "mine",
    );
    assert_eq!(
        outcome.exit_code,
        0,
        "init must succeed: {}",
        envelope_json(&outcome)
    );
    // Open the workspace on the integration branch so the stable branch stays
    // clean (mirroring the real MINE flow) and a terminal graph exists.
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["checkout", "--quiet", "-b", "dev"])
        .status()
        .unwrap();
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &["workspace", "open", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(
        outcome.exit_code,
        0,
        "workspace open must succeed: {}",
        envelope_json(&outcome)
    );
    // Commit so the tree is clean, then run the full preflight.
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["add", "-A"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["commit", "--quiet", "-m", "workspace"])
        .status()
        .unwrap();
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &["release", "--format", "json"],
        ),
        "mine",
    );
    let env = envelope_json(&outcome);
    assert_eq!(
        outcome.exit_code, 0,
        "release preflight must succeed: {}",
        env
    );
    assert_eq!(
        env["data"]["distribution_synced"], true,
        "no skills/ means parity is vacuously true: {}",
        env
    );
    assert_eq!(
        env["data"]["can_release"], true,
        "generic repo without MINE assets must be releasable: {}",
        env
    );
}
