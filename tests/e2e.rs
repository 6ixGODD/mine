// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! End-to-end tests: release preflight, bootstrap, self-hosting,
//! four-Agent isolated installation, real MCP subprocess discovery, the six
//! Work Package 3 E2E fixtures, and adversarial nested-backup detection.

#[path = "e2e/agent_e2e_tests.rs"]
mod agent_e2e_tests;
#[path = "e2e/bootstrap_tests.rs"]
mod bootstrap_tests;
#[path = "e2e/doctor_stable_tests.rs"]
mod doctor_stable_tests;
#[path = "e2e/fixture_tests.rs"]
mod fixture_tests;
#[path = "e2e/mcp_e2e_tests.rs"]
mod mcp_e2e_tests;
#[path = "e2e/nested_backup_tests.rs"]
mod nested_backup_tests;
#[path = "e2e/release_tests.rs"]
mod release_tests;

/// Private E2E controller: commit only its own initialized setup before native
/// graph publication, and perform the Skill-owned integration branch switch.
fn managed_private_dev(root: &std::path::Path) {
    std::fs::write(
        root.join(".gitignore"),
        "/.mine/runtime/\n/.mine/locks/\n/.mine/build/\n/.mine/worktrees/\n",
    )
    .unwrap();
    fixture_git(root, &["config", "commit.gpgSign", "false"]);
    fixture_git(root, &["config", "core.autocrlf", "false"]);
    fixture_git(
        root,
        &[
            "add",
            "--",
            ".gitignore",
            ".mine/config.toml",
            ".mine/.gitignore",
            ".mine/.gitattributes",
            "AGENTS.md",
            ".mine/design",
        ],
    );
    fixture_git(
        root,
        &["commit", "--quiet", "-m", "private committed MINE setup"],
    );
    fixture_git(root, &["switch", "-c", "dev"]);
}

fn fixture_git(root: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
