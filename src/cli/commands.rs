//! Command handlers for the `mine` CLI.
//!
//! Each handler returns `Result<(Envelope, Vec<HumanLine>), HandlerError>`. The
//! dispatcher builds the final [`Outcome`](super::Outcome) and renders it.
//!
//! Contract preservation:
//! - human lines for the default mode; JSON envelope (deterministic, sorted
//!   keys) for `--format json`;
//! - stable `MINE_*` error codes and the public exit-code mapping;
//! - revision + optimistic-concurrency via
//!   [`crate::infrastructure::toml_store::TomlStore::save_with_revision`]
//!   (lock → reload → recheck → mutate → atomic write → render);
//! - deterministic Markdown rendering via [`crate::render`];
//! - read-only Git evidence via [`crate::infrastructure::git`];
//! - safe repository-relative path validation via the domain layer;
//! - no shell execution and no Git mutation.

use serde_json::{Value, json};

use crate::agent_setup::install::FailPhase;
use crate::agent_setup::targets::Env;
use crate::application::adr_service::{AdrAddRequest, AdrService};
use crate::application::agent_service;
use crate::application::design_service::DesignService;
use crate::application::graph_service::{
    GraphService, PlanAcceptRequest, PlanAddRequest, PlanImplementedRequest, PlanRejectRequest,
    PlanStartRequest,
};
use crate::application::init_service::InitService;
use crate::application::plan_service::PlanService;
use crate::application::plan_service::{
    PlanReleaseRequest, PlanRewireRequest, PlanWithdrawRequest,
};
use crate::application::workspace_service::WorkspaceService;
use crate::cli::context::{CommandContext, build_context, load_config_or_error};
use crate::cli::{HandlerError, envelope_for};
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{PlanNode, PlanWorkspace};
use crate::domain::ports::Clock;
use crate::domain::validation;
use crate::infrastructure::design_backup::DesignBackup;
use crate::infrastructure::git;
use crate::infrastructure::git::GitEvidence;
use crate::infrastructure::system::{SystemClock, SystemUuidSource};
use crate::output::envelope::Envelope;
use crate::output::human::{self, HumanLine};

/// The unified handler result.
type HandlerResult = Result<(Envelope, Vec<HumanLine>), HandlerError>;

/// Entry point invoked by the dispatcher.
pub fn handle(
    parsed: &crate::cli::ParsedArgs,
    group: &str,
    sub: &str,
    rest: &[String],
) -> HandlerResult {
    match group {
        "init" => init(parsed, rest),
        "status" => status(parsed, rest),
        "doctor" => doctor(parsed, sub, rest),
        // `mine agent ...` is wired below.
        "workspace" => match sub {
            "open" => workspace_open(parsed, rest),
            "status" => workspace_status(parsed, rest),
            "close" => workspace_close(parsed, rest),
            _ => Err(HandlerError::usage(format!(
                "unknown workspace subcommand {sub:?}"
            ))),
        },
        "graph" => match sub {
            "validate" => graph_validate(parsed, rest),
            "render" => graph_render(parsed, rest),
            "repair-provenance" => graph_repair_provenance(parsed, rest),
            "status" => graph_status(parsed, rest),
            "ready" => graph_ready(parsed, rest),
            "wave" => graph_wave(parsed, rest),
            "show" => graph_show(parsed, rest),
            _ => Err(HandlerError::usage(format!(
                "unknown graph subcommand {sub:?}"
            ))),
        },
        "plan" => match sub {
            "add" => plan_add(parsed, rest),
            "show" => plan_show(parsed, rest),
            "prepare" => plan_prepare(parsed, rest),
            "start" => plan_start(parsed, rest),
            "implemented" => plan_implemented(parsed, rest),
            "accept" => plan_accept(parsed, rest),
            "reject" => plan_reject(parsed, rest),
            "release" => plan_release(parsed, rest),
            "rewire-compensation" => plan_rewire_compensation(parsed, rest),
            "withdraw" => plan_withdraw(parsed, rest),
            _ => Err(HandlerError::usage(format!(
                "unknown plan subcommand {sub:?}"
            ))),
        },
        "design" => match sub {
            "backup" => design_backup(parsed, rest),
            "validate" => design_validate(parsed, rest),
            "status" => design_status(parsed, rest),
            "render" => design_render(parsed, rest),
            "readiness" => design_readiness(parsed, rest),
            _ => Err(HandlerError::usage(format!(
                "unknown design subcommand {sub:?}"
            ))),
        },
        "adr" => match sub {
            "add" => adr_add(parsed, rest),
            "show" => adr_show(parsed, rest),
            "list" => adr_list(parsed, rest),
            "propose" => adr_propose(parsed, rest),
            "approve" => adr_approve(parsed, rest),
            "reject" => adr_reject(parsed, rest),
            "supersede" => adr_supersede(parsed, rest),
            "validate" => adr_validate(parsed, rest),
            _ => Err(HandlerError::usage(format!(
                "unknown adr subcommand {sub:?}"
            ))),
        },
        "agent" => match sub {
            "install" => agent_install(parsed, rest),
            "uninstall" => agent_uninstall(parsed, rest),
            "status" => agent_status(parsed, rest),
            "config" => agent_config(parsed, rest),
            _ => Err(HandlerError::usage(
                "agent expects a subcommand: install|uninstall|status|config",
            )),
        },
        "mcp" => match sub {
            "serve" => mcp_serve(parsed, rest),
            _ => Err(HandlerError::usage("unknown mcp subcommand: serve")),
        },
        "scan" => match sub {
            "plan-refs" => scan_plan_refs(parsed, rest),
            _ => Err(HandlerError::usage("scan expects a subcommand: plan-refs")),
        },
        "release" => release_preflight(parsed, rest),
        "setup" => setup(parsed, rest),
        "update" => update(parsed, rest),
        "uninstall" => uninstall(parsed, rest),
        "dist" => match sub {
            "sync" => dist_sync(parsed, rest),
            "verify" => dist_verify(parsed, rest),
            _ => Err(HandlerError::usage(
                "dist expects a subcommand: sync|verify",
            )),
        },
        "repository" => match sub {
            "version" => repository_version(parsed, rest),
            _ => {
                // Allow `mine repository version <verb>` (verb parsed inside).
                if sub.is_empty() {
                    Err(HandlerError::usage(
                        "repository expects a subcommand: version show|suggest|set",
                    ))
                } else {
                    Err(HandlerError::usage(format!(
                        "unknown repository subcommand {sub:?}"
                    )))
                }
            }
        },
        _ => Err(HandlerError::usage(format!(
            "unknown command group {group:?}"
        ))),
    }
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

/// Extracts `--flag value` and `--flag=value` pairs from a token list, leaving
/// positional tokens in `positional`.
fn parse_flags(rest: &[String]) -> (Vec<(String, String)>, Vec<String>) {
    let mut flags = Vec::new();
    let mut positional = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let a = &rest[i];
        if let Some(eq) = a
            .strip_prefix("--")
            .and_then(|s| s.find('=').map(|p| p + 2))
        {
            let key = a[..eq].trim_start_matches('-').to_string();
            let val = a[eq + 1..].to_string();
            flags.push((key, val));
        } else if a.starts_with("--") {
            let key = a.trim_start_matches('-').to_string();
            // Boolean flags (e.g. `--dry-run`) do not consume the next token
            // when it is itself a flag (`--config-root`); otherwise a leading
            // boolean would swallow a following `--key value` pair.
            let next = rest.get(i + 1);
            let val = match next {
                Some(v) if !v.starts_with("--") => {
                    i += 1;
                    v.clone()
                }
                _ => String::new(),
            };
            flags.push((key, val));
        } else {
            positional.push(a.clone());
        }
        i += 1;
    }
    (flags, positional)
}

fn flag<'a>(flags: &'a [(String, String)], name: &str) -> Option<&'a str> {
    flags
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

fn flags_all<'a>(flags: &'a [(String, String)], name: &str) -> Vec<&'a str> {
    flags
        .iter()
        .filter(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
        .collect()
}

fn ctx_err(e: MineError) -> HandlerError {
    HandlerError::from_mine(&e)
}

/// `mine mcp serve [--repo <path>]` — launches the stdio MCP server built on
/// the official Rust MCP SDK (`rmcp`). The server reads MCP protocol messages
/// from stdin and writes protocol responses to stdout; diagnostics go to
/// stderr only. The CLI dispatcher never reaches `render` for this command, so
/// no human/JSON CLI envelope contaminates protocol stdout.
fn mcp_serve(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let (repo_root, caller_root) =
        crate::cli::context::resolve_repo_roots(parsed.global.repo.as_deref()).map_err(ctx_err)?;
    // The rmcp-backed server runs its own stdio transport and only returns on
    // EOF/shutdown; on error, surface a HandlerError (routed to stderr).
    crate::mcp::serve(&caller_root).map_err(|e| HandlerError {
        code: "MINE_INTERNAL",
        message: e.to_string(),
        exit_code: crate::output::exit_code::EXTERNAL,
        details: Value::Null,
    })?;
    // On clean shutdown, return an empty success envelope (emitted to stderr
    // by the dispatcher only when `--format json` was requested; stdout stays
    // protocol-pure because the server never wrote a CLI envelope there).
    let env = envelope_for("mcp.serve", Some(&repo_root)).with_data(json!({
        "transport": "stdio",
    }));
    Ok((env, Vec::new()))
}

// ----------------------------------------------------------------------------
// init
// ----------------------------------------------------------------------------

fn init(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let repo_root =
        crate::cli::context::resolve_repo_root(parsed.global.repo.as_deref()).map_err(ctx_err)?;
    // Detect the stable branch from Git evidence, falling back to the managed
    // default. The init service records the configured default when git is
    // unavailable; we enrich its outcome with the detected branch.
    let detected_branch = git::detect_stable_branch(&repo_root, "master");
    let svc = InitService::new(&SystemUuidSource, &SystemClock);
    let outcome = svc
        .initialize(&repo_root, &detected_branch)
        .map_err(ctx_err)?;
    let mut env = envelope_for("init", Some(&repo_root))
        .with_workspace_id(outcome.repository_id.clone())
        .with_data(json!({
            "repository_id": outcome.repository_id,
            "mine_code_version": outcome.mine_code_version,
            "design_root": match outcome.design_root {
                crate::application::init_service::DesignRootSummary::Absent => "absent-created",
                crate::application::init_service::DesignRootSummary::Managed => "managed-preserved",
            },
            "stable_branch": detected_branch,
            "actions": outcome.actions.iter().map(|a| match a {
                crate::application::init_service::InitAction::Created(p) => json!({"kind":"created","path": p.display().to_string()}),
                crate::application::init_service::InitAction::Preserved(p) => json!({"kind":"preserved","path": p.display().to_string()}),
                crate::application::init_service::InitAction::CreatedSection(p) => json!({"kind":"created-section","path": p.display().to_string()}),
                crate::application::init_service::InitAction::BackedUpDesign { backup_path } => json!({"kind":"backed-up-design","backup_path": backup_path.display().to_string()}),
                crate::application::init_service::InitAction::RepairedStableBranch { path, from, to } => json!({"kind":"repaired-stable-branch","path": path.display().to_string(),"from": from, "to": to}),
            }).collect::<Vec<_>>(),
        }));
    env = env.with_revision(0, 0); // init is not a graph mutation
    let lines = human::init_report(&outcome);
    Ok((env, lines))
}

// ----------------------------------------------------------------------------
// status / doctor
// ----------------------------------------------------------------------------

fn status(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let ws = ctx.store.load().ok();
    let git = GitEvidence::collect(&ctx.repo_root);
    let mut env = envelope_for("status", Some(&ctx.repo_root)).with_data(json!({
        "git": {
            "current_branch": git.current_branch,
            "head_commit": git.head_commit,
            "clean": git.clean,
        },
    }));
    if let Some(w) = &ws {
        env = env
            .with_workspace_id(w.workspace_id.clone())
            .with_revision(w.revision, w.revision)
            .with_data(json!({
                "git": {
                    "current_branch": git.current_branch,
                    "head_commit": git.head_commit,
                    "clean": git.clean,
                },
                "workspace": graph_summary(w),
            }));
    }
    let lines = human::status_report(&ctx.repo_root, ws.as_ref());
    Ok((env, lines))
}

fn doctor(parsed: &crate::cli::ParsedArgs, sub: &str, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    // `mine doctor --agents all` (or `--agents <slug>`) appends the
    // agent installation diagnostics. The leaf `doctor` command may receive the
    // `--agents` token as `sub` (the dispatcher consumes tokens[1] as sub), so
    // fold it into the flag set.
    let rest_with_sub: Vec<String> = if sub.starts_with("--") {
        let mut v = vec![sub.to_string()];
        v.extend_from_slice(rest);
        v
    } else {
        rest.to_vec()
    };
    let (flags, _pos) = parse_flags(&rest_with_sub);
    let agents_scope = flag(&flags, "agents");
    let mut checks: Vec<(&'static str, bool, String)> = Vec::new();
    let config = load_config_or_error(&ctx.repo_root).ok();
    let config_present = config.is_some();
    checks.push((
        "config",
        config_present,
        if config_present {
            "ok".to_string()
        } else {
            "missing .mine/config.toml (run `mine init`)".to_string()
        },
    ));
    let marker_ok = ctx.repo_root.join("docs/design/.mine-design.toml").exists();
    checks.push((
        "design_marker",
        marker_ok,
        if marker_ok {
            "ok".to_string()
        } else {
            "missing docs/design/.mine-design.toml".to_string()
        },
    ));
    let index_ok = ctx.repo_root.join("docs/design/index.md").exists();
    checks.push((
        "design_index",
        index_ok,
        if index_ok {
            "ok".to_string()
        } else {
            "missing docs/design/index.md".to_string()
        },
    ));
    // Distinguish three graph states rather than treating "absent"
    // as unconditionally unhealthy:
    //   1. development repository, graph required: missing/invalid graph is a
    //      real failure (unchanged default);
    //   2. valid stable tree: `docs/plan/` intentionally absent is reported as
    //      `not_applicable`, not unhealthy -- but ONLY on positive
    //      authoritative evidence (current branch == the configured stable
    //      branch from `.mine/config.toml`), never merely because the graph
    //      file happens to be missing;
    //   3. malformed/incomplete repository: a graph that exists but fails to
    //      parse/validate is still a failure on any branch.
    let graph_load = ctx.store.load();
    let is_stable_branch = config.as_ref().is_some_and(|c| {
        crate::infrastructure::git::current_branch(&ctx.repo_root)
            .ok()
            .flatten()
            .as_deref()
            == Some(c.branches.stable.as_str())
    });
    let (graph_ok, graph_message) = match &graph_load {
        Ok(_) => (true, "ok".to_string()),
        Err(MineError::GraphNotInitialized { .. }) if is_stable_branch => (
            true,
            "not applicable: valid stable tree intentionally has no docs/plan/ workspace"
                .to_string(),
        ),
        Err(MineError::GraphNotInitialized { .. }) => {
            (false, "graph not initialized/invalid".to_string())
        }
        Err(e) => (false, format!("graph invalid: {e}")),
    };
    checks.push(("graph", graph_ok, graph_message));
    let git = GitEvidence::collect(&ctx.repo_root);
    let git_ok = git.current_branch.is_some();
    checks.push((
        "git",
        git_ok,
        if git_ok {
            format!("branch={}", git.current_branch.as_deref().unwrap_or("?"))
        } else {
            "no git repository detected".to_string()
        },
    ));
    let repo_ok = checks.iter().all(|(_, ok, _)| *ok);
    // Optional agent diagnostics (`--agents all` or `--agents <slug>`).
    let mut agent_section: Option<Value> = None;
    if agents_scope.is_some() {
        let scope: &str = match &agents_scope {
            Some(s) => s,
            None => "all",
        };
        let (env, version, _fp) = agent_env(parsed, rest);
        let report = crate::application::doctor_service::run(scope, &env, &version);
        agent_section = Some(serde_json::to_value(&report).unwrap_or(Value::Null));
    }
    // `agent_not_detected` and `agent_detected_mine_not_installed` are
    // informational, not failures.
    let agent_problems = agent_section.as_ref().is_some_and(|s| {
        s.get("diagnostics")
            .and_then(|d| d.as_array())
            .is_some_and(|arr| {
                arr.iter().any(|d| {
                    !matches!(
                        d.get("status").and_then(|v| v.as_str()).unwrap_or(""),
                        "healthy" | "agent_not_detected" | "agent_detected_mine_not_installed"
                    )
                })
            })
    });
    let all_ok = repo_ok && !agent_problems;
    let _ = &checks;

    let mut env = envelope_for("doctor", Some(&ctx.repo_root)).with_data({
        let checks_json = checks
            .iter()
            .map(|(name, ok, msg)| {
                json!({
                    "name": name, "ok": ok, "message": msg,
                })
            })
            .collect::<Vec<_>>();
        let mut base = json!({
            "healthy": all_ok,
            "checks": checks_json,
        });
        if let Some(ref sec) = agent_section {
            base["agents"] = sec.clone();
        }
        base
    });
    if ctx.store.load().ok().is_some() {
        if let Ok(w) = ctx.store.load() {
            env = env
                .with_workspace_id(w.workspace_id.clone())
                .with_revision(w.revision, w.revision);
        }
    }
    let mut lines = vec![HumanLine::Section(format!(
        "mine doctor: {}",
        if all_ok { "healthy" } else { "issues found" }
    ))];
    for (name, ok, msg) in &checks {
        lines.push(HumanLine::Field {
            key: format!("  {}", name),
            value: format!("{}: {}", if *ok { "ok" } else { "FAIL" }, msg),
        });
    }
    // The repo-doctor error exit fires only on REPOSITORY check failures.
    // Agent diagnostics are reported in the success envelope.
    // Even when repository checks fail, the already-computed Agent
    // diagnostics are preserved in the error `details` so callers (Skills,
    // MCP, CI) never lose the Agent section merely because the repository is
    // unhealthy. This is the partial-failure case: the command returns a
    // non-zero exit code (GATE) but the machine-readable envelope still
    // carries the full Agent diagnostics under `error.details.agents`.
    if !repo_ok {
        let mut details = json!({
            "checks": checks.iter().map(|(name, ok, msg)| json!({
                "name": name, "ok": ok, "message": msg,
            })).collect::<Vec<_>>(),
        });
        if let Some(ref sec) = agent_section {
            details["agents"] = sec.clone();
        }
        return Err(HandlerError {
            code: "MINE_DOCTOR",
            message: "one or more MINE checks failed".to_string(),
            exit_code: crate::output::exit_code::GATE,
            details,
        });
    }
    Ok((env, lines))
}

fn graph_summary(w: &PlanWorkspace) -> Value {
    json!({
        "workspace_id": w.workspace_id,
        "revision": w.revision,
        "stable_branch": w.stable_branch,
        "integration_branch": w.integration_branch,
        "plan_count": w.plans.len(),
        "ready": validation::ready_frontier(w),
    })
}

// ----------------------------------------------------------------------------
// workspace
// ----------------------------------------------------------------------------

fn workspace_open(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let baseline = git::head_commit(&ctx.repo_root).unwrap_or_default();
    let (stable_branch, integration_branch) = crate::cli::context::load_config(&ctx.repo_root)
        .map(|c| (c.branches.stable, c.branches.integration))
        .unwrap_or_else(|| ("master".to_string(), "dev".to_string()));
    let svc = WorkspaceService::new(&SystemUuidSource, &SystemClock);
    let outcome = svc
        .open(&ctx.store, &baseline, &stable_branch, &integration_branch)
        .map_err(ctx_err)?;
    let mut env = envelope_for("workspace.open", Some(&ctx.repo_root))
        .with_workspace_id(outcome.workspace_id.clone())
        .with_revision(outcome.revision_before, outcome.revision_after)
        .with_data(json!({
            "workspace_id": outcome.workspace_id,
            "stable_baseline_commit": outcome.stable_baseline_commit,
            "integration_branch": outcome.integration_branch,
        }));
    let lines = vec![
        HumanLine::Section("mine workspace open".to_string()),
        HumanLine::Field {
            key: "  workspace_id".to_string(),
            value: outcome.workspace_id.clone(),
        },
        HumanLine::Field {
            key: "  revision".to_string(),
            value: format!("{} -> {}", outcome.revision_before, outcome.revision_after),
        },
    ];
    let _ = &mut env;
    Ok((env, lines))
}

fn workspace_status(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let svc = WorkspaceService::new(&SystemUuidSource, &SystemClock);
    let outcome = svc.status(&ctx.store).map_err(ctx_err)?;
    let env = envelope_for("workspace.status", Some(&ctx.repo_root))
        .with_workspace_id(outcome.workspace_id.clone())
        .with_revision(outcome.revision, outcome.revision)
        .with_data(json!({
            "workspace_id": outcome.workspace_id,
            "revision": outcome.revision,
            "integration_branch": outcome.integration_branch,
            "stable_branch": outcome.stable_branch,
            "stable_baseline_commit": outcome.stable_baseline_commit,
            "plan_count": outcome.plan_count,
            "has_unresolved": outcome.has_unresolved,
        }));
    let lines = vec![
        HumanLine::Section("mine workspace status".to_string()),
        HumanLine::Field {
            key: "  workspace_id".to_string(),
            value: outcome.workspace_id,
        },
        HumanLine::Field {
            key: "  revision".to_string(),
            value: outcome.revision.to_string(),
        },
        HumanLine::Field {
            key: "  has_unresolved".to_string(),
            value: outcome.has_unresolved.to_string(),
        },
    ];
    Ok((env, lines))
}

fn workspace_close(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let svc = WorkspaceService::new(&SystemUuidSource, &SystemClock);
    let outcome = svc.close(&ctx.store).map_err(ctx_err)?;
    let env = envelope_for("workspace.close", Some(&ctx.repo_root))
        .with_workspace_id(outcome.workspace_id.clone())
        .with_revision(outcome.revision, outcome.revision)
        .with_data(json!({
            "workspace_id": outcome.workspace_id,
            "closable": !outcome.has_unresolved,
        }));
    let lines = vec![
        HumanLine::Section("mine workspace close".to_string()),
        HumanLine::Field {
            key: "  closable".to_string(),
            value: (!outcome.has_unresolved).to_string(),
        },
    ];
    Ok((env, lines))
}

// ----------------------------------------------------------------------------
// graph
// ----------------------------------------------------------------------------

fn graph_validate(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    // Route through the shared GraphService (same path the MCP tool uses).
    // validate() enforces generated-view byte parity: a stale or missing
    // view fails here naming `mine graph render` as the repair.
    let svc = GraphService::new(&ctx.store);
    let ws = svc.validate().map_err(ctx_err)?;
    let env = envelope_for("graph.validate", Some(&ctx.repo_root))
        .with_workspace_id(ws.workspace_id.clone())
        .with_revision(ws.revision, ws.revision)
        .with_data(json!({"plans": ws.plans.len(), "warnings_emitted": false}));
    let lines = vec![
        HumanLine::Section("mine graph validate: ok".to_string()),
        HumanLine::Field {
            key: "  plans".to_string(),
            value: ws.plans.len().to_string(),
        },
        HumanLine::Field {
            key: "  revision".to_string(),
            value: ws.revision.to_string(),
        },
    ];
    Ok((env, lines))
}

fn graph_render(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let svc = GraphService::new(&ctx.store);
    let rev_before = ctx.store.load().ok().map(|w| w.revision).unwrap_or(0);
    svc.render().map_err(ctx_err)?;
    let rev_after = ctx.store.load().map(|w| w.revision).unwrap_or(rev_before);
    let env = envelope_for("graph.render", Some(&ctx.repo_root))
        .with_revision(rev_before, rev_after)
        .with_data(json!({"rendered_path": ctx.store.md_path().display().to_string()}));
    let lines = vec![
        HumanLine::Section("mine graph render".to_string()),
        HumanLine::Field {
            key: "  output".to_string(),
            value: ctx.store.md_path().display().to_string(),
        },
    ];
    Ok((env, lines))
}

fn graph_repair_provenance(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let expected_revision = flag(&flags, "expected-revision")
        .ok_or_else(|| HandlerError::usage("graph repair-provenance requires --expected-revision"))?
        .parse::<u64>()
        .map_err(|_| HandlerError::usage("--expected-revision must be an unsigned integer"))?;
    let svc = GraphService::new(&ctx.store);
    let outcome = svc.repair_provenance(expected_revision).map_err(ctx_err)?;
    let env = envelope_for("graph.repair-provenance", Some(&ctx.repo_root))
        .with_workspace_id(outcome.workspace.workspace_id.clone())
        .with_revision(outcome.revision_before, outcome.revision_after)
        .with_data(json!({
            "changed": outcome.changed,
            "repaired_plans": outcome.repaired_plans,
            "schema_version": outcome.workspace.schema_version,
        }));
    let lines = vec![
        HumanLine::Section("mine graph repair-provenance".to_string()),
        HumanLine::Field {
            key: "  revision".to_string(),
            value: format!("{} -> {}", outcome.revision_before, outcome.revision_after),
        },
        HumanLine::Field {
            key: "  changed".to_string(),
            value: outcome.changed.to_string(),
        },
    ];
    Ok((env, lines))
}

fn graph_status(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let svc = GraphService::new(&ctx.store);
    let ws = svc.validate().map_err(ctx_err)?;
    let st = svc.status().map_err(ctx_err)?;
    let env = envelope_for("graph.status", Some(&ctx.repo_root))
        .with_workspace_id(ws.workspace_id.clone())
        .with_revision(st.revision, st.revision)
        .with_data(serde_json::to_value(&st).unwrap_or(Value::Null));
    let lines = human::status_report(&ctx.repo_root, Some(&ws));
    Ok((env, lines))
}

fn graph_ready(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let svc = GraphService::new(&ctx.store);
    let ready = svc.ready().map_err(ctx_err)?;
    let ws = svc.validate().map_err(ctx_err)?;
    let env = envelope_for("graph.ready", Some(&ctx.repo_root))
        .with_workspace_id(ws.workspace_id.clone())
        .with_revision(ws.revision, ws.revision)
        .with_data(json!({"ready": ready}));
    let lines = vec![HumanLine::Field {
        key: "  ready".to_string(),
        value: ready.join(", "),
    }];
    Ok((env, lines))
}

fn graph_wave(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let svc = GraphService::new(&ctx.store);
    let wave = svc.wave().map_err(ctx_err)?;
    let ws = svc.validate().map_err(ctx_err)?;
    let env = envelope_for("graph.wave", Some(&ctx.repo_root))
        .with_workspace_id(ws.workspace_id.clone())
        .with_revision(ws.revision, ws.revision)
        .with_data(json!({"wave": wave}));
    let lines = vec![HumanLine::Field {
        key: "  wave".to_string(),
        value: wave.join(", "),
    }];
    Ok((env, lines))
}

fn graph_show(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let ws = ctx.store.load().map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let env_data = if let Some(id) = flag(&flags, "plan") {
        let node = ws.get(id).ok_or_else(|| {
            HandlerError::from_mine(&MineError::PlanNotFound {
                plan_id: id.to_string(),
            })
        })?;
        json!({
            "plan": node_json(node),
        })
    } else {
        json!({
            "plans": ws.plans.iter().map(node_json).collect::<Vec<_>>(),
        })
    };
    let env = envelope_for("graph.show", Some(&ctx.repo_root))
        .with_workspace_id(ws.workspace_id.clone())
        .with_revision(ws.revision, ws.revision)
        .with_data(env_data);
    let lines = human::graph_table(&ws);
    Ok((env, lines))
}

fn node_json(p: &PlanNode) -> Value {
    json!({
        "id": p.id,
        "path": p.path,
        "title": p.title,
        "status": p.status.as_str(),
        "branch": p.branch,
        "branch_anchor": p.branch_anchor,
        "lineage_id": p.lineage_id,
        "worktree_path": p.worktree_path,
        "hard_predecessors": p.hard_predecessors,
        "planning_scope": p.planning_scope,
        "readiness_checked_at": p.readiness_checked_at,
        "adr_registry_revision": p.adr_registry_revision,
        "withdraw_reason": p.withdraw_reason,
        "withdrawn_at": p.withdrawn_at,
        "owner": p.owner,
        "run_id": p.run_id,
        "implementation_report": p.implementation_report,
        "implementation_commits": p.implementation_commits,
    })
}

// ----------------------------------------------------------------------------
// plan (mutations via save_with_revision)
// ----------------------------------------------------------------------------

/// Enforces planning readiness for a scope at registration time (ADR-0013).
/// The scope is the adapter-resolved planning scope — the explicit
/// `--planning-scope` value or the configured design-root default — never a
/// derived lineage id. Repositories without a bootstrapped ADR registry are
/// left to `mine init` (fail-open by design, matching `AdrService`).
fn require_plan_readiness(ctx: &CommandContext, scope: &str) -> MineResult<()> {
    let Some(config) = ctx.config.as_ref() else {
        return Ok(());
    };
    if !ctx.repo_root.join(&config.adr.registry).exists() {
        return Ok(());
    }
    AdrService::new(&ctx.repo_root, config).require_planning_ready(scope)
}

/// Resolves the planning scope for registration (ADR-0013): the explicit
/// value verbatim, or the configured design-root directory when omitted.
fn resolve_registration_scope(ctx: &CommandContext, explicit: Option<&str>) -> MineResult<String> {
    match explicit {
        Some(scope) if !scope.trim().is_empty() => {
            crate::domain::path::normalize_repo_relative(scope)?;
            Ok(scope.to_string())
        }
        _ => Ok(ctx
            .config
            .as_ref()
            .map(DesignService::default_planning_scope)
            .unwrap_or_else(|| "docs/design".to_string())),
    }
}

/// Resolves the node's persisted planning scope, falling back to the
/// design-root default for legacy pre-v4 nodes (ADR-0013), and returns the
/// release-time readiness audit evidence. A blocked readiness fails with the
/// blocking ADR DTOs in the error detail and writes nothing.
fn resolve_release_readiness(
    ctx: &CommandContext,
    id: &str,
) -> MineResult<crate::domain::plan_release::ReleaseReadinessAudit> {
    let node = GraphService::new(&ctx.store)
        .validate()?
        .get(id)
        .cloned()
        .ok_or_else(|| MineError::PlanNotFound {
            plan_id: id.to_string(),
        })?;
    let scope = if node.planning_scope.is_empty() {
        ctx.config
            .as_ref()
            .map(DesignService::default_planning_scope)
            .unwrap_or_else(|| "docs/design".to_string())
    } else {
        node.planning_scope.clone()
    };
    let checked_at = SystemClock.now_utc_rfc3339();
    let registry_revision = match ctx.config.as_ref() {
        Some(config) if ctx.repo_root.join(&config.adr.registry).exists() => {
            let readiness = AdrService::new(&ctx.repo_root, config).readiness(Some(&scope))?;
            if !readiness.ready {
                let blockers = readiness
                    .blockers
                    .iter()
                    .map(|blocker| format!("{} ({})", blocker.id, blocker.reason))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(MineError::GraphInvalid {
                    detail: format!("planning readiness blocked for {scope}: {blockers}"),
                });
            }
            readiness.registry_revision
        }
        _ => 0,
    };
    Ok(crate::domain::plan_release::ReleaseReadinessAudit {
        planning_scope: scope,
        checked_at,
        adr_registry_revision: registry_revision,
    })
}

fn plan_add(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("plan add requires --id"))?
        .to_string();
    let path = flag(&flags, "path")
        .ok_or_else(|| HandlerError::usage("plan add requires --path"))?
        .to_string();
    let title = flag(&flags, "title")
        .ok_or_else(|| HandlerError::usage("plan add requires --title"))?
        .to_string();
    let design_refs = flags_all(&flags, "design-ref");
    let design_refs: Vec<String> = if design_refs.is_empty() {
        return Err(HandlerError::usage(
            "plan add requires at least one --design-ref",
        ));
    } else {
        design_refs.iter().map(|s| s.to_string()).collect()
    };
    if design_refs.iter().any(|s| s.is_empty()) {
        return Err(HandlerError::usage("--design-ref must not be empty"));
    }
    let writes: Vec<String> = flags_all(&flags, "write")
        .iter()
        .map(|s| s.to_string())
        .collect();
    let hard: Vec<String> = flags_all(&flags, "hard")
        .iter()
        .map(|s| s.to_string())
        .collect();
    let planning_scope =
        resolve_registration_scope(&ctx, flag(&flags, "planning-scope")).map_err(ctx_err)?;
    require_plan_readiness(&ctx, &planning_scope).map_err(ctx_err)?;

    // Route through the shared PlanService (same path the MCP tool uses).
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let saved = svc
        .add(PlanAddRequest {
            id: id.clone(),
            path,
            title,
            design_references: design_refs,
            exclusive_write_paths: writes,
            hard_predecessors: hard,
            planning_scope,
        })
        .map_err(|e| map_partial(ctx_err(e), &ctx))?;
    let expected = saved.revision - 1;
    let node = saved.get(&id).expect("just added");
    let env = envelope_for("plan.add", Some(&ctx.repo_root))
        .with_workspace_id(saved.workspace_id.clone())
        .with_revision(expected, saved.revision)
        .with_data(json!({"plan": node_json(node)}));
    let lines = vec![HumanLine::Field {
        key: "  added".to_string(),
        value: format!("{} ({})", node.id, node.title),
    }];
    Ok((env, lines))
}

fn plan_show(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id").ok_or_else(|| HandlerError::usage("plan show requires --id"))?;
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let lookup = svc
        .show_with_locations(id, &ctx.caller_root)
        .map_err(ctx_err)?;
    let revision = lookup.authoritative_revision;
    let node = &lookup.node;
    let env = envelope_for("plan.show", Some(&ctx.repo_root))
        .with_workspace_id(graph.validate().map_err(ctx_err)?.workspace_id.clone())
        .with_revision(revision, revision)
        .with_data(json!({"plan": lookup}));
    let lines = vec![
        HumanLine::Field {
            key: "  id".to_string(),
            value: node.id.clone(),
        },
        HumanLine::Field {
            key: "  title".to_string(),
            value: node.title.clone(),
        },
        HumanLine::Field {
            key: "  status".to_string(),
            value: node.status.as_str().to_string(),
        },
        HumanLine::Field {
            key: "  hard_predecessors".to_string(),
            value: node.hard_predecessors.join(", "),
        },
    ];
    Ok((env, lines))
}

fn plan_prepare(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id").ok_or_else(|| HandlerError::usage("plan prepare requires --id"))?;
    let graph = GraphService::new(&ctx.store);
    let prepared = PlanService::new(&graph)
        .prepare(id, &ctx.caller_root)
        .map_err(ctx_err)?;
    let revision = prepared.lookup.authoritative_revision;
    let workspace_id = graph.validate().map_err(ctx_err)?.workspace_id.clone();
    let env = envelope_for("plan.prepare", Some(&ctx.repo_root))
        .with_workspace_id(workspace_id)
        .with_revision(revision, revision)
        .with_data(json!({"plan": prepared}));
    let lines = vec![HumanLine::Field {
        key: "  prepared".to_string(),
        value: format!("{} ({})", id, prepared.action),
    }];
    Ok((env, lines))
}

fn plan_start(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("plan start requires --id"))?
        .to_string();
    let owner = flag(&flags, "owner").unwrap_or("default").to_string();
    let run_id = flag(&flags, "run-id").unwrap_or("default-run").to_string();
    let now = SystemClock.now_utc_rfc3339();
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let expected = graph.validate().map_err(ctx_err)?.revision;
    let saved = svc
        .start(PlanStartRequest {
            id: id.clone(),
            owner,
            run_id,
            started_at: now,
        })
        .map_err(|e| map_partial(ctx_err(e), &ctx))?;
    let branch_anchor = saved
        .get(&id)
        .expect("started plan remains present")
        .branch_anchor
        .clone();
    let env = envelope_for("plan.start", Some(&ctx.repo_root))
        .with_workspace_id(saved.workspace_id.clone())
        .with_revision(expected, saved.revision)
        .with_data(json!({"plan": id, "branch_anchor": branch_anchor}));
    let lines = vec![HumanLine::Field {
        key: "  started".to_string(),
        value: id,
    }];
    Ok((env, lines))
}

fn plan_implemented(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("plan implemented requires --id"))?
        .to_string();
    let report = flag(&flags, "report")
        .ok_or_else(|| HandlerError::usage("plan implemented requires --report"))?
        .to_string();
    let commits: Vec<String> = flags_all(&flags, "commit")
        .iter()
        .map(|s| s.to_string())
        .collect();
    if commits.is_empty() {
        return Err(HandlerError::usage(
            "plan implemented requires at least one --commit",
        ));
    }
    let now = SystemClock.now_utc_rfc3339();
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let expected = graph.validate().map_err(ctx_err)?.revision;
    let saved = svc
        .mark_implemented(PlanImplementedRequest {
            id: id.clone(),
            report,
            commits: commits.clone(),
            updated_at: now,
        })
        .map_err(|e| map_partial(ctx_err(e), &ctx))?;
    let env = envelope_for("plan.implemented", Some(&ctx.repo_root))
        .with_workspace_id(saved.workspace_id.clone())
        .with_revision(expected, saved.revision)
        .with_data(json!({"plan": id, "commits": commits}));
    let lines = vec![HumanLine::Field {
        key: "  implemented".to_string(),
        value: id,
    }];
    Ok((env, lines))
}

fn plan_accept(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("plan accept requires --id"))?
        .to_string();
    let review = flag(&flags, "review").unwrap_or("").to_string();
    if review.is_empty() {
        return Err(HandlerError::usage(
            "plan accept requires --review <review report path>",
        ));
    }
    let now = SystemClock.now_utc_rfc3339();
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let expected = graph.validate().map_err(ctx_err)?.revision;
    let saved = svc
        .accept(PlanAcceptRequest {
            id: id.clone(),
            review_report: review,
            updated_at: now,
        })
        .map_err(|e| map_partial(ctx_err(e), &ctx))?;
    let env = envelope_for("plan.accept", Some(&ctx.repo_root))
        .with_workspace_id(saved.workspace_id.clone())
        .with_revision(expected, saved.revision)
        .with_data(json!({"plan": id}));
    let lines = vec![HumanLine::Field {
        key: "  accepted".to_string(),
        value: id,
    }];
    Ok((env, lines))
}

fn plan_reject(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("plan reject requires --id"))?
        .to_string();
    let reason = flag(&flags, "reason")
        .ok_or_else(|| HandlerError::usage("plan reject requires --reason"))?
        .to_string();
    let review_report = flag(&flags, "review-report")
        .ok_or_else(|| HandlerError::usage("plan reject requires --review-report"))?
        .to_string();
    let compensating = flag(&flags, "compensating-plan").unwrap_or("").to_string();
    if compensating.is_empty() {
        return Err(HandlerError::usage(
            "plan reject requires --compensating-plan <id>",
        ));
    }
    let now = SystemClock.now_utc_rfc3339();
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let expected = graph.validate().map_err(ctx_err)?.revision;
    let saved = svc
        .reject(PlanRejectRequest {
            id: id.clone(),
            reason,
            review_report: review_report.clone(),
            compensating_plan: compensating.clone(),
            updated_at: now,
        })
        .map_err(|e| map_partial(ctx_err(e), &ctx))?;
    let env = envelope_for("plan.reject", Some(&ctx.repo_root))
        .with_workspace_id(saved.workspace_id.clone())
        .with_revision(expected, saved.revision)
        .with_data(json!({
            "plan": id,
            "review_report": review_report,
            "compensating_plan": compensating
        }));
    let lines = vec![HumanLine::Field {
        key: "  rejected".to_string(),
        value: id,
    }];
    Ok((env, lines))
}

/// `mine plan release --id <id>`: the explicit gate that moves a DRAFT plan
/// to the startable frontier, routed through the shared `PlanService` (the
/// same path the MCP `mine_plan_release` tool uses).
fn plan_release(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("plan release requires --id"))?
        .to_string();
    // ADR-0013: re-run planning readiness for the node's persisted scope
    // before the transition; a blocked readiness fails with the blocking ADR
    // DTOs and writes nothing.
    let readiness_audit = resolve_release_readiness(&ctx, &id).map_err(ctx_err)?;
    let now = SystemClock.now_utc_rfc3339();
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let expected = graph.validate().map_err(ctx_err)?.revision;
    let saved = svc
        .release(PlanReleaseRequest {
            id: id.clone(),
            updated_at: now,
            readiness_audit,
        })
        .map_err(|e| map_partial(ctx_err(e), &ctx))?;
    let node = saved.get(&id).expect("present in saved ws");
    let status_after = node.status;
    let hard = node.hard_predecessors.clone();
    let planning_scope = node.planning_scope.clone();
    let unsatisfied =
        crate::domain::plan_release::unsatisfied_predecessors(&saved, &id).unwrap_or_default();
    let env = envelope_for("plan.release", Some(&ctx.repo_root))
        .with_workspace_id(saved.workspace_id.clone())
        .with_revision(expected, saved.revision)
        .with_data(json!({
            "plan": id,
            "status_before": "DRAFT",
            "status_after": status_after.as_str(),
            "hard_predecessors": hard,
            "unsatisfied_predecessors": unsatisfied,
            "planning_scope": planning_scope,
        }));
    let lines = vec![HumanLine::Field {
        key: "  released".to_string(),
        value: format!("{id} -> {}", status_after.as_str()),
    }];
    Ok((env, lines))
}

/// `mine plan rewire-compensation --id <rejected-id>`: reroutes a rejected
/// plan's downstream successors onto its registered compensating plan, routed
/// through the shared `PlanService` (the same path the MCP
/// `mine_plan_rewire_compensation` tool uses).
fn plan_rewire_compensation(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("plan rewire-compensation requires --id"))?
        .to_string();
    let now = SystemClock.now_utc_rfc3339();
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let expected = graph.validate().map_err(ctx_err)?.revision;
    let (saved, affected) = svc
        .rewire_compensation(PlanRewireRequest {
            id: id.clone(),
            updated_at: now,
        })
        .map_err(|e| map_partial(ctx_err(e), &ctx))?;
    let comp = saved
        .get(&id)
        .map(|n| n.compensating_plan.clone())
        .unwrap_or_default();
    let env = envelope_for("plan.rewire-compensation", Some(&ctx.repo_root))
        .with_workspace_id(saved.workspace_id.clone())
        .with_revision(expected, saved.revision)
        .with_data(json!({
            "rejected_plan": id,
            "compensating_plan": comp,
            "affected_successors": affected,
        }));
    let lines = vec![HumanLine::Field {
        key: "  rewired".to_string(),
        value: format!("{id} -> {comp} (affected: {})", affected.len()),
    }];
    Ok((env, lines))
}

/// `mine plan withdraw --id <id> --reason <reason>`: retires an unstarted
/// plan (`DRAFT`/`BLOCKED`/`READY`) into terminal `WITHDRAWN` (ADR-0012),
/// routed through the shared `PlanService` (the same path the MCP
/// `mine_plan_withdraw` tool uses).
fn plan_withdraw(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("plan withdraw requires --id"))?
        .to_string();
    let reason = flag(&flags, "reason")
        .ok_or_else(|| HandlerError::usage("plan withdraw requires --reason"))?
        .to_string();
    let now = SystemClock.now_utc_rfc3339();
    let graph = GraphService::new(&ctx.store);
    let svc = PlanService::new(&graph);
    let expected = graph.validate().map_err(ctx_err)?.revision;
    let (saved, withdrawal) = svc
        .withdraw(PlanWithdrawRequest {
            id: id.clone(),
            reason,
            withdrawn_at: now,
        })
        .map_err(ctx_err)?;
    let env = envelope_for("plan.withdraw", Some(&ctx.repo_root))
        .with_workspace_id(saved.workspace_id.clone())
        .with_revision(expected, saved.revision)
        .with_data(json!({
            "plan": withdrawal.id,
            "status_before": withdrawal.status_before,
            "status_after": withdrawal.status_after,
            "withdrawn_at": withdrawal.withdrawn_at,
            "cleaned_git_artifacts": withdrawal.cleaned_git_artifacts,
        }));
    let lines = vec![HumanLine::Field {
        key: "  withdrawn".to_string(),
        value: format!(
            "{} ({} -> WITHDRAWN{}, git artifacts {})",
            withdrawal.id,
            withdrawal.status_before,
            if withdrawal.cleaned_git_artifacts {
                "; cleaned"
            } else {
                ""
            },
            if withdrawal.cleaned_git_artifacts {
                "removed"
            } else {
                "none"
            }
        ),
    }];
    Ok((env, lines))
}

// ----------------------------------------------------------------------------
// design
// ----------------------------------------------------------------------------

fn adr_service_for<'a>(ctx: &'a CommandContext) -> MineResult<AdrService<'a>> {
    let config = ctx
        .config
        .as_ref()
        .ok_or_else(|| MineError::RepositoryNotFound {
            detail: "no .mine/config.toml (run `mine init`)".to_string(),
        })?;
    Ok(AdrService::new(&ctx.repo_root, config))
}

fn adr_add(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("adr add requires --id"))?
        .to_string();
    let path = flag(&flags, "path")
        .ok_or_else(|| HandlerError::usage("adr add requires --path"))?
        .to_string();
    let title = flag(&flags, "title")
        .ok_or_else(|| HandlerError::usage("adr add requires --title"))?
        .to_string();
    let planning_blocking = flag(&flags, "planning-blocking")
        .map(|v| v != "false")
        .unwrap_or(true);
    let scopes = flags_all(&flags, "scope")
        .into_iter()
        .map(str::to_string)
        .collect();
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let saved = service
        .add(AdrAddRequest {
            id: id.clone(),
            path,
            title,
            planning_blocking,
            planning_scopes: scopes,
        })
        .map_err(ctx_err)?;
    let env =
        envelope_for("adr.add", Some(&ctx.repo_root)).with_data(json!({"adr": saved.get(&id)}));
    Ok((
        env,
        vec![HumanLine::Field {
            key: "  added".to_string(),
            value: id,
        }],
    ))
}

fn adr_show(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _) = parse_flags(rest);
    let id = flag(&flags, "id").ok_or_else(|| HandlerError::usage("adr show requires --id"))?;
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let entry = service.show(id).map_err(ctx_err)?;
    let env = envelope_for("adr.show", Some(&ctx.repo_root)).with_data(json!({"adr": entry}));
    Ok((
        env,
        vec![HumanLine::Field {
            key: "  status".to_string(),
            value: format!("{} {}", entry.id, entry.status.as_str()),
        }],
    ))
}

fn adr_list(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let entries = service.list().map_err(ctx_err)?;
    let lines = entries
        .iter()
        .map(|entry| HumanLine::Field {
            key: format!("  {}", entry.id),
            value: format!("{} ({})", entry.title, entry.status.as_str()),
        })
        .collect();
    let env = envelope_for("adr.list", Some(&ctx.repo_root)).with_data(json!({"adrs": entries}));
    Ok((env, lines))
}

fn adr_propose(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("adr propose requires --id"))?
        .to_string();
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let saved = service
        .propose(&id, &SystemClock.now_utc_rfc3339())
        .map_err(ctx_err)?;
    let env =
        envelope_for("adr.propose", Some(&ctx.repo_root)).with_data(json!({"adr": saved.get(&id)}));
    Ok((
        env,
        vec![HumanLine::Field {
            key: "  proposed".to_string(),
            value: id,
        }],
    ))
}

fn adr_approve(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("adr approve requires --id"))?
        .to_string();
    let by = flag(&flags, "by").ok_or_else(|| HandlerError::usage("adr approve requires --by"))?;
    let note =
        flag(&flags, "note").ok_or_else(|| HandlerError::usage("adr approve requires --note"))?;
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let saved = service
        .approve(&id, by, note, &SystemClock.now_utc_rfc3339())
        .map_err(ctx_err)?;
    let env =
        envelope_for("adr.approve", Some(&ctx.repo_root)).with_data(json!({"adr": saved.get(&id)}));
    Ok((
        env,
        vec![HumanLine::Field {
            key: "  approved".to_string(),
            value: id,
        }],
    ))
}

fn adr_reject(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("adr reject requires --id"))?
        .to_string();
    let reason = flag(&flags, "reason")
        .ok_or_else(|| HandlerError::usage("adr reject requires --reason"))?;
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let saved = service
        .reject(&id, reason, &SystemClock.now_utc_rfc3339())
        .map_err(ctx_err)?;
    let env =
        envelope_for("adr.reject", Some(&ctx.repo_root)).with_data(json!({"adr": saved.get(&id)}));
    Ok((
        env,
        vec![HumanLine::Field {
            key: "  rejected".to_string(),
            value: id,
        }],
    ))
}

fn adr_supersede(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _) = parse_flags(rest);
    let id = flag(&flags, "id")
        .ok_or_else(|| HandlerError::usage("adr supersede requires --id"))?
        .to_string();
    let replacement = flag(&flags, "replacement")
        .ok_or_else(|| HandlerError::usage("adr supersede requires --replacement"))?
        .to_string();
    let dry_run = flags.iter().any(|(k, _)| k == "dry-run");
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    if dry_run {
        let preview = service
            .supersede_preview(&id, &replacement)
            .map_err(ctx_err)?;
        let revision = preview.registry_revision;
        let env = envelope_for("adr.supersede-preview", Some(&ctx.repo_root))
            .with_revision(revision, revision)
            .with_data(serde_json::to_value(&preview).unwrap_or(Value::Null));
        return Ok((
            env,
            vec![HumanLine::Field {
                key: "  preview".to_string(),
                value: format!("{id} -> {replacement} (no changes written)"),
            }],
        ));
    }
    let saved = service
        .supersede(&id, &replacement, &SystemClock.now_utc_rfc3339())
        .map_err(ctx_err)?;
    let env = envelope_for("adr.supersede", Some(&ctx.repo_root))
        .with_data(json!({"adr": saved.get(&id), "replacement": saved.get(&replacement)}));
    Ok((
        env,
        vec![HumanLine::Field {
            key: "  superseded".to_string(),
            value: format!("{id} -> {replacement}"),
        }],
    ))
}

fn adr_validate(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let registry = service.validate().map_err(ctx_err)?;
    let env = envelope_for("adr.validate", Some(&ctx.repo_root)).with_data(
        json!({"valid": true, "revision": registry.revision, "count": registry.entries.len()}),
    );
    Ok((
        env,
        vec![HumanLine::Field {
            key: "  valid".to_string(),
            value: format!("{} ADRs", registry.entries.len()),
        }],
    ))
}

/// `mine design render`: deterministically regenerates the generated ADR
/// decision index from the registry without touching the registry or its
/// revision; reports the path and whether bytes changed.
fn design_render(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let outcome = service.render_design_index().map_err(ctx_err)?;
    let env = envelope_for("design.render", Some(&ctx.repo_root)).with_data(json!({
        "path": outcome.path,
        "changed": outcome.changed,
    }));
    let lines = vec![
        HumanLine::Section("mine design render".to_string()),
        HumanLine::Field {
            key: "  path".to_string(),
            value: outcome.path.clone(),
        },
        HumanLine::Field {
            key: "  changed".to_string(),
            value: outcome.changed.to_string(),
        },
    ];
    Ok((env, lines))
}

fn design_readiness(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _) = parse_flags(rest);
    let scope = flag(&flags, "scope");
    let service = adr_service_for(&ctx).map_err(ctx_err)?;
    let readiness = service.readiness(scope).map_err(ctx_err)?;
    let lines = vec![HumanLine::Field {
        key: "  ready".to_string(),
        value: readiness.ready.to_string(),
    }];
    let env = envelope_for("design.readiness", Some(&ctx.repo_root))
        .with_data(serde_json::to_value(&readiness).unwrap_or(Value::Null));
    Ok((env, lines))
}

fn design_backup(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let config = load_config_or_error(&ctx.repo_root).map_err(ctx_err)?;
    let svc = DesignBackup::new(&SystemClock);
    let outcome = svc.backup(&ctx.repo_root, &config).map_err(ctx_err)?;
    let env = envelope_for("design.backup", Some(&ctx.repo_root)).with_data(json!({
        "backup_path": outcome.backup_path_relative,
        "timestamp": outcome.timestamp,
        "file_count": outcome.file_count,
        "total_bytes": outcome.total_bytes,
    }));
    let lines = vec![
        HumanLine::Section("mine design backup".to_string()),
        HumanLine::Field {
            key: "  backup_path".to_string(),
            value: outcome.backup_path_relative,
        },
        HumanLine::Field {
            key: "  files".to_string(),
            value: outcome.file_count.to_string(),
        },
    ];
    Ok((env, lines))
}

fn design_validate(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let config = load_config_or_error(&ctx.repo_root).map_err(ctx_err)?;
    // Route through the shared DesignService (same path the MCP tool uses).
    let result = DesignService::validate(&ctx.repo_root, &config).map_err(ctx_err)?;
    let env = envelope_for("design.validate", Some(&ctx.repo_root))
        .with_data(serde_json::to_value(&result).unwrap_or(Value::Null));
    let lines = vec![HumanLine::Field {
        key: "  valid".to_string(),
        value: result.valid.to_string(),
    }];
    Ok((env, lines))
}

fn design_status(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let config = load_config_or_error(&ctx.repo_root).map_err(ctx_err)?;
    let status = DesignService::status(&ctx.repo_root, &config).map_err(ctx_err)?;
    let env = envelope_for("design.status", Some(&ctx.repo_root))
        .with_data(serde_json::to_value(&status).unwrap_or(Value::Null));
    let lines = vec![
        HumanLine::Field {
            key: "  managed".to_string(),
            value: status.managed.to_string(),
        },
        HumanLine::Field {
            key: "  design_root".to_string(),
            value: status.design_root,
        },
    ];
    Ok((env, lines))
}

// ----------------------------------------------------------------------------
// repository version
// ----------------------------------------------------------------------------

fn repository_version(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    // `mine repository version show|suggest|set [...]`
    let verb = rest.first().map(String::as_str).unwrap_or("show");
    match verb {
        "show" => repo_version_show(parsed, &rest[1..]),
        "suggest" => repo_version_suggest(parsed, &rest[1..]),
        "set" => repo_version_set(parsed, &rest[1..]),
        _ => Err(HandlerError::usage(format!(
            "unknown repository version subcommand {verb:?} (show|suggest|set)"
        ))),
    }
}

fn repo_version_show(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let config = load_config_or_error(&ctx.repo_root).map_err(ctx_err)?;
    let env = envelope_for("repository.version", Some(&ctx.repo_root)).with_data(json!({
        "version": config.mine_code_version,
        "repository_id": config.repository_id,
    }));
    let lines = vec![HumanLine::Field {
        key: "  version".to_string(),
        value: config.mine_code_version,
    }];
    Ok((env, lines))
}

fn repo_version_suggest(parsed: &crate::cli::ParsedArgs, _rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let config = load_config_or_error(&ctx.repo_root).map_err(ctx_err)?;
    // A minimal suggestion: increment the patch component of the recorded
    // version, clamping to a stable MAJOR.MINOR.PATCH form. Deterministic and
    // side-effect-free.
    let suggested = suggest_next_version(&config.mine_code_version);
    let env = envelope_for("repository.version", Some(&ctx.repo_root)).with_data(json!({
        "current": config.mine_code_version,
        "suggested": suggested,
    }));
    let lines = vec![
        HumanLine::Field {
            key: "  current".to_string(),
            value: config.mine_code_version.clone(),
        },
        HumanLine::Field {
            key: "  suggested".to_string(),
            value: suggested,
        },
    ];
    Ok((env, lines))
}

fn repo_version_set(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let new_version = flag(&flags, "version")
        .ok_or_else(|| HandlerError::usage("repository version set requires --version <semver>"))?
        .to_string();
    if !is_valid_semver(&new_version) {
        return Err(HandlerError::usage(format!(
            "invalid --version {new_version:?}; expected MAJOR.MINOR.PATCH"
        )));
    }
    let config_path = ctx.repo_root.join(".mine").join("config.toml");
    let mut config = load_config_or_error(&ctx.repo_root).map_err(ctx_err)?;
    let old = config.mine_code_version.clone();
    config.mine_code_version = new_version.clone();
    let content = config.to_toml();
    crate::infrastructure::atomic_write::write(&config_path, content.as_bytes())
        .map_err(ctx_err)?;
    let env = envelope_for("repository.version", Some(&ctx.repo_root)).with_data(json!({
        "previous": old,
        "current": new_version,
    }));
    let lines = vec![
        HumanLine::Field {
            key: "  previous".to_string(),
            value: old,
        },
        HumanLine::Field {
            key: "  current".to_string(),
            value: new_version,
        },
    ];
    Ok((env, lines))
}

fn suggest_next_version(v: &str) -> String {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() == 3 {
        if let Ok(patch) = parts[2].parse::<u32>() {
            return format!("{}.{}.{}", parts[0], parts[1], patch + 1);
        }
    }
    format!("{v}+1")
}

fn is_valid_semver(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok())
}

// ----------------------------------------------------------------------------
// persistent mutation helper
// ----------------------------------------------------------------------------

/// Maps a service error to a [`HandlerError`], elevating partial-success
/// render failures (surfaced by the store as `GraphInvalid` with a `render`
/// hint) to exit code 7 (PARTIAL) per the exit-code contract. Other errors keep
/// their domain-derived exit code. Shared by the CLI and MCP adapters (the MCP
/// adapter uses the same partial-success mapping).
fn map_partial(err: HandlerError, _ctx: &CommandContext) -> HandlerError {
    let HandlerError {
        code,
        message,
        exit_code,
        details,
    } = err;
    if code == "MINE_GRAPH_INVALID" && message.contains("render") {
        HandlerError {
            code: "MINE_GRAPH_RENDER_PARTIAL",
            message,
            exit_code: crate::output::exit_code::PARTIAL,
            details,
        }
    } else {
        HandlerError {
            code,
            message,
            exit_code,
            details,
        }
    }
}

// A temporary shim for `Envelope::success("").unused()` used in `inflate`;
// kept to avoid dead-code churn. `inflate` is itself a helper reserved for
// future use and not currently called.

// ----------------------------------------------------------------------------
// Agent installer handlers (`mine agent ...`) — isolation-correct.
// ----------------------------------------------------------------------------

/// Resolves the agent installation environment. Fix 3 (isolation): when an
/// explicit `--config-root <path>` is supplied, the environment is built with
/// [`Env::isolated`] — it reads NO real process environment overrides
/// (`CLAUDE_CONFIG_DIR`/`CODEX_HOME`/`PI_HOME`/`OPENCODE_CONFIG_DIR`) and derives
/// every Agent path only from the injected root. When `--config-root` is
/// absent (production), the real-env constructor reads the live environment and
/// platform home dir. The two constructors are never mixed.
fn agent_env(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> (Env, String, FailPhase) {
    let (flags, _pos) = parse_flags(rest);
    let mine_version = match build_context(&parsed.global) {
        Ok(ctx) => crate::cli::context::load_config(&ctx.repo_root)
            .map(|c| c.mine_code_version)
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
        Err(_) => env!("CARGO_PKG_VERSION").to_string(),
    };
    let (env, fail_phase) = match flag(&flags, "config-root") {
        Some(p) => (
            agent_service::isolated_env(std::path::PathBuf::from(p)),
            FailPhase::None,
        ),
        None => (agent_service::real_env(), FailPhase::None),
    };
    (env, mine_version, fail_phase)
}

fn agent_install(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let (flags, pos) = parse_flags(rest);
    if pos.is_empty() {
        return Err(HandlerError::usage(
            "agent install requires an agent slug (claude-code|codex|pi|opencode)",
        ));
    }
    let slug = &pos[0];
    let dry_run = flags.iter().any(|(k, _)| k == "dry-run" || k == "dry");
    let (env, version, fail_phase) = agent_env(parsed, rest);
    let _ = &flags;
    let outcome = agent_service::install(slug, &env, &version, dry_run, fail_phase)
        .map_err(|e| HandlerError::from_mine(&e))?;
    let env_data = envelope_for("agent.install", None)
        .with_data(serde_json::to_value(&outcome).unwrap_or(Value::Null));
    let lines = vec![HumanLine::Section(format!(
        "agent install {}: {} skills{}",
        outcome.agent,
        outcome.skills_installed,
        if outcome.updated {
            " (updated)"
        } else {
            " (idempotent)"
        }
    ))];
    Ok((env_data, lines))
}

fn agent_uninstall(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let (flags, pos) = parse_flags(rest);
    if pos.is_empty() {
        return Err(HandlerError::usage(
            "agent uninstall requires an agent slug (claude-code|codex|pi|opencode)",
        ));
    }
    let slug = &pos[0];
    let dry_run = flags.iter().any(|(k, _)| k == "dry-run" || k == "dry");
    let (env, _version, _fp) = agent_env(parsed, rest);
    let _ = &flags;
    let outcome =
        agent_service::uninstall(slug, &env, dry_run).map_err(|e| HandlerError::from_mine(&e))?;
    let env_data = envelope_for("agent.uninstall", None)
        .with_data(serde_json::to_value(&outcome).unwrap_or(Value::Null));
    let mut lines = vec![HumanLine::Section(format!(
        "agent uninstall {}: removed {} files, {} config entries",
        outcome.agent, outcome.removed_files, outcome.removed_config_entries
    ))];
    if !outcome.drifted_files.is_empty() {
        lines.push(HumanLine::Field {
            key: "  preserved (drifted)".to_string(),
            value: outcome.drifted_files.join(", "),
        });
    }
    Ok((env_data, lines))
}

fn agent_status(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let (env, _version, _fp) = agent_env(parsed, rest);
    let summary = agent_service::status(&env).map_err(|e| HandlerError::from_mine(&e))?;
    let env_data = envelope_for("agent.status", None)
        .with_data(json!({ "installs": serde_json::to_value(&summary).unwrap_or(Value::Null) }));
    let mut lines = vec![HumanLine::Section("mine agent status".to_string())];
    if summary.is_empty() {
        lines.push(HumanLine::Field {
            key: "  ".to_string(),
            value: "no MINE-managed agent installations".to_string(),
        });
    } else {
        for s in &summary {
            lines.push(HumanLine::Field {
                key: format!("  {}", s.agent),
                value: format!(
                    "v{} ({} files, {} config entries, mcp={})",
                    s.mine_version, s.files, s.config_entries, s.mcp_registered
                ),
            });
        }
    }
    Ok((env_data, lines))
}

fn agent_config(_parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let (_flags, pos) = parse_flags(rest);
    if pos.is_empty() {
        return Err(HandlerError::usage(
            "agent config requires an agent slug (claude-code|codex|pi|opencode)",
        ));
    }
    let slug = &pos[0];
    let preview = agent_service::config_preview(slug).map_err(|e| HandlerError::from_mine(&e))?;
    let env_data = envelope_for("agent.config", None)
        .with_data(serde_json::to_value(&preview).unwrap_or(Value::Null));
    let mut lines = vec![HumanLine::Section(format!(
        "agent config: {}",
        preview.agent
    ))];
    if preview.supports_mcp {
        lines.push(HumanLine::Field {
            key: "  target".to_string(),
            value: preview.target_file.clone(),
        });
        lines.push(HumanLine::Field {
            key: "  pointer".to_string(),
            value: preview.json_pointer.clone(),
        });
        lines.push(HumanLine::Field {
            key: "  entry".to_string(),
            value: serde_json::to_string_pretty(&preview.entry).unwrap_or_default(),
        });
    } else {
        lines.push(HumanLine::Field {
            key: "  ".to_string(),
            value: "Pi has no MCP in its minimal core; Skills use the JSON CLI fallback."
                .to_string(),
        });
    }
    Ok((env_data, lines))
}

// ----------------------------------------------------------------------------
// Release preflight, stale-plan-reference scan, and dist sync/verify
// ----------------------------------------------------------------------------

fn scan_plan_refs(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let check = flag(&flags, "check").is_some();
    let result =
        crate::application::scan_service::scan_plan_refs(&ctx.repo_root, check).map_err(ctx_err)?;
    let mut data = serde_json::to_value(&result).unwrap_or(Value::Null);
    if let Some(obj) = data.as_object_mut() {
        obj.insert("check".to_string(), json!(check));
        obj.insert("clean".to_string(), json!(result.findings.is_empty()));
    }
    let env = envelope_for("scan.plan-refs", Some(&ctx.repo_root)).with_data(data);
    let mut lines = vec![
        HumanLine::Field {
            key: "  files_scanned".to_string(),
            value: result.files_scanned.to_string(),
        },
        HumanLine::Field {
            key: "  findings".to_string(),
            value: result.findings.len().to_string(),
        },
    ];
    for f in &result.findings {
        lines.push(HumanLine::Field {
            key: format!("  {}", f.file),
            value: format!("{}: {}", f.line, f.content),
        });
    }
    if check && !result.findings.is_empty() {
        return Err(HandlerError {
            code: "MINE_SCAN_PLAN_REFS",
            message: format!(
                "{} unexempted plan reference(s) found (--check)",
                result.findings.len()
            ),
            exit_code: crate::output::exit_code::VALIDATION,
            details: serde_json::to_value(&result).unwrap_or(Value::Null),
        });
    }
    Ok((env, lines))
}

fn release_preflight(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let dry_run = flags.iter().any(|(k, _)| k == "dry-run" || k == "dry");
    let _ = dry_run; // release preflight is always read-only
    let pf = crate::application::release_service::preflight(&ctx.repo_root)
        .map_err(|e| HandlerError::from_mine(&e))?;
    let env_data = envelope_for("release.preflight", Some(&ctx.repo_root))
        .with_data(serde_json::to_value(&pf).unwrap_or(Value::Null));
    let mut lines = vec![HumanLine::Section(format!(
        "release preflight: {}",
        if pf.can_release {
            "can release"
        } else {
            "NOT ready"
        }
    ))];
    if !pf.errors.is_empty() {
        for e in &pf.errors {
            lines.push(HumanLine::Field {
                key: "  FAIL".to_string(),
                value: e.clone(),
            });
        }
    } else {
        lines.push(HumanLine::Field {
            key: "  version".to_string(),
            value: pf.release_version.clone(),
        });
        lines.push(HumanLine::Field {
            key: "  dev".to_string(),
            value: pf.dev_commit.clone(),
        });
        lines.push(HumanLine::Field {
            key: "  stable".to_string(),
            value: pf.stable_commit.clone(),
        });
    }
    Ok((env_data, lines))
}

fn dist_sync(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let root_override = flag(&flags, "root");
    let root = root_override
        .map(std::path::PathBuf::from)
        .unwrap_or(ctx.repo_root.clone());
    let script = root.join("scripts/sync-plugin-assets.py");
    if !script.exists() {
        return Err(HandlerError::usage("sync script not found"));
    }
    let output = std::process::Command::new("python")
        .arg(&script)
        .arg("--root")
        .arg(&root)
        .output()
        .map_err(|e| HandlerError {
            code: "MINE_IO",
            message: e.to_string(),
            exit_code: crate::output::exit_code::EXTERNAL,
            details: Value::Null,
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(HandlerError {
            code: "MINE_DIST_SYNC",
            message: format!("sync failed: {stderr}"),
            exit_code: crate::output::exit_code::VALIDATION,
            details: json!({"stdout": stdout, "stderr": stderr}),
        });
    }
    let env = envelope_for("dist.sync", Some(&ctx.repo_root)).with_data(json!({"output": stdout}));
    Ok((
        env,
        vec![HumanLine::Section(format!("dist sync: {stdout}"))],
    ))
}

fn dist_verify(parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let ctx = build_context(&parsed.global).map_err(ctx_err)?;
    let (flags, _pos) = parse_flags(rest);
    let root_override = flag(&flags, "root");
    let root = root_override
        .map(std::path::PathBuf::from)
        .unwrap_or(ctx.repo_root.clone());
    let script = root.join("scripts/sync-plugin-assets.py");
    if !script.exists() {
        return Err(HandlerError::usage("sync script not found"));
    }
    let output = std::process::Command::new("python")
        .arg(&script)
        .arg("--check")
        .arg("--root")
        .arg(&root)
        .output()
        .map_err(|e| HandlerError {
            code: "MINE_IO",
            message: e.to_string(),
            exit_code: crate::output::exit_code::EXTERNAL,
            details: Value::Null,
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(HandlerError {
            code: "MINE_DIST_VERIFY",
            message: format!("distribution out of sync: {stderr}"),
            exit_code: crate::output::exit_code::VALIDATION,
            details: json!({"stdout": stdout, "stderr": stderr}),
        });
    }
    let env =
        envelope_for("dist.verify", Some(&ctx.repo_root)).with_data(json!({"output": stdout}));
    Ok((
        env,
        vec![HumanLine::Section(format!("dist verify: {stdout}"))],
    ))
}

// ----------------------------------------------------------------------------
// mine setup / mine update / mine uninstall — end-user installer lifecycle.
// ----------------------------------------------------------------------------

fn setup(_parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let (flags, _pos) = parse_flags(rest);
    let agents = flag(&flags, "agents").map(|s| s.to_string());
    let yes = flags.iter().any(|(k, _)| k == "yes" || k == "y");
    let config_root = flag(&flags, "config-root").map(std::path::PathBuf::from);
    let args = crate::setup::SetupArgs {
        agents,
        yes,
        config_root,
    };
    let report = crate::setup::run_setup(&args).map_err(|e| HandlerError::from_mine(&e))?;
    let env_data =
        envelope_for("setup", None).with_data(serde_json::to_value(&report).unwrap_or(Value::Null));
    let mut lines = vec![HumanLine::Section(if report.cancelled {
        "mine setup cancelled".to_string()
    } else {
        "mine setup done".to_string()
    })];
    if !report.installed.is_empty() {
        lines.push(HumanLine::Field {
            key: "  installed".to_string(),
            value: report.installed.join(", "),
        });
    }
    if !report.uninstalled.is_empty() {
        lines.push(HumanLine::Field {
            key: "  uninstalled".to_string(),
            value: report.uninstalled.join(", "),
        });
    }
    if !report.errors.is_empty() {
        lines.push(HumanLine::Field {
            key: "  errors".to_string(),
            value: report.errors.join("; "),
        });
    }
    Ok((env_data, lines))
}

fn update(_parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let (flags, _pos) = parse_flags(rest);
    let yes = flags.iter().any(|(k, _)| k == "yes" || k == "y");
    let config_root = flag(&flags, "config-root").map(std::path::PathBuf::from);
    let args = crate::setup::UpdateArgs { yes, config_root };
    let report = crate::setup::run_update(&args).map_err(|e| HandlerError::from_mine(&e))?;
    let env_data = envelope_for("update", None)
        .with_data(serde_json::to_value(&report).unwrap_or(Value::Null));
    let mut lines = vec![HumanLine::Section(format!(
        "mine update: {} -> {} ({})",
        report.from, report.to, report.note
    ))];
    for a in &report.skills_refreshed {
        lines.push(HumanLine::Field {
            key: "  skills refreshed".to_string(),
            value: a.clone(),
        });
    }
    for e in &report.skills_errors {
        lines.push(HumanLine::Field {
            key: "  skills error".to_string(),
            value: e.clone(),
        });
    }
    Ok((env_data, lines))
}

fn uninstall(_parsed: &crate::cli::ParsedArgs, rest: &[String]) -> HandlerResult {
    let (flags, _pos) = parse_flags(rest);
    let yes = flags.iter().any(|(k, _)| k == "yes" || k == "y");
    let args = crate::setup::UninstallArgs { yes };
    let report = crate::setup::run_uninstall(&args).map_err(|e| HandlerError::from_mine(&e))?;
    let env_data = envelope_for("uninstall", None)
        .with_data(serde_json::to_value(&report).unwrap_or(Value::Null));
    let mut lines = vec![HumanLine::Section(format!(
        "mine uninstall: {}",
        report.note
    ))];
    if !report.agents_cleared.is_empty() {
        lines.push(HumanLine::Field {
            key: "  agents cleared".to_string(),
            value: report.agents_cleared.join(", "),
        });
    }
    lines.push(HumanLine::Field {
        key: "  binary removed".to_string(),
        value: report.binary_removed.to_string(),
    });
    Ok((env_data, lines))
}
