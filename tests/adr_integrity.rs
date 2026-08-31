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
    let file = format!("docs/design/decisions/{}-preview.md", &id[4..]);
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
    let repo = temp.path().join("docs/plan");
    std::fs::create_dir_all(&repo).unwrap();
    let real_graph: mine::domain::graph::PlanWorkspace =
        toml::from_str(include_str!("fixtures/development-execution-graph.toml"))
            .expect("development graph fixture parses");
    let mut ws = real_graph;
    ws.plans = vec![];
    std::fs::write(
        repo.join("execution-graph.toml"),
        toml::to_string(&ws).unwrap(),
    )
    .unwrap();
    mine::infrastructure::toml_store::TomlStore::new(temp.path())
        .render()
        .unwrap();
    temp
}

/// Registers a blocking ADR in the given state and scope, creating its body.
fn scoped_adr(repo: &Path, id: &str, scope: &str, approve: bool) {
    let file = format!("docs/design/decisions/{}-scope.md", &id[4..]);
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
    // A DRAFT blocking ADR scoped under the design root applies to the
    // default registration scope and must block it.
    scoped_adr(repo, "ADR-0010", "docs/design/execution-graph", false);
    let base_add = [
        "plan",
        "add",
        "--id",
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        "plan02",
        "--path",
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        "docs/plan/plan02-fixture.md",
        "--title",
        "fixture",
        "--design-ref",
        "docs/design/index.md",
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
        repo.join("docs/plan/plan02-fixture.md"),
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        "# plan02
",
    )
    .unwrap();
    let mut scoped_add = base_add.to_vec();
    scoped_add.push("--planning-scope");
    scoped_add.push("docs/other-zone");
    let (code, _out, _err) = run_json(repo, &scoped_add);
    assert_eq!(code, 0, "out-of-scope registration proceeds");
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
        repo.join("docs/plan/plan02-fixture.md"),
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        "# plan02
",
    )
    .unwrap();
    assert_eq!(
        run(
            repo,
            &[
                "plan",
                "add",
                "--id",
                // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                "plan02",
                "--path",
                // mine-release-allow-plan-reference: plan-id lifecycle fixture
                "docs/plan/plan02-fixture.md",
                "--title",
                "fixture",
                "--design-ref",
                "docs/design/index.md",
            ],
        ),
        0,
        "registration with no blockers"
    );

    // A PROPOSED blocking ADR in the default scope appears after registration
    // and must fail the release-time re-check, writing nothing.
    scoped_adr(repo, "ADR-0010", "docs/design", false);
    let before = std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let (code, _out, err) = run_json(repo, &["plan", "release", "--id", "plan02"]);
    assert_ne!(code, 0, "release re-check must block");
    assert!(
        err.contains("planning readiness blocked") && err.contains("ADR-0010"),
        "blocker DTOs in the error: {err}"
    );
    assert_eq!(
        std::fs::read(repo.join("docs/plan/execution-graph.toml")).unwrap(),
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
    let registry = AdrStore::new(repo, &AdrConfig::default()).load().unwrap();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    let (code, out, _err) = run_json(repo, &["plan", "release", "--id", "plan02"]);
    assert_eq!(code, 0, "release proceeds once ready: {out}");
    assert!(
        out.contains("\"planning_scope\":\"docs/design\""),
        "release envelope carries the scope: {out}"
    );
    let graph_text = std::fs::read_to_string(repo.join("docs/plan/execution-graph.toml")).unwrap();
    assert!(
        graph_text.starts_with("schema_version = \"mine-graph-v4\""),
        "mutation upgraded the wire schema"
    );
    let graph: mine::domain::graph::PlanWorkspace = toml::from_str(&graph_text).unwrap();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let node = graph.get("plan02").unwrap();
    assert_eq!(node.status.as_str(), "READY");
    assert_eq!(node.planning_scope, "docs/design");
    assert!(!node.readiness_checked_at.is_empty());
    assert_eq!(node.adr_registry_revision, registry.revision);
}

#[test]
fn supersede_dry_run_previews_scopes_without_writing() {
    let temp = bootstrap_fixture();
    // One approved ADR per scope, so each preview affects exactly one scope.
    approve_extra(temp.path(), "ADR-0010", &["docs/design/execution-graph"]);
    approve_extra(temp.path(), "ADR-0012", &["docs/design/other-zone"]);
    // A still-DRAFT scoped ADR keeps its scope blocked after the previewed
    // supersession, so the two previews report both outcomes.
    let draft_file = "docs/design/decisions/0011-preview-draft.md";
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
                "docs/design/other-zone"
            ]
        ),
        0
    );
    let registry_path = temp.path().join("docs/design/decisions/adr-registry.toml");
    let index_path = temp.path().join("docs/design/decisions/index.md");
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
    let index_path = temp.path().join("docs/design/decisions/index.md");
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
