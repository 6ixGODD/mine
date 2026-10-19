// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Real MCP subprocess discovery: spawn `mine mcp serve` as a real subprocess,
//! initialize the MCP client, and verify exactly nineteen tools are exposed.

use std::io::Write;
use std::path::PathBuf;

fn mine_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mine"))
}

#[test]
fn mcp_serve_exposes_exactly_nineteen_tools() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut child = std::process::Command::new(mine_bin())
        .arg("mcp")
        .arg("serve")
        .arg("--repo")
        .arg(&repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn mine mcp serve");
    {
        let mut stdin = child.stdin.take().unwrap();
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e-test","version":"0.0.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
"#;
        stdin.write_all(init.as_bytes()).unwrap();
        stdin.flush().unwrap();
        // Close stdin to trigger EOF/shutdown.
    }
    let output = child.wait_with_output().expect("wait");
    assert!(
        output.status.success(),
        "mcp serve exited non-zero: {:?}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    // Parse stdout: find the tools/list response (id:2).
    let out = String::from_utf8_lossy(&output.stdout);
    let tools_line = out
        .lines()
        .find(|line| line.contains(r#""id":2"#))
        .expect("tools/list response found");
    let parsed: serde_json::Value = serde_json::from_str(tools_line).expect("valid JSON-RPC");
    assert!(parsed.get("jsonrpc").is_some(), "jsonrpc field present");
    let tools = parsed["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 19, "exactly 19 MCP tools exposed");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in [
        "mine_workspace_status",
        "mine_graph_validate",
        "mine_graph_status",
        "mine_graph_ready",
        "mine_graph_wave",
        "mine_plan_show",
        "mine_graph_repair_provenance",
        "mine_graph_repair_compensation_predecessors",
        "mine_design_validate",
        "mine_adr_show",
        "mine_adr_list",
        "mine_adr_validate",
        "mine_design_readiness",
        "mine_plan_add",
        "mine_plan_start",
        "mine_plan_mark_implemented",
        "mine_plan_accept",
        "mine_plan_reject",
        "mine_plan_withdraw",
    ] {
        assert!(
            names.contains(&expected),
            "missing MCP tool {expected}; got {names:?}"
        );
    }
}
