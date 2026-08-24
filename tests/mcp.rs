// Enforce no `unsafe` in test code (mirrors the library crate's `forbid`).
#![forbid(unsafe_code)]

//! End-to-end MCP server integration tests.
//!
//! These tests drive the **real rmcp stdio transport**: each test spawns the
//! built `mine mcp serve` binary as a child process, connects to it as an rmcp
//! client (`()` client handler over `TokioChildProcess`), and exercises the
//! full MCP lifecycle (`initialize` -> `initialized` -> `tools/list` ->
//! `tools/call` -> shutdown) exactly as an external MCP client would.
//!
//! They do NOT call `MineServer` directly or substitute a hand-written
//! dispatcher; the request goes through rmcp's JSON-RPC framing, the
//! `#[tool_router]` macro dispatch, SDK-generated JSON Schema, and the stdio
//! transport -- the same code path real clients use.
//!
//! Every test uses an ISOLATED TEMPORARY repository seeded via
//! `tests::common::seeded_repo`. The live repository graph is snapshotted
//! before and after the test binary and asserted byte-identical.

// `tests/common/mod.rs` is a shared helper module (reused across
// suite); this binary uses only a subset of its helpers, so silence the
// per-binary dead-code lint without modifying the shared file.
#[allow(dead_code)]
mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use rmcp::model::CallToolRequestParams;
use rmcp::model::CallToolResult;
use rmcp::service::{RoleClient, RunningService, serve_client};
use rmcp::transport::child_process::TokioChildProcess;
use serde_json::{Value, json};
use tempfile::TempDir;

use mine::domain::status::PlanStatus;
use mine::{cli, cli::Outcome};

use common::{live_graph_bytes, node, seeded_repo};

/// Path to the built `mine` binary (compiled by `cargo test`).
fn mine_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mine"))
}

/// Spawns `mine mcp serve --repo <repo>` as an rmcp child-process transport,
/// connects as a client (empty `()` client handler), and returns the running
/// client service. The caller keeps `tmp` alive for the test duration; dropping
/// the returned service triggers a graceful transport close, which closes the
/// child's stdin and lets the server exit on EOF.
async fn connect(repo: PathBuf) -> RunningService<RoleClient, ()> {
    let mut cmd = tokio::process::Command::new(mine_bin());
    cmd.arg("mcp").arg("serve").arg("--repo").arg(&repo);
    // `TokioChildProcess` implements `Transport<RoleClient>`, which blanket-impls
    // `IntoTransport`; hand it directly to `serve_client`. The `()` client
    // handler is the minimal client (no client-side capabilities needed).
    let proc = TokioChildProcess::builder(cmd)
        .spawn()
        .expect("spawn mine mcp serve")
        .0;
    serve_client((), proc)
        .await
        .expect("client initialize handshake")
}

/// Converts a JSON value into the `JsonObject` (`Map<String, Value>`) expected
/// by `CallToolRequestParams::arguments`.
fn args_obj(v: Value) -> rmcp::model::JsonObject {
    v.as_object()
        .cloned()
        .unwrap_or_else(|| panic!("args must be a JSON object, got {v}"))
}

/// Calls a tool by name with JSON arguments and returns the structured-content
/// JSON (or, for error results, the textual content).
async fn call_tool(client: &RunningService<RoleClient, ()>, name: &str, args: Value) -> Value {
    let result = call_tool_raw(client, name, args).await;
    parse_result(&result)
}

/// Calls a tool and returns the raw `CallToolResult` (for inspecting
/// `is_error`).
async fn call_tool_raw(
    client: &RunningService<RoleClient, ()>,
    name: &str,
    args: Value,
) -> CallToolResult {
    client
        .peer()
        .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(args_obj(args)))
        .await
        .expect("call_tool succeeds at protocol level")
}

/// Extracts the structured-content JSON from a `CallToolResult`; falls back to
/// the textual content for tool-level errors (which carry the message as text).
fn parse_result(result: &CallToolResult) -> Value {
    if let Some(sc) = &result.structured_content {
        return sc.clone();
    }
    let text: String = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("");
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

/// Fixture: plan `01` ACCEPTED, plan `02` READY (soft-chained off `01`), plan
/// `03` READY (no predecessors). `02` and `03` are startable; `01` is a
/// terminal anchor.
fn fixture() -> (TempDir, PathBuf) {
    let p01 = node("01", PlanStatus::Accepted, &[], &[]);
    let p02 = node("02", PlanStatus::Ready, &[], &["01"]);
    let p03 = node("03", PlanStatus::Ready, &[], &[]);
    seeded_repo(vec![p01, p02, p03])
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn dispatch(repo: &Path, args: &[&str]) -> Outcome {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
    ];
    argv.extend(args.iter().map(ToString::to_string));
    cli::dispatch(&argv, "mine")
}

fn start_fixture() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_path_buf();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "test"]);
    std::fs::write(repo.join("README.md"), "fixture\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "/.mine/\n").unwrap();
    git(&repo, &["add", "README.md", ".gitignore"]);
    git(&repo, &["commit", "--quiet", "-m", "initial"]);
    assert_eq!(dispatch(&repo, &["init"]).exit_code, 0);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "mine init"]);
    git(&repo, &["switch", "-c", "dev"]);
    assert_eq!(dispatch(&repo, &["workspace", "open"]).exit_code, 0);
    // mine-release-allow-plan-reference: MCP multi-Plan setup fixture
    for id in ["plan02", "plan03"] {
        let path = format!("docs/plan/{id}-fixture.md");
        std::fs::write(repo.join(&path), format!("# {id}\n")).unwrap();
        assert_eq!(
            dispatch(
                &repo,
                &[
                    "plan",
                    "add",
                    "--id",
                    id,
                    "--path",
                    &path,
                    "--title",
                    "fixture",
                    "--design-ref",
                    "docs/design/index.md",
                    "--write",
                    "src/",
                ],
            )
            .exit_code,
            0
        );
        assert_eq!(
            dispatch(&repo, &["plan", "release", "--id", id]).exit_code,
            0
        );
    }
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "register plans"]);
    // mine-release-allow-plan-reference: MCP multi-Plan prepare fixture
    for id in ["plan02", "plan03"] {
        assert_eq!(
            dispatch(&repo, &["plan", "prepare", "--id", id]).exit_code,
            0
        );
    }
    (tmp, repo)
}

/// Gracefully closes the client so the child server exits on stdin EOF instead
/// of being killed on drop (avoids leaking processes on test failure).
async fn shutdown(client: RunningService<RoleClient, ()>) {
    let mut c = client;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), c.close()).await;
}

#[tokio::test]
async fn lists_all_seventeen_tools_with_schemas() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let tools = client.peer().list_all_tools().await.expect("list tools");
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    let expected = [
        "mine_workspace_status",
        "mine_graph_validate",
        "mine_graph_status",
        "mine_graph_ready",
        "mine_graph_wave",
        "mine_plan_show",
        "mine_graph_repair_provenance",
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
    ];
    for e in expected {
        assert!(names.contains(&e), "missing tool {e}; got {names:?}");
    }
    assert_eq!(names.len(), 17, "exactly 17 tools; got {names:?}");
    for t in &tools {
        assert!(
            !t.input_schema.is_empty(),
            "tool {} has an empty input schema",
            t.name
        );
    }
}

#[tokio::test]
async fn workspace_status_reports_revision_and_plan_count() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let v = call_tool(&client, "mine_workspace_status", json!({})).await;
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
    assert_eq!(v.get("ok").and_then(|o| o.as_bool()), Some(true));
    assert_eq!(v.get("plan_count").and_then(|p| p.as_u64()), Some(3));
    assert!(v.get("revision").and_then(|r| r.as_u64()).is_some());
    assert!(v.get("ready").and_then(|r| r.as_array()).is_some());
}

#[tokio::test]
async fn graph_status_aliases_workspace_status() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let v = call_tool(&client, "mine_graph_status", json!({})).await;
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
    assert_eq!(v.get("ok").and_then(|o| o.as_bool()), Some(true));
    assert_eq!(
        v.get("command").and_then(|c| c.as_str()),
        Some("graph.status")
    );
    assert_eq!(v.get("plan_count").and_then(|p| p.as_u64()), Some(3));
}

#[tokio::test]
async fn graph_validate_ready_wave() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;

    let v = call_tool(&client, "mine_graph_validate", json!({})).await;
    assert_eq!(v.get("ok").and_then(|o| o.as_bool()), Some(true));
    assert_eq!(v.get("plans").and_then(|p| p.as_u64()), Some(3));

    // `ready` is the ready-frontier (plans in READY status): 02 and 03.
    let r = call_tool(&client, "mine_graph_ready", json!({})).await;
    assert_eq!(r.get("ok").and_then(|o| o.as_bool()), Some(true));
    let ready = r
        .get("ready")
        .and_then(|x| x.as_array())
        .expect("ready array");
    let ready_ids: Vec<&str> = ready.iter().filter_map(|x| x.as_str()).collect();
    assert!(ready_ids.contains(&"02"), "02 ready, got {ready_ids:?}");
    assert!(ready_ids.contains(&"03"), "03 ready, got {ready_ids:?}");

    // `wave` is a write-scope-disjoint subset of ready.
    let w = call_tool(&client, "mine_graph_wave", json!({})).await;
    assert_eq!(w.get("ok").and_then(|o| o.as_bool()), Some(true));
    assert!(w.get("wave").and_then(|x| x.as_array()).is_some());

    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
}

#[tokio::test]
async fn plan_show_returns_plan_fields() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let v = call_tool(&client, "mine_plan_show", json!({ "id": "02" })).await;
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
    assert_eq!(v.get("ok").and_then(|o| o.as_bool()), Some(true));
    let plan = v
        .get("data")
        .and_then(|d| d.get("plan"))
        .expect("data.plan");
    assert_eq!(plan.get("id").and_then(|i| i.as_str()), Some("02"));
    assert_eq!(plan.get("status").and_then(|s| s.as_str()), Some("READY"));
    assert!(plan.get("registered_worktree_path").is_some());
    assert!(plan.get("resolved_worktree_path").is_some());
    assert!(plan.get("caller_location").is_some());
    assert_eq!(
        plan.get("authoritative_revision").and_then(|r| r.as_u64()),
        Some(72)
    );
}

#[tokio::test]
async fn current_graph_provenance_repair_is_an_idempotent_mcp_operation() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let value = call_tool(
        &client,
        "mine_graph_repair_provenance",
        json!({ "expected_revision": 72 }),
    )
    .await;
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
    assert_eq!(value.get("ok").and_then(|ok| ok.as_bool()), Some(true));
    assert_eq!(
        value
            .get("data")
            .and_then(|data| data.get("changed"))
            .and_then(|changed| changed.as_bool()),
        Some(false)
    );
    assert_eq!(
        value.get("revision_before").and_then(|rev| rev.as_u64()),
        Some(72)
    );
    assert_eq!(
        value.get("revision_after").and_then(|rev| rev.as_u64()),
        Some(72)
    );
}

#[tokio::test]
async fn design_validate_runs() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let v = call_tool(&client, "mine_design_validate", json!({})).await;
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
    assert_eq!(v.get("ok").and_then(|o| o.as_bool()), Some(true));
    assert!(
        v.get("data").and_then(|d| d.get("valid")).is_some(),
        "design_validate returns data.valid: {v}"
    );
}

#[tokio::test]
async fn plan_add_creates_draft_plan() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;

    let added = call_tool(
        &client,
        "mine_plan_add",
        json!({
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "id": "plan01-04",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "path": "docs/plan/plan01-04-test-node.md",
            "title": "Test node",
            "design_references": ["docs/design/principles.md"],
            "exclusive_write_paths": ["tests/04/"],
            "hard_predecessors": ["01"]
        }),
    )
    .await;
    assert_eq!(added.get("ok").and_then(|o| o.as_bool()), Some(true));
    assert_eq!(
        added.get("command").and_then(|c| c.as_str()),
        Some("plan.add")
    );
    // revision_after == revision_before + 1
    let before_rev = added
        .get("revision_before")
        .and_then(|r| r.as_u64())
        .expect("revision_before");
    let after_rev = added
        .get("revision_after")
        .and_then(|r| r.as_u64())
        .expect("revision_after");
    assert_eq!(after_rev, before_rev + 1);

    // Confirm the new plan is DRAFT (not startable until promoted to READY).
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let shown = call_tool(&client, "mine_plan_show", json!({ "id": "plan01-04" })).await;
    let plan = shown
        .get("data")
        .and_then(|d| d.get("plan"))
        .expect("data.plan");
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    assert_eq!(plan.get("id").and_then(|i| i.as_str()), Some("plan01-04"));
    assert_eq!(plan.get("status").and_then(|s| s.as_str()), Some("DRAFT"));

    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
}

#[tokio::test]
async fn plan_lifecycle_start_implemented_accept() {
    let before = live_graph_bytes();
    let (tmp, repo) = start_fixture();
    let client = connect(repo).await;

    // Start a READY node (READY -> IN_PROGRESS).
    // mine-release-allow-plan-reference: MCP lifecycle Plan fixture
    let started = call_tool(&client, "mine_plan_start", json!({ "id": "plan02" })).await;
    assert_eq!(started.get("ok").and_then(|o| o.as_bool()), Some(true));
    assert_eq!(
        started.get("command").and_then(|c| c.as_str()),
        Some("plan.start")
    );
    // mine-release-allow-plan-reference: MCP lifecycle Plan fixture
    let s = call_tool(&client, "mine_plan_show", json!({ "id": "plan02" })).await;
    assert_eq!(
        s.get("data")
            .and_then(|d| d.get("plan"))
            .and_then(|p| p.get("status"))
            .and_then(|st| st.as_str()),
        Some("IN_PROGRESS")
    );

    // Mark implemented (IN_PROGRESS -> IMPLEMENTED).
    let impld = call_tool(
        &client,
        "mine_plan_mark_implemented",
        json!({
            // mine-release-allow-plan-reference: MCP lifecycle Plan fixture
            "id": "plan02",
            "report": "docs/plan/reports/02-impl.md",
            "commits": ["abc123"]
        }),
    )
    .await;
    assert_eq!(impld.get("ok").and_then(|o| o.as_bool()), Some(true));
    // mine-release-allow-plan-reference: MCP lifecycle Plan fixture
    let s = call_tool(&client, "mine_plan_show", json!({ "id": "plan02" })).await;
    assert_eq!(
        s.get("data")
            .and_then(|d| d.get("plan"))
            .and_then(|p| p.get("status"))
            .and_then(|st| st.as_str()),
        Some("IMPLEMENTED")
    );

    // Accept (IMPLEMENTED -> ACCEPTED).
    let accepted = call_tool(
        &client,
        "mine_plan_accept",
        json!({
            // mine-release-allow-plan-reference: MCP lifecycle Plan fixture
            "id": "plan02",
            "review": "docs/plan/reports/02-review.md"
        }),
    )
    .await;
    assert_eq!(accepted.get("ok").and_then(|o| o.as_bool()), Some(true));
    // mine-release-allow-plan-reference: MCP lifecycle Plan fixture
    let s = call_tool(&client, "mine_plan_show", json!({ "id": "plan02" })).await;
    assert_eq!(
        s.get("data")
            .and_then(|d| d.get("plan"))
            .and_then(|p| p.get("status"))
            .and_then(|st| st.as_str()),
        Some("ACCEPTED")
    );

    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
}

#[tokio::test]
async fn plan_reject_in_progress_with_review_evidence() {
    let before = live_graph_bytes();
    let (tmp, repo) = start_fixture();
    // mine-release-allow-plan-reference: MCP rejection review-path fixture
    let review = "docs/plan/reports/plan03-fixture-review.md";
    let client = connect(repo.clone()).await;

    // Start the Plan, then reject directly from IN_PROGRESS with independent
    // review evidence. No implementation evidence may be synthesized.
    // mine-release-allow-plan-reference: MCP rejection Plan fixture
    let started = call_tool(&client, "mine_plan_start", json!({ "id": "plan03" })).await;
    assert_eq!(
        started.get("ok").and_then(|value| value.as_bool()),
        Some(true),
        "start result: {started}"
    );
    std::fs::create_dir_all(repo.join("docs/plan/reports")).unwrap();
    std::fs::write(repo.join(review), "independent review\n").unwrap();

    let rejected = call_tool(
        &client,
        "mine_plan_reject",
        json!({
            // mine-release-allow-plan-reference: MCP rejection Plan fixture
            "id": "plan03",
            "reason": "scope drift",
            "review_report": review,
            // mine-release-allow-plan-reference: MCP compensation Plan fixture
            "compensating_plan": "plan03-C01"
        }),
    )
    .await;
    assert_eq!(
        rejected.get("ok").and_then(|o| o.as_bool()),
        Some(true),
        "reject result: {rejected}"
    );
    assert_eq!(
        rejected.get("command").and_then(|c| c.as_str()),
        Some("plan.reject")
    );

    // mine-release-allow-plan-reference: MCP rejection Plan fixture
    let s = call_tool(&client, "mine_plan_show", json!({ "id": "plan03" })).await;
    let plan = s
        .get("data")
        .and_then(|d| d.get("plan"))
        .expect("data.plan");
    assert_eq!(
        plan.get("status").and_then(|st| st.as_str()),
        Some("REJECTED")
    );
    assert_eq!(
        plan.get("compensating_plan").and_then(|c| c.as_str()),
        // mine-release-allow-plan-reference: MCP compensation assertion fixture
        Some("plan03-C01")
    );
    assert_eq!(
        plan.get("review_report").and_then(|value| value.as_str()),
        Some(review)
    );
    assert!(
        plan.get("implementation_report")
            .and_then(|value| value.as_str())
            .is_some_and(str::is_empty)
    );

    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
}

#[tokio::test]
async fn unknown_tool_returns_protocol_error() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    // An unknown tool must surface as a protocol-level error (the SDK router
    // returns `Err(ErrorData)` for tools it cannot route), not a tool-level
    // error result.
    let res = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("mine_does_not_exist".to_string())
                .with_arguments(args_obj(json!({}))),
        )
        .await;
    assert!(res.is_err(), "unknown tool must be a protocol error");
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
}

#[tokio::test]
async fn missing_required_argument_is_tool_level_error() {
    // rmcp converts argument-deserialization failures (e.g. a missing required
    // field) into a tool-level error result (`is_error: true` with a textual
    // message), NOT a protocol-level `Err`. The caller sees the decode failure.
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let result = call_tool_raw(&client, "mine_plan_show", json!({})).await;
    assert_eq!(
        result.is_error,
        Some(true),
        "missing arg must be a tool error"
    );
    let text: String = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("");
    assert!(
        text.contains("missing field") || text.contains("deserialize"),
        "error text should mention the decode failure: {text}"
    );
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
}

#[tokio::test]
async fn stdout_is_protocol_only() {
    // Spawn the server raw (no rmcp client), send a JSON-RPC `initialize`, then
    // close stdin to trigger EOF/shutdown. Assert that every non-empty stdout
    // line is a single JSON-RPC object -- no CLI envelope, no human text, no
    // panic message contaminates protocol stdout.
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();

    use std::io::Write;
    let mut child = std::process::Command::new(mine_bin())
        .arg("mcp")
        .arg("serve")
        .arg("--repo")
        .arg(&repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    {
        let mut stdin = child.stdin.take().unwrap();
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"raw","version":"0.0.0"}}}"#;
        stdin.write_all(format!("{init}\n").as_bytes()).unwrap();
        stdin.flush().unwrap();
        // drop stdin -> child sees EOF and shuts down
    }
    let output = child.wait_with_output().expect("wait");
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "live graph must not change");
    assert!(
        output.status.success(),
        "server exited non-zero: {:?}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let out = String::from_utf8_lossy(&output.stdout);
    assert!(
        !out.trim().is_empty(),
        "server should have emitted an initialize response on stdout"
    );
    for line in out.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("stdout line is not valid JSON-RPC: {line:?} (err {e})"));
        assert!(
            parsed.get("jsonrpc").is_some(),
            "stdout line missing jsonrpc field: {line}"
        );
    }
}
