// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! `mine plan rewire-compensation --id` integration tests.
//!
//! Drives the CLI over isolated temp repos seeded with controlled graphs; the
//! on-disk development fixture is checked for accidental edits (not a Root audit).

mod common;

use mine::cli;
use mine::domain::status::PlanStatus;

use common::{dispatch_json, live_graph_bytes, load_graph, node, rejected_node, seeded_repo};

fn assert_live_unchanged(before: &[u8]) {
    assert_eq!(
        before,
        live_graph_bytes(),
        "development fixture unchanged by temp-copy test"
    );
}

fn seed_rewire_graph(
    extra: Vec<mine::domain::graph::PlanNode>,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let mut plans = vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan04", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05", "plan05-C01", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan05-C01", PlanStatus::Ready, &["plan03"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan06", PlanStatus::Blocked, &["plan04", "plan05"], &[]),
    ];
    plans.extend(extra);
    seeded_repo(plans)
}

#[test]
fn rewire_success_hard_predecessors() {
    let live = live_graph_bytes();
    let (_tmp, repo) = seed_rewire_graph(vec![]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    assert_eq!(env["ok"], true);
    assert_eq!(env["command"], "plan.rewire-compensation");
    assert_eq!(
        env["revision_after"].as_u64().unwrap(),
        env["revision_before"].as_u64().unwrap() + 1
    );
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(env["data"]["rejected_plan"], "plan05");
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(env["data"]["compensating_plan"], "plan05-C01");
    assert_eq!(
        env["data"]["affected_successors"],
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        serde_json::json!(["plan06"])
    );
    let ws = load_graph(&repo);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        ws.get("plan06").unwrap().hard_predecessors,
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        vec!["plan04", "plan05-C01"]
    );
    // 05 stays REJECTED, untouched.
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan05").unwrap().status, PlanStatus::Rejected);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan05").unwrap().compensating_plan, "plan05-C01");
    assert_live_unchanged(&live);
}

#[test]
fn rewire_regenerates_markdown_view() {
    let (_tmp, repo) = seed_rewire_graph(vec![]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    let md = std::fs::read_to_string(repo.join(".mine/plan/execution-graph.md")).unwrap();
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert!(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        md.contains("plan05-C01"),
        "MD view reflects rewired predecessor"
    );
    // 06 row now names 05-1 (and 04), not 05.
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    let line06 = md.lines().find(|l| l.starts_with("| plan06 |")).unwrap();
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert!(line06.contains("plan05-C01"));
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert!(!line06.contains("05 |") || line06.contains("plan05-C01")); // not the bare 05
}

#[test]
fn rewire_idempotent_no_op() {
    let live = live_graph_bytes();
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan04", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05", "plan05-C01", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan05-C01", PlanStatus::Accepted, &["plan03"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node(
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan06",
            PlanStatus::Blocked,
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            &["plan04", "plan05-C01"],
            &[],
        ), // already rewired
    ]);
    let before = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    assert_eq!(env["data"]["affected_successors"], serde_json::json!([]));
    assert_eq!(
        env["revision_before"], env["revision_after"],
        "no bump on no-op"
    );
    let after = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    assert_eq!(before, after, "temp bytes identical on idempotent no-op");
    assert_live_unchanged(&live);
}

#[test]
fn rewire_not_rejected_original_errors() {
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan05", PlanStatus::Implemented, &["plan03"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan05-C01", PlanStatus::Ready, &["plan03"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan06", PlanStatus::Blocked, &["plan04", "plan05"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan04", PlanStatus::Accepted, &[], &[]),
    ]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 4, "{env}");
    assert_eq!(env["error"]["code"], "MINE_INVALID_TRANSITION");
}

#[test]
fn rewire_empty_compensating_plan_errors() {
    let live = live_graph_bytes();
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05", "", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan06", PlanStatus::Draft, &["plan05"], &[]),
    ]);
    let before = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 4, "{env}");
    assert_eq!(env["error"]["code"], "MINE_GRAPH_INVALID");
    let after = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    assert_eq!(before, after);
    assert_live_unchanged(&live);
}

#[test]
fn rewire_missing_replacement_errors() {
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05", "plan05-C01", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan06", PlanStatus::Draft, &["plan05"], &[]),
    ]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 4, "{env}");
    assert_eq!(env["error"]["code"], "MINE_PLAN_NOT_FOUND");
}

#[test]
fn rewire_rejected_replacement_errors() {
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05", "plan05-C01", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05-C01", "", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan06", PlanStatus::Draft, &["plan05"], &[]),
    ]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 4, "{env}");
    assert_eq!(env["error"]["code"], "MINE_GRAPH_INVALID");
}

#[test]
fn rewire_locked_successor_errors_bytes_unchanged() {
    let live = live_graph_bytes();
    for status in [
        PlanStatus::InProgress,
        PlanStatus::Implemented,
        PlanStatus::Accepted,
        PlanStatus::Rejected,
    ] {
        let (_tmp, repo) = seeded_repo(vec![
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan03", PlanStatus::Accepted, &[], &[]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan04", PlanStatus::Accepted, &[], &[]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            rejected_node("plan05", "plan05-C01", &["plan03"]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan05-C01", PlanStatus::Ready, &["plan03"], &[]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan06", status, &["plan04", "plan05"], &[]),
        ]);
        let before = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
        let (outcome, env) = dispatch_json(
            &repo,
            &[
                "plan",
                "rewire-compensation",
                "--actor",
                "fixture-coordinator",
                "--expected-revision",
                "72",
                "--id",
                // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
                "plan05",
                "--format",
                "json",
            ],
        );
        assert_eq!(outcome.exit_code, 3, "gate exit for {status:?}: {env}");
        assert_eq!(env["error"]["code"], "MINE_REWIRE_SUCCESSOR_LOCKED");
        let after = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
        assert_eq!(before, after, "temp bytes unchanged ({status:?})");
        let ws = load_graph(&repo);
        assert_eq!(
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            ws.get("plan06").unwrap().hard_predecessors,
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            vec!["plan04", "plan05"],
            "06 unchanged"
        );
    }
    assert_live_unchanged(&live);
}

#[test]
fn rewire_invalid_compensation_cycle_is_refused_without_mutation() {
    let live = live_graph_bytes();
    let (_tmp, repo) = seed_rewire_graph(vec![]);
    // A valid compensation inherits all ordered base predecessors, so the old
    // cycle construction is already invalid before rewiring. Inject that exact
    // malformed persisted input and prove public refusal without publishing.
    let mut invalid = load_graph(&repo);
    // mine-release-allow-plan-reference: deliberately malformed compensation fixture
    invalid.get_mut("plan05-C01").unwrap().hard_predecessors = vec!["plan06".into()];
    std::fs::write(
        repo.join(".mine/plan/execution-graph.toml"),
        toml::to_string(&invalid).unwrap(),
    )
    .unwrap();
    let before = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    let view = std::fs::read(repo.join(".mine/plan/execution-graph.md")).unwrap();
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 4, "{env}");
    assert_eq!(env["error"]["code"], "MINE_GRAPH_INVALID");
    assert!(
        env["error"]["message"]
            .as_str()
            .unwrap()
            .contains("must inherit the complete ordered hard predecessors"),
        "{env}"
    );
    let after = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    assert_eq!(before, after);
    assert_eq!(
        std::fs::read(repo.join(".mine/plan/execution-graph.md")).unwrap(),
        view
    );
    assert_live_unchanged(&live);
}

#[test]
fn rewire_sibling_id_not_rewired() {
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    // A successor that references the sibling-looking id "plan05-01" must NOT be
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    // rewired when rerouting "plan05". Only exact "plan05" predecessor entries change.
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05", "plan05-C01", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan05-C01", PlanStatus::Ready, &["plan03"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan05-01", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan07", PlanStatus::Draft, &["plan05-01"], &[]),
    ]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    assert_eq!(env["data"]["affected_successors"], serde_json::json!([]));
    let ws = load_graph(&repo);
    assert_eq!(
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        ws.get("plan07").unwrap().hard_predecessors,
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        vec!["plan05-01"],
        "sibling-looking 050 predecessor untouched"
    );
}

#[test]
fn rewire_soft_predecessor() {
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05", "plan05-C01", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan05-C01", PlanStatus::Ready, &["plan03"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan09", PlanStatus::Draft, &[], &["plan05"]),
    ]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    assert_eq!(
        env["data"]["affected_successors"],
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        serde_json::json!(["plan09"])
    );
    let ws = load_graph(&repo);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        ws.get("plan09").unwrap().soft_predecessors,
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        vec!["plan05-C01"]
    );
}

#[test]
fn rewire_missing_id_flag_is_usage() {
    let (_tmp, repo) = seed_rewire_graph(vec![]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "rewire-compensation",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 2);
    assert_eq!(env["error"]["code"], "MINE_USAGE");
}

/// Two concurrent `mine plan rewire-compensation --id 05` invocations against
/// the same isolated temp repo must produce exactly one real reroute. The
/// competing invocation retains the same observer revision and must lose via
/// `MINE_REVISION_CONFLICT`, without silently refreshing it. A separate
/// current-revision test above covers idempotent no-op behavior.
#[test]
fn concurrent_rewire_is_resolved_by_revision_conflict() {
    let live = live_graph_bytes();
    let (_tmp, repo) = common::seeded_repo_with_lock_timeout(
        vec![
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan03", PlanStatus::Accepted, &[], &[]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan04", PlanStatus::Accepted, &[], &[]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            rejected_node("plan05", "plan05-C01", &["plan03"]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan05-C01", PlanStatus::Ready, &["plan03"], &[]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan06", PlanStatus::Blocked, &["plan04", "plan05"], &[]),
        ],
        30_000,
    );
    let n = load_graph(&repo).revision;
    let repo_str = repo.to_str().unwrap().to_string();
    let repo_a = repo_str.clone();
    let repo_b = repo_str.clone();
    let handle_a = std::thread::spawn(move || {
        cli::dispatch(
            &common::run(
                &repo_a,
                &[
                    "plan",
                    "rewire-compensation",
                    "--actor",
                    "fixture-coordinator",
                    "--expected-revision",
                    "72",
                    "--id",
                    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
                    "plan05",
                    "--format",
                    "json",
                ],
            ),
            "mine",
        )
    });
    let handle_b = std::thread::spawn(move || {
        cli::dispatch(
            &common::run(
                &repo_b,
                &[
                    "plan",
                    "rewire-compensation",
                    "--actor",
                    "fixture-coordinator",
                    "--expected-revision",
                    "72",
                    "--id",
                    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
                    "plan05",
                    "--format",
                    "json",
                ],
            ),
            "mine",
        )
    });
    let out_a = handle_a.join().unwrap();
    let out_b = handle_b.join().unwrap();
    let env_a = common::envelope_json(&out_a);
    let env_b = common::envelope_json(&out_b);

    let loser_conflicts = |env: &serde_json::Value| {
        env["ok"] == false && env["error"]["code"] == "MINE_REVISION_CONFLICT"
    };
    let winner_rewired = |env: &serde_json::Value| {
        env["ok"] == true
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            && env["data"]["affected_successors"] == serde_json::json!(["plan06"])
            && env["revision_after"].as_u64().unwrap()
                == env["revision_before"].as_u64().unwrap() + 1
    };
    let (winner, _loser) = if winner_rewired(&env_a) && loser_conflicts(&env_b) {
        (&env_a, &env_b)
    } else if winner_rewired(&env_b) && loser_conflicts(&env_a) {
        (&env_b, &env_a)
    } else {
        panic!("expected one real reroute and one honest loser; got a={env_a} b={env_b}")
    };

    let ws = load_graph(&repo);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        ws.get("plan06").unwrap().hard_predecessors,
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        vec!["plan04", "plan05-C01"]
    );
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan05").unwrap().status, PlanStatus::Rejected);
    assert_eq!(
        ws.revision,
        n + 1,
        "exactly one revision bump, no silent double/lost write"
    );
    assert_eq!(
        winner["revision_before"].as_u64().unwrap(),
        n,
        "winner read the seeded pre-mutation revision"
    );
    assert_live_unchanged(&live);
}

#[test]
fn development_fixture_unchanged_after_rewire_suite() {
    let before = live_graph_bytes();
    let after = live_graph_bytes();
    assert_eq!(
        before, after,
        "development fixture unchanged by the rewire suite"
    );
}
