// Enforce no `unsafe` in MINE-owned test crates either (see src/lib.rs).
#![forbid(unsafe_code)]

//! Persistence-level integration tests for the execution graph.
//!
//! These exercise pure wire round-trips and rendering, plus revision-checked
//! native publication in private real-Git Roots. Historical fixture decoding
//! is not authority validation; current writes never repair an unproven view.

use std::path::PathBuf;
use std::time::Duration;

use mine::domain::graph::{PlanNode, PlanWorkspace};
use mine::domain::status::PlanStatus;
use mine::infrastructure::toml_store::TomlStore;

// Shared fixture module exports helpers for several test binaries.
#[allow(dead_code)]
mod common;

/// Development graph fixture used to verify the domain model's byte-for-byte
/// round trip without requiring the temporary workspace on stable branches.
fn development_graph_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("development-execution-graph.toml")
}

/// Supported readers must not invent evidence which predates the native producer.
#[test]
fn native_schema_reads_v4_v5_v6_v7_without_fabricating_evidence() {
    let mut canonical = sample_workspace(0);
    canonical.schema_version = 4;
    let original = toml::to_string(&canonical).unwrap();
    for version in [4, 5, 6, 7] {
        let bytes = original.replacen("mine-graph-v4", &format!("mine-graph-v{version}"), 1);
        let ws: PlanWorkspace = toml::from_str(&bytes).unwrap();
        mine::domain::validation::validate(&ws).unwrap();
        assert_eq!(ws.schema_version, version);
        // A read is not a migration, nor proof of historical acceptance/prepare.
        assert_eq!(toml::to_string(&ws).unwrap().trim_end(), bytes.trim_end());
    }
}

#[test]
fn native_schema_writer_is_v7_and_unknown_newer_version_is_refused() {
    assert_eq!(mine::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION, 7);
    let mut ws = sample_workspace(0);
    ws.schema_version = 8;
    assert!(mine::domain::validation::validate(&ws).is_err());
}

fn prepared_workspace() -> PlanWorkspace {
    use mine::domain::graph::{
        CommitId, PlanIdentity, PlanIdentityFields, PreparationGeneration, RepoPath, Sha256,
        UtcTimestamp,
    };
    let mut ws = sample_workspace(2);
    let node = &mut ws.plans[0];
    // mine-release-allow-plan-reference: canonical native schema regression fixture
    node.id = "plan01".into();
    // mine-release-allow-plan-reference: canonical native schema regression fixture
    node.path = ".mine/plan/plan01-fixture.md".into();
    // mine-release-allow-plan-reference: canonical native schema regression fixture
    node.branch = "plan/plan01-fixture".into();
    // mine-release-allow-plan-reference: canonical native schema regression fixture
    node.lineage_id = "plan01".into();
    // mine-release-allow-plan-reference: canonical native schema regression fixture
    node.worktree_path = ".mine/worktrees/plan01".into();
    let body = Sha256::new("a".repeat(64)).unwrap();
    node.body_sha256 = Some(body.clone());
    node.preparations.push(PreparationGeneration {
        ordinal: 1,
        operation_id: mine::domain::lifecycle_operation::OperationId::generate(),
        actor: "executor".into(),
        prepared_at: UtcTimestamp::new("2026-10-14T00:00:00Z").unwrap(),
        plan: PlanIdentity::new(PlanIdentityFields {
            repository_id: "repository-fixture".into(),
            workspace_id: ws.workspace_id.clone(),
            plan_id: node.id.clone(),
            plan_path: RepoPath::new(node.path.clone()).unwrap(),
            body_sha256: Some(body),
            lineage_id: node.lineage_id.clone(),
            branch: node.branch.clone(),
        })
        .unwrap(),
        policy_sha256: Sha256::new("b".repeat(64)).unwrap(),
        worktree: RepoPath::new(node.worktree_path.clone()).unwrap(),
        prepared_branch_commit: CommitId::new("1".repeat(40)).unwrap(),
        observed_root_commit: CommitId::new("2".repeat(40)).unwrap(),
        observed_graph_revision: 1,
    });
    ws
}

#[test]
fn native_preparation_round_trip_keeps_distinct_root_and_branch_commits() {
    let ws = prepared_workspace();
    mine::domain::validation::validate(&ws).unwrap();
    let decoded: PlanWorkspace = toml::from_str(&toml::to_string(&ws).unwrap()).unwrap();
    assert_eq!(decoded, ws);
    let g = &decoded.plans[0].preparations[0];
    assert_ne!(g.observed_root_commit, g.prepared_branch_commit);
    assert!(decoded.plans[0].acceptance.is_none());
    assert!(decoded.plans[0].started_preparation_generation.is_none());
}

#[test]
fn native_preparation_refuses_bad_generation_identity_and_old_schema() {
    let original = prepared_workspace();
    let mutations: [fn(&mut PlanWorkspace); 8] = [
        |w| w.schema_version = 6,
        |w| w.plans[0].preparations[0].ordinal = 0,
        |w| w.plans[0].preparations[0].ordinal = 2,
        |w| w.plans[0].preparations[0].actor.clear(),
        |w| w.plans[0].preparations[0].observed_graph_revision = w.revision,
        |w| w.plans[0].started_preparation_generation = Some(1),
        |w| {
            w.plans[0].body_sha256 = Some(mine::domain::graph::Sha256::new("c".repeat(64)).unwrap())
        },
        |w| {
            let duplicate = w.plans[0].preparations[0].clone();
            w.plans[0].preparations.push(duplicate);
        },
    ];
    for mutation in mutations {
        let mut changed = original.clone();
        mutation(&mut changed);
        assert!(mine::domain::validation::validate(&changed).is_err());
    }
}

#[test]
fn native_started_generation_pins_exact_prepared_anchor() {
    let mut ws = prepared_workspace();
    ws.plans[0].status = PlanStatus::InProgress;
    ws.plans[0].started_preparation_generation = Some(1);
    ws.plans[0].branch_anchor = ws.plans[0].preparations[0]
        .prepared_branch_commit
        .as_str()
        .into();
    mine::domain::validation::validate(&ws).unwrap();
    ws.plans[0].branch_anchor = ws.plans[0].preparations[0]
        .observed_root_commit
        .as_str()
        .into();
    assert!(mine::domain::validation::validate(&ws).is_err());
}

#[test]
fn native_successor_cannot_rewrite_or_invent_historical_identity() {
    use mine::domain::validation::validate_native_successor;
    let before = prepared_workspace();
    let mut after = before.clone();
    after.revision += 1;
    validate_native_successor(&before, &after).unwrap();
    after.plans[0].preparations.clear();
    assert!(validate_native_successor(&before, &after).is_err());
    let legacy = sample_workspace(1);
    let mut forged = legacy.clone();
    forged.revision += 1;
    forged.plans[0].body_sha256 = before.plans[0].body_sha256.clone();
    assert!(validate_native_successor(&legacy, &forged).is_err());
}

#[test]
fn native_noop_preserves_supported_legacy_schema_bytes_and_revision() {
    let mut workspace = sample_workspace(3);
    workspace.schema_version = 4;
    let (_dir, store) = transaction_fixture(workspace);
    let workspace = store.load().unwrap();
    let source = std::fs::read_to_string(store.toml_path()).unwrap();
    let view = std::fs::read(store.md_path()).unwrap();
    let result = store
        .save_with_revision(3, "persistence-fixture", Ok)
        .unwrap();
    assert_eq!(result, workspace);
    assert_eq!(std::fs::read_to_string(store.toml_path()).unwrap(), source);
    assert_eq!(std::fs::read(store.md_path()).unwrap(), view);
    for bad_revision in [3, 5] {
        let result = store.save_with_revision(3, "persistence-fixture", |mut workspace| {
            workspace.revision = bad_revision;
            workspace.plans[0].title = "changed without one revision".into();
            Ok(workspace)
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(store.toml_path()).unwrap(), source);
        assert_eq!(std::fs::read(store.md_path()).unwrap(), view);
    }
}

fn sample_workspace(rev: u64) -> PlanWorkspace {
    PlanWorkspace {
        schema_version: mine::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
        revision: rev,
        project_id: "mine-is-not-everyones".to_string(),
        workspace_id: "persistence-fixture".to_string(),
        stable_branch: "master".to_string(),
        integration_branch: "dev".to_string(),
        stable_baseline_commit: "1".repeat(40),
        design_root: ".mine/design/index.md".to_string(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans: vec![PlanNode {
            // mine-release-allow-plan-reference: canonical pure wire fixture
            id: "plan01".to_string(),
            // mine-release-allow-plan-reference: canonical pure wire fixture
            path: ".mine/plan/plan01-fixture.md".to_string(),
            title: "First plan".to_string(),
            // mine-release-allow-plan-reference: canonical pure wire fixture
            branch: "plan/plan01-fixture".to_string(),
            branch_anchor: String::new(),
            status: PlanStatus::Ready,
            hard_predecessors: vec![],
            soft_predecessors: vec![],
            design_references: vec![".mine/design/principles.md".to_string()],
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
            // mine-release-allow-plan-reference: canonical pure wire fixture
            lineage_id: "plan01".to_string(),
            // mine-release-allow-plan-reference: canonical pure wire fixture
            worktree_path: ".mine/worktrees/plan01".to_string(),
            body_sha256: None,
            ownership: None,
            ownership_policy: None,
            clarifications: vec![],
            acceptance: None,
            preparations: vec![],
            started_preparation_generation: None,
        }],
    }
}

/// Native transaction fixtures own a complete committed Root. Pure wire tests
/// above intentionally do not need Git or fabricate native lifecycle evidence.
fn transaction_fixture(mut workspace: PlanWorkspace) -> (tempfile::TempDir, TomlStore) {
    let (directory, root) = common::seeded_repo(vec![]);
    let private = common::load_graph(&root);
    workspace.project_id = private.project_id;
    workspace.workspace_id = private.workspace_id;
    workspace.stable_baseline_commit = private.stable_baseline_commit;
    for node in &workspace.plans {
        std::fs::write(
            root.join(&node.path),
            format!("# Private {} wire fixture\n", node.id),
        )
        .unwrap();
    }
    let store = TomlStore::new(&root);
    std::fs::write(store.toml_path(), toml::to_string(&workspace).unwrap()).unwrap();
    store.render().unwrap();
    fixture_git(&root, &["add", "--", ".mine/plan"]);
    fixture_git(
        &root,
        &["commit", "-m", "explicit native transaction inputs"],
    );
    assert_eq!(
        fixture_git(&root, &["status", "--porcelain=v1", "-uall"]),
        ""
    );
    (directory, store)
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

/// An old wire snapshot still round-trips without gaining missing provenance.
#[test]
fn obsolete_development_snapshot_round_trips_but_is_not_valid_authority() {
    let original = std::fs::read_to_string(development_graph_path())
        .expect("development graph fixture must exist in the repository");
    let ws: PlanWorkspace =
        toml::from_str(&original).expect("real graph must parse into PlanWorkspace");
    // Wire decoding is not canonical authority. Preserve the historical sample
    // verbatim, but never resurrect the removed synthetic-workspace bypass.
    let error = mine::domain::validation::validate(&ws).unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert!(
        error
            .to_string()
            .contains("incomplete canonical provenance")
    );

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
    let (_dir, store) = transaction_fixture(sample_workspace(0));

    let result = store
        .save_with_revision(0, "persistence-fixture", |mut w| {
            w.revision = 1;
            w.plans[0].title = "Updated declaration".to_string();
            Ok(w)
        })
        .unwrap();
    assert_eq!(result.revision, 1);

    // Markdown view exists and reports the same revision.
    let md = std::fs::read_to_string(store.md_path()).unwrap();
    assert!(md.contains("Revision: `1`"));
    assert!(md.contains("Updated declaration"));

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
        .save_with_revision(4, "persistence-fixture", |w| {
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
    let (_dir, store) = transaction_fixture(sample_workspace(0));
    let store = store.with_lock_timeout(Duration::from_millis(2000));

    // First writer commits revision 1.
    store
        .save_with_revision(0, "persistence-fixture", |mut w| {
            w.revision = 1;
            w.plans[0].title = "writer-1".to_string();
            Ok(w)
        })
        .unwrap();

    // Second writer still believes revision is 0; must conflict, not overwrite.
    let err = store
        .save_with_revision(0, "persistence-fixture", |mut w| {
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
    // Seed a v3 graph (the pre-v4 wire schema without the optional node
    // fields).
    let mut ws = sample_workspace(0);
    ws.schema_version = mine::domain::graph::PREVIOUS_GRAPH_SCHEMA_VERSION;
    let (_dir, store) = transaction_fixture(ws);

    // The loader accepts v3 read-only.
    let loaded = store.load().unwrap();
    assert_eq!(
        loaded.schema_version,
        mine::domain::graph::PREVIOUS_GRAPH_SCHEMA_VERSION
    );

    // The next lifecycle mutation upgrades the wire schema implicitly (the
    // revision bumps once for the mutation itself, not for the schema).
    store
        .save_with_revision(0, "persistence-fixture", |mut w| {
            w.revision = 1;
            w.plans[0].title = "Schema-upgraded declaration".to_string();
            Ok(w)
        })
        .unwrap();
    let text = std::fs::read_to_string(store.toml_path()).unwrap();
    assert!(
        text.starts_with(
            "schema_version = \"mine-graph-v7\"
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
    let newer = text.replace("mine-graph-v7", "mine-graph-v8");
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
fn native_write_refuses_missing_unproven_view_without_mutation() {
    let (_dir, store) = transaction_fixture(sample_workspace(0));
    let source = std::fs::read(store.toml_path()).unwrap();
    let head = fixture_git(store.repo_root(), &["rev-parse", "HEAD"]);
    std::fs::remove_file(store.md_path()).unwrap();
    assert!(!store.md_path().exists());

    let error = store
        .save_with_revision(0, "persistence-fixture", |mut w| {
            w.revision = 1;
            Ok(w)
        })
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), source);
    assert_eq!(fixture_git(store.repo_root(), &["rev-parse", "HEAD"]), head);
    assert!(!store.md_path().exists());
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
