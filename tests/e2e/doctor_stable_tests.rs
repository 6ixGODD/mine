// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Adversarial tests: stable-tree doctor compatibility.
//!
//! These tests prove that `mine doctor --agents all` correctly handles:
//! 1. a valid graph-less stable candidate (branch == configured stable,
//!    no `.mine/plan/`) reports healthy;
//! 2. all four isolated Agent installations are healthy on that stable tree;
//! 3. a development repository with an unexpectedly missing graph still fails;
//! 4. an invalid graph still fails;
//! 5. merely deleting `.mine/plan/` on a non-stable branch does NOT falsely
//!    convert the repository into a valid stable tree;
//! 6. Agent drift or missing files still produces unhealthy Agent diagnostics;
//! 7. repository failure does not suppress the computed Agent diagnostics
//!    (partial-failure case: error envelope carries `error.details.agents`);
//! 8. JSON and human-readable output remain truthful and deterministic;
//! 9. real HOME and real Agent configurations remain untouched.

use mine::cli;
use std::path::{Path, PathBuf};
use std::process::Command;

const FOUR_AGENTS: &[&str] = &["claude-code", "codex", "pi", "opencode"];

#[allow(dead_code)]
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn real_homedir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn files_under(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(
                        p.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
    }
    out.sort();
    out
}

/// Initializes a temp git repo with an initial commit on the default branch.
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
    std::fs::write(dir.join("README.md"), "test\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["add", "."])
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["commit", "--quiet", "-m", "initial"])
        .status()
        .unwrap();
}

fn git_branch(dir: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["symbolic-ref", "--short", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn git_checkout_b(dir: &Path, branch: &str) {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["checkout", "-b", branch, "--quiet"])
        .status()
        .unwrap();
}

/// Runs `mine --repo <repo> <args...> --config-root <root>` and returns
/// (exit_code, parsed_json_envelope).
fn run_mine(repo: &Path, config_root: &Path, args: &[&str]) -> (i32, serde_json::Value) {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        repo.to_string_lossy().to_string(),
    ];
    for a in args {
        argv.push(a.to_string());
    }
    argv.push("--config-root".to_string());
    argv.push(config_root.to_string_lossy().to_string());
    let outcome = cli::dispatch(&argv, "mine");
    let (stdout, stderr) = cli::render(&outcome, true, false);
    let body = if outcome.exit_code == 0 {
        stdout
    } else {
        stderr
    };
    (
        outcome.exit_code,
        serde_json::from_str(&body).unwrap_or(serde_json::Value::Null),
    )
}

/// Runs `mine --repo <repo> doctor --agents <agents> --config-root <root>`
/// and returns (exit_code, parsed_json_envelope, human_output).
fn run_doctor(repo: &Path, config_root: &Path, agents: &str) -> (i32, serde_json::Value, String) {
    let argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        repo.to_string_lossy().to_string(),
        "doctor".to_string(),
        "--agents".to_string(),
        agents.to_string(),
        "--config-root".to_string(),
        config_root.to_string_lossy().to_string(),
    ];
    let outcome = cli::dispatch(&argv, "mine");
    let (stdout_json, stderr_json) = cli::render(&outcome, true, false);
    let (_stdout_human, stderr_human) = cli::render(&outcome, false, false);
    let body = if outcome.exit_code == 0 {
        stdout_json
    } else {
        stderr_json
    };
    let human = if outcome.exit_code == 0 {
        _stdout_human
    } else {
        stderr_human
    };
    (
        outcome.exit_code,
        serde_json::from_str(&body).unwrap_or(serde_json::Value::Null),
        human,
    )
}

/// Builds an isolated stable candidate: a temp git repo on the default
/// branch (detected by `mine init` as the stable branch), with `.mine/config.toml`,
/// `.mine/design/.mine-design.toml`, `.mine/design/index.md`, and NO `.mine/plan/`.
/// Returns the temp dir.
fn build_stable_candidate() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_git_repo(repo);
    // `mine init` detects the current branch as the stable branch and writes
    // it into `.mine/config.toml` under `branches.stable`. It creates the
    // design scaffold but does NOT create `.mine/plan/`.
    let (exit, env) = run_mine(repo, repo, &["init", "--format", "json"]);
    assert_eq!(exit, 0, "mine init failed: {env}");
    assert!(repo.join(".mine/config.toml").exists(), "config created");
    assert!(
        repo.join(".mine/design/.mine-design.toml").exists(),
        "marker created"
    );
    assert!(repo.join(".mine/design/index.md").exists(), "index created");
    assert!(
        !repo.join(".mine/plan").exists(),
        "no plan workspace on stable"
    );
    tmp
}

/// Reads the configured stable branch from `.mine/config.toml`.
fn configured_stable_branch(repo: &Path) -> String {
    let cfg = std::fs::read_to_string(repo.join(".mine/config.toml")).unwrap();
    for line in cfg.lines() {
        if line.trim_start().starts_with("stable") {
            return line
                .split('=')
                .nth(1)
                .unwrap()
                .trim()
                .trim_matches('"')
                .to_string();
        }
    }
    panic!("stable branch not found in config");
}

// ---------------------------------------------------------------------------
// Test 1: stable candidate without .mine/plan/ passes `mine doctor --agents all`
// ---------------------------------------------------------------------------

#[test]
fn stable_candidate_without_plan_passes_doctor() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let cfg_tmp = tempfile::tempdir().unwrap();

    // Install all four agents into the isolated config root.
    for slug in FOUR_AGENTS {
        let (exit, env) = run_mine(repo, cfg_tmp.path(), &["agent", "install", slug]);
        assert_eq!(exit, 0, "install {slug} failed: {env}");
    }

    let (exit, env, _human) = run_doctor(repo, cfg_tmp.path(), "all");
    assert_eq!(exit, 0, "doctor must succeed on stable tree: {env}");
    assert_eq!(env["ok"], true, "ok must be true");
    assert_eq!(
        env["data"]["healthy"], true,
        "doctor must report healthy on valid stable tree"
    );

    // The graph check must be "not applicable", not a failure.
    let checks = env["data"]["checks"]
        .as_array()
        .expect("checks array exists");
    let graph_check = checks
        .iter()
        .find(|c| c["name"] == "graph")
        .expect("graph check exists");
    assert_eq!(
        graph_check["ok"], true,
        "graph check must be ok on stable tree"
    );
    assert!(
        graph_check["message"]
            .as_str()
            .unwrap()
            .contains("not applicable"),
        "graph check must report 'not applicable' on stable tree, got: {}",
        graph_check["message"]
    );

    // Verify the stable branch is correctly identified.
    let stable = configured_stable_branch(repo);
    assert_eq!(
        git_branch(repo),
        stable,
        "must be on the configured stable branch"
    );
}

// ---------------------------------------------------------------------------
// Test 2: all four isolated Agent installations are reported healthy
// ---------------------------------------------------------------------------

#[test]
fn all_four_agents_healthy_on_stable_tree() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let cfg_tmp = tempfile::tempdir().unwrap();

    for slug in FOUR_AGENTS {
        let (exit, env) = run_mine(repo, cfg_tmp.path(), &["agent", "install", slug]);
        assert_eq!(exit, 0, "install {slug} failed: {env}");
    }

    let (exit, env, _) = run_doctor(repo, cfg_tmp.path(), "all");
    assert_eq!(exit, 0, "doctor must succeed: {env}");

    let diags = env["data"]["agents"]["diagnostics"]
        .as_array()
        .expect("diagnostics array exists");
    assert_eq!(diags.len(), 4, "all four agents diagnosed");
    for d in diags {
        let agent = d["agent"].as_str().unwrap();
        assert_eq!(
            d["status"], "healthy",
            "agent {agent} must be healthy on stable tree"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: development repository with unexpectedly missing graph still fails
// ---------------------------------------------------------------------------

#[test]
fn dev_repo_with_missing_graph_fails() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let cfg_tmp = tempfile::tempdir().unwrap();

    // Switch to dev branch (non-stable).
    git_checkout_b(repo, "dev");
    assert_ne!(git_branch(repo), configured_stable_branch(repo));

    // No .mine/plan/ exists (mine init didn't create it). On a dev branch,
    // the missing graph is a real failure.
    assert!(!repo.join(".mine/plan").exists());

    let (exit, env, _) = run_doctor(repo, cfg_tmp.path(), "all");
    assert_ne!(exit, 0, "doctor must fail on dev repo with missing graph");
    assert_eq!(env["ok"], false, "ok must be false");

    let checks = if exit == 0 {
        env["data"]["checks"].as_array().unwrap().clone()
    } else {
        env["error"]["details"]["checks"]
            .as_array()
            .unwrap()
            .clone()
    };
    let graph_check = checks
        .iter()
        .find(|c| c["name"] == "graph")
        .expect("graph check exists");
    assert_eq!(
        graph_check["ok"], false,
        "graph check must fail on dev repo with missing graph"
    );
    assert!(
        !graph_check["message"]
            .as_str()
            .unwrap()
            .contains("not applicable"),
        "graph check must NOT say 'not applicable' on dev branch"
    );
}

// ---------------------------------------------------------------------------
// Test 4: invalid graph still fails (even on stable branch)
// ---------------------------------------------------------------------------

#[test]
fn invalid_graph_fails_on_any_branch() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let cfg_tmp = tempfile::tempdir().unwrap();

    // We're still on the stable branch. Create a corrupt graph file.
    // Even on the stable branch, a present-but-invalid graph is a failure
    // (malformed repository), NOT "not applicable".
    std::fs::create_dir_all(repo.join(".mine/plan")).unwrap();
    std::fs::write(
        repo.join(".mine/plan/execution-graph.toml"),
        "this is not valid TOML {{{",
    )
    .unwrap();

    let (exit, env, _) = run_doctor(repo, cfg_tmp.path(), "all");
    assert_ne!(exit, 0, "doctor must fail with invalid graph");
    assert_eq!(env["ok"], false, "ok must be false");

    let checks = env["error"]["details"]["checks"]
        .as_array()
        .unwrap()
        .clone();
    let graph_check = checks
        .iter()
        .find(|c| c["name"] == "graph")
        .expect("graph check exists");
    assert_eq!(
        graph_check["ok"], false,
        "graph check must fail with invalid graph"
    );
    assert!(
        graph_check["message"].as_str().unwrap().contains("invalid"),
        "graph check must report 'invalid' for corrupt graph, got: {}",
        graph_check["message"]
    );
}

// ---------------------------------------------------------------------------
// Test 5: merely deleting .mine/plan/ cannot falsely convert to stable tree
// ---------------------------------------------------------------------------

#[test]
fn deleting_plan_does_not_falsely_convert_to_stable() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let cfg_tmp = tempfile::tempdir().unwrap();

    // Switch to dev (non-stable branch).
    git_checkout_b(repo, "dev");

    // Simulate "merely deleting .mine/plan/" — create it, then delete it.
    std::fs::create_dir_all(repo.join(".mine/plan")).unwrap();
    std::fs::write(
        repo.join(".mine/plan/execution-graph.toml"),
        "revision = 1\nworkspace_id = \"test\"\nstable_branch = \"master\"\nintegration_branch = \"dev\"\nplans = []\n",
    )
    .unwrap();
    assert!(repo.join(".mine/plan").exists());
    std::fs::remove_dir_all(repo.join(".mine/plan")).unwrap();
    assert!(!repo.join(".mine/plan").exists());

    // On dev branch, the missing graph must NOT be treated as "not applicable".
    let (exit, env, _) = run_doctor(repo, cfg_tmp.path(), "all");
    assert_ne!(
        exit, 0,
        "doctor must fail: deleting .mine/plan/ on dev does not make a stable tree"
    );

    let checks = env["error"]["details"]["checks"]
        .as_array()
        .unwrap()
        .clone();
    let graph_check = checks
        .iter()
        .find(|c| c["name"] == "graph")
        .expect("graph check exists");
    assert_eq!(
        graph_check["ok"], false,
        "graph must fail: not a stable tree just because .mine/plan/ is absent"
    );
    assert!(
        !graph_check["message"]
            .as_str()
            .unwrap()
            .contains("not applicable"),
        "must not say 'not applicable' on non-stable branch"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Agent drift or missing files still produces unhealthy diagnostics
// ---------------------------------------------------------------------------

#[test]
fn agent_drift_produces_unhealthy_diagnostics() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let cfg_tmp = tempfile::tempdir().unwrap();

    // Install all four agents, then corrupt one skill file.
    for slug in FOUR_AGENTS {
        let (exit, env) = run_mine(repo, cfg_tmp.path(), &["agent", "install", slug]);
        assert_eq!(exit, 0, "install {slug} failed: {env}");
    }

    // Corrupt a Claude Code skill file (drift). Claude Code always uses its
    // own skills directory, so this is stable regardless of the Pi shared-
    // Skills deduplication rule.
    let cc_skill = cfg_tmp.path().join(".claude/skills/mine-sync/SKILL.md");
    assert!(cc_skill.exists(), "claude skill file exists before drift");
    std::fs::write(&cc_skill, "CORRUPTED CONTENT\n").unwrap();

    let (exit, env, _) = run_doctor(repo, cfg_tmp.path(), "all");
    // Doctor exit code: repo_ok is true (stable tree), but agent problems exist.
    // The existing contract returns exit 0 with healthy=false in data.
    assert_eq!(exit, 0, "doctor runs on stable tree even with agent drift");
    assert_eq!(
        env["data"]["healthy"], false,
        "healthy must be false when agent drift is detected"
    );

    let diags = env["data"]["agents"]["diagnostics"]
        .as_array()
        .expect("diagnostics array exists");
    let cc = diags.iter().find(|d| d["agent"] == "claude-code").unwrap();
    assert_ne!(
        cc["status"], "healthy",
        "claude-code must not be healthy after drift"
    );
}

// ---------------------------------------------------------------------------
// Test 7: repository failure does not suppress computed Agent diagnostics
// ---------------------------------------------------------------------------

#[test]
fn repository_failure_preserves_agent_diagnostics() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let cfg_tmp = tempfile::tempdir().unwrap();

    // Switch to dev (non-stable) so the missing graph is a real failure.
    git_checkout_b(repo, "dev");

    // Install all four agents.
    for slug in FOUR_AGENTS {
        let (exit, env) = run_mine(repo, cfg_tmp.path(), &["agent", "install", slug]);
        assert_eq!(exit, 0, "install {slug} failed: {env}");
    }

    // Run doctor: repo_ok=false (missing graph on dev), but agents were computed.
    let (exit, env, _) = run_doctor(repo, cfg_tmp.path(), "all");
    assert_ne!(exit, 0, "doctor must fail: missing graph on dev branch");
    assert_eq!(env["ok"], false, "ok must be false");

    // The error envelope's details MUST include the agent diagnostics.
    let agents = &env["error"]["details"]["agents"];
    assert!(
        !agents.is_null(),
        "agent diagnostics must be preserved in error.details even when repo fails"
    );
    let diags = agents["diagnostics"]
        .as_array()
        .expect("diagnostics array exists in error details");
    assert_eq!(diags.len(), 4, "all four agents diagnosed in error path");
    for d in diags {
        assert!(
            d["status"].is_string(),
            "each agent has a status in the error path"
        );
    }

    // The checks must also be in the error details.
    let checks = env["error"]["details"]["checks"]
        .as_array()
        .expect("checks array exists in error details");
    assert!(
        checks
            .iter()
            .any(|c| c["name"] == "graph" && c["ok"] == false),
        "graph check must be in error details and failing"
    );
}

// ---------------------------------------------------------------------------
// Test 8: JSON and human-readable output remain truthful and deterministic
// ---------------------------------------------------------------------------

#[test]
fn json_and_human_output_truthful_and_deterministic() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let cfg_tmp = tempfile::tempdir().unwrap();

    for slug in FOUR_AGENTS {
        run_mine(repo, cfg_tmp.path(), &["agent", "install", slug]);
    }

    // Run twice: JSON output must be byte-identical (deterministic).
    let (_exit1, env1, human1) = run_doctor(repo, cfg_tmp.path(), "all");
    let (_exit2, env2, human2) = run_doctor(repo, cfg_tmp.path(), "all");

    let json1 = serde_json::to_string(&env1).unwrap();
    let json2 = serde_json::to_string(&env2).unwrap();
    assert_eq!(json1, json2, "JSON output must be deterministic");

    assert_eq!(human1, human2, "human output must be deterministic");

    // Truthful: JSON has the expected structure.
    assert_eq!(env1["ok"], true);
    assert_eq!(env1["command"], "doctor");
    assert!(env1["data"]["checks"].is_array(), "checks is an array");
    assert!(
        env1["data"]["agents"]["diagnostics"].is_array(),
        "agents.diagnostics is an array"
    );

    // Truthful: human output mentions "healthy" and the checks.
    assert!(
        human1.contains("healthy") || human1.contains("mine doctor"),
        "human output must mention health status: {human1}"
    );
    assert!(
        human1.contains("graph"),
        "human output must mention the graph check: {human1}"
    );

    // The graph check message must contain "not applicable" on stable tree.
    let checks = env1["data"]["checks"].as_array().unwrap();
    let graph_check = checks.iter().find(|c| c["name"] == "graph").unwrap();
    assert!(
        graph_check["message"]
            .as_str()
            .unwrap()
            .contains("not applicable"),
        "JSON graph check must say 'not applicable' on stable tree"
    );
}

// ---------------------------------------------------------------------------
// Test 9: real HOME and real Agent configurations remain untouched
// ---------------------------------------------------------------------------

#[test]
fn real_home_and_agent_configs_untouched() {
    let tmp = build_stable_candidate();
    let repo = tmp.path();
    let home = real_homedir();
    let dirs = [".claude", ".codex", ".pi", ".config/opencode"];

    let before: Vec<(String, Vec<String>)> = dirs
        .iter()
        .map(|d| {
            let p = home.join(d);
            let files = if p.exists() {
                files_under(&p)
            } else {
                Vec::new()
            };
            (d.to_string(), files)
        })
        .collect();

    let cfg_tmp = tempfile::tempdir().unwrap();

    // Install all four agents into the isolated config root.
    for slug in FOUR_AGENTS {
        run_mine(repo, cfg_tmp.path(), &["agent", "install", slug]);
    }

    // Run doctor.
    let (exit, env, _) = run_doctor(repo, cfg_tmp.path(), "all");
    assert_eq!(exit, 0, "doctor must succeed: {env}");

    // Verify real HOME is unchanged.
    let after: Vec<(String, Vec<String>)> = dirs
        .iter()
        .map(|d| {
            let p = home.join(d);
            let files = if p.exists() {
                files_under(&p)
            } else {
                Vec::new()
            };
            (d.to_string(), files)
        })
        .collect();
    assert_eq!(before, after, "real HOME Agent dirs must be unchanged");
}
