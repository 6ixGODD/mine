//! Integration tests for the setup-only initialization service.
//!
//! These cover absent, valid, legacy, and foreign
//! design roots, ownership mismatch, malformed markers, idempotent
//! initialization, AGENTS.md section handling, and root-version evidence.

use std::fs;
use std::path::{Path, PathBuf};

use mine::application::init_service::{DesignRootSummary, InitAction, InitOutcome};
use mine::domain::config::MineConfig;
use mine::domain::design_marker::DesignMarker;
use mine::domain::error::MineResult;
use mine::domain::ports::{Clock, UuidSource};

use tempfile::TempDir;

struct FixedUuid;
impl UuidSource for FixedUuid {
    fn new_repository_id(&self) -> String {
        "fixed-uuid-0000-0000-000000000000".to_string()
    }
}

struct FixedClock;
impl Clock for FixedClock {
    fn now_utc_rfc3339(&self) -> String {
        "2026-07-23T00:00:00Z".to_string()
    }
}

/// Runs the initialization service with deterministic ports against `root`.
fn run_init(root: &Path) -> MineResult<InitOutcome> {
    let uuid = FixedUuid;
    let clock = FixedClock;
    let svc = mine::application::init_service::InitService::new(&uuid, &clock);
    svc.initialize(root, "master")
}

fn marker_path(root: &Path) -> PathBuf {
    root.join(".mine").join("design").join(".mine-design.toml")
}

fn config_path(root: &Path) -> PathBuf {
    root.join(".mine").join("config.toml")
}

fn write_marker(root: &Path, repository_id: &str, managed_by: &str, created_at: &str) {
    let content = format!(
        "schema_version = 1\nmanaged_by = {managed_by:?}\nrepository_id = {repository_id:?}\ncreated_at = {created_at:?}\n"
    );
    fs::create_dir_all(marker_path(root).parent().unwrap()).unwrap();
    fs::write(marker_path(root), content).unwrap();
}

fn write_managed_marker(root: &Path, repository_id: &str) {
    write_marker(root, repository_id, "MINE", "2026-07-23T00:00:00Z");
}

fn write_config(root: &Path, repository_id: &str, mine_code_version: &str) {
    let content = format!(
        "schema_version = 1\nrepository_id = {repository_id:?}\nmine_code_version = {mine_code_version:?}\n\n[branches]\nstable = \"master\"\nintegration = \"dev\"\n\n[design]\nroot = \".mine/design/index.md\"\nmarker = \".mine/design/.mine-design.toml\"\nlanguage = \"en\"\nindex_soft_limit_lines = 250\nleaf_soft_limit_lines = 400\n\n[plan]\nroot = \".mine/plan\"\nephemeral = true\npurge_before_stable_release = true\n\n[graph]\nsource = \".mine/plan/execution-graph.toml\"\nrendered = \".mine/plan/execution-graph.md\"\nlock_timeout_ms = 5000\n"
    );
    fs::create_dir_all(config_path(root).parent().unwrap()).unwrap();
    fs::write(config_path(root), content).unwrap();
}

fn read_marker(root: &Path) -> DesignMarker {
    let path = marker_path(root);
    let content = fs::read_to_string(&path).unwrap();
    DesignMarker::parse(&path, &content).unwrap()
}

fn read_config(root: &Path) -> MineConfig {
    let path = config_path(root);
    let content = fs::read_to_string(&path).unwrap();
    MineConfig::parse(&path, &content).unwrap()
}

#[test]
fn absent_design_root_creates_scaffold_marker_and_config() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let outcome = run_init(root).expect("absent root initializes");

    assert_eq!(outcome.repository_id, "fixed-uuid-0000-0000-000000000000");
    assert_eq!(outcome.mine_code_version, "0.1.0");
    assert_eq!(outcome.design_root, DesignRootSummary::Absent);

    // Marker created with the generated id and fixed timestamp.
    let marker = read_marker(root);
    assert_eq!(marker.repository_id, "fixed-uuid-0000-0000-000000000000");
    assert_eq!(marker.managed_by, "MINE");
    assert_eq!(marker.schema_version, 1);
    assert_eq!(marker.created_at, "2026-07-23T00:00:00Z");

    // Config created and persisted.
    let config = read_config(root);
    assert_eq!(config.repository_id, "fixed-uuid-0000-0000-000000000000");
    assert_eq!(config.mine_code_version, "0.1.0");
    assert_eq!(config.branches.stable, "master");
    assert_eq!(config.branches.integration, "dev");
    assert_eq!(config.design.root, ".mine/design/index.md");
    assert!(config.plan.ephemeral);
    assert!(config.plan.purge_before_stable_release);

    // Design root index scaffold created.
    assert!(root.join(".mine").join("design").join("index.md").exists());

    // Runtime ignore rules created.
    let gitignore = root.join(".mine").join(".gitignore");
    assert!(gitignore.exists());
    let ignore_content = fs::read_to_string(gitignore).unwrap();
    assert!(ignore_content.contains("runtime/"));
    assert!(ignore_content.contains("locks/"));
    assert!(ignore_content.contains("build/"));

    // AGENTS.md created.
    assert!(root.join("AGENTS.md").exists());

    // No plan workspace is created by init.
    assert!(!root.join(".mine").join("plan").exists());

    // Marker and config creation recorded as actions.
    assert!(outcome.actions.iter().any(
        |a| matches!(a, InitAction::Created(p) if p == &marker_path(root).canonicalize().unwrap())
    ));
    assert!(
        outcome
            .actions
            .iter()
            .any(|a| matches!(a, InitAction::Created(p) if p == &config_path(root)))
    );
}

#[test]
fn init_is_idempotent_when_managed() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let first = run_init(root).expect("first init succeeds");
    let marker_before = fs::read_to_string(marker_path(root)).unwrap();
    let config_before = fs::read_to_string(config_path(root)).unwrap();

    let second = run_init(root).expect("second init succeeds");

    assert_eq!(second.repository_id, first.repository_id);
    assert_eq!(second.mine_code_version, first.mine_code_version);
    assert_eq!(second.design_root, DesignRootSummary::Managed);

    // Marker preserved byte-for-byte (created_at not bumped).
    assert_eq!(
        fs::read_to_string(marker_path(root)).unwrap(),
        marker_before
    );
    // Config preserved byte-for-byte (not rewritten).
    assert_eq!(
        fs::read_to_string(config_path(root)).unwrap(),
        config_before
    );
    // Every follow-up action is a Preserve (no Created/CreatedSection).
    assert!(
        second
            .actions
            .iter()
            .all(|a| matches!(a, InitAction::Preserved(_)))
    );
}

#[test]
fn user_docs_are_not_claimed_or_backed_up() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("docs/design")).unwrap();
    fs::write(
        root.join("docs/design/legacy.md"),
        "user architecture notes",
    )
    .unwrap();
    run_init(root).expect("new governance does not occupy user docs");
    assert!(marker_path(root).exists());
    assert_eq!(
        fs::read_to_string(root.join("docs/design/legacy.md")).unwrap(),
        "user architecture notes"
    );
    assert_eq!(fs::read_dir(root.join("docs")).unwrap().count(), 1);
}

#[test]
fn foreign_marker_is_refused_and_preserved() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_marker(root, "some-id", "OtherTool", "2026-07-23T00:00:00Z");
    let before = fs::read(marker_path(root)).unwrap();
    let error = run_init(root).expect_err("foreign governance cannot be claimed");
    assert_eq!(error.code(), "MINE_DESIGN_NAMESPACE_CONFLICT");
    assert_eq!(fs::read(marker_path(root)).unwrap(), before);
    assert!(!config_path(root).exists());
}

#[test]
fn ownership_mismatch_is_rejected_without_mutation() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_managed_marker(root, "marker-id-1111-1111-111111111111");
    write_config(root, "config-id-2222-2222-222222222222", "0.1.0");
    let marker_before = fs::read_to_string(marker_path(root)).unwrap();
    let config_before = fs::read_to_string(config_path(root)).unwrap();

    let err = run_init(root).expect_err("ownership mismatch is rejected");
    assert_eq!(err.code(), "MINE_DESIGN_OWNERSHIP_MISMATCH");

    assert_eq!(
        fs::read_to_string(marker_path(root)).unwrap(),
        marker_before
    );
    assert_eq!(
        fs::read_to_string(config_path(root)).unwrap(),
        config_before
    );
}

#[test]
fn malformed_marker_is_rejected_without_mutation() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::create_dir_all(marker_path(root).parent().unwrap()).unwrap();
    fs::write(marker_path(root), "this is = = not valid toml =\n").unwrap();
    let marker_before = fs::read_to_string(marker_path(root)).unwrap();

    let err = run_init(root).expect_err("malformed marker is rejected");
    assert_eq!(err.code(), "MINE_DESIGN_MARKER_INVALID");

    assert_eq!(
        fs::read_to_string(marker_path(root)).unwrap(),
        marker_before
    );
    assert!(!config_path(root).exists());
}

#[test]
fn managed_marker_with_config_preserves_existing_values() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_managed_marker(root, "preserved-id-3333-3333-333333333333");
    write_config(root, "preserved-id-3333-3333-333333333333", "2.1.0");
    let marker_before = read_marker(root);
    let config_before = fs::read_to_string(config_path(root)).unwrap();

    let outcome = run_init(root).expect("managed root initializes");

    assert_eq!(outcome.repository_id, "preserved-id-3333-3333-333333333333");
    assert_eq!(outcome.mine_code_version, "2.1.0");
    assert_eq!(outcome.design_root, DesignRootSummary::Managed);

    let marker_after = read_marker(root);
    assert_eq!(marker_after.created_at, marker_before.created_at);
    assert_eq!(marker_after.repository_id, marker_before.repository_id);
    assert_eq!(
        fs::read_to_string(config_path(root)).unwrap(),
        config_before
    );
}

#[test]
fn root_version_evidence_is_used_when_config_absent() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"3.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();

    let outcome = run_init(root).expect("absent root initializes");

    assert_eq!(outcome.mine_code_version, "3.1.0");
    let config = read_config(root);
    assert_eq!(config.mine_code_version, "3.1.0");
}

#[test]
fn agents_md_section_appended_idempotently() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    fs::write(root.join("AGENTS.md"), "# Project rules\n\nDo good work.\n").unwrap();

    let first = run_init(root).expect("first init succeeds");
    assert!(
        first
            .actions
            .iter()
            .any(|a| matches!(a, InitAction::CreatedSection(_)))
    );

    let agents_after = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents_after.contains("Do good work."));
    assert!(agents_after.contains("<!-- mine-managed-agents -->"));
    assert_eq!(
        agents_after.matches("<!-- mine-managed-agents -->").count(),
        1
    );

    let second = run_init(root).expect("second init succeeds");
    assert!(
        second
            .actions
            .iter()
            .any(|a| matches!(a, InitAction::Preserved(p) if p == &root.join("AGENTS.md")))
    );
    let agents_final = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert_eq!(
        agents_final.matches("<!-- mine-managed-agents -->").count(),
        1
    );
}

#[test]
fn invalid_existing_config_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_managed_marker(root, "id-4444-4444-444444444444");
    fs::create_dir_all(config_path(root).parent().unwrap()).unwrap();
    fs::write(config_path(root), "schema_version = 1\n").unwrap();

    let err = run_init(root).expect_err("invalid config is rejected");
    assert_eq!(err.code(), "MINE_CONFIG_INVALID");
}
