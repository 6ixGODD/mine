// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Contract tests: every MCP tool a Skill references is in the accepted tool
//! surface; every CLI fallback command is implemented; no stale references
//! survive.

use super::common::*;

#[derive(Clone, Copy)]
struct PlanIdentity<'a> {
    id: &'a str,
    path: &'a str,
}

fn invocation_matches(node: PlanIdentity<'_>, input: &str) -> bool {
    if input == node.id || input == node.path {
        return true;
    }
    if node.id == "01" {
        // mine-release-allow-plan-reference: legacy bootstrap invocation fixture
        return matches!(input, "P01" | "plan01");
    }
    let Some(short) = node.id.strip_prefix("plan") else {
        return false;
    };
    input == short || input.strip_prefix('P') == Some(short)
}

fn resolve_invocation<'a>(
    nodes: &'a [PlanIdentity<'a>],
    input: &str,
) -> Result<&'a str, &'static str> {
    let matches = nodes
        .iter()
        .copied()
        .filter(|node| invocation_matches(*node, input))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [node] => Ok(node.id),
        [] => Err("no match"),
        _ => Err("ambiguous"),
    }
}

#[test]
fn exactly_five_root_skills_exist() {
    let mut names: Vec<String> = std::fs::read_dir(skills_root())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        FIVE_SKILLS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        "exactly five first-class Skills exist"
    );
}

#[test]
fn every_skill_references_only_accepted_mcp_tools() {
    for s in FIVE_SKILLS {
        let body = skill_body(s);
        let refs = mcp_tool_refs(&body);
        for tool in &refs {
            assert!(
                ACCEPTED_MCP_TOOLS.contains(tool),
                "{s} references MCP tool {tool:?} not in the accepted MCP surface"
            );
        }
    }
}

#[test]
fn planning_skills_reference_native_mcp_entries_not_an_unpublished_surface() {
    for skill in ["mine-plan-create", "mine-plan-exec", "mine-plan-review"] {
        let body = skill_body(skill);
        assert!(
            !mcp_tool_refs(&body).is_empty(),
            "{skill} omitted all typed MCP entries"
        );
        for name in mcp_tool_refs(&body) {
            assert!(ACCEPTED_MCP_TOOLS.contains(&name), "{skill}: {name}");
        }
    }
}

#[test]
fn every_bundled_markdown_reference_resolves_inside_its_actual_skill() {
    for relative in rel_files(&skills_root()) {
        if !relative.ends_with(".md") {
            continue;
        }
        let source = skills_root().join(&relative);
        let skill = skills_root()
            .join(relative.split('/').next().unwrap())
            .canonicalize()
            .unwrap();
        let body = read_str(&source);
        for tail in body.split("](").skip(1) {
            let target = tail.split(')').next().unwrap();
            if target.contains("://") || target.starts_with('#') {
                continue;
            }
            let file = target.split('#').next().unwrap();
            let resolved = source
                .parent()
                .unwrap()
                .join(file)
                .canonicalize()
                .unwrap_or_else(|error| panic!("{relative}: broken reference {target}: {error}"));
            assert!(
                resolved.starts_with(&skill),
                "{relative} unexpectedly depends on another installed skill: {target}"
            );
            assert!(
                resolved.is_file(),
                "{relative} reference is not a file: {target}"
            );
        }
    }
}

#[test]
fn graph_backed_plan_resolution_accepts_only_complete_unique_forms() {
    let nodes = [
        PlanIdentity {
            // mine-release-allow-plan-reference: canonical Plan identity fixture
            id: "plan03",
            // mine-release-allow-plan-reference: canonical registered-path fixture
            path: "docs/plan/plan03-recover-plan-branch-anchor-governance.md",
        },
        PlanIdentity {
            // mine-release-allow-plan-reference: compensation Plan identity fixture
            id: "plan02-C02",
            // mine-release-allow-plan-reference: compensation registered-path fixture
            path: "docs/plan/plan02-C02-complete-release-readiness.md",
        },
        PlanIdentity {
            id: "01",
            path: "docs/plan/01-bootstrap.md",
        },
    ];

    for input in [
        // mine-release-allow-plan-reference: canonical Plan invocation fixture
        "plan03",
        "03",
        "P03",
        // mine-release-allow-plan-reference: canonical registered-path fixture
        "docs/plan/plan03-recover-plan-branch-anchor-governance.md",
    ] {
        // mine-release-allow-plan-reference: canonical Plan resolution fixture
        assert_eq!(resolve_invocation(&nodes, input), Ok("plan03"));
    }
    // mine-release-allow-plan-reference: compensation invocation fixture
    for input in ["plan02-C02", "02-C02", "P02-C02"] {
        // mine-release-allow-plan-reference: compensation resolution fixture
        assert_eq!(resolve_invocation(&nodes, input), Ok("plan02-C02"));
    }
    // mine-release-allow-plan-reference: legacy bootstrap invocation fixture
    for input in ["01", "P01", "plan01"] {
        assert_eq!(resolve_invocation(&nodes, input), Ok("01"));
    }

    assert_eq!(resolve_invocation(&nodes, "02-C"), Err("no match"));
    assert_eq!(resolve_invocation(&nodes, "03-extra"), Err("no match"));
    assert_eq!(resolve_invocation(&nodes, "p03"), Err("no match"));
}

#[test]
fn graph_backed_plan_resolution_refuses_ambiguity() {
    let nodes = [
        PlanIdentity {
            id: "01",
            path: "docs/plan/01-bootstrap.md",
        },
        PlanIdentity {
            // mine-release-allow-plan-reference: ambiguity fixture Plan identity
            id: "plan01",
            // mine-release-allow-plan-reference: ambiguity fixture registered path
            path: "docs/plan/plan01-modern.md",
        },
    ];
    assert_eq!(resolve_invocation(&nodes, "01"), Err("ambiguous"));
    assert_eq!(resolve_invocation(&nodes, "P01"), Err("ambiguous"));
    assert_eq!(resolve_invocation(&nodes, "missing"), Err("no match"));
}

#[test]
fn review_begin_uses_strict_typed_identity_not_unchecked_caller_assertions() {
    use mine::application::native_lifecycle::BeginInput;
    let id = format!("plan{:02}", 63);
    let mut request = serde_json::json!({
        "schema_version":1,"actor":"independent-fixture-reviewer",
        "intent":{"kind":"acceptance","plan_id":id,
        "review":{"reviewed_tip":"1".repeat(40),
        "report":{"path":format!(".mine/plan/reports/{id}-fixture-review.md"),"bytes":12,"sha256":"a".repeat(64)}}}
    });
    assert!(serde_json::from_value::<BeginInput>(request.clone()).is_ok());
    request["writer_stopped"] = true.into();
    assert!(
        serde_json::from_value::<BeginInput>(request.clone()).is_err(),
        "unchecked stopped-writer input must not bypass the pinned report"
    );
    request.as_object_mut().unwrap().remove("writer_stopped");
    request["intent"]["review"]["report"]["sha256"] = "changed-or-uppercase".into();
    assert!(serde_json::from_value::<BeginInput>(request).is_err());
}

#[test]
fn native_cli_only_workflows_parse_without_inventing_mcp_adapters() {
    use clap::Parser;
    use mine::cli::args::Cli;
    let id = format!("plan{:02}", 63);
    let uuid = uuid::Uuid::new_v4().to_string();
    for arguments in [
        vec![
            "mine",
            "plan",
            "release",
            "--actor",
            "reviewer",
            "--expected-revision",
            "7",
            "--id",
            &id,
        ],
        vec![
            "mine",
            "plan",
            "rewire-compensation",
            "--actor",
            "reviewer",
            "--expected-revision",
            "7",
            "--id",
            &id,
        ],
        vec!["mine", "operation", "show", "--operation-id", &uuid],
        vec![
            "mine",
            "operation",
            "stage",
            "--expected-revision",
            "7",
            "--input",
            ".mine/runtime/stage.json",
        ],
        vec![
            "mine",
            "verification",
            "check",
            "--receipt",
            "receipt.json",
            "--input",
            ".mine/build/request/check.json",
        ],
    ] {
        assert!(
            Cli::try_parse_from(arguments.clone()).is_ok(),
            "{arguments:?}"
        );
    }
}

#[test]
fn no_stale_mine_design_sync_reference() {
    for s in FIVE_SKILLS {
        let body = skill_body(s);
        assert!(
            !body.contains("mine-design-sync"),
            "{s} must not reference the obsolete `mine-design-sync` name"
        );
    }
}

#[test]
fn no_stale_doctor_agents_all_reference() {
    // `mine doctor --agents <scope>` is a real, accepted flag (added for agent
    // diagnostics, corrected for stable-tree compatibility). This test
    // previously banned it as imaginary; that assumption is stale. Instead,
    // verify no Skill invents a flag that truly does not exist, e.g. a
    // `--agents` value the CLI does not accept.
    for s in FIVE_SKILLS {
        let body = skill_body(s);
        assert!(
            !body.contains("mine doctor --agent "),
            "{s} must use the real `--agents` flag name, not a singular `--agent`"
        );
    }
}

#[test]
fn no_stale_architecture_and_detailed_design_as_source() {
    // The progressive design root is docs/design/index.md. The stale
    // single-document path may appear only in negative guidance.
    for s in FIVE_SKILLS {
        let body = skill_body(s);
        for (idx, _) in body.match_indices("architecture-and-detailed-design.md") {
            let start = idx.saturating_sub(80);
            let window = &body[start..idx + 5].to_lowercase();
            assert!(
                window.contains("do not")
                    || window.contains("never")
                    || window.contains("not introduce")
                    || window.contains("competing"),
                "{s} references the stale path as a real source (only negative guidance allowed)"
            );
        }
    }
}

#[test]
fn skills_do_not_reference_unimplemented_cli_groups() {
    // `mine agent` and `mine dist` are now
    // implemented and accepted; Skills may reference them. This test now
    // guards against genuinely unimplemented groups instead.
    for s in FIVE_SKILLS {
        let body = skill_body(s);
        assert!(
            !body.contains("mine publish ") && !body.contains("mine release --publish"),
            "{s} must not reference an unimplemented publish/release-mutation command"
        );
    }
}

#[test]
fn skills_never_edit_graph_files_directly() {
    for s in FIVE_SKILLS {
        let body = skill_body(s);
        assert!(
            body.contains("Never edit")
                || body.contains("never edit")
                || body.contains("Do not edit")
                || body.contains("never edits"),
            "{s} must state it never edits execution-graph files directly"
        );
        assert!(
            body.contains("execution-graph.toml") || body.contains("execution-graph.md"),
            "{s} must reference the execution-graph files it must not edit"
        );
    }
}

#[test]
fn plan_mutable_outputs_are_disjoint_and_stale_sentinels_are_not_observed() {
    let temp = tempfile::tempdir().unwrap();
    let output = |plan: &str| {
        temp.path()
            .join(".mine/build")
            .join(plan)
            .join("cargo-target")
    };
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let base = output("plan01");
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let compensation = output("plan01-C01");
    std::fs::create_dir_all(base.join("debug")).unwrap();
    std::fs::write(base.join("debug/stale-sentinel"), b"rejected-plan-output").unwrap();

    assert_ne!(base, compensation);
    assert!(base.starts_with(temp.path().join(".mine/build")));
    assert!(compensation.starts_with(temp.path().join(".mine/build")));
    assert!(base.join("debug/stale-sentinel").exists());
    assert!(
        !compensation.join("debug/stale-sentinel").exists(),
        "a compensation Plan must not observe base-Plan mutable output"
    );
}
