//! Clap-derived command-line model for `mine`.
//!
//! This module only models the CLI surface (commands, subcommands, flags,
//! and their help text). Parsing produces a [`Cli`] whose [`Cli::command`]
//! field is a [`Commands`] enum. Dispatch converts the parsed structure back
//! into the `ParsedArgs` + rest-token form expected by the existing handlers
//! in [`super::commands`], so handlers and the JSON envelope contract are
//! unchanged.
//!
//! Global options (`--repo`, `--format`, `--quiet`, `--no-color`) are declared
//! on the top-level [`Cli`] struct with `global = true` so they are accepted
//! before or after any subcommand.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use super::context::OutputFormat;

#[derive(Parser, Debug)]
#[command(
    name = "mine",
    version,
    about = "MINE Is Not Everyone's: deterministic engineering-workflow core",
    long_about = None,
    propagate_version = false,
    color = clap::ColorChoice::Auto,
)]
pub struct Cli {
    /// Output format (default: human).
    #[arg(long, global = true, value_name = "FORMAT")]
    pub format: Option<OutputFormatArg>,

    /// Suppress non-error human output.
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,

    /// Plain text (default; accepted for compatibility).
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Repository root override.
    #[arg(long, global = true, value_name = "PATH")]
    pub repo: Option<PathBuf>,

    /// Isolated config root (CI/tests; do not touch the real environment).
    #[arg(long, global = true, value_name = "PATH")]
    pub config_root: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

/// Clap-compatible mirror of [`OutputFormat`].
#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OutputFormatArg {
    Human,
    Json,
}

impl From<OutputFormatArg> for OutputFormat {
    fn from(value: OutputFormatArg) -> Self {
        match value {
            OutputFormatArg::Human => OutputFormat::Human,
            OutputFormatArg::Json => OutputFormat::Json,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a repository for MINE.
    Init,
    /// Show repository and execution-graph status.
    Status,
    /// Diagnose MINE configuration.
    Doctor {
        /// Restrict diagnostics to one agent slug, or "all".
        #[arg(long, value_name = "SLUG")]
        agents: Option<String>,
    },
    /// Install MINE into coding agents (interactive).
    Setup {
        /// Non-interactive agent list, e.g. "claude-code,codex".
        #[arg(long, value_name = "LIST")]
        agents: Option<String>,
        /// Skip interactive prompts.
        #[arg(long = "yes", short = 'y')]
        yes: bool,
    },
    /// Update the mine binary to the latest release.
    Update {
        /// Skip the confirmation prompt.
        #[arg(long = "yes", short = 'y')]
        yes: bool,
    },
    /// Remove MINE from all agents and this machine.
    Uninstall {
        /// Skip the confirmation prompt.
        #[arg(long = "yes", short = 'y')]
        yes: bool,
    },
    /// Workspace lifecycle.
    Workspace {
        #[command(subcommand)]
        action: WorkspaceCmd,
    },
    /// Execution graph queries.
    Graph {
        #[command(subcommand)]
        action: GraphCmd,
    },
    /// Plan lifecycle.
    Plan {
        #[command(subcommand)]
        action: PlanCmd,
    },
    /// Inspect a recoverable lifecycle operation (CLI only).
    Operation {
        #[command(subcommand)]
        action: OperationCmd,
    },
    /// Capture, record or check native verification evidence (never runs checks).
    Verification {
        #[command(subcommand)]
        action: VerificationCmd,
    },
    /// Design backup / validation / status.
    Design {
        #[command(subcommand)]
        action: DesignCmd,
    },
    /// ADR registry lifecycle.
    Adr {
        #[command(subcommand)]
        action: AdrCmd,
    },
    /// Agent installer management.
    Agent {
        #[command(subcommand)]
        action: AgentCmd,
    },
    /// Repository version management.
    Repository {
        #[command(subcommand)]
        action: RepositoryCmd,
    },
    /// Distribution asset sync / verify.
    Dist {
        #[command(subcommand)]
        action: DistCmd,
    },
    /// Live release preflight or explicitly selected read-only phase diagnostics.
    Release {
        /// Existing Release operation; diagnostic only, never current gate admission.
        #[arg(long, value_name = "UUID")]
        operation_id: Option<String>,
    },
    /// Read-only validation scans.
    Scan {
        #[command(subcommand)]
        action: ScanCmd,
    },
    /// Run the MINE stdio MCP server.
    Mcp {
        #[command(subcommand)]
        action: McpCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum OperationCmd {
    /// Finalize independently verified/cleaned acceptance through its checkpoint.
    Finalize {
        #[arg(long, value_name = "REVISION")]
        expected_revision: u64,
        #[arg(long, value_name = "PATH")]
        input: String,
    },
    /// Record one objectively observed typed stage; never performs its effects.
    Stage {
        #[arg(long, value_name = "REVISION")]
        expected_revision: u64,
        #[arg(long, value_name = "PATH")]
        input: String,
    },
    /// Begin a pinned independent-review closure; never merges or runs checks.
    Begin {
        #[arg(long, value_name = "REVISION")]
        expected_revision: u64,
        /// Root-contained strict JSON actor and typed closure intent.
        #[arg(long, value_name = "PATH")]
        input: String,
    },
    /// Query one exact operation; never repeats its effects.
    Show {
        #[arg(long, value_name = "UUID")]
        operation_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum VerificationCmd {
    /// Capture real input identities before the runner executes its check.
    Capture {
        /// Root-relative `.mine/build/<exact-plan-id>/<output-class>/<file>`.
        #[arg(long, value_name = "PATH")]
        input: String,
    },
    /// Record a previously captured, externally observed run.
    Record {
        /// Root-relative `.mine/build/<exact-plan-id>/<output-class>/<file>`.
        #[arg(long, value_name = "PATH")]
        input: String,
    },
    /// Check current applicability; success output is not necessarily a pass.
    Check {
        #[arg(long, value_name = "PATH")]
        receipt: String,
        /// Root-relative `.mine/build/<exact-plan-id>/<output-class>/<file>`.
        #[arg(long, value_name = "PATH")]
        input: String,
        #[arg(long, value_name = "PATH")]
        policy: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum WorkspaceCmd {
    /// Open the configured temporary Plan workspace (fresh default: .mine/plan/).
    Open {
        #[arg(long, value_name = "ACTOR")]
        actor: String,
        #[arg(long, value_name = "REVISION")]
        expected_revision: u64,
    },
    /// Show workspace status.
    Status,
    /// Close and purge the workspace.
    Close,
}

#[derive(Subcommand, Debug)]
pub enum GraphCmd {
    /// Validate the execution graph.
    Validate,
    /// Restore only a missing checkpoint-proven Markdown graph view.
    Render {
        #[arg(long, value_name = "REVISION")]
        expected_revision: u64,
    },
    /// Repair supported schema-v1 Plan provenance and upgrade the graph.
    RepairProvenance {
        #[arg(long, value_name = "ACTOR")]
        actor: String,
        /// Authoritative graph revision expected by the caller.
        #[arg(long, value_name = "REVISION")]
        expected_revision: u64,
    },
    /// Repair one invalid, unstarted compensation predecessor list.
    RepairCompensationPredecessors {
        #[arg(long, value_name = "ACTOR")]
        actor: String,
        /// Canonical compensation Plan ID.
        #[arg(long, value_name = "ID")]
        id: String,
        /// Authoritative graph revision expected by the caller.
        #[arg(long, value_name = "REVISION")]
        expected_revision: u64,
        /// Non-empty operator reason returned as repair evidence.
        #[arg(long, value_name = "TEXT")]
        reason: String,
    },
    /// Show graph status (revision, branches, plan count).
    Status,
    /// Show the ready frontier.
    Ready,
    /// Show the next executable wave.
    Wave,
    /// Show a plan node or the whole graph.
    Show {
        /// Plan id to show (omit for the whole graph).
        #[arg(long, value_name = "ID")]
        plan: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct PlanAddArgs {
    /// Explicit initiating actor, not Git identity or stored owner.
    #[arg(long, value_name = "ACTOR")]
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Contained JSON semantic declarations and policy/ADR selectors.
    #[arg(long, value_name = "PATH")]
    pub ownership_input: Option<String>,
    /// Plan id (e.g. "01").
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Path to the plan document.
    #[arg(long, value_name = "PATH")]
    pub path: String,
    /// Plan title.
    #[arg(long, value_name = "TITLE")]
    pub title: String,
    /// Design reference (repeatable).
    #[arg(long = "design-ref", value_name = "REF")]
    pub design_refs: Vec<String>,
    /// Exclusive write path (repeatable). Conservative registration requires a
    /// nonempty mapping; a verified semantic input may supply it instead.
    #[arg(long, value_name = "PATH")]
    pub write: Vec<String>,
    /// Hard predecessor id (repeatable).
    #[arg(long, value_name = "ID")]
    pub hard: Vec<String>,
    /// Planning scope recorded on the node (default: the design root).
    #[arg(long = "planning-scope", value_name = "SCOPE")]
    pub planning_scope: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum PlanCmd {
    /// Register a new DRAFT plan.
    Add(PlanAddArgs),
    /// Append an approved, body-bound ownership clarification (CLI only).
    Clarify(PlanClarifyArgs),
    /// Show a plan node.
    Show(PlanShowArgs),
    /// Prepare or reuse the registered linked worktree.
    Prepare(PlanPrepareArgs),
    /// Start a plan (DRAFT/READY -> IN_PROGRESS).
    Start(PlanStartArgs),
    /// Mark a plan IMPLEMENTED.
    Implemented(PlanImplementedArgs),
    /// Accept a plan.
    Accept(PlanAcceptArgs),
    /// Reject a plan with independent review evidence and a compensation.
    Reject(PlanRejectArgs),
    /// Release a DRAFT plan into the startable frontier.
    Release(PlanReleaseArgs),
    /// Rewire downstream dependencies from a rejected plan onto its compensating plan.
    RewireCompensation(PlanRewireCompensationArgs),
    /// Withdraw an unstarted plan into terminal WITHDRAWN.
    Withdraw(PlanWithdrawArgs),
}

#[derive(Args, Debug)]
pub struct PlanShowArgs {
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct PlanClarifyArgs {
    #[arg(long, value_name = "ID")]
    pub id: String,
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Contained typed JSON declaration/identity/actor/reason/mapping input.
    #[arg(long, value_name = "PATH")]
    pub input: String,
}

#[derive(Args, Debug)]
pub struct PlanPrepareArgs {
    /// Exact registered Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Original observed graph revision; never silently refreshed.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Identity retained across interrupted preparation and checkpoint recovery.
    #[arg(long, value_name = "UUID")]
    pub operation_id: String,
    /// Explicit attributable caller; not Git identity or a future start owner.
    #[arg(long, value_name = "ACTOR")]
    pub actor: String,
    /// Explicit safe READY baseline/policy refresh; never resets divergent work.
    #[arg(long, conflicts_with = "recover_preparation")]
    pub reprepare: bool,
    /// Establish missing legacy preparation from fresh exact initial evidence.
    #[arg(long, conflicts_with = "reprepare")]
    pub recover_preparation: bool,
}

#[derive(Args, Debug)]
pub struct PlanStartArgs {
    /// Graph revision observed by the caller, never refreshed internally.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Explicit owner label and initiating actor; no inferred default.
    #[arg(long, value_name = "OWNER")]
    pub owner: String,
    /// Run id (default: "default-run").
    #[arg(long = "run-id", value_name = "ID")]
    pub run_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct PlanImplementedArgs {
    /// Explicit initiating actor, not Git identity or stored owner.
    #[arg(long, value_name = "ACTOR")]
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Implementation report path.
    #[arg(long, value_name = "PATH")]
    pub report: String,
    /// Commit hash (repeatable; at least one required).
    #[arg(long, value_name = "HASH")]
    pub commit: Vec<String>,
}

#[derive(Args, Debug)]
pub struct PlanAcceptArgs {
    /// Graph revision observed by the caller, never refreshed internally.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Existing independently reviewed closure operation.
    #[arg(long, value_name = "UUID")]
    pub operation_id: String,
    /// Root-contained typed finalize request and current check selections.
    #[arg(long, value_name = "PATH")]
    pub input: String,
}

#[derive(Args, Debug)]
pub struct PlanRejectArgs {
    /// Explicit initiating actor, not Git identity or stored owner.
    #[arg(long, value_name = "ACTOR")]
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Compensating plan id.
    #[arg(long, value_name = "ID")]
    pub compensating_plan: Option<String>,
    /// Rejection reason.
    #[arg(long, value_name = "REASON")]
    pub reason: Option<String>,
    /// Canonical independent review report path.
    #[arg(long = "review-report", value_name = "PATH")]
    pub review_report: Option<String>,
}

#[derive(Args, Debug)]
pub struct PlanReleaseArgs {
    /// Explicit initiating actor, not Git identity or stored owner.
    #[arg(long, value_name = "ACTOR")]
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct PlanRewireCompensationArgs {
    /// Explicit initiating actor, not Git identity or stored owner.
    #[arg(long, value_name = "ACTOR")]
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Rejected plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct PlanWithdrawArgs {
    /// Exact operation identity, retained across an interrupted withdrawal.
    #[arg(long, value_name = "UUID")]
    pub operation_id: String,
    /// Attributable initiating actor; no inferred identity.
    #[arg(long, value_name = "ACTOR")]
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: u64,
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Non-empty withdrawal reason.
    #[arg(long, value_name = "REASON")]
    pub reason: String,
}

#[derive(Subcommand, Debug)]
pub enum DesignCmd {
    /// Create a timestamped design backup.
    Backup,
    /// Validate the design knowledge base.
    Validate,
    /// Show design status.
    Status,
    /// Deterministically regenerate the generated ADR decision index.
    Render,
    /// Check scoped ADR and Design readiness.
    Readiness {
        #[arg(long, value_name = "SCOPE")]
        scope: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum AdrCmd {
    /// Register a DRAFT ADR body.
    Add(AdrAddArgs),
    /// Show one ADR.
    Show(AdrShowArgs),
    /// List all registry entries.
    List,
    /// Freeze a DRAFT ADR with its body digest.
    Propose(AdrIdArgs),
    /// Record explicit owner approval for a proposed digest.
    Approve(AdrApproveArgs),
    /// Reject a proposed ADR.
    Reject(AdrRejectArgs),
    /// Supersede an approved ADR with an approved replacement.
    Supersede(AdrSupersedeArgs),
    /// Validate registry structure and body digests.
    Validate,
}

#[derive(Args, Debug)]
pub struct AdrAddArgs {
    #[arg(long, value_name = "ID")]
    pub id: String,
    #[arg(long, value_name = "PATH")]
    pub path: String,
    #[arg(long, value_name = "TITLE")]
    pub title: String,
    #[arg(long, default_value_t = true)]
    pub planning_blocking: bool,
    #[arg(long = "scope", value_name = "SCOPE")]
    pub planning_scopes: Vec<String>,
}

#[derive(Args, Debug)]
pub struct AdrShowArgs {
    #[arg(long, value_name = "ID")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct AdrIdArgs {
    #[arg(long, value_name = "ID")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct AdrApproveArgs {
    #[arg(long, value_name = "ID")]
    pub id: String,
    #[arg(long, value_name = "ACTOR")]
    pub by: String,
    #[arg(long, value_name = "NOTE")]
    pub note: String,
}

#[derive(Args, Debug)]
pub struct AdrRejectArgs {
    #[arg(long, value_name = "ID")]
    pub id: String,
    #[arg(long, value_name = "REASON")]
    pub reason: String,
}

#[derive(Args, Debug)]
pub struct AdrSupersedeArgs {
    #[arg(long, value_name = "ID")]
    pub id: String,
    #[arg(long, value_name = "ID")]
    pub replacement: String,
    /// Preview post-supersession readiness without writing anything.
    #[arg(long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Subcommand, Debug)]
pub enum AgentCmd {
    /// Install/upgrade MINE for an agent.
    Install {
        /// Agent slug: claude-code | codex | pi | opencode.
        slug: String,
        /// Dry run (no mutation).
        #[arg(long = "dry-run", short = 'd')]
        dry_run: bool,
    },
    /// Uninstall MINE for an agent.
    Uninstall {
        /// Agent slug.
        slug: String,
        /// Dry run.
        #[arg(long = "dry-run", short = 'd')]
        dry_run: bool,
    },
    /// List MINE-managed agent installations.
    Status,
    /// Preview the MCP entry MINE would merge for an agent.
    Config {
        /// Agent slug.
        slug: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum RepositoryCmd {
    /// Repository version subcommands.
    Version {
        #[command(subcommand)]
        action: VersionCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum VersionCmd {
    /// Show the resolved repository version.
    Show,
    /// Suggest the next version.
    Suggest,
    /// Set the repository version.
    Set {
        /// The version to set.
        #[arg(long, value_name = "VERSION")]
        version: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum DistCmd {
    /// Synchronize root skills/ into plugins/mine/skills/.
    Sync,
    /// Verify distribution assets are in sync.
    Verify,
}

#[derive(Subcommand, Debug)]
pub enum ScanCmd {
    /// Scan tracked content for temporary historical Plan references.
    PlanRefs {
        /// Fail (exit 1) when unexempted findings exist (release gate).
        #[arg(long)]
        check: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum McpCmd {
    /// Serve the MINE MCP server over stdio.
    Serve,
}
