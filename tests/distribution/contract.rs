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
fn planning_skills_are_mcp_first_with_cli_fallback() {
    for s in ["mine-plan-create", "mine-plan-exec", "mine-plan-review"] {
        let body = skill_body(s);
        assert!(
            !mcp_tool_refs(&body).is_empty(),
            "{s} must reference accepted MCP tools (MCP-first)"
        );
        assert!(
            body.contains("CLI fallback"),
            "{s} must document the CLI fallback explicitly"
        );
        assert!(
            body.contains("mine plan ") || body.contains("mine graph "),
            "{s} must reference the `mine` CLI as fallback"
        );
    }
}

#[test]
fn execution_and_review_skills_define_graph_backed_plan_resolution() {
    for skill in ["mine-plan-exec", "mine-plan-review"] {
        let body = skill_body(skill);
        for required in [
            "mine graph show --format json",
            "planNN",
            "PNN",
            "planNN-CNN",
            "PNN-CNN",
            "legacy bootstrap",
            "exact stored ID",
            "exact registered path",
            "exactly one distinct graph",
            "prefix/substring",
            "NN-C",
        ] {
            assert!(
                body.contains(required),
                "{skill} must document graph-backed invocation contract fragment {required:?}"
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
fn skills_govern_anchor_start_and_active_rejection() {
    let arch = skill_body("mine-arch");
    assert!(arch.contains("objectively infeasible `IN_PROGRESS` Plan"));
    assert!(arch.contains("canonical Root-visible review report"));

    let create = skill_body("mine-plan-create");
    assert!(create.contains("stored-anchor schema recovery"));
    assert!(create.contains("next contiguous sibling compensation"));

    let exec = skill_body("mine-plan-exec");
    assert!(exec.contains("mine plan prepare --id <exact-stored-id>"));
    assert!(exec.contains("persisted `branch_anchor` equals the prepared worktree HEAD"));
    assert!(exec.contains("keep the graph `IN_PROGRESS`"));

    let review = skill_body("mine-plan-review");
    assert!(review.contains("Review an infeasible active Plan"));
    assert!(review.contains("--review-report <review report path>"));
    assert!(review.contains("merge the Plan branch into Root `dev` with `--no-ff`"));
}

#[test]
fn skills_document_cli_only_operations_without_mcp() {
    // Skills must state which operations are CLI-only (no MCP tool).
    // mine-plan-create must document that `release` is CLI-only.
    let create = skill_body("mine-plan-create");
    assert!(
        create.contains("no MCP tool for release") || create.contains("CLI-only"),
        "mine-plan-create must document release as a CLI-only fallback"
    );
    // mine-plan-review must document that rewire-compensation is CLI-only.
    let review = skill_body("mine-plan-review");
    assert!(
        review.contains("no MCP tool for rewiring") || review.contains("rewire-compensation"),
        "mine-plan-review must document rewire-compensation as CLI-only"
    );
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

    for skill in FIVE_SKILLS {
        let body = skill_body(skill);
        assert!(
            body.contains("<Root>/.mine/build/<plan-id>/<tool-or-output-class>/"),
            "{skill} must document exact Plan output ownership"
        );
        assert!(
            body.contains("CARGO_TARGET_DIR"),
            "{skill} must document the Rust output adapter"
        );
        assert!(
            body.contains("very large") || body.contains("exceptionally large"),
            "{skill} must document the measured-capacity exception"
        );
    }
}
