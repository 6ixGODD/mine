// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Native `mine scan plan-refs` cross-platform tests.
//!
//! These tests exercise the authoritative native scanner through the CLI
//! (or the service directly) and must pass on Windows without WSL and without
//! a usable `bash` on PATH. No test in this file invokes a shell.

use mine::cli;
use std::path::Path;

#[path = "support/config.rs"]
mod fixture_config;

fn run_scan(repo: &Path, check: bool) -> (i32, serde_json::Value) {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        repo.to_string_lossy().to_string(),
        "scan".to_string(),
        "plan-refs".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    if check {
        argv.push("--check".to_string());
    }
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

fn init_repo(root: &Path) {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["init", "-q"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.email", "t@t"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.name", "t"])
        .status()
        .unwrap();
}

fn commit_all(root: &Path, msg: &str) {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["add", "-A"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["commit", "-qm", msg])
        .status()
        .unwrap();
}

fn findings(env: &serde_json::Value) -> Vec<String> {
    let arr = if env["data"]["findings"].is_array() {
        &env["data"]["findings"]
    } else {
        &env["error"]["details"]["findings"]
    };
    arr.as_array()
        .map(|a| {
            a.iter()
                .map(|f| {
                    format!(
                        "{}:{}",
                        f["file"].as_str().unwrap_or("?"),
                        f["line"].as_u64().unwrap_or(0)
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[test]
fn scan_flags_go_layout() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::create_dir_all(tmp.path().join("cmd")).unwrap();
    std::fs::write(
        tmp.path().join("cmd/main.go"),
        // mine-release-allow-plan-reference: scanner test fixture
        "package main\n// Plan 99 historical comment\nfunc main() {}\n",
    )
    .unwrap();
    commit_all(tmp.path(), "go layout");

    let (exit, env) = run_scan(tmp.path(), true);
    assert_ne!(exit, 0, "unexempted finding must fail --check: {env}");
    let f = findings(&env);
    assert!(
        f.iter().any(|x| x == "cmd/main.go:2"),
        "must flag cmd/main.go:2, got {f:?}"
    );
}

#[test]
fn scan_flags_python_layout() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    // mine-release-allow-plan-reference: scanner test fixture
    std::fs::write(tmp.path().join("app.py"), "# Plan 7 in python\n").unwrap();
    commit_all(tmp.path(), "python layout");

    let (exit, env) = run_scan(tmp.path(), true);
    assert_ne!(exit, 0, "unexempted finding must fail --check: {env}");
    let f = findings(&env);
    assert!(
        f.iter().any(|x| x == "app.py:1"),
        "must flag app.py:1, got {f:?}"
    );
}

#[test]
fn scan_flags_typescript_layout() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    // mine-release-allow-plan-reference: scanner test fixture
    std::fs::write(tmp.path().join("src/index.ts"), "// Plan 12 ref\n").unwrap();
    commit_all(tmp.path(), "ts layout");

    let (exit, env) = run_scan(tmp.path(), true);
    assert_ne!(exit, 0, "unexempted finding must fail --check: {env}");
    let f = findings(&env);
    assert!(
        f.iter().any(|x| x == "src/index.ts:1"),
        "must flag src/index.ts:1, got {f:?}"
    );
}

#[test]
fn scan_flags_monorepo_tracked_layout() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::create_dir_all(tmp.path().join("packages/core")).unwrap();
    // mine-release-allow-plan-reference: scanner test fixture
    std::fs::write(tmp.path().join("packages/core/lib.go"), "// Plan 31\n").unwrap();
    commit_all(tmp.path(), "monorepo");

    let (exit, env) = run_scan(tmp.path(), true);
    assert_ne!(exit, 0, "unexempted finding must fail --check: {env}");
    let f = findings(&env);
    assert!(
        f.iter().any(|x| x == "packages/core/lib.go:1"),
        "must flag nested monorepo file, got {f:?}"
    );
}

#[test]
fn scan_excludes_docs_and_plan_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::create_dir_all(tmp.path().join(".mine/design")).unwrap();
    std::fs::write(
        tmp.path().join(".mine/design/arch.md"),
        // mine-release-allow-plan-reference: scanner test fixture
        "# Plan 99 in design doc\n",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join(".mine/plan")).unwrap();
    std::fs::write(
        tmp.path().join(".mine/plan/01-x.md"),
        // mine-release-allow-plan-reference: scanner test fixture
        "# Plan 99 in plan workspace\n",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("testdata")).unwrap();
    // mine-release-allow-plan-reference: scanner test fixture
    std::fs::write(tmp.path().join("testdata/fixture.txt"), "Plan 5 data\n").unwrap();
    commit_all(tmp.path(), "docs only");

    let (exit, env) = run_scan(tmp.path(), true);
    assert_eq!(
        exit, 0,
        "owned Design/Plan/testdata must be excluded: {env}"
    );
    assert!(
        findings(&env).is_empty(),
        "no findings expected: {:?}",
        findings(&env)
    );
}

#[test]
fn configured_roots_are_authoritative_without_permanent_path_aliases() {
    for legacy in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let mut config = fixture_config::canonical();
        if legacy {
            config.design.root = "docs/design/index.md".into();
            config.design.marker = "docs/design/.mine-design.toml".into();
            config.plan.root = "docs/plan".into();
            config.graph.source = "docs/plan/execution-graph.toml".into();
            config.graph.rendered = "docs/plan/execution-graph.md".into();
            config.adr.registry = "docs/design/decisions/adr-registry.toml".into();
            config.adr.rendered_index = "docs/design/decisions/index.md".into();
        }
        config.layout().unwrap();
        std::fs::create_dir_all(tmp.path().join(".mine")).unwrap();
        std::fs::write(tmp.path().join(".mine/config.toml"), config.to_toml()).unwrap();
        for prefix in ["docs", ".mine"] {
            let path = tmp.path().join(format!("{prefix}/design/reference.md"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            // mine-release-allow-plan-reference: deliberate scanner input in two alternate roots
            std::fs::write(path, "Plan 99\n").unwrap();
        }
        commit_all(tmp.path(), "explicit layout authority fixture");
        let (exit, envelope) = run_scan(tmp.path(), true);
        assert_ne!(exit, 0, "unowned-root text must remain visible: {envelope}");
        assert_eq!(
            findings(&envelope),
            vec![if legacy {
                ".mine/design/reference.md:1"
            } else {
                "docs/design/reference.md:1"
            }]
        );
    }
}

#[test]
fn scan_honors_fixture_exemption() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::write(
        tmp.path().join("t.rs"),
        "// mine-release-allow-plan-reference: protocol fixture\nlet input = \"Plan 08-2\";\n",
    )
    .unwrap();
    commit_all(tmp.path(), "exempted fixture");

    let (exit, env) = run_scan(tmp.path(), true);
    assert_eq!(exit, 0, "exempted fixture must pass --check: {env}");
    assert!(
        findings(&env).is_empty(),
        "exempted fixture must not be a finding: {:?}",
        findings(&env)
    );
    assert!(
        !env["data"]["exempted"]
            .as_array()
            .unwrap_or(&vec![])
            .is_empty(),
        "exempted findings must be recorded"
    );
}

#[test]
fn scan_clean_repo_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    std::fs::write(tmp.path().join("main.py"), "print('hi')\n").unwrap();
    commit_all(tmp.path(), "clean");

    let (exit, env) = run_scan(tmp.path(), true);
    assert_eq!(exit, 0, "clean repo must pass --check: {env}");
    assert_eq!(env["data"]["clean"], true);
    assert!(findings(&env).is_empty());
}

#[test]
fn scan_unexempted_finding_fails_check() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    // mine-release-allow-plan-reference: scanner test fixture
    std::fs::write(tmp.path().join("main.py"), "# Plan 3\n").unwrap();
    commit_all(tmp.path(), "dirty");

    // Repair mode (no --check): exit 0 but findings reported.
    let (exit, env) = run_scan(tmp.path(), false);
    assert_eq!(exit, 0, "repair mode exits 0: {env}");
    assert_eq!(findings(&env).len(), 1);

    // --check mode: non-zero.
    let (exit, _env) = run_scan(tmp.path(), true);
    assert_ne!(exit, 0, "--check must fail on unexempted finding");
}

#[test]
fn scan_non_git_repo_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    // mine-release-allow-plan-reference: scanner test fixture
    std::fs::write(tmp.path().join("f.txt"), "Plan 9\n").unwrap();
    // Plan-isolated TMP lives inside the real repository. Prevent Git's normal
    // parent discovery from turning this deliberately non-Git fixture into a
    // subdirectory of the host repository. Child-only environment, no unsafe
    // process-global mutation. Git documents this discovery ceiling at:
    // https://git-scm.com/docs/git#Documentation/git.txt-codeGITCEILINGDIRECTORIEScode
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mine"))
        .current_dir(tmp.path())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env("GIT_CEILING_DIRECTORIES", tmp.path().parent().unwrap())
        .args(["--repo"])
        .arg(tmp.path())
        .args(["scan", "plan-refs", "--check", "--format", "json"])
        .output()
        .unwrap();
    let env: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(!output.status.success(), "non-git must fail closed: {env}");
    assert_eq!(env["ok"], false);
}

#[test]
fn scan_honors_repo_flag_from_other_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    // mine-release-allow-plan-reference: scanner test fixture
    std::fs::write(tmp.path().join("main.py"), "# Plan 21\n").unwrap();
    commit_all(tmp.path(), "target");

    // CWD is a different directory; --repo targets the temp repo.
    let elsewhere = tempfile::tempdir().unwrap();
    let argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        tmp.path().to_string_lossy().to_string(),
        "scan".to_string(),
        "plan-refs".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--check".to_string(),
    ];
    let outcome = cli::dispatch(&argv, "mine");
    let (stdout, stderr) = cli::render(&outcome, true, false);
    let body = if outcome.exit_code == 0 {
        stdout
    } else {
        stderr
    };
    let env: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    assert_ne!(outcome.exit_code, 0, "--repo must scan the target: {env}");
    let f = findings(&env);
    assert!(
        f.iter().any(|x| x == "main.py:1"),
        "must flag main.py:1 in target repo, got {f:?}"
    );
    let _ = elsewhere;
}

#[test]
fn scan_json_output_is_machine_readable() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo(tmp.path());
    // mine-release-allow-plan-reference: scanner test fixture
    std::fs::write(tmp.path().join("a.rs"), "// Plan 1\n").unwrap();
    commit_all(tmp.path(), "json");

    let (exit, env) = run_scan(tmp.path(), false);
    assert_eq!(exit, 0);
    assert_eq!(env["command"], "scan.plan-refs");
    assert!(env["data"]["findings"].is_array());
    assert!(env["data"]["files_scanned"].is_number());
    assert_eq!(
        env["data"]["findings"][0]["file"].as_str().unwrap_or(""),
        "a.rs"
    );
    assert_eq!(env["data"]["findings"][0]["line"].as_u64().unwrap_or(0), 1);
}
