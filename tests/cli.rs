// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! End-to-end CLI integration tests.
//!
//! These drive `mine::cli::dispatch` against private repositories and assert
//! envelopes, refusal boundaries and native checkpoint/recovery behavior.

// This binary uses a subset of the shared private-repository fixture helpers.
#[allow(dead_code)]
mod common;

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
    init_git_repo_with_branch(root, "master");
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
fn init_preserves_unrelated_user_design_without_claiming_or_moving_it() {
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
    assert_eq!(outcome.exit_code, 0, "{}", envelope_json(&outcome));
    let env = envelope_json(&outcome);
    assert_eq!(env["ok"], true);
    // User docs are not the configured owned namespace and must remain untouched.
    let actions = env["data"]["actions"].as_array().unwrap();
    let backed_up = actions.iter().any(|a| a["kind"] == "backed-up-design");
    assert!(!backed_up, "init must not move unrelated user docs");
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
    assert!(backups.is_empty(), "no unsolicited user-document backup");
    assert_eq!(
        std::fs::read(root.path().join("docs/design/legacy.md")).unwrap(),
        b"legacy"
    );
    assert!(!root.path().join("docs/design/.mine-design.toml").exists());
    // Only the canonical managed namespace was created.
    assert!(
        root.path()
            .join(".mine")
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
fn graph_validate_detects_missing_view_and_render_repairs() {
    let tmp = checkpointed_fixture();
    let store = TomlStore::new(tmp.path());
    let source = std::fs::read(store.toml_path()).unwrap();
    let head = fixture_git(tmp.path(), &["rev-parse", "HEAD"]);
    std::fs::remove_file(store.md_path()).unwrap();
    let invalid = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &["graph", "validate", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(invalid.exit_code, 4);
    let envelope = envelope_json(&invalid);
    let detail = envelope["error"]["message"].as_str().unwrap();
    assert!(
        detail.contains("generated view missing") && detail.contains("mine graph render"),
        "{detail}"
    );
    let restored = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &[
                "graph",
                "render",
                "--expected-revision",
                "73",
                "--format",
                "json",
            ],
        ),
        "mine",
    );
    assert_eq!(restored.exit_code, 0, "{}", envelope_json(&restored));
    let validated = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &["graph", "validate", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(validated.exit_code, 0, "{}", envelope_json(&validated));
    assert_eq!(envelope_json(&validated)["data"]["plans"], 9);
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), source);
    assert_eq!(fixture_git(tmp.path(), &["rev-parse", "HEAD"]), head);
    let mut foreign = std::fs::read(store.md_path()).unwrap();
    foreign.extend_from_slice(b"hand-edited\n");
    std::fs::write(store.md_path(), &foreign).unwrap();
    let invalid = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &["graph", "validate", "--format", "json"],
        ),
        "mine",
    );
    assert_eq!(invalid.exit_code, 4);
    assert!(
        envelope_json(&invalid)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("generated view stale")
    );
    let refused = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &[
                "graph",
                "render",
                "--expected-revision",
                "73",
                "--format",
                "json",
            ],
        ),
        "mine",
    );
    assert_ne!(refused.exit_code, 0, "foreign view must not be overwritten");
    assert_eq!(std::fs::read(store.md_path()).unwrap(), foreign);
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), source);
    assert_eq!(fixture_git(tmp.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn graph_render_is_deterministic_and_idempotent() {
    let tmp = checkpointed_fixture();
    let store = TomlStore::new(tmp.path());
    let first = std::fs::read(store.md_path()).unwrap();
    let source = std::fs::read(store.toml_path()).unwrap();
    let head = fixture_git(tmp.path(), &["rev-parse", "HEAD"]);
    for _ in 0..2 {
        let outcome = cli::dispatch(
            &run(
                tmp.path().to_str().unwrap(),
                &[
                    "graph",
                    "render",
                    "--expected-revision",
                    "73",
                    "--format",
                    "json",
                ],
            ),
            "mine",
        );
        assert_eq!(outcome.exit_code, 0, "{}", envelope_json(&outcome));
        assert_eq!(std::fs::read(store.md_path()).unwrap(), first);
        assert_eq!(std::fs::read(store.toml_path()).unwrap(), source);
        assert_eq!(fixture_git(tmp.path(), &["rev-parse", "HEAD"]), head);
    }
}

/// Every fixture owns real Git, canonical bodies and its own graph identity.
fn managed_graph_fixture() -> tempfile::TempDir {
    use mine::domain::status::PlanStatus::*;
    common::seeded_repo(vec![
        // mine-release-allow-plan-reference: private canonical graph
        common::node("plan01", Accepted, &[], &[]),
        // mine-release-allow-plan-reference: private rejected lineage
        common::rejected_node("plan02", "plan02-C01", &["plan01"]),
        // mine-release-allow-plan-reference: inherited predecessors
        common::node("plan02-C01", Accepted, &["plan01"], &[]),
        // mine-release-allow-plan-reference: accepted dependency
        common::node("plan03", Accepted, &["plan02-C01"], &[]),
        // mine-release-allow-plan-reference: ready frontier
        common::node("plan04", Ready, &["plan03"], &[]),
        // mine-release-allow-plan-reference: native checkpoint producer
        common::node("plan05", Draft, &["plan03"], &[]),
        // mine-release-allow-plan-reference: blocked dependency
        common::node("plan06", Blocked, &["plan05"], &[]),
        // mine-release-allow-plan-reference: disjoint ready scope
        common::node("plan07", Ready, &[], &[]),
        // mine-release-allow-plan-reference: disjoint ready scope
        common::node("plan08", Ready, &[], &[]),
    ])
    .0
}

fn fixture_git(root: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn checkpointed_fixture() -> tempfile::TempDir {
    let directory = managed_graph_fixture();
    let outcome = cli::dispatch(
        &run(
            directory.path().to_str().unwrap(),
            &[
                // mine-release-allow-plan-reference: real checkpoint before view restoration
                "plan",
                "release",
                "--id",
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan05",
                "--actor",
                "fixture-coordinator",
                "--expected-revision",
                "72",
            ],
        ),
        "mine",
    );
    assert_eq!(outcome.exit_code, 0, "{}", envelope_json(&outcome));
    directory
}

fn started_fixture() -> tempfile::TempDir {
    use mine::domain::status::PlanStatus::*;
    let directory = common::seeded_repo(vec![
        // mine-release-allow-plan-reference: real prepared/startable parent
        common::node("plan99", Ready, &[], &[]),
        // mine-release-allow-plan-reference: successor depends on acceptance
        common::node("plan99-01", Blocked, &["plan99"], &[]),
    ])
    .0;
    let operation = uuid::Uuid::new_v4().to_string();
    for args in [
        // mine-release-allow-plan-reference: actual preparation
        vec![
            "plan",
            "prepare",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan99",
            "--actor",
            "fixture-preparer",
            "--operation-id",
            &operation,
            "--expected-revision",
            "72",
        ],
        // mine-release-allow-plan-reference: actual start
        vec![
            "plan",
            "start",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan99",
            "--owner",
            "cli-executor",
            "--run-id",
            "cli-fixture-run",
            "--expected-revision",
            "73",
        ],
    ] {
        let outcome = cli::dispatch(&run(directory.path().to_str().unwrap(), &args), "mine");
        assert_eq!(outcome.exit_code, 0, "{}", envelope_json(&outcome));
    }
    directory
}

#[test]
fn cli_reads_writes_and_v3_repair_fail_closed_on_a_forged_anchor() {
    let tmp = managed_graph_fixture();
    let toml_path = tmp.path().join(".mine/plan/execution-graph.toml");
    let mut workspace =
        toml::from_str::<PlanWorkspace>(&std::fs::read_to_string(&toml_path).unwrap()).unwrap();
    workspace.plans[0].branch_anchor = "HEAD".to_string();
    std::fs::write(&toml_path, toml::to_string(&workspace).unwrap()).unwrap();
    let before = std::fs::read(&toml_path).unwrap();

    let revision = workspace.revision.to_string();
    let commands = [
        vec!["graph", "validate", "--format", "json"],
        vec!["graph", "status", "--format", "json"],
        // mine-release-allow-plan-reference: private canonical CLI fixture
        vec![
            "plan",
            "start",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan02-C01",
            "--owner",
            "fixture-executor",
            "--expected-revision",
            revision.as_str(),
            "--format",
            "json",
        ],
        vec![
            "graph",
            "repair-provenance",
            "--actor",
            "fixture-repairer",
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
    // repository graph. We snapshot the development fixture bytes before/after and
    // assert they are unchanged (independent of the current revision number).
    let live_path = development_graph_fixture();
    let live_before = std::fs::read_to_string(&live_path).unwrap();
    let tmp = managed_graph_fixture();
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            // mine-release-allow-plan-reference: private canonical CLI fixture
            &[
                "plan",
                "start",
                "--id",
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan02-C01",
                "--owner",
                "fixture-executor",
                "--expected-revision",
                "72",
                "--format",
                "json",
            ],
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
        "development fixture unchanged by the temp-copy test"
    );
}

#[test]
fn acceptance_begin_requires_implemented_state() {
    let tmp = started_fixture();
    let store = TomlStore::new(tmp.path());
    let source = std::fs::read(store.toml_path()).unwrap();
    let head = fixture_git(tmp.path(), &["rev-parse", "HEAD"]);
    let request = serde_json::json!({"schema_version":1,"actor":"independent-reviewer","intent":{
        // mine-release-allow-plan-reference: typed request cannot accept IN_PROGRESS
        "kind":"acceptance","plan_id":"plan99","review":{"reviewed_tip":head,"report":{"path":".mine/plan/reports/plan99-fixture-review.md","sha256":"0".repeat(64),"bytes":1}}
    }});
    std::fs::create_dir_all(tmp.path().join(".mine/runtime")).unwrap();
    std::fs::write(
        tmp.path().join(".mine/runtime/acceptance-input.json"),
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    let outcome = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &[
                "operation",
                "begin",
                "--expected-revision",
                "74",
                "--input",
                ".mine/runtime/acceptance-input.json",
                "--format",
                "json",
            ],
        ),
        "mine",
    );
    assert_ne!(outcome.exit_code, 0);
    let envelope = envelope_json(&outcome);
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("implemented"),
        "{envelope}"
    );
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), source);
    assert_eq!(fixture_git(tmp.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn implemented_without_native_acceptance_keeps_successor_blocked() {
    let tmp = started_fixture();
    // mine-release-allow-plan-reference: exact registered subject checkout
    let target = tmp.path().join(".mine/worktrees/plan99");
    // mine-release-allow-plan-reference: actual declared fixture output
    let product = "tests/plan99/result.txt";
    // mine-release-allow-plan-reference: declared fixture namespace
    std::fs::create_dir_all(target.join("tests/plan99")).unwrap();
    std::fs::write(target.join(product), "actual private implementation\n").unwrap();
    fixture_git(&target, &["add", "--", product]);
    fixture_git(&target, &["commit", "-m", "private implementation"]);
    let tip = fixture_git(&target, &["rev-parse", "HEAD"]);
    // mine-release-allow-plan-reference: canonical implementation report
    let report = ".mine/plan/reports/plan99-fixture-implementation.md";
    std::fs::create_dir_all(tmp.path().join(".mine/plan/reports")).unwrap();
    std::fs::write(
        tmp.path().join(report),
        "Actual private implementation commit\n",
    )
    .unwrap();
    fixture_git(tmp.path(), &["add", "--", report]);
    fixture_git(
        tmp.path(),
        &["commit", "-m", "explicit implementation evidence"],
    );
    let implemented = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &[
                // mine-release-allow-plan-reference: actual implementation transition
                "plan",
                "implemented",
                "--id",
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan99",
                "--actor",
                "cli-executor",
                "--expected-revision",
                "74",
                "--report",
                report,
                "--commit",
                &tip,
            ],
        ),
        "mine",
    );
    assert_eq!(implemented.exit_code, 0, "{}", envelope_json(&implemented));
    let store = TomlStore::new(tmp.path());
    let source = std::fs::read(store.toml_path()).unwrap();
    let head = fixture_git(tmp.path(), &["rev-parse", "HEAD"]);
    let operation = uuid::Uuid::new_v4().to_string();
    let refused = cli::dispatch(
        &run(
            tmp.path().to_str().unwrap(),
            &[
                // mine-release-allow-plan-reference: unbacked alias cannot authorize acceptance
                "plan",
                "accept",
                "--id",
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan99",
                "--expected-revision",
                "75",
                "--operation-id",
                &operation,
                "--input",
                ".mine/runtime/no-closure.json",
            ],
        ),
        "mine",
    );
    assert_ne!(refused.exit_code, 0);
    let workspace = store.load().unwrap();
    // mine-release-allow-plan-reference: implementation is not independent acceptance
    assert_eq!(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        workspace.get("plan99").unwrap().status,
        mine::domain::status::PlanStatus::Implemented
    );
    // mine-release-allow-plan-reference: no premature successor release
    assert_eq!(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        workspace.get("plan99-01").unwrap().status,
        mine::domain::status::PlanStatus::Blocked
    );
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), source);
    assert_eq!(fixture_git(tmp.path(), &["rev-parse", "HEAD"]), head);
    // The real positive acceptance/successor release is retained in the native
    // terminal-delta/recovery case in lifecycle_operations, with actual gates.
}

#[test]
fn workspace_status_on_development_fixture_is_idempotent() {
    let tmp = managed_graph_fixture();
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
    let directory = managed_graph_fixture();
    let repo_root = directory.path().to_path_buf();
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
    let directory = managed_graph_fixture();
    let repo_root = directory.path().to_path_buf();
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
        design_root: ".mine/design/index.md".to_string(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans: vec![],
    };
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();

    let err = store.save_with_revision(2, "cli-fixture", Ok).unwrap_err();
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
    std::fs::write(root.path().join(".mine/design/leaf.md"), "# Leaf\n").unwrap();

    let outcome = cli::dispatch(
        &run(repo, &["design", "backup", "--format", "json"]),
        "mine",
    );
    assert_eq!(outcome.exit_code, 0);
    let env = envelope_json(&outcome);
    let backup_rel = env["data"]["backup_path"].as_str().unwrap().to_string();
    assert!(backup_rel.starts_with(".mine/runtime/design-backups/"));
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
    let tmp = managed_graph_fixture();
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
    fixture_git(root, &["config", "commit.gpgSign", "false"]);
    fixture_git(root, &["config", "core.autocrlf", "false"]);
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
    let (tmp, root) = common::seeded_repo(vec![]);
    assert!(!root.join("skills").exists() && !root.join("plugins").exists());
    // Supply genuinely valid private governance so this isolates asset parity,
    // not the separate refusal of missing/unapproved Design decisions.
    std::fs::create_dir_all(root.join(".mine/design/decisions")).unwrap();
    let decision = ".mine/design/decisions/0001-generic-fixture.md";
    std::fs::write(
        root.join(decision),
        "# Generic fixture decision\n\nThis private fixture has no bundled agent assets.\n",
    )
    .unwrap();
    for args in [
        vec![
            "adr",
            "add",
            "--id",
            "ADR-0001",
            "--path",
            decision,
            "--title",
            "Generic fixture",
        ],
        vec!["adr", "propose", "--id", "ADR-0001"],
        vec![
            "adr",
            "approve",
            "--id",
            "ADR-0001",
            "--by",
            "fixture-owner",
            "--note",
            "explicit private fixture approval",
        ],
    ] {
        let outcome = cli::dispatch(&run(root.to_str().unwrap(), &args), "mine");
        assert_eq!(outcome.exit_code, 0, "{}", envelope_json(&outcome));
    }
    fixture_git(&root, &["add", "--", ".mine/design/decisions"]);
    fixture_git(
        &root,
        &["commit", "-m", "explicit valid generic fixture Design"],
    );
    let outcome = cli::dispatch(
        &run(root.to_str().unwrap(), &["release", "--format", "json"]),
        "mine",
    );
    let envelope = envelope_json(&outcome);
    assert_eq!(outcome.exit_code, 0, "{envelope}");
    assert_eq!(envelope["data"]["distribution_synced"], true, "{envelope}");
    assert_eq!(envelope["data"]["can_release"], true, "{envelope}");
    drop(tmp);
}
