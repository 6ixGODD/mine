// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Adversarial test: committed nested Design-backup blocks release preflight.
//! Proves the recursive `git ls-tree -r --name-only` gate
//! detects nested `docs/design-backup-*/...` paths at arbitrary depth.
//!
//! Tests the actual production `branch_has_design_backups` function directly
//! (not through the full preflight stack, which requires a complete MINE repo).

use std::path::Path;
use std::process::Command;

fn init_git_repo(dir: &Path) {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["init", "--quiet"])
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

fn git_commit(dir: &Path, msg: &str) {
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

// Import the production function directly.
use mine::application::release_service::branch_has_design_backups;

#[test]
fn committed_nested_design_backup_detected_on_branch() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    std::fs::write(tmp.path().join("README.md"), "stable\n").unwrap();
    git_commit(tmp.path(), "initial");

    // Commit a nested design backup on the current branch.
    let backup_dir = tmp.path().join("docs/design-backup-20260101T000000Z");
    std::fs::create_dir_all(&backup_dir).unwrap();
    std::fs::write(backup_dir.join("leaf.md"), "backup content").unwrap();
    std::fs::create_dir_all(backup_dir.join("nested/deep")).unwrap();
    std::fs::write(backup_dir.join("nested/deep/file.md"), "deep content").unwrap();
    git_commit(tmp.path(), "add nested design backup");

    // The production function must detect the nested backup.
    let result = branch_has_design_backups(tmp.path(), "HEAD");
    assert!(result.is_ok(), "branch_has_design_backups must not crash");
    assert!(
        result.unwrap(),
        "committed nested design-backup MUST be detected by recursive ls-tree"
    );
}

#[test]
fn multiple_nested_backup_files_detected() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    std::fs::write(tmp.path().join("README.md"), "stable\n").unwrap();
    git_commit(tmp.path(), "initial");

    for ts in ["20260101T000000Z", "20260102T120000Z"] {
        let dir = tmp.path().join(format!("docs/design-backup-{ts}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.md"), "backup").unwrap();
    }
    git_commit(tmp.path(), "multiple backups");

    let result = branch_has_design_backups(tmp.path(), "HEAD");
    assert!(result.unwrap(), "multiple nested backups detected");
}

#[test]
fn unrelated_similar_name_not_flagged() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    std::fs::write(tmp.path().join("README.md"), "stable\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("docs/design-notes")).unwrap();
    std::fs::write(tmp.path().join("docs/design-notes/notes.md"), "notes").unwrap();
    std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
    git_commit(tmp.path(), "unrelated similar name");

    let result = branch_has_design_backups(tmp.path(), "HEAD");
    assert!(
        !result.unwrap(),
        "design-notes is NOT a design-backup-* path; must not be flagged"
    );
}

#[test]
fn clean_repository_has_no_design_backups() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    std::fs::write(tmp.path().join("README.md"), "stable\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("docs/design")).unwrap();
    std::fs::write(tmp.path().join("docs/design/index.md"), "# Design").unwrap();
    git_commit(tmp.path(), "clean repo with design, no backups");

    let result = branch_has_design_backups(tmp.path(), "HEAD");
    assert!(
        !result.unwrap(),
        "clean repository with no backups must not be flagged"
    );
}

#[test]
fn git_tree_inspection_failure_fails_closed() {
    // An invalid private Git marker makes discovery fail, even when isolated
    // TEMP is beneath the real repository. Never discover an ancestor Root.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(".git"), "gitdir: absent-private-admin\n").unwrap();
    let result = branch_has_design_backups(tmp.path(), "master");
    assert!(
        result.unwrap(),
        "non-git directory must fail closed (backups suspected)"
    );
}

#[test]
fn cleanup_preview_lists_design_backups_under_docs() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("docs/design-backup-20260101T000000Z")).unwrap();
    std::fs::write(
        tmp.path()
            .join("docs/design-backup-20260101T000000Z/backup.md"),
        "backup content",
    )
    .unwrap();
    let preview = mine::application::release_service::preview_cleanup(tmp.path()).unwrap();
    assert!(
        preview
            .design_backups
            .iter()
            .any(|f| f.contains("design-backup-")),
        "design backup under docs/ detected: {:?}",
        preview.design_backups
    );
}
