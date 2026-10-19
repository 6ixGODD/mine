#![forbid(unsafe_code)]
//! Observable governance-layout boundaries; fixtures never touch real Root.
use std::fs;
use std::process::{Command, Output};

fn mine(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mine"))
        .arg("--repo")
        .arg(root)
        .args(args)
        .env("HOME", root.join(".git/fixture-home"))
        .env("USERPROFILE", root.join(".git/fixture-home"))
        .output()
        .unwrap()
}

fn diagnostic(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn git(root: &std::path::Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("HOME", root.join(".git/fixture-home"))
        .env("USERPROFILE", root.join(".git/fixture-home"))
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    output
}

fn repository(root: &std::path::Path, autocrlf: &str) {
    git(root, &["init", "--initial-branch=dev"]);
    git(root, &["config", "user.name", "Layout fixture"]);
    git(root, &["config", "user.email", "layout@example.invalid"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.autocrlf", autocrlf]);
}

#[test]
fn ordinary_add_clone_and_linked_checkout_preserve_lf_and_crlf_bytes() {
    for autocrlf in ["true", "false", "input"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("spaces 非ASCII repository");
        fs::create_dir(&root).unwrap();
        repository(&root, autocrlf);
        let initialized = mine(&root, &["init", "--format", "json"]);
        assert!(initialized.status.success(), "{autocrlf}: {initialized:?}");
        let bodies = [
            (
                ".mine/design/decisions/0001-lf.md",
                b"# LF\nExact bytes\n".as_slice(),
            ),
            (
                ".mine/design/decisions/0002-crlf.md",
                b"# CRLF\r\nExact bytes\r\n".as_slice(),
            ),
            (
                // mine-release-allow-plan-reference: byte-stability fixture body, not historical implementation provenance
                ".mine/plan/plan99-fixture.md",
                b"# Plan\r\nExact bytes\r\n".as_slice(),
            ),
        ];
        for (path, bytes) in bodies {
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            fs::write(root.join(path), bytes).unwrap();
        }
        let binary = b"\x89PNG\r\n\0\xff";
        fs::write(root.join(".mine/design/diagram.png"), binary).unwrap();
        for path in ["runtime", "locks", "build", "worktrees"] {
            fs::create_dir(root.join(".mine").join(path)).unwrap();
            fs::write(root.join(".mine").join(path).join("local.json"), b"{}").unwrap();
        }
        git(&root, &["add", "--", ".mine"]);
        let staged = git(&root, &["diff", "--cached", "--name-only"]);
        let paths = String::from_utf8(staged.stdout).unwrap();
        assert!(paths.contains(".mine/.gitattributes"));
        assert!(!paths.contains("local.json"));
        git(
            &root,
            &[
                "commit",
                "-m",
                "fixture: canonical byte-preserving baseline",
            ],
        );
        for (path, bytes) in bodies {
            assert_eq!(git(&root, &["show", &format!("HEAD:{path}")]).stdout, bytes);
            assert_eq!(fs::read(root.join(path)).unwrap(), bytes);
        }
        let linked = temp.path().join("linked checkout");
        git(
            &root,
            &[
                "worktree",
                "add",
                "--detach",
                linked.to_str().unwrap(),
                "HEAD",
            ],
        );
        let cloned = temp.path().join("clean clone");
        git(
            temp.path(),
            &[
                "-c",
                &format!("core.autocrlf={autocrlf}"),
                "clone",
                "--no-hardlinks",
                root.to_str().unwrap(),
                cloned.to_str().unwrap(),
            ],
        );
        for checkout in [&linked, &cloned] {
            for (path, bytes) in bodies {
                assert_eq!(fs::read(checkout.join(path)).unwrap(), bytes);
            }
            assert_eq!(
                fs::read(checkout.join(".mine/design/diagram.png")).unwrap(),
                binary
            );
            fs::write(checkout.join(".mine/design/new-file.md"), b"new\n").unwrap();
            git(checkout, &["add", "--", ".mine/design/new-file.md"]);
        }
    }
}

#[test]
fn excluded_parent_is_refused_with_exact_rule_before_governance_writes() {
    for source in ["repository", "local", "global"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        repository(root, "true");
        let exclusion = match source {
            "repository" => root.join(".gitignore"),
            "local" => root.join(".git/info/exclude"),
            _ => {
                let global = root.join(".git/global-excludes");
                git(
                    root,
                    &["config", "core.excludesFile", global.to_str().unwrap()],
                );
                global
            }
        };
        fs::write(&exclusion, b".mine/\n").unwrap();
        let result = mine(root, &["init", "--format", "json"]);
        assert!(!result.status.success(), "{source}: {result:?}");
        let output = diagnostic(&result);
        assert!(output.contains(".mine/"), "{output}");
        assert!(output.contains("excluded"), "{output}");
        assert!(!root.join(".mine/config.toml").exists());
        assert!(!root.join(".mine/design").exists());
        assert_eq!(fs::read(exclusion).unwrap(), b".mine/\n");
    }
}

#[test]
fn higher_precedence_attributes_refuse_without_claiming_design() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    repository(root, "true");
    fs::write(root.join(".git/info/attributes"), b".mine/design/** text\n").unwrap();
    let result = mine(root, &["init", "--format", "json"]);
    assert!(!result.status.success(), "{result:?}");
    assert!(
        diagnostic(&result).contains("byte-preserving"),
        "{result:?}"
    );
    assert!(!root.join(".mine/config.toml").exists());
    assert!(!root.join(".mine/design").exists());
    assert!(!root.join(".mine/.gitattributes").exists());
    assert_eq!(
        fs::read(root.join(".git/info/attributes")).unwrap(),
        b".mine/design/** text\n"
    );
}

#[test]
fn active_legacy_init_refusal_preserves_config_graph_and_body() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let initialized = mine(root, &["init", "--format", "json"]);
    assert!(initialized.status.success(), "{initialized:?}");
    let config = fs::read_to_string(root.join(".mine/config.toml"))
        .unwrap()
        .replace(".mine/design", "docs/design")
        .replace(".mine/plan", "docs/plan");
    fs::write(root.join(".mine/config.toml"), &config).unwrap();
    fs::create_dir_all(root.join("docs/plan")).unwrap();
    fs::write(
        root.join("docs/plan/execution-graph.toml"),
        b"immutable active legacy evidence\r\n",
    )
    .unwrap();
    let result = mine(root, &["init", "--format", "json"]);
    assert!(!result.status.success(), "{result:?}");
    assert!(diagnostic(&result).contains("old binary"), "{result:?}");
    assert_eq!(
        fs::read_to_string(root.join(".mine/config.toml")).unwrap(),
        config
    );
    assert_eq!(
        fs::read(root.join("docs/plan/execution-graph.toml")).unwrap(),
        b"immutable active legacy evidence\r\n"
    );
}

fn draft_fixture(root: &std::path::Path, bytes: &[u8]) {
    repository(root, "true");
    let initialized = mine(root, &["init", "--format", "json"]);
    assert!(initialized.status.success(), "{initialized:?}");
    fs::create_dir_all(root.join(".mine/design/decisions")).unwrap();
    fs::write(root.join(".mine/design/decisions/9001-draft.md"), bytes).unwrap();
    let added = mine(
        root,
        &[
            "adr",
            "add",
            "--id",
            "ADR-9001",
            "--path",
            ".mine/design/decisions/9001-draft.md",
            "--title",
            "Draft",
            "--format",
            "json",
        ],
    );
    assert!(added.status.success(), "{added:?}");
    git(root, &["add", "--", ".mine", "AGENTS.md"]);
    git(
        root,
        &["commit", "-m", "fixture: committed governance baseline"],
    );
}

#[test]
fn crlf_draft_is_frozen_without_normalization_and_validates_after_clone() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    let bytes = b"# Decision\r\n\r\nActual CRLF author input.\r\n";
    draft_fixture(&root, bytes);
    let proposed = mine(
        &root,
        &["adr", "propose", "--id", "ADR-9001", "--format", "json"],
    );
    assert!(proposed.status.success(), "{proposed:?}");
    let registry = mine::infrastructure::adr_store::AdrStore::new(
        &root,
        &mine::domain::config::AdrConfig::default(),
    )
    .load()
    .unwrap();
    assert_eq!(
        registry.get("ADR-9001").unwrap().content_sha256,
        mine::domain::adr::sha256_hex(bytes)
    );
    assert_eq!(
        fs::read(root.join(".mine/design/decisions/9001-draft.md")).unwrap(),
        bytes
    );
    git(&root, &["add", "--", ".mine/design"]);
    git(
        &root,
        &["commit", "-m", "fixture: exact proposed CRLF body"],
    );
    let cloned = temp.path().join("clone");
    git(
        temp.path(),
        &[
            "-c",
            "core.autocrlf=true",
            "clone",
            root.to_str().unwrap(),
            cloned.to_str().unwrap(),
        ],
    );
    assert_eq!(
        fs::read(cloned.join(".mine/design/decisions/9001-draft.md")).unwrap(),
        bytes
    );
    let validated = mine(&cloned, &["adr", "validate", "--format", "json"]);
    assert!(validated.status.success(), "{validated:?}");
}

#[test]
fn proposal_refuses_missing_or_index_conflicting_attributes_without_freezing() {
    for conflict in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        draft_fixture(root, b"# CRLF\r\nUnfrozen draft\r\n");
        let registry = fs::read(root.join(".mine/design/decisions/adr-registry.toml")).unwrap();
        if conflict {
            let deeper = root.join(".mine/design/.gitattributes");
            fs::write(&deeper, b"*.md text eol=lf\n").unwrap();
            git(root, &["add", "--", ".mine/design/.gitattributes"]);
            fs::write(
                &deeper,
                b"*.md -text -filter -ident -working-tree-encoding\n",
            )
            .unwrap();
        } else {
            fs::remove_file(root.join(".mine/.gitattributes")).unwrap();
        }
        let proposed = mine(
            root,
            &["adr", "propose", "--id", "ADR-9001", "--format", "json"],
        );
        assert!(!proposed.status.success(), "{proposed:?}");
        if conflict {
            assert!(
                diagnostic(&proposed).contains("byte-preserving"),
                "{proposed:?}"
            );
            assert!(diagnostic(&proposed).contains("index"), "{proposed:?}");
        } else {
            assert!(
                diagnostic(&proposed).contains("committed baseline"),
                "{proposed:?}"
            );
        }
        assert_eq!(
            fs::read(root.join(".mine/design/decisions/adr-registry.toml")).unwrap(),
            registry
        );
        assert_eq!(
            fs::read(root.join(".mine/design/decisions/9001-draft.md")).unwrap(),
            b"# CRLF\r\nUnfrozen draft\r\n"
        );
    }
}

#[test]
fn init_refuses_traversal_and_mismatched_namespaces_without_rewriting_config() {
    for bad in [
        "../outside/index.md",
        "/outside/index.md",
        ".mine/plan/index.md",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let result = mine(root, &["init", "--format", "json"]);
        assert!(result.status.success(), "{result:?}");
        let config = fs::read_to_string(root.join(".mine/config.toml"))
            .unwrap()
            .replace(".mine/design/index.md", bad);
        fs::write(root.join(".mine/config.toml"), &config).unwrap();
        let result = mine(root, &["init", "--format", "json"]);
        assert!(!result.status.success(), "{bad}: {result:?}");
        assert_eq!(
            fs::read_to_string(root.join(".mine/config.toml")).unwrap(),
            config
        );
        assert!(!root.join(".mine/plan").exists());
    }
}

#[cfg(windows)]
#[test]
fn junction_escape_is_refused_before_init_writes_outside_root() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join(".mine")).unwrap();
    fs::write(outside.path().join("sentinel"), b"foreign data").unwrap();
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(
            temp.path()
                .join(".mine/design")
                .to_string_lossy()
                .replace('/', "\\"),
        )
        .arg(outside.path().to_string_lossy().replace('/', "\\"))
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "native junction fixture unavailable: {status:?}"
    );
    let result = mine(temp.path(), &["init", "--format", "json"]);
    assert!(!result.status.success(), "{result:?}");
    assert!(diagnostic(&result).contains("escapes Root"), "{result:?}");
    assert!(!temp.path().join(".mine/config.toml").exists());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    assert_eq!(
        fs::read(outside.path().join("sentinel")).unwrap(),
        b"foreign data"
    );
}

#[test]
fn linked_invocation_reads_authoritative_root_graph_not_its_checkout_copy() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Root 工作区");
    fs::create_dir(&root).unwrap();
    repository(&root, "true");
    git(&root, &["symbolic-ref", "HEAD", "refs/heads/master"]);
    let initialized = mine(&root, &["init", "--format", "json"]);
    assert!(initialized.status.success(), "{initialized:?}");
    git(&root, &["add", "--", ".mine", "AGENTS.md"]);
    git(
        &root,
        &["commit", "-m", "fixture: canonical stable baseline"],
    );
    git(&root, &["switch", "-c", "dev"]);
    let opened = mine(
        &root,
        &[
            "workspace",
            "open",
            "--expected-revision",
            "0",
            "--actor",
            "fixture-owner",
            "--format",
            "json",
        ],
    );
    assert!(opened.status.success(), "{opened:?}");
    let before = fs::read(root.join(".mine/plan/execution-graph.toml")).unwrap();
    let linked = temp.path().join("linked caller");
    git(
        &root,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );
    fs::remove_file(linked.join(".mine/plan/execution-graph.toml")).unwrap();
    let observed = mine(&linked, &["graph", "status", "--format", "json"]);
    assert!(observed.status.success(), "{observed:?}");
    let status: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
    assert_eq!(status["data"]["revision"], 1);
    assert_eq!(
        fs::read(root.join(".mine/plan/execution-graph.toml")).unwrap(),
        before
    );
    assert_eq!(git(&root, &["branch", "--show-current"]).stdout, b"dev\n");
}

#[test]
fn fresh_init_uses_mine_roots_and_preserves_user_docs() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("docs/design")).unwrap();
    let user = b"# User-owned architecture\r\nnot MINE governance\r\n";
    fs::write(temp.path().join("docs/design/index.md"), user).unwrap();
    let initialized = mine(temp.path(), &["init", "--format", "json"]);
    assert!(initialized.status.success(), "{initialized:?}");
    assert!(temp.path().join(".mine/design/index.md").is_file());
    assert!(temp.path().join(".mine/design/.mine-design.toml").is_file());
    assert!(temp.path().join(".mine/.gitattributes").is_file());
    assert!(!temp.path().join(".mine/plan").exists());
    assert_eq!(
        fs::read(temp.path().join("docs/design/index.md")).unwrap(),
        user
    );
    let before = fs::read(temp.path().join(".mine/config.toml")).unwrap();
    let repeated = mine(temp.path(), &["init", "--format", "json"]);
    assert!(repeated.status.success(), "{repeated:?}");
    assert_eq!(
        fs::read(temp.path().join(".mine/config.toml")).unwrap(),
        before
    );
}
