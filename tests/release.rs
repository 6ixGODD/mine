// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! `mine plan release --id` integration tests.
//!
//! Drives the CLI over isolated temp repos seeded with controlled graphs; the
//! on-disk development fixture is checked for accidental edits (not a Root audit).

mod common;

use mine::cli;
use mine::domain::status::PlanStatus;
use mine::infrastructure::toml_store::TomlStore;

use common::{dispatch_json, live_graph_bytes, load_graph, node, rejected_node, seeded_repo};

fn assert_live_unchanged(before: &[u8]) {
    let after = live_graph_bytes();
    assert_eq!(
        before, after,
        "development fixture unchanged by temp-copy test"
    );
}

#[test]
fn release_no_predecessors_draft_to_ready() {
    let live = live_graph_bytes();
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    let (_tmp, repo) = seeded_repo(vec![node("plan09", PlanStatus::Draft, &[], &[])]);
    let (outcome, env) = dispatch_json(
        &repo,
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        &[
            "plan",
            "release",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan09",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    assert_eq!(env["ok"], true);
    assert_eq!(env["command"], "plan.release");
    // Every successful release bumps the graph revision exactly once.
    assert_eq!(
        env["revision_after"].as_u64().unwrap(),
        env["revision_before"].as_u64().unwrap() + 1
    );
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(env["data"]["plan"], "plan09");
    assert_eq!(env["data"]["status_before"], "DRAFT");
    assert_eq!(env["data"]["status_after"], "READY");
    assert_eq!(env["data"]["hard_predecessors"], serde_json::json!([]));
    assert_eq!(
        env["data"]["unsatisfied_predecessors"],
        serde_json::json!([])
    );
    let ws = load_graph(&repo);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan09").unwrap().status, PlanStatus::Ready);
    // MD view regenerated deterministically with the new status.
    let md = std::fs::read_to_string(repo.join(".mine/plan/execution-graph.md")).unwrap();
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert!(md.contains("| plan09 |") && md.contains("READY"));
    assert_live_unchanged(&live);
}

#[test]
fn release_all_accepted_predecessors_to_ready() {
    let (tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan01", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan09", PlanStatus::Draft, &["plan01", "plan03"], &[]),
    ]);
    let _ = tmp;
    let (outcome, env) = dispatch_json(
        &repo,
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        &[
            "plan",
            "release",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan09",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    assert_eq!(env["data"]["status_after"], "READY");
    assert_eq!(
        env["data"]["unsatisfied_predecessors"],
        serde_json::json!([])
    );
    let ws = load_graph(&repo);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan09").unwrap().status, PlanStatus::Ready);
}

#[test]
fn release_one_unaccepted_predecessor_to_blocked() {
    let live = live_graph_bytes();
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan01", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan02", PlanStatus::Blocked, &["plan01"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan09", PlanStatus::Draft, &["plan01", "plan02"], &[]),
    ]);
    let (outcome, env) = dispatch_json(
        &repo,
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        &[
            "plan",
            "release",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan09",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    assert_eq!(env["data"]["status_after"], "BLOCKED");
    assert_eq!(
        env["data"]["unsatisfied_predecessors"],
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        serde_json::json!(["plan02"])
    );
    assert_eq!(
        env["revision_after"].as_u64().unwrap(),
        env["revision_before"].as_u64().unwrap() + 1
    );
    let ws = load_graph(&repo);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan09").unwrap().status, PlanStatus::Blocked);
    assert_live_unchanged(&live);
}

#[test]
fn release_non_draft_is_invalid_transition_bytes_unchanged() {
    let live = live_graph_bytes();
    for status in [
        PlanStatus::Blocked,
        PlanStatus::Ready,
        PlanStatus::InProgress,
        PlanStatus::Implemented,
        PlanStatus::Accepted,
        PlanStatus::Rejected,
    ] {
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        let (_tmp, repo) = seeded_repo(vec![node("plan09", status, &[], &[])]);
        let before = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
        let (outcome, env) = dispatch_json(
            &repo,
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            &[
                "plan",
                "release",
                "--actor",
                "fixture-coordinator",
                "--expected-revision",
                "72",
                "--id",
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan09",
                "--format",
                "json",
            ],
        );
        assert_eq!(
            outcome.exit_code, 4,
            "validation exit for {status:?}: {env}"
        );
        assert_eq!(env["ok"], false);
        assert_eq!(env["error"]["code"], "MINE_INVALID_TRANSITION");
        let after = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
        assert_eq!(before, after, "temp bytes unchanged ({status:?})");
    }
    assert_live_unchanged(&live);
}

#[test]
fn release_missing_plan_is_not_found() {
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    let (_tmp, repo) = seeded_repo(vec![node("plan09", PlanStatus::Draft, &[], &[])]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "release",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            "ghost",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 4);
    assert_eq!(env["error"]["code"], "MINE_PLAN_NOT_FOUND");
}

#[test]
fn release_missing_id_flag_is_usage() {
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    let (_tmp, repo) = seeded_repo(vec![node("plan09", PlanStatus::Draft, &[], &[])]);
    let (outcome, env) = dispatch_json(
        &repo,
        &[
            "plan",
            "release",
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

#[test]
fn release_not_idempotent_second_release_errors() {
    let live = live_graph_bytes();
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    let (_tmp, repo) = seeded_repo(vec![node("plan09", PlanStatus::Draft, &[], &[])]);
    let (o1, e1) = dispatch_json(
        &repo,
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        &[
            "plan",
            "release",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan09",
            "--format",
            "json",
        ],
    );
    assert_eq!(o1.exit_code, 0, "{e1}");
    let rev_after_first = load_graph(&repo).revision;
    let (o2, e2) = dispatch_json(
        &repo,
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        &[
            "plan",
            "release",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "73",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan09",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        o2.exit_code, 4,
        "second release must not be idempotent-success: {e2}"
    );
    assert_eq!(e2["error"]["code"], "MINE_INVALID_TRANSITION");
    let rev_after_second = load_graph(&repo).revision;
    assert_eq!(
        rev_after_first, rev_after_second,
        "no extra revision bump on error"
    );
    assert_live_unchanged(&live);
}

#[test]
fn release_does_not_alter_other_plans() {
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan01", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Blocked, &["plan01"], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan09", PlanStatus::Draft, &["plan01", "plan03"], &[]),
    ]);
    let (outcome, env) = dispatch_json(
        &repo,
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        &[
            "plan",
            "release",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan09",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{env}");
    assert_eq!(env["data"]["status_after"], "BLOCKED");
    assert_eq!(
        env["data"]["unsatisfied_predecessors"],
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        serde_json::json!(["plan03"])
    );
    let ws = load_graph(&repo);
    // 09 released to BLOCKED; 01 and 03 untouched.
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan09").unwrap().status, PlanStatus::Blocked);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan01").unwrap().status, PlanStatus::Accepted);
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan03").unwrap().status, PlanStatus::Blocked);
}

#[test]
fn release_rejects_rejected_plan_others_unchanged() {
    let live = live_graph_bytes();
    let (_tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan03", PlanStatus::Accepted, &[], &[]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        rejected_node("plan05", "plan05-C01", &["plan03"]),
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        node("plan05-C01", PlanStatus::Ready, &["plan03"], &[]),
    ]);
    let before = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    let (outcome, env) = dispatch_json(
        &repo,
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        &[
            "plan",
            "release",
            "--actor",
            "fixture-coordinator",
            "--expected-revision",
            "72",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan05",
            "--format",
            "json",
        ],
    );
    assert_eq!(outcome.exit_code, 4, "{env}");
    assert_eq!(env["error"]["code"], "MINE_INVALID_TRANSITION");
    let after = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    assert_eq!(before, after);
    assert_live_unchanged(&live);
}

/// Two concurrent `mine plan release --id` invocations against the same
/// isolated temp repo must be resolved by the shared `lock -> reload ->
/// revision check -> semantic check -> mutation -> atomic write -> render`
/// transaction. Exactly one writer performs the real `DRAFT` release
/// (revision bumps +1, node `DRAFT -> READY`); the loser is an *honest* loser
/// and must mutate nothing. The design
/// (`docs/design/execution-graph/state-machine-and-algorithms.md#plan-release`)
/// requires a stale observer to fail with `MINE_REVISION_CONFLICT` before
/// business validation. Both callers explicitly retain the same revision.
/// The safety invariants must also hold: exactly one
/// winner, exactly one revision bump, final `READY`, no stale overwrite, no
/// unrelated graph data change, and TOML/Markdown consistency. This is the
/// dedicated concurrency test required by the independent review for the new
/// mutation command; the over-constrained single-code loser
/// assertion and strengthened the invariants.
#[test]
fn concurrent_release_is_resolved_by_revision_conflict() {
    let live = live_graph_bytes();
    // Seed the raced `DRAFT` plan plus an unrelated `ACCEPTED` plan that the
    // release must NOT touch, to prove no unrelated graph data changes.
    let (_tmp, repo) = common::seeded_repo_with_lock_timeout(
        vec![
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan01", PlanStatus::Accepted, &[], &[]),
            // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
            node("plan09-01", PlanStatus::Draft, &[], &[]),
        ],
        30_000,
    );
    let n = load_graph(&repo).revision; // pre-mutation revision both readers observe
    let unrelated_before = load_graph(&repo)
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        .get("plan01")
        .expect("seeded unrelated node 01")
        .clone();
    let repo_str = repo.to_str().unwrap().to_string();
    let repo_a = repo_str.clone();
    let repo_b = repo_str.clone();
    let handle_a = std::thread::spawn(move || {
        cli::dispatch(
            &common::run(
                &repo_a,
                // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
                &[
                    "plan",
                    "release",
                    "--actor",
                    "fixture-coordinator",
                    "--expected-revision",
                    "72",
                    "--id",
                    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                    "plan09-01",
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
                // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
                &[
                    "plan",
                    "release",
                    "--actor",
                    "fixture-coordinator",
                    "--expected-revision",
                    "72",
                    "--id",
                    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                    "plan09-01",
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

    let ok_a = out_a.exit_code == 0 && env_a["ok"] == true;
    let ok_b = out_b.exit_code == 0 && env_b["ok"] == true;
    // Exactly one writer performed the real DRAFT release.
    assert!(ok_a ^ ok_b, "exactly one release won: a={ok_a} b={ok_b}");

    // Both explicit observers used revision72: the loser must conflict before
    // business validation, not silently refresh to revision73.
    let (loser_env, winner_env) = if ok_a {
        (&env_b, &env_a)
    } else {
        (&env_a, &env_b)
    };
    assert_eq!(loser_env["ok"], false);
    let loser_code = loser_env["error"]["code"].as_str().unwrap_or("(none)");
    assert_eq!(
        loser_code, "MINE_REVISION_CONFLICT",
        "both callers supplied revision72: {loser_env}"
    );

    // The winner performed exactly the DRAFT -> READY transition off the
    // pre-mutation revision `n` (which both writers read before either lock).
    assert_eq!(winner_env["ok"], true);
    assert_eq!(winner_env["data"]["status_before"], "DRAFT");
    assert_eq!(winner_env["data"]["status_after"], "READY");
    assert_eq!(
        winner_env["revision_before"].as_u64().unwrap(),
        n,
        "winner read the same pre-mutation revision as the seeded graph"
    );
    assert_eq!(
        winner_env["revision_after"].as_u64().unwrap(),
        n + 1,
        "winner bumped the revision by exactly +1"
    );

    // Graph revision increased exactly once; no stale writer overwrote the
    // winner (revision is exactly n+1, never higher) and the loser wrote
    // nothing observable.
    let ws = load_graph(&repo);
    assert_eq!(
        ws.revision,
        n + 1,
        "exactly one revision bump, no silent double/lost write"
    );
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    assert_eq!(ws.get("plan09-01").unwrap().status, PlanStatus::Ready);

    // No unrelated graph data changed: the Accepted node survives identical.
    // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
    let unrelated_after = ws.get("plan01").expect("unrelated plan survives");
    assert_eq!(
        unrelated_after, &unrelated_before,
        "the concurrent race must not alter the unrelated Accepted node"
    );

    // TOML and generated Markdown remain mutually consistent: the Markdown
    // view reflects the final revision and the released READY status.
    let toml = std::fs::read_to_string(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    let parsed_toml: mine::domain::graph::PlanWorkspace = toml::from_str(&toml).unwrap();
    assert_eq!(
        parsed_toml.revision,
        n + 1,
        "TOML revision matches the bumped graph"
    );
    let md = std::fs::read_to_string(repo.join(".mine/plan/execution-graph.md")).unwrap();
    assert!(
        md.contains("READY"),
        "Markdown view reflects the READY status"
    );
    assert!(
        // mine-release-allow-plan-reference: isolated canonical lifecycle fixture
        md.contains("| plan09-01 |"),
        "Markdown view contains the released plan row"
    );

    assert_live_unchanged(&live);
}

#[test]
fn development_fixture_unchanged_after_release_suite() {
    // Final guard: the whole release suite uses temp repos; the development fixture is
    // invariant. (Other tests assert the same; this is the explicit guard.)
    let before = live_graph_bytes();
    // No-op read.
    let _ = TomlStore::new(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let after = live_graph_bytes();
    assert_eq!(before, after);
}
