// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Release preflight tests: version resolution, refusal on dirty/divergent/
//! incomplete repositories, stable-tree checks.

use mine::cli;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str], repo: &std::path::Path) -> (i32, serde_json::Value) {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        repo.to_string_lossy().to_string(),
    ];
    argv.extend(args.iter().map(|s| s.to_string()));
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

#[test]
fn release_preflight_reports_version_and_state() {
    let repo = repo_root();
    let (exit, env) = run(&["release", "--format", "json"], &repo);
    // Stable release trees intentionally omit the temporary graph workspace,
    // so preflight is not applicable there. Development trees instead return
    // the readiness data without crashing.
    if exit == 0 {
        assert_eq!(env["ok"], true);
        assert!(env["data"]["can_release"].is_boolean());
    } else {
        assert_eq!(exit, 4, "unexpected release-preflight failure: {env}");
        assert!(
            matches!(
                env["error"]["code"].as_str(),
                Some("MINE_GRAPH_NOT_INITIALIZED" | "MINE_GRAPH_INVALID")
            ),
            "temporary graph absence or fail-closed schema refusal expected: {env}"
        );
    }
}

#[test]
fn release_preflight_fails_on_non_git_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let (exit, env) = run(&["release", "--format", "json"], tmp.path());
    // A non-git temp dir with no .mine/config.toml should fail gracefully.
    // On a non-git dir with no config, preflight returns an error
    // (MINE_GRAPH_NOT_INITIALIZED at exit 4), which is expected.
    // A non-git/dir with no graph returns exit 4 (MINE_GRAPH_NOT_INITIALIZED);
    // that is an expected refusal, not a crash. The data envelope may be Null
    // (error path), so we only verify it is not a usage error (exit 2).
    assert_ne!(exit, 2, "preflight must not crash with usage error: {env}");
}

#[test]
fn dist_verify_passes_on_repo() {
    let repo = repo_root();
    let (exit, _env) = run(&["dist", "verify", "--format", "json"], &repo);
    assert_eq!(exit, 0, "dist verify must pass on the repo");
}

#[test]
fn release_version_source_is_config_not_hardcoded() {
    let repo = repo_root();
    let (_exit, env) = run(
        &["repository", "version", "show", "--format", "json"],
        &repo,
    );
    let version = env["data"]["version"].as_str().unwrap_or("");
    assert!(
        !version.is_empty(),
        "version comes from config.toml, not hardcoded"
    );
    // The version must match what .mine/config.toml records, proving it is
    // read from config rather than a hardcoded constant.
    let config = std::fs::read_to_string(repo.join(".mine/config.toml")).unwrap();
    let expected = config
        .lines()
        .find_map(|l| {
            l.strip_prefix("mine_code_version = ")
                .map(|v| v.trim_matches('"'))
        })
        .unwrap_or("");
    assert_eq!(
        version, expected,
        "release version must equal config.toml mine_code_version"
    );
}

#[test]
fn repository_version_suggest_increments_patch() {
    let repo = repo_root();
    let (_exit, env) = run(
        &["repository", "version", "suggest", "--format", "json"],
        &repo,
    );
    let current = env["data"]["current"].as_str().unwrap_or("");
    let suggested = env["data"]["suggested"].as_str().unwrap_or("");
    assert_eq!(current, env!("CARGO_PKG_VERSION"));
    let current_patch: u32 = current
        .rsplit('.')
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let suggested_patch: u32 = suggested
        .rsplit('.')
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    assert!(
        suggested_patch > current_patch,
        "suggest increments the patch component: {current} -> {suggested}"
    );
}

#[test]
fn external_repo_without_mine_assets_passes_release_preflight_gates() {
    // A minimal non-MINE repository: git repo with NO skills/ and NO
    // plugins/mine/skills/. Release preflight must not fail solely because
    // MINE distribution assets are absent.
    let tmp = tempfile::tempdir().unwrap();
    // Init a git repo with a main branch.
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["init", "--quiet", "-b", "main"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["config", "user.email", "t@t"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["config", "user.name", "t"])
        .status()
        .unwrap();
    std::fs::write(tmp.path().join("README.md"), "external product\n").unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["add", "README.md"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["commit", "--quiet", "-m", "init"])
        .status()
        .unwrap();

    // mine init persists the detected main branch.
    let (exit, env) = run(&["init", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "init must succeed: {env}");
    assert_eq!(env["data"]["stable_branch"], "main");

    // The distribution gate must not fail preflight for this repo.
    // We call the preflight via `mine release`; without a graph it may
    // report MINE_GRAPH_NOT_INITIALIZED, but must NOT report
    // distribution_synced:false as the blocker.
    let (exit, env) = run(&["release", "--format", "json"], tmp.path());
    if exit == 0 {
        assert_eq!(env["data"]["distribution_synced"], true);
    } else {
        // Preflight on a graph-less repo reports graph-not-initialized;
        // assert the error is NOT about distribution.
        let msg = env["error"]["message"].as_str().unwrap_or("");
        assert!(
            !msg.contains("distribution"),
            "release preflight must not blame distribution for a generic repo: {msg}"
        );
    }
}

// ---------------------------------------------------------------------------
// Release-preflight regression tests (distribution gate, stale-branch
// migration, and CWD-vs---repo version resolution).
// ---------------------------------------------------------------------------

/// Builds a terminal external repo: `main` stable baseline and Root `dev`,
/// own approved Design and native workspace checkpoint, so each generic gate
/// except the one under test passes. Returns the temp dir.
fn build_terminal_external_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    super::fixture_git(tmp.path(), &["init", "--quiet", "-b", "main"]);
    for (key, value) in [
        ("user.email", "fixture@example.invalid"),
        ("user.name", "Release fixture"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
    ] {
        super::fixture_git(tmp.path(), &["config", key, value]);
    }
    std::fs::write(tmp.path().join("README.md"), "external product\n").unwrap();
    super::fixture_git(tmp.path(), &["add", "--", "README.md"]);
    super::fixture_git(
        tmp.path(),
        &["commit", "--quiet", "-m", "private initial product"],
    );
    let (exit, env) = run(&["init", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "{env}");

    // Freeze ADRs only after both working-tree and index attribute policy
    // have the real init-generated managed baseline. Do not waive the native
    // byte-preservation gate just to construct this release fixture.
    super::fixture_git(
        tmp.path(),
        &["add", "--", ".mine/.gitattributes", ".mine/.gitignore"],
    );
    super::fixture_git(
        tmp.path(),
        &[
            "commit",
            "--quiet",
            "-m",
            "private managed attribute baseline",
        ],
    );

    // Explicit private governance isolates asset parity from missing approval.
    let decision = ".mine/design/decisions/0001-external-fixture.md";
    std::fs::create_dir_all(tmp.path().join(".mine/design/decisions")).unwrap();
    std::fs::write(
        tmp.path().join(decision),
        "# External fixture decision\n\nNo bundled MINE agent assets.\n",
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
            "External fixture",
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
        let (exit, env) = run(&args, tmp.path());
        assert_eq!(exit, 0, "{env}");
    }
    super::managed_private_dev(tmp.path());
    let (exit, env) = run(
        &[
            "workspace",
            "open",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            "0",
            "--format",
            "json",
        ],
        tmp.path(),
    );
    assert_eq!(exit, 0, "workspace open: {env}");
    assert_eq!(
        super::fixture_git(tmp.path(), &["status", "--porcelain=v1", "-uall"]),
        ""
    );
    tmp
}

#[test]
fn external_repo_with_unrelated_skills_is_not_blocked() {
    // An external repository with its OWN unrelated skills/ directory (e.g.
    // team notes) but NO plugins/mine/skills/ must NOT be blocked by the
    // distribution gate. Generic preflight must not guess the repository role
    // from directory presence (Design "Repository roles").
    let tmp = build_terminal_external_repo();
    std::fs::create_dir_all(tmp.path().join("skills/team-notes")).unwrap();
    std::fs::write(
        tmp.path().join("skills/team-notes/README.md"),
        "team notes\n",
    )
    .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["add", "-A"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["commit", "--quiet", "-m", "add unrelated skills"])
        .status()
        .unwrap();

    let (exit, env) = run(&["release", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "preflight must succeed: {env}");
    // The parity field is informational: it may be false for a repo with an
    // unrelated skills/ dir, but it must NOT flip can_release to false.
    assert_eq!(
        env["data"]["distribution_synced"], false,
        "informational parity is false for unrelated skills/ without plugins"
    );
    assert_eq!(
        env["data"]["can_release"], true,
        "distribution parity must not block a generic repository release"
    );
}

#[test]
fn release_version_reads_target_repo_when_cwd_differs() {
    // The actual child must read B's version, never process CWD A's config.
    let repo_a = build_terminal_external_repo();
    let repo_b = build_terminal_external_repo();
    let mut config = mine::cli::context::load_config(repo_b.path()).unwrap();
    config.mine_code_version = "9.9.9".into();
    std::fs::write(repo_b.path().join(".mine/config.toml"), config.to_toml()).unwrap();
    super::fixture_git(repo_b.path(), &["add", "--", ".mine/config.toml"]);
    super::fixture_git(
        repo_b.path(),
        &["commit", "--quiet", "-m", "distinct private B version"],
    );

    // Run the REAL compiled binary from CWD=repo_a targeting --repo repo_b.
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_mine"));
    let out = std::process::Command::new(bin)
        .current_dir(repo_a.path())
        .arg("--repo")
        .arg(repo_b.path())
        .args(["release", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "release must succeed: {out:?}");
    let body = String::from_utf8_lossy(&out.stdout);
    let env: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    assert_eq!(
        env["data"]["release_version"], "9.9.9",
        "release version must come from target repo B, not CWD repo A: {body}"
    );
}

#[test]
fn init_repairs_stale_stable_branch_on_main_only_repo() {
    // Migration regression: a v0.1.1-era repo recorded stable="master" while
    // its only branch is main. `mine init` must repair the stale value with an
    // explicit action (not silently keep master, not silently use master).
    let tmp = tempfile::tempdir().unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["init", "--quiet", "-b", "main"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["config", "user.email", "t@t"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["config", "user.name", "t"])
        .status()
        .unwrap();
    std::fs::write(tmp.path().join("README.md"), "x\n").unwrap();
    // Write a stale v0.1.1-style config + valid design marker + index.
    std::fs::create_dir_all(tmp.path().join(".mine")).unwrap();
    std::fs::create_dir_all(tmp.path().join(".mine/design")).unwrap();
    std::fs::write(
        tmp.path().join(".mine/config.toml"),
        r#"schema_version = 1
repository_id = "stale-repo"
mine_code_version = "0.1.1"

[branches]
stable = "master"
integration = "dev"

[design]
root = ".mine/design/index.md"
marker = ".mine/design/.mine-design.toml"
language = "en"
index_soft_limit_lines = 250
leaf_soft_limit_lines = 400

[plan]
root = ".mine/plan"
ephemeral = true
purge_before_stable_release = true

[graph]
source = ".mine/plan/execution-graph.toml"
rendered = ".mine/plan/execution-graph.md"
lock_timeout_ms = 5000
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join(".mine/design/.mine-design.toml"),
        "schema_version = 1\nmanaged_by = \"MINE\"\nrepository_id = \"stale-repo\"\ncreated_at = \"2026-07-23T00:00:00Z\"\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join(".mine/design/index.md"), "# Index\n").unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["add", "-A"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["commit", "--quiet", "-m", "stale v0.1.1 tree"])
        .status()
        .unwrap();

    // Run mine init: it must repair stable="master" -> "main" with an action.
    let (exit, env) = run(&["init", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "init must succeed: {env}");
    let actions = env["data"]["actions"].as_array().unwrap();
    assert!(
        actions.iter().any(|a| {
            a["kind"] == "repaired-stable-branch" && a["from"] == "master" && a["to"] == "main"
        }),
        "init must record a repaired-stable-branch action: {env}"
    );
    let config_text = std::fs::read_to_string(tmp.path().join(".mine/config.toml")).unwrap();
    assert!(
        config_text.contains("stable = \"main\""),
        "config must be repaired to main: {config_text}"
    );
    assert!(
        !config_text.contains("stable = \"master\""),
        "stale master must be gone: {config_text}"
    );

    // After repair, doctor must be healthy on the graph-less stable tree
    // (main branch, no .mine/plan/), per the stable-tree doctor contract.
    let (exit, env) = run(&["doctor", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "doctor must succeed after repair: {env}");
    let checks = env["data"]["checks"].as_array().unwrap();
    let graph = checks.iter().find(|c| c["name"] == "graph").unwrap();
    assert_eq!(
        graph["ok"], true,
        "graph check must pass on repaired stable tree"
    );
    assert!(
        graph["message"]
            .as_str()
            .unwrap()
            .contains("not applicable"),
        "graph must report not-applicable on the repaired stable tree"
    );
}

#[test]
fn release_reports_stale_stable_branch_as_decisive_error() {
    // Native open requires a real stable baseline. Seed valid history first,
    // then explicitly damage private configuration/graph to test read-only
    // preflight, rather than expecting open to authorize a missing baseline.
    let tmp = build_terminal_external_repo();
    let mut config = mine::cli::context::load_config(tmp.path()).unwrap();
    config.branches.stable = "master".into();
    std::fs::write(tmp.path().join(".mine/config.toml"), config.to_toml()).unwrap();
    let mut graph: mine::domain::graph::PlanWorkspace =
        toml::from_str(&std::fs::read_to_string(tmp.path().join(&config.graph.source)).unwrap())
            .unwrap();
    graph.stable_branch = "master".into();
    std::fs::write(
        tmp.path().join(&config.graph.source),
        toml::to_string(&graph).unwrap(),
    )
    .unwrap();
    std::fs::write(
        tmp.path().join(&config.graph.rendered),
        mine::infrastructure::toml_store::render_markdown(&graph).unwrap(),
    )
    .unwrap();
    super::fixture_git(
        tmp.path(),
        &[
            "add",
            "--",
            ".mine/config.toml",
            &config.graph.source,
            &config.graph.rendered,
        ],
    );
    super::fixture_git(
        tmp.path(),
        &[
            "commit",
            "--quiet",
            "-m",
            "explicit private missing-stable corruption",
        ],
    );

    let (exit, env) = run(&["release", "--format", "json"], tmp.path());
    assert_eq!(exit, 0, "preflight must not crash: {env}");
    assert_eq!(
        env["data"]["can_release"], false,
        "stale stable branch must block release: {env}"
    );
    let errors = env["data"]["errors"].as_array().unwrap();
    assert!(
        errors.iter().any(|e| e
            .as_str()
            .unwrap_or("")
            .contains("configured stable branch 'master' not found")),
        "release must report the stale branch as a decisive error: {env}"
    );
}
