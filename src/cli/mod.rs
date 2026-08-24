//! `mine` CLI dispatcher, argument parsing, and outcome wiring.
//!
//! Implements the human/JSON CLI surface from
//! `docs/design/interfaces/cli-contract.md`. The dispatcher parses a fixed
//! argument vector (no shell), routes to a command handler, and renders the
//! result as a deterministic JSON envelope (`--format json`) or concise human
//! text (default). It performs **no** Git mutation and supports only
//! read-only/subcommand-defined mutations; no automatic commit, merge, reset,
//! clean, stash, rebase, push, or branch deletion.

pub mod args;
pub mod commands;
pub mod context;

use clap::Parser;

use crate::output::envelope::{Envelope, EnvelopeError, ErrorEnvelope};
use crate::output::exit_code;
use crate::output::human::HumanLine;

use context::GlobalOpts;

/// A fully resolved command outcome carrying both the machine envelope and the
/// human rendering so the dispatcher can choose per `--format`.
#[derive(Debug)]
pub struct Outcome {
    pub command: &'static str,
    pub exit_code: i32,
    pub payload: OutcomePayload,
}

#[derive(Debug)]
pub enum OutcomePayload {
    Success {
        envelope: Envelope,
        human: Vec<HumanLine>,
    },
    Error {
        envelope: ErrorEnvelope,
        message: String,
    },
}

/// A handler error: a typed code + message + exit code + optional details.
/// Handlers build this from a [`crate::domain::error::MineError`] or a usage
/// failure.
#[derive(Debug)]
pub struct HandlerError {
    pub code: &'static str,
    pub message: String,
    pub exit_code: i32,
    pub details: serde_json::Value,
}

impl HandlerError {
    /// Builds a handler error from a domain error, mapping it to the public
    /// exit-code contract.
    pub fn from_mine(err: &crate::domain::error::MineError) -> Self {
        Self {
            code: err.code(),
            message: format!("{err}"),
            exit_code: crate::output::exit_code_for(err),
            details: serde_json::Value::Null,
        }
    }

    /// Builds a usage error (invalid invocation).
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            code: "MINE_USAGE",
            message: message.into(),
            exit_code: exit_code::USAGE,
            details: serde_json::Value::Null,
        }
    }
}

/// A parsed invocation: global options and the remaining command tokens.
#[derive(Debug, Clone)]
pub struct ParsedArgs {
    pub global: GlobalOpts,
    pub tokens: Vec<String>,
}

/// Parses a raw argument vector (program name expected as `argv[0]`).
///
/// Runs the CLI against a raw argument vector and returns the final
/// [`Outcome`] for the caller (`main`) to render and exit on.
///
/// `program` is the binary name used in usage messages.
pub fn dispatch(argv: &[String], _program: &str) -> Outcome {
    // Clap owns --help/--version: it prints colored help and exits directly.
    // For everything else we parse into our model, build the rest-token vector
    // the existing handlers expect, and route through commands::handle so the
    // JSON envelope contract and handler logic stay unchanged.
    let cli = match args::Cli::try_parse_from(argv.iter().cloned()) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() == clap::error::ErrorKind::DisplayHelp
                || e.kind() == clap::error::ErrorKind::DisplayVersion
            {
                let _ = e.print();
                std::process::exit(e.exit_code());
            }
            return error_outcome(
                "usage",
                HandlerError {
                    code: "MINE_USAGE",
                    message: e.to_string(),
                    exit_code: exit_code::USAGE,
                    details: serde_json::Value::Null,
                },
                None,
                None,
            );
        }
    };

    let global = context::GlobalOpts {
        format: cli.format.map(|f| f.into()).unwrap_or_default(),
        quiet: cli.quiet,
        repo: cli.repo.clone(),
        no_color: cli.no_color,
        config_root: cli.config_root.clone(),
    };
    let parsed = ParsedArgs {
        global,
        tokens: Vec::new(),
    };

    let (group, sub, mut rest, command) = command_route(&cli.command);
    // Inject global --config-root into rest tokens so handlers that read it
    // via parse_flags(rest) still find it.
    if let Some(ref cr) = cli.config_root {
        rest.push("--config-root".to_string());
        rest.push(cr.display().to_string());
    }
    match commands::handle(&parsed, group, sub, &rest) {
        Ok((envelope, human)) => Outcome {
            command,
            exit_code: exit_code::SUCCESS,
            payload: OutcomePayload::Success { envelope, human },
        },
        Err(err) => {
            let repo = parsed
                .global
                .repo
                .as_ref()
                .and_then(|p| p.canonicalize().ok())
                .map(|p| p.display().to_string());
            error_outcome(command, err, repo.as_deref(), None)
        }
    }
}

/// Maps a parsed [`Commands`] to (group, sub, rest-tokens, command-name).
#[allow(clippy::too_many_lines)]
fn command_route(cmd: &Commands) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match cmd {
        Commands::Init => ("init", "", vec![], "init"),
        Commands::Status => ("status", "", vec![], "status"),
        Commands::Doctor { agents } => ("doctor", "", doctor_tokens(agents.as_deref()), "doctor"),
        Commands::Setup { agents, yes } => {
            ("setup", "", setup_tokens(agents.as_deref(), *yes), "setup")
        }
        Commands::Update { yes } => ("update", "", yes_tokens(*yes), "update"),
        Commands::Uninstall { yes } => ("uninstall", "", yes_tokens(*yes), "uninstall"),
        Commands::Workspace { action } => workspace_route(action),
        Commands::Graph { action } => graph_route(action),
        Commands::Plan { action } => plan_route(action),
        Commands::Design { action } => design_route(action),
        Commands::Adr { action } => adr_route(action),
        Commands::Agent { action } => agent_route(action),
        Commands::Repository { action } => repository_route(action),
        Commands::Dist { action } => dist_route(action),
        Commands::Release => ("release", "", vec![], "release"),
        Commands::Scan { action } => scan_route(action),
        Commands::Mcp { action } => mcp_route(action),
    }
}

/// Maps a (group, sub) pair to the dotted command identifier emitted in the
/// JSON envelope.
fn error_outcome(
    command: &'static str,
    err: HandlerError,
    repository: Option<&str>,
    workspace_id: Option<&str>,
) -> Outcome {
    let exit_code = err.exit_code;
    let mut envelope_err = EnvelopeError::new(err.code, err.message.clone());
    if !err.details.is_null() {
        envelope_err = envelope_err.with_details(err.details);
    }
    let mut env = ErrorEnvelope::new(command, envelope_err);
    if let Some(r) = repository {
        env = env.with_repository(r);
    }
    if let Some(w) = workspace_id {
        env = env.with_workspace_id(w);
    }
    Outcome {
        command,
        exit_code,
        payload: OutcomePayload::Error {
            envelope: env,
            message: err.message,
        },
    }
}

/// Renders an [`Outcome`] to `(stdout_text, stderr_text)`. On success, output
/// goes to stdout; on error, the human/json body goes to stderr so stdout
/// stays clean for pipelines consuming JSON only on success.
#[must_use]
pub fn render(outcome: &Outcome, json: bool, quiet: bool) -> (String, String) {
    match &outcome.payload {
        OutcomePayload::Success { envelope, human } => {
            if json {
                (envelope.to_json(), String::new())
            } else if quiet {
                (String::new(), String::new())
            } else {
                let mut out = String::new();
                for line in human {
                    out.push_str(&line.render());
                    out.push('\n');
                }
                (out, String::new())
            }
        }
        OutcomePayload::Error { envelope, message } => {
            if json {
                (String::new(), envelope.to_json())
            } else {
                (String::new(), format!("error: {message}\n"))
            }
        }
    }
}

// A helper for handlers to build a success envelope with repository context.
/// Convenience: returns a base envelope for `command` with the repository root
/// attached.
#[must_use]
pub fn envelope_for(command: &'static str, repo_root: Option<&std::path::Path>) -> Envelope {
    let mut env = Envelope::success(command);
    if let Some(r) = repo_root {
        env = env.with_repository(r.display().to_string());
    }
    env
}

// ---------------------------------------------------------------------------
// Clap -> rest-token conversion helpers.
//
// Handlers still consume `rest: &[String]` via their internal `parse_flags`.
// These helpers rebuild the `--flag value` token sequence from the parsed
// clap structs so the handlers work unchanged.
// ---------------------------------------------------------------------------

use args::*;

fn push_opt(out: &mut Vec<String>, flag: &str, val: Option<&str>) {
    if let Some(v) = val {
        out.push(format!("--{flag}"));
        out.push(v.to_string());
    }
}

fn push_bool(out: &mut Vec<String>, flag: &str, on: bool) {
    if on {
        out.push(format!("--{flag}"));
    }
}

fn doctor_tokens(agents: Option<&str>) -> Vec<String> {
    let mut v = Vec::new();
    push_opt(&mut v, "agents", agents);
    v
}

fn setup_tokens(agents: Option<&str>, yes: bool) -> Vec<String> {
    let mut v = Vec::new();
    push_opt(&mut v, "agents", agents);
    push_bool(&mut v, "yes", yes);
    v
}

fn yes_tokens(yes: bool) -> Vec<String> {
    let mut v = Vec::new();
    push_bool(&mut v, "yes", yes);
    v
}

fn workspace_route(a: &WorkspaceCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        WorkspaceCmd::Open => ("workspace", "open", vec![], "workspace.open"),
        WorkspaceCmd::Status => ("workspace", "status", vec![], "workspace.status"),
        WorkspaceCmd::Close => ("workspace", "close", vec![], "workspace.close"),
    }
}

fn graph_route(a: &GraphCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        GraphCmd::Validate => ("graph", "validate", vec![], "graph.validate"),
        GraphCmd::Render => ("graph", "render", vec![], "graph.render"),
        GraphCmd::RepairProvenance { expected_revision } => (
            "graph",
            "repair-provenance",
            vec![
                "--expected-revision".to_string(),
                expected_revision.to_string(),
            ],
            "graph.repair-provenance",
        ),
        GraphCmd::Status => ("graph", "status", vec![], "graph.status"),
        GraphCmd::Ready => ("graph", "ready", vec![], "graph.ready"),
        GraphCmd::Wave => ("graph", "wave", vec![], "graph.wave"),
        GraphCmd::Show { plan } => {
            let mut v = Vec::new();
            push_opt(&mut v, "plan", plan.as_deref());
            ("graph", "show", v, "graph.show")
        }
    }
}

fn plan_route(a: &PlanCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        PlanCmd::Add(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            push_opt(&mut v, "path", Some(&a.path));
            push_opt(&mut v, "title", Some(&a.title));
            for r in &a.design_refs {
                v.push("--design-ref".to_string());
                v.push(r.clone());
            }
            for w in &a.write {
                v.push("--write".to_string());
                v.push(w.clone());
            }
            for h in &a.hard {
                v.push("--hard".to_string());
                v.push(h.clone());
            }
            ("plan", "add", v, "plan.add")
        }
        PlanCmd::Show(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            ("plan", "show", v, "plan.show")
        }
        PlanCmd::Prepare(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            ("plan", "prepare", v, "plan.prepare")
        }
        PlanCmd::Start(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            push_opt(&mut v, "owner", a.owner.as_deref());
            push_opt(&mut v, "run-id", a.run_id.as_deref());
            ("plan", "start", v, "plan.start")
        }
        PlanCmd::Implemented(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            push_opt(&mut v, "report", Some(&a.report));
            for c in &a.commit {
                v.push("--commit".to_string());
                v.push(c.clone());
            }
            ("plan", "implemented", v, "plan.implemented")
        }
        PlanCmd::Accept(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            push_opt(&mut v, "review", a.review.as_deref());
            ("plan", "accept", v, "plan.accept")
        }
        PlanCmd::Reject(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            push_opt(&mut v, "compensating-plan", a.compensating_plan.as_deref());
            push_opt(&mut v, "reason", a.reason.as_deref());
            push_opt(&mut v, "review-report", a.review_report.as_deref());
            ("plan", "reject", v, "plan.reject")
        }
        PlanCmd::Release(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            ("plan", "release", v, "plan.release")
        }
        PlanCmd::RewireCompensation(a) => {
            let mut v = Vec::new();
            push_opt(&mut v, "id", Some(&a.id));
            ("plan", "rewire-compensation", v, "plan.rewire-compensation")
        }
    }
}

fn design_route(a: &DesignCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        DesignCmd::Backup => ("design", "backup", vec![], "design.backup"),
        DesignCmd::Validate => ("design", "validate", vec![], "design.validate"),
        DesignCmd::Status => ("design", "status", vec![], "design.status"),
        DesignCmd::Readiness { scope } => {
            let mut v = Vec::new();
            push_opt(&mut v, "scope", scope.as_deref());
            ("design", "readiness", v, "design.readiness")
        }
    }
}

fn adr_route(a: &AdrCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        AdrCmd::Add(a) => {
            let mut v = vec![
                "--id".to_string(),
                a.id.clone(),
                "--path".to_string(),
                a.path.clone(),
                "--title".to_string(),
                a.title.clone(),
            ];
            v.push("--planning-blocking".to_string());
            v.push(a.planning_blocking.to_string());
            for scope in &a.planning_scopes {
                v.push("--scope".to_string());
                v.push(scope.clone());
            }
            ("adr", "add", v, "adr.add")
        }
        AdrCmd::Show(a) => (
            "adr",
            "show",
            vec!["--id".to_string(), a.id.clone()],
            "adr.show",
        ),
        AdrCmd::List => ("adr", "list", vec![], "adr.list"),
        AdrCmd::Propose(a) => (
            "adr",
            "propose",
            vec!["--id".to_string(), a.id.clone()],
            "adr.propose",
        ),
        AdrCmd::Approve(a) => (
            "adr",
            "approve",
            vec![
                "--id".to_string(),
                a.id.clone(),
                "--by".to_string(),
                a.by.clone(),
                "--note".to_string(),
                a.note.clone(),
            ],
            "adr.approve",
        ),
        AdrCmd::Reject(a) => (
            "adr",
            "reject",
            vec![
                "--id".to_string(),
                a.id.clone(),
                "--reason".to_string(),
                a.reason.clone(),
            ],
            "adr.reject",
        ),
        AdrCmd::Supersede(a) => (
            "adr",
            "supersede",
            vec![
                "--id".to_string(),
                a.id.clone(),
                "--replacement".to_string(),
                a.replacement.clone(),
            ],
            "adr.supersede",
        ),
        AdrCmd::Validate => ("adr", "validate", vec![], "adr.validate"),
    }
}

fn agent_route(a: &AgentCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        AgentCmd::Install { slug, dry_run } => {
            let mut v = vec![slug.clone()];
            push_bool(&mut v, "dry-run", *dry_run);
            ("agent", "install", v, "agent.install")
        }
        AgentCmd::Uninstall { slug, dry_run } => {
            let mut v = vec![slug.clone()];
            push_bool(&mut v, "dry-run", *dry_run);
            ("agent", "uninstall", v, "agent.uninstall")
        }
        AgentCmd::Status => ("agent", "status", vec![], "agent.status"),
        AgentCmd::Config { slug } => ("agent", "config", vec![slug.clone()], "agent.config"),
    }
}

fn repository_route(a: &RepositoryCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        RepositoryCmd::Version { action } => match action {
            VersionCmd::Show => (
                "repository",
                "version",
                vec!["show".to_string()],
                "repository.version",
            ),
            VersionCmd::Suggest => (
                "repository",
                "version",
                vec!["suggest".to_string()],
                "repository.version",
            ),
            VersionCmd::Set { version } => (
                "repository",
                "version",
                vec!["set".to_string(), "--version".to_string(), version.clone()],
                "repository.version",
            ),
        },
    }
}

fn dist_route(a: &DistCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        DistCmd::Sync => ("dist", "sync", vec![], "dist.sync"),
        DistCmd::Verify => ("dist", "verify", vec![], "dist.verify"),
    }
}

fn mcp_route(a: &McpCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        McpCmd::Serve => ("mcp", "serve", vec![], "mcp.serve"),
    }
}

fn scan_route(a: &ScanCmd) -> (&'static str, &'static str, Vec<String>, &'static str) {
    match a {
        ScanCmd::PlanRefs { check } => {
            let mut v = Vec::new();
            if *check {
                v.push("--check".to_string());
            }
            ("scan", "plan-refs", v, "scan.plan-refs")
        }
    }
}
