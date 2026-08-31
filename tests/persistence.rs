// Enforce no `unsafe` in MINE-owned test crates either (see src/lib.rs).
#![forbid(unsafe_code)]

//! Persistence-level integration tests for the execution graph.
//!
//! These exercise the infrastructure: TOML load/save round-trip, revision
//! conflict detection, atomic-write recovery, lock acquisition/timeout,
//! Markdown render determinism and revision parity, and a load of the real
//! repository execution graph to prove byte-compatible round-tripping.

//! conflict detection, atomic-write recovery, lock acquisition/timeout,
//! Markdown render determinism and revision parity, and a load of the real
//! repository execution graph to prove byte-compatible round-tripping with the
//! existing fact source.

use std::path::PathBuf;
use std::time::Duration;

use mine::domain::graph::{PlanNode, PlanWorkspace};
use mine::domain::status::PlanStatus;
use mine::infrastructure::toml_store::TomlStore;

/// Development graph fixture used to verify the domain model's byte-for-byte
/// round trip without requiring the temporary workspace on stable branches.
fn development_graph_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("development-execution-graph.toml")
}

fn sample_workspace(rev: u64) -> PlanWorkspace {
    PlanWorkspace {
        schema_version: mine::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
        revision: rev,
        project_id: "mine-is-not-everyones".to_string(),
        workspace_id: "bootstrap-mine-v1".to_string(),
        stable_branch: "master".to_string(),
        integration_branch: "dev".to_string(),
        stable_baseline_commit: "abc123".to_string(),
        design_root: "docs/design/index.md".to_string(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans: vec![PlanNode {
            id: "01".to_string(),
            path: "docs/plan/01.md".to_string(),
            title: "First plan".to_string(),
            branch: String::new(),
            branch_anchor: String::new(),
            status: PlanStatus::Ready,
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
            withdraw_reason: String::new(),
            withdrawn_at: String::new(),
            planning_scope: String::new(),
            readiness_checked_at: String::new(),
            adr_registry_revision: 0,
            lineage_id: String::new(),
            worktree_path: String::new(),
        }],
    }
}

/// The real repository graph must parse, validate, and round-trip through the
/// domain model without drift. This proves the TOML model is byte-compatible
/// with the bootstrap fact source (including `ACCEPTED`/`IN_PROGRESS`/`REJECTED`
/// statuses, the compensating node, and the flat-string-array design
/// references).
#[test]
fn development_graph_fixture_round_trips_byte_for_byte() {
    let original = std::fs::read_to_string(development_graph_path())
        .expect("development graph fixture must exist in the repository");
    let ws: PlanWorkspace =
        toml::from_str(&original).expect("real graph must parse into PlanWorkspace");
    // Structural validation passes.
    mine::domain::validation::validate(&ws).expect("real graph must validate");

    // Re-serialize and compare. TOML key ordering is preserved by the derive
    // order, so the round-trip should be byte-identical.
    let reserialized = toml::to_string(&ws).expect("PlanWorkspace must serialize");
    assert_eq!(
        original.trim_end(),
        reserialized.trim_end(),
        "real execution-graph.toml must round-trip byte-for-byte"
    );

    // The bootstrap graph now carries the compensating node (added when
    // was REJECTED and compensated). Assert the compensation node round-trips.
    assert!(
        ws.plans.len() >= 9,
        "bootstrap graph must include the 02-1 compensation node: got {} plans",
        ws.plans.len()
    );
    assert!(
        ws.get("02-1").is_some(),
        "Compensating node must round-trip"
    );
    assert!(ws.revision >= 2);
    let p01 = ws.get("01").expect("node 01 exists");
    assert_eq!(p01.status, PlanStatus::Accepted);
}

#[test]
fn load_returns_not_initialized_for_absent_graph() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlStore::new(dir.path());
    let err = store.load().unwrap_err();
    assert_eq!(err.code(), "MINE_GRAPH_NOT_INITIALIZED");
}

#[test]
fn save_with_revision_renders_markdown_with_parity() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlStore::new(dir.path());
    let ws = sample_workspace(0);
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();

    let result = store
        .save_with_revision(0, |mut w| {
            w.revision = 1;
            w.plans[0].status = PlanStatus::InProgress;
            w.plans[0].owner = "tester".to_string();
            Ok(w)
        })
        .unwrap();
    assert_eq!(result.revision, 1);

    // Markdown view exists and reports the same revision.
    let md = std::fs::read_to_string(store.md_path()).unwrap();
    assert!(md.contains("Revision: `1`"));
    assert!(md.contains("IN_PROGRESS"));

    // Byte parity validation passes through the graph service.
    let svc = mine::application::graph_service::GraphService::new(&store);
    svc.validate().unwrap();
}

#[test]
fn revision_conflict_does_not_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlStore::new(dir.path());
    let ws = sample_workspace(5);
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();
    let before = std::fs::read_to_string(store.toml_path()).unwrap();

    let err = store
        .save_with_revision(4, |w| {
            // Caller mistakenly expected revision 4; on disk it is 5.
            Ok(w)
        })
        .unwrap_err();
    assert_eq!(err.code(), "MINE_REVISION_CONFLICT");

    // TOML unchanged byte-for-byte.
    assert_eq!(std::fs::read_to_string(store.toml_path()).unwrap(), before);
}

#[test]
fn concurrent_writers_do_not_silently_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlStore::new(dir.path()).with_lock_timeout(Duration::from_millis(2000));
    let ws = sample_workspace(0);
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();

    // First writer commits revision 1.
    store
        .save_with_revision(0, |mut w| {
            w.revision = 1;
            w.plans[0].title = "writer-1".to_string();
            Ok(w)
        })
        .unwrap();

    // Second writer still believes revision is 0; must conflict, not overwrite.
    let err = store
        .save_with_revision(0, |mut w| {
            w.revision = 1;
            w.plans[0].title = "writer-2".to_string();
            Ok(w)
        })
        .unwrap_err();
    assert_eq!(err.code(), "MINE_REVISION_CONFLICT");

    // writer-1's title is preserved; writer-2 did not overwrite.
    let after = store.load().unwrap();
    assert_eq!(after.plans[0].title, "writer-1");
    assert_eq!(after.revision, 1);
}

#[test]
fn v3_graph_upgrades_implicitly_on_next_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlStore::new(dir.path());
    // Seed a v3 graph (the pre-v4 wire schema without the optional node
    // fields).
    let mut ws = sample_workspace(0);
    ws.schema_version = mine::domain::graph::PREVIOUS_GRAPH_SCHEMA_VERSION;
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();

    // The loader accepts v3 read-only.
    let loaded = store.load().unwrap();
    assert_eq!(
        loaded.schema_version,
        mine::domain::graph::PREVIOUS_GRAPH_SCHEMA_VERSION
    );

    // The next lifecycle mutation upgrades the wire schema implicitly (the
    // revision bumps once for the mutation itself, not for the schema).
    store
        .save_with_revision(0, |mut w| {
            w.revision = 1;
            w.plans[0].status = PlanStatus::Withdrawn;
            w.plans[0].withdraw_reason = "retired".to_string();
            w.plans[0].withdrawn_at = "2026-08-31T00:00:00Z".to_string();
            Ok(w)
        })
        .unwrap();
    let text = std::fs::read_to_string(store.toml_path()).unwrap();
    assert!(
        text.starts_with(
            "schema_version = \"mine-graph-v4\"
"
        ),
        "{text}"
    );
    let upgraded = store.load().unwrap();
    assert_eq!(
        upgraded.schema_version,
        mine::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION
    );
    assert_eq!(upgraded.revision, 1);

    // Newer, unsupported schemas stay fail-closed.
    let newer = text.replace("mine-graph-v4", "mine-graph-v5");
    std::fs::write(store.toml_path(), newer).unwrap();
    assert!(store.load().is_err());
}

#[test]
fn render_is_deterministic_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlStore::new(dir.path());
    let ws = sample_workspace(3);
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();

    store.render().unwrap();
    let md1 = std::fs::read_to_string(store.md_path()).unwrap();
    store.render().unwrap();
    let md2 = std::fs::read_to_string(store.md_path()).unwrap();
    assert_eq!(md1, md2);
    assert!(md1.contains("Revision: `3`"));
}

#[test]
fn render_repair_fixes_stale_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlStore::new(dir.path());
    let ws = sample_workspace(9);
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();
    // Stale markdown claims the wrong revision.
    std::fs::write(store.md_path(), "# Execution Graph\n\n- Revision: `0`\n").unwrap();

    store.render().unwrap();
    let md = std::fs::read_to_string(store.md_path()).unwrap();
    assert!(md.contains("Revision: `9`"));
    assert!(!md.contains("Revision: `0`"));
}

#[test]
fn atomic_write_recovers_from_missing_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlStore::new(dir.path());
    let ws = sample_workspace(0);
    std::fs::create_dir_all(store.toml_path().parent().unwrap()).unwrap();
    std::fs::write(store.toml_path(), toml::to_string(&ws).unwrap()).unwrap();
    // No markdown exists.
    assert!(!store.md_path().exists());

    store
        .save_with_revision(0, |mut w| {
            w.revision = 1;
            Ok(w)
        })
        .unwrap();
    assert!(store.md_path().exists());
}

#[test]
fn lock_acquired_and_released() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("graph.lock");
    let lock =
        mine::infrastructure::file_lock::acquire_exclusive(&lock_path, Duration::from_millis(500))
            .unwrap();
    assert!(lock_path.exists());
    drop(lock);
    // Re-acquire after release.
    let _lock2 =
        mine::infrastructure::file_lock::acquire_exclusive(&lock_path, Duration::from_millis(500))
            .unwrap();
}
