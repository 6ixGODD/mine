// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! ADR bootstrap and forged-body regression tests.

use mine::cli;
use mine::domain::adr::sha256_hex;
use mine::domain::config::{AdrConfig, GovernanceLayout, MineConfig};
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
    // Locate source Design through its real configuration, so the same fixture
    // works before and after terminal layout curation without probing aliases.
    let path = repo_root().join(".mine/config.toml");
    let mut config = MineConfig::parse(&path, &std::fs::read_to_string(&path).unwrap()).unwrap();
    let source_layout = config.layout().unwrap();
    let layout = GovernanceLayout::canonical();
    copy_tree(
        &repo_root().join(source_layout.design_dir().as_str()),
        &temp.path().join(layout.design_dir().as_str()),
    );
    config.design.root = layout.design_index().as_str().into();
    config.design.marker = layout.design_marker().as_str().into();
    config.plan.root = layout.plan_dir().as_str().into();
    config.graph.source = layout.graph_source().as_str().into();
    config.graph.rendered = layout.graph_view().as_str().into();
    config.adr.registry = layout.adr_registry().as_str().into();
    config.adr.rendered_index = layout.adr_index().as_str().into();
    std::fs::write(temp.path().join(".mine/config.toml"), config.to_toml()).unwrap();
    std::fs::remove_file(temp.path().join(".mine/design/decisions/adr-registry.toml")).unwrap();
    std::fs::remove_file(temp.path().join(".mine/design/decisions/index.md")).unwrap();
    AdrStore::new(temp.path(), &AdrConfig::default())
        .bootstrap_if_absent("2026-08-21T00:00:00Z")
        .unwrap();
    prepare_git_governance(temp.path());
    temp
}

fn prepare_git_governance(root: &Path) {
    fixture_git(root, &["init", "-b", "master"]);
    for (key, value) in [
        ("user.name", "ADR Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "true"),
    ] {
        fixture_git(root, &["config", key, value]);
    }
    mine::infrastructure::governance_git::establish(root).unwrap();
    fixture_git(root, &["add", "--", ".mine"]);
    fixture_git(
        root,
        &["commit", "-m", "fixture: immutable governance Git baseline"],
    );
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

/// Runs the CLI and returns (exit_code, stdout, stderr) with JSON rendering.
fn run_json(repo: &Path, args: &[&str]) -> (i32, String, String) {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        repo.to_string_lossy().to_string(),
    ];
    argv.extend(args.iter().map(|arg| (*arg).to_string()));
    let outcome = cli::dispatch(&argv, "mine");
    let code = outcome.exit_code;
    let (out, err) = cli::render(&outcome, true, false);
    (code, out, err)
}

/// Registers and approves one extra ADR in the temp repository.
fn approve_extra(repo: &Path, id: &str, scopes: &[&str]) {
    let file = format!(".mine/design/decisions/{}-preview.md", &id[4..]);
    std::fs::write(
        repo.join(&file),
        format!("# {id}: Preview Fixture\n\nBody for the supersede preview test.\n"),
    )
    .unwrap();
    let mut add = vec!["adr", "add", "--id", id, "--path", &file, "--title", id];
    for scope in scopes {
        add.push("--scope");
        add.push(scope);
    }
    assert_eq!(run(repo, &add), 0, "adr add {id}");
    assert_eq!(
        run(repo, &["adr", "propose", "--id", id]),
        0,
        "propose {id}"
    );
    assert_eq!(
        run(
            repo,
            &[
                "adr",
                "approve",
                "--id",
                id,
                "--by",
                "owner",
                "--note",
                "test approval"
            ],
        ),
        0,
        "approve {id}"
    );
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

/// A fixture with both the bootstrapped ADR registry and an empty execution
/// graph, so plan registration/release gating can be exercised end to end.
fn graphed_fixture() -> tempfile::TempDir {
    let temp = bootstrap_fixture();
    let root = temp.path();
    fixture_git(root, &["init", "-b", "master"]);
    for (key, value) in [
        ("user.name", "ADR Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
    ] {
        fixture_git(root, &["config", key, value]);
    }
    std::fs::write(root.join("README.md"), "# Private ADR gate fixture\n").unwrap();
    std::fs::write(
        root.join(".gitignore"),
        "/.mine/runtime/\n/.mine/locks/\n/.mine/worktrees/\n/.mine/build/\n",
    )
    .unwrap();
    fixture_git(root, &["add", "README.md", ".gitignore"]);
    fixture_git(root, &["commit", "-m", "private Git baseline"]);
    assert_eq!(run(root, &["init"]), 0);
    fixture_commit(
        root,
        &[
            ".mine/config.toml",
            ".mine/.gitignore",
            ".mine/.gitattributes",
            "AGENTS.md",
            ".mine/design",
        ],
    );
    let baseline = fixture_git(root, &["rev-parse", "HEAD"]);
    fixture_git(root, &["switch", "-c", "dev"]);
    let repo = temp.path().join(".mine/plan");
    std::fs::create_dir_all(&repo).unwrap();
    let ws = mine::domain::graph::PlanWorkspace {
        schema_version: 4,
        revision: 72,
        project_id: "adr-gate-fixture".into(),
        workspace_id: uuid::Uuid::new_v4().to_string(),
        stable_branch: "master".into(),
        integration_branch: "dev".into(),
        stable_baseline_commit: baseline,
        design_root: ".mine/design/index.md".into(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans: vec![],
    };
    std::fs::write(
        repo.join("execution-graph.toml"),
        toml::to_string(&ws).unwrap(),
    )
    .unwrap();
    mine::infrastructure::toml_store::TomlStore::new(temp.path())
        .render()
        .unwrap();
    fixture_commit(root, &[".mine/plan"]);
    temp
}

fn fixture_git(root: &Path, args: &[&str]) -> String {
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

fn fixture_commit(root: &Path, paths: &[&str]) {
    let mut args = vec!["add", "--"];
    args.extend_from_slice(paths);
    fixture_git(root, &args);
    fixture_git(
        root,
        &["commit", "-m", "explicit fixture governance inputs"],
    );
}

/// Registers a blocking ADR in the given state and scope, creating its body.
fn scoped_adr(repo: &Path, id: &str, scope: &str, approve: bool) {
    let file = format!(".mine/design/decisions/{}-scope.md", &id[4..]);
    std::fs::write(
        repo.join(&file),
        format!(
            "# {id}: Scoped Fixture

Decision body.
"
        ),
    )
    .unwrap();
    assert_eq!(
        run(
            repo,
            &[
                "adr", "add", "--id", id, "--path", &file, "--title", id, "--scope", scope
            ],
        ),
        0
    );
    assert_eq!(run(repo, &["adr", "propose", "--id", id]), 0);
    if approve {
        assert_eq!(
            run(
                repo,
                &[
                    "adr", "approve", "--id", id, "--by", "owner", "--note", "test"
                ],
            ),
            0
        );
    }
}

#[test]
fn scoped_adr_gates_plan_registration_by_scope() {
    let temp = graphed_fixture();
    let repo = temp.path();
    // An unapproved blocking ADR scoped under the design root applies to the
    // default registration scope and must block it.
    scoped_adr(repo, "ADR-0010", ".mine/design/execution-graph", false);
    fixture_commit(repo, &[".mine/design/decisions"]);
    let base_add = [
        "plan",
        "add",
        "--actor",
        "fixture-planner",
        "--expected-revision",
        "72",
        "--write",
        "tests/adr-fixture/",
        "--id",
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        "plan02",
        "--path",
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        ".mine/plan/plan02-fixture.md",
        "--title",
        "fixture",
        "--design-ref",
        ".mine/design/index.md",
    ];
    let (code, _out, err) = run_json(repo, &base_add);
    assert_ne!(code, 0, "default-scope registration must be gated");
    assert!(
        err.contains("planning readiness blocked"),
        "error names the gate: {err}"
    );

    // An out-of-scope registration is unaffected and records its explicit
    // scope verbatim.
    std::fs::write(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        repo.join(".mine/plan/plan02-fixture.md"),
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        "# plan02
",
    )
    .unwrap();
    // mine-release-allow-plan-reference: explicit private Plan body and scoped ADR inputs
    fixture_commit(
        repo,
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        &[".mine/design/decisions", ".mine/plan/plan02-fixture.md"],
    );
    let mut scoped_add = base_add.to_vec();
    scoped_add.push("--planning-scope");
    scoped_add.push("docs/other-zone");
    let (code, _out, err) = run_json(repo, &scoped_add);
    assert_eq!(code, 0, "out-of-scope registration proceeds: {err}");
    let (code, out, _err) = run_json(
        repo,
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        &["plan", "show", "--id", "plan02", "--format", "json"],
    );
    assert_eq!(code, 0);
    assert!(
        out.contains("\"planning_scope\":\"docs/other-zone\""),
        "explicit scope recorded verbatim: {out}"
    );
}

#[test]
fn scoped_adr_added_after_registration_blocks_release_until_approved() {
    let temp = graphed_fixture();
    let repo = temp.path();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    std::fs::write(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        repo.join(".mine/plan/plan02-fixture.md"),
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        "# plan02
",
    )
    .unwrap();
    // mine-release-allow-plan-reference: registration captures actual committed Plan bytes
    fixture_commit(repo, &[".mine/plan/plan02-fixture.md"]);
    assert_eq!(
        run(
            repo,
            &[
                "plan",
                "add",
                "--actor",
                "fixture-planner",
                "--expected-revision",
                "72",
                "--write",
                "tests/adr-fixture/",
                "--id",
                // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                "plan02",
                "--path",
                // mine-release-allow-plan-reference: plan-id lifecycle fixture
                ".mine/plan/plan02-fixture.md",
                "--title",
                "fixture",
                "--design-ref",
                ".mine/design/index.md",
            ],
        ),
        0,
        "registration with no blockers"
    );

    // A PROPOSED blocking ADR in the default scope appears after registration
    // and must fail the release-time re-check, writing nothing.
    scoped_adr(repo, "ADR-0010", ".mine/design", false);
    fixture_commit(repo, &[".mine/design/decisions"]);
    let before = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let (code, _out, err) = run_json(
        repo,
        &[
            "plan",
            "release",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan02",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            "73",
        ],
    );
    assert_ne!(code, 0, "release re-check must block");
    assert!(
        err.contains("planning readiness blocked") && err.contains("ADR-0010"),
        "blocker DTOs in the error: {err}"
    );
    assert_eq!(
        std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap(),
        before,
        "nothing written"
    );

    // Approving the ADR unblocks release and records the audit evidence.
    assert_eq!(
        run(
            repo,
            &[
                "adr", "approve", "--id", "ADR-0010", "--by", "owner", "--note", "approved"
            ],
        ),
        0
    );
    fixture_commit(repo, &[".mine/design/decisions"]);
    let registry = AdrStore::new(repo, &AdrConfig::default()).load().unwrap();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let (code, out, err) = run_json(
        repo,
        &[
            "plan",
            "release",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan02",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            "73",
        ],
    );
    assert_eq!(code, 0, "release proceeds once ready: {out} {err}");
    assert!(
        out.contains("\"planning_scope\":\".mine/design\""),
        "release envelope carries the scope: {out}"
    );
    let graph_text = std::fs::read_to_string(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    assert!(
        graph_text.starts_with("schema_version = \"mine-graph-v7\""),
        "mutation upgraded the wire schema"
    );
    let graph: mine::domain::graph::PlanWorkspace = toml::from_str(&graph_text).unwrap();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let node = graph.get("plan02").unwrap();
    assert_eq!(node.status.as_str(), "READY");
    assert_eq!(node.planning_scope, ".mine/design");
    assert!(!node.readiness_checked_at.is_empty());
    assert_eq!(node.adr_registry_revision, registry.revision);
}

#[test]
fn supersede_dry_run_previews_scopes_without_writing() {
    let temp = bootstrap_fixture();
    // One approved ADR per scope, so each preview affects exactly one scope.
    approve_extra(temp.path(), "ADR-0010", &[".mine/design/execution-graph"]);
    approve_extra(temp.path(), "ADR-0012", &[".mine/design/other-zone"]);
    // A still-DRAFT scoped ADR keeps its scope blocked after the previewed
    // supersession, so the two previews report both outcomes.
    let draft_file = ".mine/design/decisions/0011-preview-draft.md";
    std::fs::write(
        temp.path().join(draft_file),
        "# ADR-0011: Preview Fixture\n\nUnapproved decision.\n",
    )
    .unwrap();
    assert_eq!(
        run(
            temp.path(),
            &[
                "adr",
                "add",
                "--id",
                "ADR-0011",
                "--path",
                draft_file,
                "--title",
                "ADR-0011",
                "--scope",
                ".mine/design/other-zone"
            ]
        ),
        0
    );
    let registry_path = temp.path().join(".mine/design/decisions/adr-registry.toml");
    let index_path = temp.path().join(".mine/design/decisions/index.md");
    let registry_before = std::fs::read(&registry_path).unwrap();
    let index_before = std::fs::read(&index_path).unwrap();

    // Scope without unresolved blockers: the preview reports ready.
    let (code, out, err) = run_json(
        temp.path(),
        &[
            "adr",
            "supersede",
            "--id",
            "ADR-0010",
            "--replacement",
            "ADR-0009",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, 0, "dry-run succeeded; stderr: {err}");
    assert!(
        out.contains("\"command\":\"adr.supersede-preview\""),
        "{out}"
    );
    assert!(
        out.contains("\"ready\":true"),
        "ready scope reported: {out}"
    );

    // Scope blocked by the DRAFT ADR-0011: the preview names the blocker.
    let (code, out, err) = run_json(
        temp.path(),
        &[
            "adr",
            "supersede",
            "--id",
            "ADR-0012",
            "--replacement",
            "ADR-0009",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, 0, "dry-run succeeded; stderr: {err}");
    assert!(
        out.contains("\"ready\":false"),
        "blocked scope reported: {out}"
    );
    assert!(out.contains("ADR-0011"), "blocked blocker named: {out}");

    // No preview wrote anything.
    assert_eq!(std::fs::read(&registry_path).unwrap(), registry_before);
    assert_eq!(std::fs::read(&index_path).unwrap(), index_before);

    // The same invalid pair as a real supersede is refused by the preview too.
    let (code, _out, _err) = run_json(
        temp.path(),
        &[
            "adr",
            "supersede",
            "--id",
            "ADR-0010",
            "--replacement",
            "ADR-0011",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert_ne!(code, 0, "unapproved replacement must be refused");
    assert_eq!(std::fs::read(&registry_path).unwrap(), registry_before);
}

#[test]
fn design_render_repairs_stale_index_and_validate_distinguishes() {
    let temp = bootstrap_fixture();
    let index_path = temp.path().join(".mine/design/decisions/index.md");
    // Corrupt the generated index: validate must fail naming the repair.
    std::fs::write(&index_path, "# corrupted\n").unwrap();
    let (code, _out, err) = run_json(temp.path(), &["adr", "validate", "--format", "json"]);
    assert_ne!(code, 0, "stale index must fail validation");
    assert!(
        err.contains("stale") && err.contains("mine design render"),
        "error must name the stale index and the repair: {err}"
    );
    // Render repairs deterministically.
    let (code, out, _err) = run_json(temp.path(), &["design", "render", "--format", "json"]);
    assert_eq!(code, 0);
    assert!(out.contains("\"changed\":true"), "{out}");
    assert_eq!(
        run(temp.path(), &["adr", "validate", "--format", "json"]),
        0
    );
    // Render is idempotent byte-preserving on a fresh index.
    let (code, out, _err) = run_json(temp.path(), &["design", "render", "--format", "json"]);
    assert_eq!(code, 0);
    assert!(out.contains("\"changed\":false"), "{out}");
    // A missing index is created by render and reported as changed.
    std::fs::remove_file(&index_path).unwrap();
    let (code, _out, _err) = run_json(temp.path(), &["design", "render", "--format", "json"]);
    assert_eq!(code, 0);
    assert!(index_path.exists(), "render recreated the missing index");
    assert_eq!(
        run(temp.path(), &["adr", "validate", "--format", "json"]),
        0
    );
}

/// An ordinary repository (no MINE bootstrap import) bootstraps its ADR
/// registry implicitly on the first `adr add`; read paths fail precisely
/// until then, and the normal lifecycle continues afterwards.
#[test]
fn first_adr_add_bootstraps_registry_in_ordinary_repository() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    assert_eq!(run(repo, &["init"]), 0);
    std::fs::create_dir_all(repo.join(".mine/design/decisions")).unwrap();
    prepare_git_governance(repo);
    assert!(
        !repo
            .join(".mine/design/decisions/adr-registry.toml")
            .exists()
    );

    // Read paths fail with the precise absent-registry error before the
    // first ADR exists.
    let (code, _out, err) = run_json(repo, &["adr", "list", "--format", "json"]);
    assert_ne!(code, 0, "{err}");
    assert!(
        err.contains("registry is absent; create the first ADR with `mine adr add`"),
        "{err}"
    );

    // The first add bootstraps the empty registry and records the DRAFT.
    std::fs::write(
        repo.join(".mine/design/decisions/9001-ordinary-decision.md"),
        "# An ordinary decision\n\nBody for the first-add bootstrap test.\n",
    )
    .unwrap();
    let (code, out, _err) = run_json(
        repo,
        &[
            "adr",
            "add",
            "--id",
            "ADR-9001",
            "--path",
            ".mine/design/decisions/9001-ordinary-decision.md",
            "--title",
            "Ordinary decision",
            "--planning-blocking",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"status\":\"DRAFT\""), "{out}");
    assert!(
        repo.join(".mine/design/decisions/adr-registry.toml")
            .exists()
    );

    // The normal lifecycle continues on the bootstrapped registry.
    assert_eq!(
        run(
            repo,
            &["adr", "propose", "--id", "ADR-9001", "--format", "json"]
        ),
        0
    );
    assert_eq!(
        run(
            repo,
            &[
                "adr",
                "approve",
                "--id",
                "ADR-9001",
                "--by",
                "owner",
                "--note",
                "first-add bootstrap test",
                "--format",
                "json"
            ]
        ),
        0
    );
    let (code, out, _err) = run_json(repo, &["design", "readiness", "--format", "json"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"ready\":true"), "{out}");
    assert_eq!(run(repo, &["adr", "validate", "--format", "json"]), 0);
}
