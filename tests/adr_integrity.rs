// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! ADR bootstrap and forged-body regression tests.

use mine::cli;
use mine::domain::adr::sha256_hex;
use mine::domain::config::AdrConfig;
use mine::infrastructure::adr_store::AdrStore;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path);
        } else {
            std::fs::copy(source_path, destination_path).unwrap();
        }
    }
}

fn bootstrap_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    copy_tree(
        &repo_root().join("docs/design"),
        &temp.path().join("docs/design"),
    );
    std::fs::create_dir_all(temp.path().join(".mine")).unwrap();
    std::fs::copy(
        repo_root().join(".mine/config.toml"),
        temp.path().join(".mine/config.toml"),
    )
    .unwrap();
    std::fs::remove_file(temp.path().join("docs/design/decisions/adr-registry.toml")).unwrap();
    std::fs::remove_file(temp.path().join("docs/design/decisions/index.md")).unwrap();
    AdrStore::new(temp.path(), &AdrConfig::default())
        .bootstrap_if_absent("2026-08-21T00:00:00Z")
        .unwrap();
    temp
}

fn run(repo: &Path, args: &[&str]) -> i32 {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        repo.to_string_lossy().to_string(),
    ];
    argv.extend(args.iter().map(|arg| (*arg).to_string()));
    cli::dispatch(&argv, "mine").exit_code
}

#[test]
fn bootstrap_binds_all_nine_approved_entries_to_current_bytes() {
    let temp = bootstrap_fixture();
    let registry = AdrStore::new(temp.path(), &AdrConfig::default())
        .load()
        .unwrap();
    assert_eq!(registry.entries.len(), 9);
    for entry in &registry.entries {
        let bytes = std::fs::read(temp.path().join(&entry.path)).unwrap();
        assert_eq!(
            entry.content_sha256,
            sha256_hex(&bytes),
            "bootstrap must persist the exact current bytes for {}",
            entry.id
        );
        if ("ADR-0007"..="ADR-0009").contains(&entry.id.as_str()) {
            assert!(
                entry
                    .approval_note
                    .contains("non-authoritative audit evidence"),
                "{} must retain historical evidence only as an audit note",
                entry.id
            );
        }
    }
}

#[test]
fn forged_bootstrap_digest_fails_cli_validation_and_readiness() {
    let temp = bootstrap_fixture();
    let registry = AdrStore::new(temp.path(), &AdrConfig::default())
        .load()
        .unwrap();
    let entry = registry.get("ADR-0007").unwrap();
    let body_path = temp.path().join(&entry.path);
    let original = std::fs::read_to_string(&body_path).unwrap();
    let historical = original
        .lines()
        .find_map(|line| line.split('`').nth(1))
        .unwrap()
        .to_string();
    let forged = original
        .replace(&historical, &entry.content_sha256)
        .replace(
            "Adopt the following case-sensitive",
            "Adopt this unauthorized case-sensitive",
        );
    assert_ne!(forged, original);
    std::fs::write(&body_path, forged).unwrap();

    assert_ne!(
        run(temp.path(), &["adr", "validate", "--format", "json"]),
        0,
        "forged ADR-0007 body digest must fail validation"
    );
    assert_ne!(
        run(
            temp.path(),
            &[
                "design",
                "readiness",
                "--scope",
                // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                "plan01",
                "--format",
                "json"
            ]
        ),
        0,
        "forged ADR-0007 body digest must fail readiness"
    );
}
