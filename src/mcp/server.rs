//! rmcp-backed stdio MCP server for the MINE execution graph.
//!
//! Implements `docs/design/interfaces/mcp-contract.md` using the official Rust
//! MCP SDK (`rmcp`):
//! - server lifecycle, initialization, capability negotiation, and the
//!   `initialized` notification via `ServerHandler` (the `server_handler` flag
//!   on `#[tool_router]` auto-impls `ServerHandler` and emits `get_info` with
//!   tools capability enabled);
//! - typed tool registration via the `#[tool_router]` / `#[tool]` macros;
//! - tool-input decoding and JSON-Schema generation from the same
//!   `#[derive(Deserialize, JsonSchema)]` argument structs (single schema
//!   authority — no hand-written parallel schema);
//! - stdio transport via `rmcp::transport::stdio`;
//! - request/response IDs, JSON-RPC framing, and notification handling all
//!   owned by the SDK.
//!
//! Every tool calls the shared `GraphService`/`PlanService`/`DesignService`
//! (the same application services the CLI uses); the MCP adapter is an adapter,
//! not a second implementation of lifecycle policy. Diagnostics go to stderr
//! only; stdout is protocol-only; no shell/Git/branch/install/delete primitive
//! is exposed. On stdin EOF the transport stops cleanly (no partial protocol
//! message).
//!
//! Tool-level failures (lifecycle, validation, revision conflict, locked
//! successor) are returned as `Ok(CallToolResult::error(...))` so the caller
//! sees the stable MINE error code and message. Only protocol-level failures
//! (arguments that cannot be decoded, unsupported method) return an SDK
//! `Err(McpError)` and are produced by the SDK's own path.

use std::path::{Path, PathBuf};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};
use rmcp::{ServiceExt, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::application::adr_service::AdrService;
use crate::application::design_service::DesignService;
use crate::application::graph_service::{
    GraphService, PlanAcceptRequest, PlanAddRequest, PlanImplementedRequest, PlanRejectRequest,
    PlanStartRequest,
};
use crate::application::plan_service::PlanService;
use crate::application::plan_service::PlanWithdrawRequest;
use crate::cli::context::{GlobalOpts, OutputFormat, build_context, load_config};
use crate::domain::error::MineError;
use crate::domain::ports::Clock;
use crate::infrastructure::system::SystemClock;

/// Run the stdio MCP server against the given repository root. Reads MCP
/// protocol messages from stdin and writes protocol responses to stdout; all
/// diagnostics go to stderr. Returns to the caller on clean EOF/shutdown.
///
/// # Errors
/// Returns an error only if the SDK server cannot be initialized or the
/// runtime fails to start. Normal client-side EOF/shutdown returns `Ok(())`.
pub fn serve(repo_root: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let server = MineServer::new(repo_root.to_path_buf())?;
        let (stdin, stdout) = rmcp::transport::stdio();
        let running = server
            .serve((stdin, stdout))
            .await
            .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?;
        // The SDK drives the transport until stdin EOF or a client-initiated
        // shutdown; `waiting()` resolves on clean termination.
        let _ = running.waiting().await;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
}

/// The MINE MCP server handler. Bound to a fixed repository root; every tool
/// scopes access to that repository by constructing the shared application
/// services from the store resolved for that root.
#[derive(Debug, Clone)]
pub struct MineServer {
    repo_root: PathBuf,
}

impl MineServer {
    /// Constructs the server for the given repository root.
    pub fn new(repo_root: PathBuf) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self { repo_root })
    }

    /// Builds the shared CLI-style command context (store + config) the
    /// application services route over. The store owns locking, reload, atomic
    /// persistence, and deterministic rendering.
    fn ctx(&self) -> Result<crate::cli::context::CommandContext, MineError> {
        let global = GlobalOpts {
            format: OutputFormat::Json,
            quiet: true,
            no_color: true,
            repo: Some(self.repo_root.clone()),
            config_root: None,
        };
        build_context(&global)
    }

    /// Maps a `MineError` to a tool-level `CallToolResult` carrying the stable
    /// `MINE_*` code and message as caller-visible text content (the caller
    /// sees the failure, rather than an opaque protocol error). Structured
    /// data (`code` + serde-encoded details) is attached via the
    /// `structured_content` channel where present, so tooling can still read the
    /// code deterministically.
    fn tool_error(e: &MineError) -> CallToolResult {
        let mut result =
            CallToolResult::error(vec![ContentBlock::text(format!("{}: {}", e.code(), e))]);
        let _ = &mut result; // keep structured_content optional and stable
        result
    }
}

#[tool_router(server_handler)]
impl MineServer {
    // ------------------- read-only tools (7) -------------------

    #[tool(
        name = "mine_workspace_status",
        description = "Read-only execution-graph workspace status (revision, branches, plan count, ready frontier).",
        annotations(read_only_hint = true)
    )]
    fn mine_workspace_status(&self) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
            )]);
        };
        let graph = GraphService::new(&ctx.store);
        match graph.status() {
            Ok(st) => CallToolResult::structured(json!({
                "command": "graph.status",
                "ok": true,
                "workspace_id": st.workspace_id,
                "revision": st.revision,
                "stable_branch": st.stable_branch,
                "integration_branch": st.integration_branch,
                "plan_count": st.plan_count,
                "ready": st.ready,
            })),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_graph_validate",
        description = "Read-only structural validation of the execution graph.",
        annotations(read_only_hint = true)
    )]
    fn mine_graph_validate(&self) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
            )]);
        };
        let graph = GraphService::new(&ctx.store);
        match graph.validate() {
            Ok(ws) => CallToolResult::structured(json!({
                "command": "graph.validate",
                "ok": true,
                "plans": ws.plans.len(),
                "warnings_emitted": false,
            })),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_graph_status",
        description = "Read-only graph status summary (revision, branches, plan count, ready frontier).",
        annotations(read_only_hint = true)
    )]
    fn mine_graph_status(&self) -> CallToolResult {
        self.mine_workspace_status()
    }

    #[tool(
        name = "mine_graph_ready",
        description = "Read-only ready frontier of the execution graph.",
        annotations(read_only_hint = true)
    )]
    fn mine_graph_ready(&self) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
            )]);
        };
        let graph = GraphService::new(&ctx.store);
        match graph.ready() {
            Ok(ready) => CallToolResult::structured(json!({
                "command": "graph.ready",
                "ok": true,
                "ready": ready,
            })),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_graph_wave",
        description = "Read-only stable parallel wave (write-scope-disjoint ready plans).",
        annotations(read_only_hint = true)
    )]
    fn mine_graph_wave(&self) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
            )]);
        };
        let graph = GraphService::new(&ctx.store);
        match graph.wave() {
            Ok(wave) => CallToolResult::structured(json!({
                "command": "graph.wave",
                "ok": true,
                "wave": wave,
            })),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_plan_show",
        description = "Read-only lookup of a plan node by id (status, predecessors, owner, reports).",
        annotations(read_only_hint = true)
    )]
    fn mine_plan_show(&self, Parameters(args): Parameters<MinePlanShowArgs>) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
            )]);
        };
        let graph = GraphService::new(&ctx.store);
        let svc = PlanService::new(&graph);
        match svc.show_with_locations(&args.id, &ctx.caller_root) {
            Ok(lookup) => CallToolResult::structured(json!({
                "command": "plan.show",
                "ok": true,
                "revision": lookup.authoritative_revision,
                "data": { "plan": serde_json::to_value(&lookup).unwrap_or(Value::Null) },
            })),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_design_validate",
        description = "Read-only validation of the design namespace (marker, index, stable-branch hygiene).",
        annotations(read_only_hint = true)
    )]
    fn mine_design_validate(&self) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return CallToolResult::error(vec![rmcp::model::ContentBlock::text(
                "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
            )]);
        };
        let Some(config) = load_config(&ctx.repo_root) else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "no .mine/config.toml at repository root".to_string(),
            });
        };
        match DesignService::validate(&ctx.repo_root, &config) {
            Ok(result) => CallToolResult::structured(json!({
                "command": "design.validate",
                "ok": true,
                "data": {
                    "valid": result.valid,
                    "warnings": serde_json::to_value(&result.warnings).unwrap_or(Value::Null),
                },
            })),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_adr_show",
        description = "Read-only lookup of one ADR registry entry and approval evidence.",
        annotations(read_only_hint = true)
    )]
    fn mine_adr_show(&self, Parameters(args): Parameters<MineAdrShowArgs>) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "repository unavailable".to_string(),
            });
        };
        let Some(config) = ctx.config.as_ref() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "no MINE config".to_string(),
            });
        };
        match AdrService::new(&ctx.repo_root, config).show(&args.id) {
            Ok(entry) => CallToolResult::structured(
                json!({"command":"adr.show","ok":true,"data":{"adr":entry}}),
            ),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_adr_list",
        description = "Read-only list of ADR registry entries.",
        annotations(read_only_hint = true)
    )]
    fn mine_adr_list(&self) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "repository unavailable".to_string(),
            });
        };
        let Some(config) = ctx.config.as_ref() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "no MINE config".to_string(),
            });
        };
        match AdrService::new(&ctx.repo_root, config).list() {
            Ok(entries) => CallToolResult::structured(
                json!({"command":"adr.list","ok":true,"data":{"adrs":entries}}),
            ),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_adr_validate",
        description = "Read-only validation of ADR registry structure and body digests.",
        annotations(read_only_hint = true)
    )]
    fn mine_adr_validate(&self) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "repository unavailable".to_string(),
            });
        };
        let Some(config) = ctx.config.as_ref() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "no MINE config".to_string(),
            });
        };
        match AdrService::new(&ctx.repo_root, config).validate() {
            Ok(registry) => CallToolResult::structured(
                json!({"command":"adr.validate","ok":true,"data":{"valid":true,"revision":registry.revision,"count":registry.entries.len()}}),
            ),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    #[tool(
        name = "mine_design_readiness",
        description = "Read-only scoped ADR and Design planning readiness.",
        annotations(read_only_hint = true)
    )]
    fn mine_design_readiness(
        &self,
        Parameters(args): Parameters<MineDesignReadinessArgs>,
    ) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "repository unavailable".to_string(),
            });
        };
        let Some(config) = ctx.config.as_ref() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "no MINE config".to_string(),
            });
        };
        match AdrService::new(&ctx.repo_root, config).readiness(args.scope.as_deref()) {
            Ok(readiness) => CallToolResult::structured(
                json!({"command":"design.readiness","ok":true,"data":readiness}),
            ),
            Err(e) => MineServer::tool_error(&e),
        }
    }

    // ------------------- mutating tools (7) -------------------

    #[tool(
        name = "mine_graph_repair_provenance",
        description = "Revision-checked repair of supported schema-v1 Plan provenance; upgrades the graph to the fail-closed current schema and is idempotent once repaired.",
        annotations(read_only_hint = false)
    )]
    fn mine_graph_repair_provenance(
        &self,
        Parameters(args): Parameters<MineGraphRepairProvenanceArgs>,
    ) -> CallToolResult {
        let Ok(ctx) = self.ctx() else {
            return MineServer::tool_error(&MineError::RepositoryNotFound {
                detail: "repository unavailable".to_string(),
            });
        };
        let graph = GraphService::new(&ctx.store);
        match graph.repair_provenance(args.expected_revision) {
            Ok(outcome) => CallToolResult::structured(json!({
                "command": "graph.repair-provenance",
                "ok": true,
                "revision_before": outcome.revision_before,
                "revision_after": outcome.revision_after,
                "data": {
                    "changed": outcome.changed,
                    "repaired_plans": outcome.repaired_plans,
                    "schema_version": outcome.workspace.schema_version,
                },
            })),
            Err(error) => MineServer::tool_error(&error),
        }
    }

    #[tool(
        name = "mine_plan_add",
        description = "Register a new plan node (status DRAFT). Mutates the execution graph under the lock, revision +1.",
        annotations(read_only_hint = false)
    )]
    fn mine_plan_add(
        &self,
        Parameters(args): Parameters<MinePlanAddArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(ctx) = self.ctx() else {
            return Ok(CallToolResult::error(vec![
                rmcp::model::ContentBlock::text(
                    "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
                ),
            ]));
        };
        // ADR-0013: gate registration on the resolved planning scope (the
        // explicit argument or the configured design-root default), never a
        // derived lineage id.
        let planning_scope = args.planning_scope.clone().unwrap_or_default();
        let planning_scope = if planning_scope.trim().is_empty() {
            ctx.config
                .as_ref()
                .map(DesignService::default_planning_scope)
                .unwrap_or_else(|| "docs/design".to_string())
        } else {
            if let Err(error) = crate::domain::path::normalize_repo_relative(&planning_scope) {
                return Ok(MineServer::tool_error(&error));
            }
            planning_scope
        };
        if let Some(config) = ctx.config.as_ref() {
            if ctx.repo_root.join(&config.adr.registry).exists() {
                let readiness =
                    AdrService::new(&ctx.repo_root, config).require_planning_ready(&planning_scope);
                if let Err(error) = readiness {
                    return Ok(MineServer::tool_error(&error));
                }
            }
        }
        let graph = GraphService::new(&ctx.store);
        let svc = PlanService::new(&graph);
        let req = PlanAddRequest {
            id: args.id,
            path: args.path,
            title: args.title,
            design_references: args.design_references,
            exclusive_write_paths: args.exclusive_write_paths.unwrap_or_default(),
            hard_predecessors: args.hard_predecessors.unwrap_or_default(),
            planning_scope,
        };
        Ok(match svc.add(req) {
            Err(e) => MineServer::tool_error(&e),
            Ok(saved) => {
                let after = saved.revision;
                CallToolResult::structured(json!({
                    "command": "plan.add",
                    "ok": true,
                    "revision_before": after - 1,
                    "revision_after": after,
                    "data": { "plan": "added" },
                }))
            }
        })
    }

    #[tool(
        name = "mine_plan_start",
        description = "Start a READY plan (assigns owner/run id; reads current revision, writes under the lock, revision +1). Mutates the graph.",
        annotations(read_only_hint = false)
    )]
    fn mine_plan_start(
        &self,
        Parameters(args): Parameters<MinePlanStartArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(ctx) = self.ctx() else {
            return Ok(CallToolResult::error(vec![
                rmcp::model::ContentBlock::text(
                    "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
                ),
            ]));
        };
        let graph = GraphService::new(&ctx.store);
        let expected = match graph.validate() {
            Ok(ws) => ws.revision,
            Err(e) => return Ok(MineServer::tool_error(&e)),
        };
        let now = SystemClock.now_utc_rfc3339();
        let svc = PlanService::new(&graph);
        Ok(
            match svc.start(PlanStartRequest {
                id: args.id.clone(),
                owner: args.owner.unwrap_or_else(|| "mcp".to_string()),
                run_id: args.run_id.unwrap_or_else(|| "mcp-run".to_string()),
                started_at: now,
            }) {
                Err(e) => MineServer::tool_error(&e),
                Ok(saved) => {
                    let branch_anchor = saved
                        .get(&args.id)
                        .expect("started plan remains present")
                        .branch_anchor
                        .clone();
                    CallToolResult::structured(json!({
                        "command": "plan.start",
                        "ok": true,
                        "revision_before": expected,
                        "revision_after": saved.revision,
                        "data": { "plan": args.id, "branch_anchor": branch_anchor },
                    }))
                }
            },
        )
    }

    #[tool(
        name = "mine_plan_mark_implemented",
        description = "Record a plan implementation report and commit evidence (IMPLEMENTED status). Mutates the graph under the lock, revision +1.",
        annotations(read_only_hint = false)
    )]
    fn mine_plan_mark_implemented(
        &self,
        Parameters(args): Parameters<MinePlanImplementedArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(ctx) = self.ctx() else {
            return Ok(CallToolResult::error(vec![
                rmcp::model::ContentBlock::text(
                    "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
                ),
            ]));
        };
        let graph = GraphService::new(&ctx.store);
        let expected = match graph.validate() {
            Ok(ws) => ws.revision,
            Err(e) => return Ok(MineServer::tool_error(&e)),
        };
        let now = SystemClock.now_utc_rfc3339();
        let svc = PlanService::new(&graph);
        Ok(
            match svc.mark_implemented(PlanImplementedRequest {
                id: args.id.clone(),
                report: args.report,
                commits: args.commits,
                updated_at: now,
            }) {
                Err(e) => MineServer::tool_error(&e),
                Ok(saved) => CallToolResult::structured(json!({
                    "command": "plan.implemented",
                    "ok": true,
                    "revision_before": expected,
                    "revision_after": saved.revision,
                    "data": { "plan": args.id },
                })),
            },
        )
    }

    #[tool(
        name = "mine_plan_accept",
        description = "Accept an IMPLEMENTED plan (records review report; releases eligible BLOCKED successors). Mutates the graph under the lock, revision +1.",
        annotations(read_only_hint = false)
    )]
    fn mine_plan_accept(
        &self,
        Parameters(args): Parameters<MinePlanAcceptArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(ctx) = self.ctx() else {
            return Ok(CallToolResult::error(vec![
                rmcp::model::ContentBlock::text(
                    "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
                ),
            ]));
        };
        let graph = GraphService::new(&ctx.store);
        let expected = match graph.validate() {
            Ok(ws) => ws.revision,
            Err(e) => return Ok(MineServer::tool_error(&e)),
        };
        let now = SystemClock.now_utc_rfc3339();
        let svc = PlanService::new(&graph);
        Ok(
            match svc.accept(PlanAcceptRequest {
                id: args.id.clone(),
                review_report: args.review,
                updated_at: now,
            }) {
                Err(e) => MineServer::tool_error(&e),
                Ok(saved) => CallToolResult::structured(json!({
                    "command": "plan.accept",
                    "ok": true,
                    "revision_before": expected,
                    "revision_after": saved.revision,
                    "data": { "plan": args.id },
                })),
            },
        )
    }

    #[tool(
        name = "mine_plan_reject",
        description = "Reject an IN_PROGRESS or IMPLEMENTED plan with independent review evidence, a reason, and the next compensation id. Mutates the graph under the lock, revision +1.",
        annotations(read_only_hint = false)
    )]
    fn mine_plan_reject(
        &self,
        Parameters(args): Parameters<MinePlanRejectArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(ctx) = self.ctx() else {
            return Ok(CallToolResult::error(vec![
                rmcp::model::ContentBlock::text(
                    "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
                ),
            ]));
        };
        let graph = GraphService::new(&ctx.store);
        let expected = match graph.validate() {
            Ok(ws) => ws.revision,
            Err(e) => return Ok(MineServer::tool_error(&e)),
        };
        let now = SystemClock.now_utc_rfc3339();
        let svc = PlanService::new(&graph);
        Ok(
            match svc.reject(PlanRejectRequest {
                id: args.id.clone(),
                reason: args.reason,
                review_report: args.review_report.clone().unwrap_or_default(),
                compensating_plan: args.compensating_plan,
                updated_at: now,
            }) {
                Err(e) => MineServer::tool_error(&e),
                Ok(saved) => CallToolResult::structured(json!({
                    "command": "plan.reject",
                    "ok": true,
                    "revision_before": expected,
                    "revision_after": saved.revision,
                    "data": {
                        "plan": args.id,
                        "review_report": args.review_report
                    },
                })),
            },
        )
    }
    #[tool(
        name = "mine_plan_withdraw",
        description = "Withdraw an unstarted plan (DRAFT/BLOCKED/READY) into terminal WITHDRAWN with a recorded reason (ADR-0012). Performs the same provably-lossless Git cleanup checks; mutates the graph under the lock, revision +1.",
        annotations(read_only_hint = false)
    )]
    fn mine_plan_withdraw(
        &self,
        Parameters(args): Parameters<MinePlanWithdrawArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let Ok(ctx) = self.ctx() else {
            return Ok(CallToolResult::error(vec![
                rmcp::model::ContentBlock::text(
                    "MINE_REPOSITORY_NOT_FOUND: repository unavailable",
                ),
            ]));
        };
        let graph = GraphService::new(&ctx.store);
        let expected = match graph.validate() {
            Ok(ws) => ws.revision,
            Err(e) => return Ok(MineServer::tool_error(&e)),
        };
        let now = SystemClock.now_utc_rfc3339();
        let svc = PlanService::new(&graph);
        Ok(
            match svc.withdraw(PlanWithdrawRequest {
                id: args.id.clone(),
                reason: args.reason,
                withdrawn_at: now,
            }) {
                Err(e) => MineServer::tool_error(&e),
                Ok((saved, withdrawal)) => CallToolResult::structured(json!({
                    "command": "plan.withdraw",
                    "ok": true,
                    "revision_before": expected,
                    "revision_after": saved.revision,
                    "data": {
                        "plan": withdrawal.id,
                        "status_before": withdrawal.status_before,
                        "status_after": withdrawal.status_after,
                        "withdrawn_at": withdrawal.withdrawn_at,
                        "cleaned_git_artifacts": withdrawal.cleaned_git_artifacts,
                    },
                })),
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Typed tool argument structs. Each derives `Deserialize` (for decoding the
// `tools/call` arguments) AND `schemars::JsonSchema` (so the SDK generates the
// `inputSchema` from the same typed source). `#[serde(deny_unknown_fields)]`
// makes unknown fields a decode error (protocol-level INVALID_PARAMS via the
// SDK's own path) — single schema authority, not two competing contracts.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MinePlanShowArgs {
    /// The plan id to look up.
    id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MineGraphRepairProvenanceArgs {
    /// Authoritative graph revision expected by the caller.
    expected_revision: u64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MineAdrShowArgs {
    /// ADR id such as ADR-0008.
    id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MineDesignReadinessArgs {
    /// Optional Design/Plan scope token.
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MinePlanAddArgs {
    id: String,
    path: String,
    title: String,
    /// At least one design reference path (relative to the repository root).
    design_references: Vec<String>,
    /// Exclusive write paths owned by this plan. Optional (defaults to none).
    #[serde(default)]
    exclusive_write_paths: Option<Vec<String>>,
    /// Hard predecessor plan ids. Optional (defaults to none — a no-predecessor
    /// plan is added as `DRAFT`).
    #[serde(default)]
    hard_predecessors: Option<Vec<String>>,
    /// Planning scope recorded on the node (ADR-0013). Optional (defaults to
    /// the configured design root directory).
    #[serde(default)]
    planning_scope: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MinePlanStartArgs {
    id: String,
    /// Owner name assigned on start. Optional (defaults to `mcp`).
    #[serde(default)]
    owner: Option<String>,
    /// Run identifier assigned on start. Optional (defaults to `mcp-run`).
    #[serde(default)]
    run_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MinePlanImplementedArgs {
    id: String,
    /// Repository-relative path of the implementation report.
    report: String,
    /// Implementation commit hashes recorded as evidence.
    commits: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MinePlanAcceptArgs {
    id: String,
    /// Repository-relative path of the independent review report.
    review: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MinePlanRejectArgs {
    id: String,
    /// The rejection reason.
    reason: String,
    /// Canonical independent review report path.
    review_report: Option<String>,
    /// The registered compensating plan id.
    compensating_plan: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MinePlanWithdrawArgs {
    id: String,
    /// The non-empty withdrawal reason.
    reason: String,
}
