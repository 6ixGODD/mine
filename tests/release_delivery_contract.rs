#![forbid(unsafe_code)]
//! Workflow dependency counterexamples plus the accepted platform policy.
//! Static/harness checks are not native installation evidence.
use std::path::Path;
use std::process::Command;

#[test]
fn typed_legacy_source_fixture_for_actual_phase_probes() {
    use mine::domain::{adr::AdrRegistry, config::MineConfig, graph::PlanWorkspace};
    use mine::infrastructure::{adr_store::render_index, toml_store::render_markdown};
    use serde_json::json;
    let id = format!("plan{:02}", 91);
    let baseline = std::env::var("MINE_DELIVERY_FIXTURE_BASELINE").unwrap_or("a".repeat(40));
    let exported = std::env::var("MINE_DELIVERY_FIXTURE_ROOT").ok();
    let config = exported.as_ref().map(|directory| {
        let path = Path::new(directory).join(".mine/config.toml");
        MineConfig::parse(&path, &std::fs::read_to_string(&path).unwrap()).unwrap()
    });
    let plan_root = config
        .as_ref()
        .map_or("docs/plan", |c| c.plan.root.as_str());
    let design = config
        .as_ref()
        .map_or("docs/design/index.md", |c| c.design.root.as_str());
    let project = config
        .as_ref()
        .map_or("delivery-phase-fixture", |c| c.repository_id.as_str());
    let workspace: PlanWorkspace = serde_json::from_value(json!({
        "schema_version":"mine-graph-v4", "revision":1,
        "project_id":project, "workspace_id":"delivery-phase-fixture",
        "stable_branch":"master", "integration_branch":"dev",
        "stable_baseline_commit":baseline, "design_root":design,
        "ephemeral_workspace":true, "purge_before_stable_release":true,
        "plans":[{"id":id,"path":format!("{plan_root}/{id}-delivery.md"),
            "title":"Historical delivery fixture", "status":"ACCEPTED",
            "branch":format!("plan/{id}-delivery"), "branch_anchor":baseline,
            "lineage_id":id,"worktree_path":format!(".mine/worktrees/{id}"),
            "design_references":[design],
            "implementation_commits":[baseline], "owner":"historical-fixture-writer",
            "run_id":"historical-fixture-run",
            "implementation_report":format!("{plan_root}/reports/{id}-delivery-implementation.md"),
            "review_report":format!("{plan_root}/reports/{id}-delivery-review.md")}]
    }))
    .unwrap();
    assert_eq!(workspace.schema_version, 4);
    let rendered = render_markdown(&workspace).unwrap();
    assert!(rendered.contains(&id));
    assert!(rendered.contains("ACCEPTED"));
    // Test-fixture serialization uses the actual public model/renderers. It
    // never fabricates approval or writes the real repository's graph.
    if let Some(directory) = exported {
        let root = Path::new(&directory).canonicalize().unwrap();
        assert!(root.join(".git").is_dir());
        assert_eq!(
            std::fs::read(root.join(".mine/runtime/delivery-fixture.marker")).unwrap(),
            b"isolated-delivery-phase-fixture\n"
        );
        let config = config.unwrap();
        let layout = config.layout().unwrap();
        assert!(!root.join(&config.plan.root).exists());
        let plan_root = &config.plan.root;
        let files = [
            (config.graph.source, toml::to_string(&workspace).unwrap()),
            (config.graph.rendered, rendered),
            (
                format!("{plan_root}/{id}-delivery.md"),
                "# Historical fixture Plan\n".to_string(),
            ),
            (
                format!("{plan_root}/reports/{id}-delivery-implementation.md"),
                "# Fixture implementation\n".to_string(),
            ),
            (
                format!("{plan_root}/reports/{id}-delivery-review.md"),
                "# Historical fixture review\n\nVerdict: ACCEPTED\n".to_string(),
            ),
            (
                config.adr.registry,
                toml::to_string(&AdrRegistry::empty()).unwrap(),
            ),
            (
                config.adr.rendered_index,
                render_index(&AdrRegistry::empty(), layout.decisions_dir().as_str()).unwrap(),
            ),
        ];
        for (relative, body) in files {
            let target = root.join(relative);
            assert!(!target.exists());
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, body).unwrap();
        }
    }
}

#[test]
fn publication_gate_counterexamples_execute_the_real_harness() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("python")
        .arg(root.join("scripts/test-install-delivery.py"))
        .arg("--contracts-only")
        .current_dir(root)
        .output()
        .expect("managed Python is required for delivery contract tests");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Ran 5 tests"));
}

#[test]
fn workflow_matrix_is_the_accepted_platform_manifest_not_a_parallel_table() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("python")
        .arg(root.join("scripts/test-install-delivery.py"))
        .arg("--matrix")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    let matrix: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let source: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("distribution/platforms.json")).unwrap())
            .unwrap();
    assert_eq!(matrix["include"], source["platforms"]);
    for row in source["platforms"].as_array().unwrap() {
        let native = mine::setup::platform::lookup(
            row["os"].as_str().unwrap(),
            row["arch"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(row["archive"], native.archive);
        assert_eq!(row["target"], native.target);
        assert_eq!(row["binary"], native.binary);
        assert_eq!(row["runner"], native.runner);
    }
    assert!(mine::setup::platform::lookup("macos", "x86_64").is_err());
    let release = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(release.contains("python scripts/test-install-delivery.py --matrix"));
    assert!(release.contains("matrix: ${{ fromJSON(needs.platforms.outputs.matrix) }}"));
    let ci = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    assert!(ci.contains("matrix: ${{ fromJSON(inputs.matrix) }}"));
    assert!(ci.contains("cargo test --all-targets --all-features"));
    assert!(ci.contains("RUSTDOCFLAGS: \"-D warnings\""));
    assert!(ci.contains("design validate --format json"));
    assert!(ci.contains("adr validate --format json"));
    assert!(ci.contains("--preflight"));
    assert!(ci.contains("never mutate bootstrap state"));
}

#[test]
fn release_notes_use_canonical_upstream_and_truthful_native_support() {
    let notes = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/RELEASE_NOTES_TEMPLATE.md"),
    )
    .unwrap();
    assert!(!notes.contains("6ixGODD/mine-is-not-everyones"));
    for platform in mine::setup::platform::PLATFORMS {
        assert!(notes.contains(platform.archive));
    }
    assert!(notes.contains("macOS delivery is owner-DEFERRED"));
    assert!(
        notes.contains("0.2.0 archive; preserved source/history is not a native verification pass")
    );
    assert!(
        mine::setup::platform::PLATFORMS
            .iter()
            .all(|platform| !platform.target.contains("apple"))
    );
    assert!(!notes.contains("mine-macos-"));
    assert!(notes.contains("SHA256SUMS.txt"));
}

#[test]
fn local_v020_exception_excludes_only_that_tag_and_preserves_normal_gates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["release.yml", "changelog.yml"] {
        let source = std::fs::read_to_string(root.join(".github/workflows").join(name)).unwrap();
        let header = source
            .lines()
            .skip_while(|line| *line != "on:")
            .take_while(|line| *line != "permissions:")
            .collect::<Vec<_>>();
        let filters = header
            .iter()
            .filter_map(|line| line.strip_prefix("    tags: "))
            .collect::<Vec<_>>();
        let events = header
            .iter()
            .copied()
            .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            if name == "release.yml" {
                vec!["  push:", "  pull_request:"]
            } else {
                vec!["  push:", "  workflow_dispatch:"]
            },
            "unexpected automatic event policy: {name}"
        );
        assert_eq!(filters.len(), 1, "one push-tag filter in {name}");
        let patterns: Vec<String> = serde_json::from_str(filters[0]).unwrap();
        assert_eq!(patterns, ["v*", "!v0.2.0"], "ordered source policy: {name}");
        // Documented ordered semantics for this exact two-pattern subset,
        // not a claim to run or emulate the GitHub Actions service.
        for (tag, expected) in [
            ("v0.2.0", false),
            ("v0.2.1", true),
            ("v1.0.0", true),
            ("other", false),
        ] {
            let mut selected = false;
            for pattern in &patterns {
                if pattern == "v*" && tag.starts_with('v') {
                    selected = true;
                } else if pattern.strip_prefix('!') == Some(tag) {
                    selected = false;
                }
            }
            assert_eq!(selected, expected, "{name} {tag}");
        }
        for unexpected in ["  release:", "  create:", "  workflow_run:", "  schedule:"] {
            assert!(
                !header.contains(&unexpected),
                "new automatic event in {name}"
            );
        }
        if name == "release.yml" {
            assert!(header.contains(&"    branches: [master, dev, \"plan/**\"]"));
            assert!(
                source.contains(
                    "needs: [quality, preflight, mcp-discovery, payload, native-delivery]"
                )
            );
            assert!(source.contains("--gate-needs \"$NEEDS_JSON\" --ref \"$GITHUB_REF\""));
        } else {
            assert!(header.contains(&"  workflow_dispatch:"));
        }
    }
    let ci = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let ci_events = ci
        .lines()
        .skip_while(|line| *line != "on:")
        .take_while(|line| *line != "permissions:")
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .collect::<Vec<_>>();
    assert_eq!(ci_events, ["  workflow_call:"]);
    let notes = std::fs::read_to_string(root.join(".github/RELEASE_NOTES_TEMPLATE.md")).unwrap();
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    for text in [notes, changelog] {
        assert!(text.contains("GitHub workflows: NOT_RUN, not PASS"));
        assert!(text.contains("0.2.0"));
    }
}
