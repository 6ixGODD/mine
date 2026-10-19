#![forbid(unsafe_code)]
//! Whole-payload references checked against real parser and live stdio schemas.
use super::common::*;
use clap::Parser;
use mine::cli::args::Cli;
use rmcp::{service::serve_client, transport::child_process::TokioChildProcess};
use std::collections::BTreeSet;

#[test]
fn mcp_reference_extraction_does_not_hide_an_invented_tool() {
    assert_eq!(
        mcp_tool_refs("invoke mine_unknown_mutation"),
        ["mine_unknown_mutation"]
    );
    assert!(!ACCEPTED_MCP_TOOLS.contains(&mcp_tool_refs("mine_unknown_mutation")[0]));
}

#[tokio::test]
async fn every_bundled_reference_resolves_to_real_stdio_mcp_contract() {
    let fixture = tempfile::tempdir().unwrap();
    let initialized = std::process::Command::new(env!("CARGO_BIN_EXE_mine"))
        .args(["init", "--format", "json", "--repo"])
        .arg(fixture.path())
        .env("HOME", fixture.path().join("fixture-home"))
        .env("USERPROFILE", fixture.path().join("fixture-home"))
        .output()
        .unwrap();
    assert!(initialized.status.success(), "{initialized:?}");
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_mine"));
    command
        .args(["mcp", "serve", "--repo"])
        .arg(fixture.path())
        .env("HOME", fixture.path().join("fixture-home"))
        .env("USERPROFILE", fixture.path().join("fixture-home"));
    let transport = TokioChildProcess::builder(command).spawn().unwrap().0;
    let mut client = serve_client((), transport).await.unwrap();
    let tools = client.peer().list_all_tools().await.unwrap();
    client.close().await.unwrap();
    let actual: BTreeSet<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(actual, ACCEPTED_MCP_TOOLS.iter().copied().collect());
    for relative in rel_files(&skills_root()) {
        let payload = read_str(&skills_root().join(&relative));
        for tool in mcp_tool_refs(&payload) {
            assert!(
                actual.contains(tool),
                "{relative} names unpublished MCP tool {tool}"
            );
        }
    }
    for name in [
        "mine_plan_add",
        "mine_plan_start",
        "mine_plan_mark_implemented",
        "mine_plan_accept",
        "mine_plan_reject",
        "mine_plan_withdraw",
    ] {
        let tool = tools.iter().find(|tool| tool.name == name).unwrap();
        let schema = serde_json::to_value(&tool.input_schema).unwrap();
        let required = schema["required"].as_array().unwrap();
        assert!(
            required.iter().any(|field| field == "expected_revision"),
            "{name} silently refreshed its observer"
        );
        assert!(schema["properties"].get("id").is_some());
    }
    for name in [
        "mine_plan_add",
        "mine_plan_mark_implemented",
        "mine_plan_reject",
        "mine_plan_withdraw",
    ] {
        let tool = tools.iter().find(|tool| tool.name == name).unwrap();
        let schema = serde_json::to_value(&tool.input_schema).unwrap();
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "actor")
        );
    }
    assert!(
        !actual.iter().any(|name| name.starts_with("mine_operation_")
            || name.starts_with("mine_verification_")
            || *name == "mine_plan_prepare")
    );
}

#[test]
fn native_parser_requires_explicit_prepare_identity_and_guarded_acceptance() {
    let id = format!("plan{:02}", 63);
    let uuid = uuid::Uuid::new_v4().to_string();
    let valid = [
        "mine",
        "plan",
        "prepare",
        "--id",
        &id,
        "--actor",
        "fixture-reviewer",
        "--expected-revision",
        "7",
        "--operation-id",
        &uuid,
    ];
    assert!(Cli::try_parse_from(valid).is_ok());
    for missing in ["--actor", "--expected-revision", "--operation-id"] {
        let position = valid.iter().position(|token| *token == missing).unwrap();
        let mut incomplete = valid.to_vec();
        incomplete.drain(position..position + 2);
        assert!(
            Cli::try_parse_from(incomplete).is_err(),
            "accepted missing {missing}"
        );
    }
    let mut conflicting = valid.to_vec();
    conflicting.extend(["--reprepare", "--recover-preparation"]);
    assert!(Cli::try_parse_from(conflicting).is_err());
    assert!(
        Cli::try_parse_from([
            "mine",
            "plan",
            "accept",
            "--id",
            &id,
            "--review",
            "report.md"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "mine",
            "plan",
            "accept",
            "--id",
            &id,
            "--expected-revision",
            "7",
            "--operation-id",
            &uuid,
            "--input",
            ".mine/runtime/finalize.json"
        ])
        .is_ok()
    );
    assert!(Cli::try_parse_from(["mine", "operation", "show", "--operation-id", &uuid]).is_ok());
    assert!(Cli::try_parse_from(["mine", "graph", "checkpoint"]).is_err());
}
