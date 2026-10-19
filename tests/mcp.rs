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
//! `tests::common::seeded_repo`. On-disk development-fixture byte guards detect
//! accidental fixture edits; they do not claim a live Root graph audit.

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
    // mine-release-allow-plan-reference: isolated canonical MCP fixture
    let p01 = node("plan01", PlanStatus::Accepted, &[], &[]);
    // mine-release-allow-plan-reference: isolated canonical MCP fixture
    let p02 = node("plan02", PlanStatus::Ready, &[], &["plan01"]);
    // mine-release-allow-plan-reference: isolated canonical MCP fixture
    let p03 = node("plan03", PlanStatus::Ready, &[], &[]);
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
    let (tmp, repo) = seeded_repo(vec![
        // mine-release-allow-plan-reference: explicit legacy READY fixture, native preparation follows
        node("plan02", PlanStatus::Ready, &[], &[]),
        // mine-release-allow-plan-reference: disjoint second READY lineage fixture
        node("plan03", PlanStatus::Ready, &[], &[]),
    ]);
    // mine-release-allow-plan-reference: MCP multi-Plan prepare fixture
    for (id, observed) in [("plan02", "72"), ("plan03", "73")] {
        let operation = uuid::Uuid::new_v4().to_string();
        let outcome = dispatch(
            &repo,
            &[
                "plan",
                "prepare",
                "--id",
                id,
                "--actor",
                "fixture-preparer",
                "--expected-revision",
                observed,
                "--operation-id",
                &operation,
            ],
        );
        assert_eq!(outcome.exit_code, 0, "{}", common::envelope_json(&outcome));
    }
    assert_eq!(common::load_graph(&repo).revision, 74);
    (tmp, repo)
}

fn commit_fixture_files(repo: &Path, paths: &[&str]) {
    let mut args = vec!["add", "--"];
    args.extend_from_slice(paths);
    git(repo, &args);
    git(
        repo,
        &[
            "commit",
            "--quiet",
            "-m",
            "explicit public transport fixture inputs",
        ],
    );
}

fn git_text(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repo)
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

/// Gracefully closes the client so the child server exits on stdin EOF instead
/// of being killed on drop (avoids leaking processes on test failure).
async fn shutdown(client: RunningService<RoleClient, ()>) {
    let mut c = client;
    let reason = tokio::time::timeout(std::time::Duration::from_secs(5), c.close())
        .await
        .expect("MCP transport shutdown timed out")
        .expect("MCP transport task failed during shutdown");
    assert!(
        matches!(
            reason,
            rmcp::service::QuitReason::Cancelled | rmcp::service::QuitReason::Closed
        ),
        "MCP shutdown failed: {reason:?}"
    );
    assert!(c.is_closed());
}

#[tokio::test]
async fn lists_all_nineteen_tools_with_schemas() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let initialization = client
        .peer_info()
        .expect("actual server initialization info")
        .clone();
    let tools = client.peer().list_all_tools().await.expect("list tools");
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");

    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    let expected = [
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
    ];
    for e in expected {
        assert!(names.contains(&e), "missing tool {e}; got {names:?}");
    }
    assert_eq!(names.len(), 19, "exactly 19 tools; got {names:?}");
    for t in &tools {
        assert!(
            !t.input_schema.is_empty(),
            "tool {} has an empty input schema",
            t.name
        );
    }
    let repair = tools
        .iter()
        .find(|tool| tool.name == "mine_graph_repair_compensation_predecessors")
        .expect("repair tool is registered");
    let schema = serde_json::to_value(&repair.input_schema).unwrap();
    assert_eq!(
        schema["required"],
        serde_json::json!(["actor", "id", "expected_revision", "reason"])
    );
    let properties = schema["properties"].as_object().unwrap();
    assert!(properties.contains_key("id"));
    assert!(properties.contains_key("actor"));
    assert!(properties.contains_key("expected_revision"));
    assert!(properties.contains_key("reason"));
    assert!(!properties.contains_key("hard_predecessors"));
    if let Some(directory) = std::env::var_os("MINE_PUBLIC_CONTRACT_OUTPUT") {
        let directory = PathBuf::from(directory);
        let target =
            PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").expect("isolated target namespace"));
        assert!(directory.is_absolute() && directory.starts_with(target.parent().unwrap()));
        std::fs::create_dir_all(&directory).unwrap();
        let binary = Path::new(env!("CARGO_BIN_EXE_mine"));
        let evidence = json!({
            "scope":"actual_stdio_initialization_and_tools_list_only",
            "performance_contract":"OPEN_NOT_VALIDATED_BY_THIS_ARTIFACT",
            "binary_sha256":mine::infrastructure::graph_checkpoint::digest(&std::fs::read(binary).unwrap()),
            "initialization":initialization,
            "tools":tools,
        });
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("actual-mcp-inventory.json"))
            .unwrap()
            .write_all(&serde_json::to_vec_pretty(&evidence).unwrap())
            .unwrap();
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
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
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
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
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
    // mine-release-allow-plan-reference: isolated canonical MCP fixture
    assert!(ready_ids.contains(&"plan02"), "02 ready, got {ready_ids:?}");
    // mine-release-allow-plan-reference: isolated canonical MCP fixture
    assert!(ready_ids.contains(&"plan03"), "03 ready, got {ready_ids:?}");

    // `wave` is a write-scope-disjoint subset of ready.
    let w = call_tool(&client, "mine_graph_wave", json!({})).await;
    assert_eq!(w.get("ok").and_then(|o| o.as_bool()), Some(true));
    assert!(w.get("wave").and_then(|x| x.as_array()).is_some());

    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
}

#[tokio::test]
async fn plan_show_returns_plan_fields() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    // mine-release-allow-plan-reference: isolated canonical MCP fixture
    let v = call_tool(&client, "mine_plan_show", json!({ "id": "plan02" })).await;
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
    assert_eq!(v.get("ok").and_then(|o| o.as_bool()), Some(true));
    let plan = v
        .get("data")
        .and_then(|d| d.get("plan"))
        .expect("data.plan");
    // mine-release-allow-plan-reference: isolated canonical MCP fixture
    assert_eq!(plan.get("id").and_then(|i| i.as_str()), Some("plan02"));
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
async fn supported_graph_provenance_repair_migrates_once_then_mcp_is_idempotent() {
    let before = live_graph_bytes();
    let (tmp, repo) = fixture();
    let client = connect(repo).await;
    let migrated = call_tool(
        &client,
        "mine_graph_repair_provenance",
        json!({ "actor":"fixture-repairer", "expected_revision": 72 }),
    )
    .await;
    assert_eq!(migrated["ok"], true, "{migrated}");
    assert_eq!(migrated["data"]["changed"], true, "{migrated}");
    assert_eq!(migrated["revision_before"], 72);
    assert_eq!(migrated["revision_after"], 73);
    let value = call_tool(
        &client,
        "mine_graph_repair_provenance",
        json!({"actor":"fixture-repairer","expected_revision":73}),
    )
    .await;
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
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
        Some(73)
    );
    assert_eq!(
        value.get("revision_after").and_then(|rev| rev.as_u64()),
        Some(73)
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
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
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
    // mine-release-allow-plan-reference: immutable new Plan body for actual MCP registration
    let plan_path = ".mine/plan/plan01-04-test-node.md";
    std::fs::write(repo.join(plan_path), "# Public MCP registration fixture\n").unwrap();
    commit_fixture_files(&repo, &[plan_path]);
    let client = connect(repo).await;

    let added = call_tool(
        &client,
        "mine_plan_add",
        json!({
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "id": "plan01-04",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "path": ".mine/plan/plan01-04-test-node.md",
            "title": "Test node",
            "actor":"fixture-planner", "expected_revision":72,
            "design_references": [".mine/design/index.md"],
            "exclusive_write_paths": ["tests/04/"],
            // mine-release-allow-plan-reference: isolated canonical MCP fixture
            "hard_predecessors": ["plan01"]
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
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
}

#[tokio::test]
async fn registration_refuses_omitted_and_empty_conservative_mapping_without_mutation() {
    let (_tmp, repo) = seeded_repo(vec![]);
    // mine-release-allow-plan-reference: otherwise valid registration body
    let body = ".mine/plan/plan06-fixture.md";
    std::fs::write(repo.join(body), "# Explicit registration fixture\n").unwrap();
    commit_fixture_files(&repo, &[body]);
    let state = || {
        let directory = repo.join(".mine/runtime/operations");
        let mut journal = if directory.exists() {
            std::fs::read_dir(directory)
                .unwrap()
                .map(|entry| {
                    let entry = entry.unwrap();
                    assert!(entry.file_type().unwrap().is_file());
                    (entry.file_name(), std::fs::read(entry.path()).unwrap())
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        journal.sort();
        (
            std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap(),
            std::fs::read(repo.join(".mine/plan/execution-graph.md")).unwrap(),
            git_text(&repo, &["rev-parse", "HEAD"]),
            git_text(
                &repo,
                &["for-each-ref", "--format=%(refname) %(objectname)"],
            ),
            std::fs::read(repo.join(".git/index")).unwrap(),
            journal,
        )
    };
    let before = state();
    let client = connect(repo.clone()).await;
    for empty_array in [false, true] {
        let mut arguments = json!({
            // mine-release-allow-plan-reference: valid identity cannot replace missing authority
            "id":"plan06", "path":body,"title":"Missing mapping","actor":"fixture-planner","expected_revision":72,"design_references":[".mine/design/index.md"]
        });
        if empty_array {
            arguments["exclusive_write_paths"] = json!([]);
        }
        let result = call_tool_raw(&client, "mine_plan_add", arguments).await;
        assert_eq!(result.is_error, Some(true), "{result:?}");
        assert!(
            format!("{result:?}").contains("conservative mapping is required"),
            "{result:?}"
        );
        assert_eq!(
            state(),
            before,
            "empty_array={empty_array}: failed registration mutated state"
        );
    }
    shutdown(client).await;
}

#[tokio::test]
async fn plan_lifecycle_start_implemented_refuses_report_only_acceptance() {
    let before = live_graph_bytes();
    let (tmp, repo) = start_fixture();
    let client = connect(repo.clone()).await;
    // mine-release-allow-plan-reference: prepared MCP lifecycle subject
    let started = call_tool(&client, "mine_plan_start", json!({"id":"plan02","expected_revision":74,"owner":"mcp-executor","run_id":"mcp-fixture-run"})).await;
    assert_eq!(started["ok"], true, "{started}");
    // mine-release-allow-plan-reference: exact registered worktree/write namespace
    let target = repo.join(".mine/worktrees/plan02");
    // mine-release-allow-plan-reference: explicit fixture product output
    let product = "tests/plan02/result.txt";
    // mine-release-allow-plan-reference: isolated canonical MCP fixture
    std::fs::create_dir_all(target.join("tests/plan02")).unwrap();
    std::fs::write(target.join(product), "actual fixture implementation\n").unwrap();
    commit_fixture_files(&target, &[product]);
    let tip = git_text(&target, &["rev-parse", "HEAD"]);
    // mine-release-allow-plan-reference: canonical report path for implemented fixture
    let report = ".mine/plan/reports/plan02-fixture-implementation.md";
    std::fs::create_dir_all(repo.join(".mine/plan/reports")).unwrap();
    std::fs::write(
        repo.join(report),
        "Actual implementation commit and focused fixture evidence\n",
    )
    .unwrap();
    commit_fixture_files(&repo, &[report]);
    let implemented = call_tool(&client, "mine_plan_mark_implemented", json!({
        // mine-release-allow-plan-reference: native implementation transport fixture
        "id":"plan02","actor":"mcp-executor","expected_revision":75,"report":report,"commits":[tip]
    })).await;
    assert_eq!(implemented["ok"], true, "{implemented}");
    let graph = std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap();
    let head = git_text(&repo, &["rev-parse", "HEAD"]);
    // Report-string acceptance was deliberately removed by the accepted native
    // closure contract. A real positive MCP finalization/replay is independently
    // covered in lifecycle_operations; do not fabricate evidence to revive it.
    let legacy = call_tool_raw(
        &client,
        "mine_plan_accept",
        json!({
            // mine-release-allow-plan-reference: obsolete request must refuse before effects
            "id":"plan02","review":".mine/plan/reports/plan02-fixture-review.md"
        }),
    )
    .await;
    assert_eq!(legacy.is_error, Some(true), "{legacy:?}");
    let forged = call_tool_raw(&client, "mine_plan_accept", json!({
        // mine-release-allow-plan-reference: well-typed request still needs a real closure
        "id":"plan02","expected_revision":76,"operation_id":uuid::Uuid::new_v4().to_string(),"input":".mine/runtime/no-closure.json"
    })).await;
    assert_eq!(forged.is_error, Some(true), "{forged:?}");
    // mine-release-allow-plan-reference: refused acceptance preserves IMPLEMENTED state
    let shown = call_tool(&client, "mine_plan_show", json!({"id":"plan02"})).await;
    assert_eq!(shown["data"]["plan"]["status"], "IMPLEMENTED", "{shown}");
    assert_eq!(
        std::fs::read(repo.join(".mine/plan/execution-graph.toml")).unwrap(),
        graph
    );
    assert_eq!(git_text(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git_text(&target, &["rev-parse", "HEAD"]), tip);
    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
}

#[tokio::test]
async fn plan_add_records_explicit_planning_scope() {
    let before = live_graph_bytes();
    let (tmp, repo) = start_fixture();
    let client = connect(repo.clone()).await;
    // mine-release-allow-plan-reference: MCP scope-binding Plan fixture
    std::fs::write(
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        repo.join(".mine/plan/plan04-fixture.md"),
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        "# plan04
",
    )
    .unwrap();
    // mine-release-allow-plan-reference: commit exact new fixture body before registration
    commit_fixture_files(&repo, &[".mine/plan/plan04-fixture.md"]);
    let added = call_tool(
        &client,
        "mine_plan_add",
        json!({
            // mine-release-allow-plan-reference: MCP scope-binding Plan fixture
            "id": "plan04",
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            "path": ".mine/plan/plan04-fixture.md",
            "title": "scoped fixture",
            "actor":"fixture-planner", "expected_revision":74,
            "design_references": [".mine/design/index.md"],
            "exclusive_write_paths":["tests/scoped-fixture/"],
            "planning_scope": ".mine/design/execution-graph"
        }),
    )
    .await;
    assert_eq!(
        added.get("ok").and_then(|o| o.as_bool()),
        Some(true),
        "add result: {added}"
    );
    // mine-release-allow-plan-reference: MCP scope-binding Plan fixture
    let shown = call_tool(&client, "mine_plan_show", json!({ "id": "plan04" })).await;
    assert_eq!(
        shown["ok"], true,
        "registration succeeded but show failed: {shown}"
    );
    let plan = shown
        .get("data")
        .and_then(|d| d.get("plan"))
        .expect("data.plan");
    assert_eq!(
        plan.get("planning_scope").and_then(|s| s.as_str()),
        Some(".mine/design/execution-graph"),
        "explicit planning scope recorded: {plan}"
    );

    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
}

#[tokio::test]
async fn plan_reject_in_progress_with_review_evidence() {
    use mine::domain::{
        graph::{PlanIdentity, PlanIdentityFields, RepoPath},
        ports::Clock,
    };
    use mine::infrastructure::{graph_checkpoint::digest, system::SystemClock};
    let before = live_graph_bytes();
    let (tmp, repo) = start_fixture();
    // mine-release-allow-plan-reference: MCP rejection review-path fixture
    let review = ".mine/plan/reports/plan03-fixture-review.md";
    let client = connect(repo.clone()).await;

    // Start the Plan, then reject directly from IN_PROGRESS with independent
    // review evidence. No implementation evidence may be synthesized.
    // mine-release-allow-plan-reference: MCP rejection Plan fixture
    let started = call_tool(&client, "mine_plan_start", json!({ "id": "plan03", "expected_revision":74, "owner":"mcp-executor", "run_id":"mcp-rejection-run" })).await;
    assert_eq!(
        started.get("ok").and_then(|value| value.as_bool()),
        Some(true),
        "start result: {started}"
    );
    let workspace = common::load_graph(&repo);
    // mine-release-allow-plan-reference: exact independently reviewed subject
    let subject = workspace.get("plan03").unwrap();
    let config = mine::cli::context::load_config(&repo).unwrap();
    let identity = PlanIdentity::new(PlanIdentityFields {
        repository_id: config.repository_id,
        workspace_id: workspace.workspace_id.clone(),
        plan_id: subject.id.clone(),
        plan_path: RepoPath::new(subject.path.clone()).unwrap(),
        body_sha256: Some(digest(&std::fs::read(repo.join(&subject.path)).unwrap())),
        lineage_id: subject.lineage_id.clone(),
        branch: subject.branch.clone(),
    })
    .unwrap();
    let assertion = json!({"schema_version":1,"plan":identity,"writer_owner":"mcp-executor","writer_run_id":"mcp-rejection-run","reviewer":"mcp-reviewer","independent_review":true,"writer_stopped":true,"stopped_at":SystemClock.now_utc_rfc3339(),"stop_basis":"Private fixture controller stopped the writer and handed off its unchanged checkout"});
    let report = format!(
        "# Independent fixture review\n\nVerdict: REJECTED\n\n```mine-review-closure\n{assertion}\n```\n"
    );
    // mine-release-allow-plan-reference: next sibling body precedes pinned rejection intent
    let compensation_path = ".mine/plan/plan03-C01-fixture.md";
    let compensation_body = b"# Private compensating Plan fixture\n";
    std::fs::create_dir_all(repo.join(".mine/plan/reports")).unwrap();
    std::fs::write(repo.join(review), &report).unwrap();
    std::fs::write(repo.join(compensation_path), compensation_body).unwrap();
    commit_fixture_files(&repo, &[review, compensation_path]);
    let mut compensation = identity.fields().clone();
    // mine-release-allow-plan-reference: typed sibling compensation identity
    compensation.plan_id = "plan03-C01".into();
    compensation.plan_path = RepoPath::new(compensation_path).unwrap();
    // mine-release-allow-plan-reference: exact registered sibling branch
    compensation.branch = "plan/plan03-C01-fixture".into();
    compensation.body_sha256 = Some(digest(compensation_body));
    let compensation = PlanIdentity::new(compensation).unwrap();
    let input = json!({"schema_version":1,"actor":"mcp-reviewer","intent":{"kind":"rejected_handoff","plan_id":subject.id,"review":{"reviewed_tip":git_text(&repo.join(&subject.worktree_path), &["rev-parse","HEAD"]),"report":{"path":review,"bytes":report.len(),"sha256":digest(report.as_bytes())}},"compensation":compensation}});
    let input_path = ".mine/runtime/mcp-rejection-input.json";
    std::fs::create_dir_all(repo.join(".mine/runtime")).unwrap();
    std::fs::write(repo.join(input_path), serde_json::to_vec(&input).unwrap()).unwrap();
    let begun = dispatch(
        &repo,
        &[
            "operation",
            "begin",
            "--expected-revision",
            "75",
            "--input",
            input_path,
        ],
    );
    assert_eq!(begun.exit_code, 0, "{}", common::envelope_json(&begun));

    let rejected = call_tool(
        &client,
        "mine_plan_reject",
        json!({
            // mine-release-allow-plan-reference: MCP rejection Plan fixture
            "id": "plan03",
            "actor":"mcp-reviewer", "expected_revision":75,
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
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
}

#[tokio::test]
async fn plan_withdraw_retires_prepared_ready_plan() {
    let before = live_graph_bytes();
    let (tmp, repo) = start_fixture();
    // mine-release-allow-plan-reference: plan-id lifecycle fixture
    // The fixture prepared plan02's lane at the integration tip, so the
    // withdrawal must clean up the never-started branch/worktree losslessly.
    // mine-release-allow-plan-reference: MCP withdrawal Plan fixture
    let worktree = repo.join(".mine/worktrees/plan02");
    assert!(worktree.exists(), "fixture prepared the linked worktree");
    let client = connect(repo.clone()).await;

    let withdrawn = call_tool(
        &client,
        "mine_plan_withdraw",
        json!({
            // mine-release-allow-plan-reference: MCP withdrawal Plan fixture
            "id": "plan02",
            "actor":"mcp-coordinator", "expected_revision":74, "operation_id":uuid::Uuid::new_v4().to_string(),
            "reason": "stale after architecture replacement"
        }),
    )
    .await;
    assert_eq!(
        withdrawn.get("ok").and_then(|o| o.as_bool()),
        Some(true),
        "withdraw result: {withdrawn}"
    );
    assert_eq!(
        withdrawn.get("command").and_then(|c| c.as_str()),
        Some("plan.withdraw")
    );
    assert_eq!(
        withdrawn
            .get("data")
            .and_then(|d| d.get("status_after"))
            .and_then(|s| s.as_str()),
        Some("WITHDRAWN")
    );
    assert_eq!(
        withdrawn
            .get("data")
            .and_then(|d| d.get("cleaned_git_artifacts"))
            .and_then(|c| c.as_bool()),
        Some(true),
        "prepared lane cleaned: {withdrawn}"
    );
    // The never-started branch/worktree are gone.
    assert!(!worktree.exists(), "linked worktree removed");
    let branch_list = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        .args(["branch", "--list", "plan/plan02*"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&branch_list.stdout)
            .trim()
            .is_empty(),
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        "plan02 branch deleted"
    );

    // mine-release-allow-plan-reference: MCP withdrawal Plan fixture
    let shown = call_tool(&client, "mine_plan_show", json!({ "id": "plan02" })).await;
    let plan = shown
        .get("data")
        .and_then(|d| d.get("plan"))
        .expect("data.plan");
    assert_eq!(
        plan.get("status").and_then(|s| s.as_str()),
        Some("WITHDRAWN")
    );
    assert_eq!(
        plan.get("withdraw_reason").and_then(|r| r.as_str()),
        Some("stale after architecture replacement")
    );

    // Withdrawal is not idempotent-success: re-running is a stable refusal.
    let rerun = call_tool(
        &client,
        "mine_plan_withdraw",
        json!({
            // mine-release-allow-plan-reference: plan-id lifecycle fixture
            "id": "plan02",
            "actor":"mcp-coordinator", "expected_revision":75, "operation_id":uuid::Uuid::new_v4().to_string(),
            "reason": "again"
        }),
    )
    .await;
    assert!(
        rerun.get("ok").and_then(|o| o.as_bool()).is_none_or(|v| !v),
        "re-run refused: {rerun}"
    );

    shutdown(client).await;
    drop(tmp);
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
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
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
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
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
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
    assert_eq!(live_graph_bytes(), before, "development fixture unchanged");
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
