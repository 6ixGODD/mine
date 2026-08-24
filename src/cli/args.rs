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
    /// Release preflight (validation only).
    Release,
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
pub enum WorkspaceCmd {
    /// Open the temporary docs/plan/ workspace.
    Open,
    /// Show workspace status.
    Status,
    /// Close and purge the workspace.
    Close,
}

#[derive(Subcommand, Debug)]
pub enum GraphCmd {
    /// Validate the execution graph.
    Validate,
    /// Render the graph to Markdown.
    Render,
    /// Repair supported schema-v1 Plan provenance and upgrade the graph.
    RepairProvenance {
        /// Authoritative graph revision expected by the caller.
        #[arg(long, value_name = "REVISION")]
        expected_revision: u64,
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
    /// Exclusive write path (repeatable).
    #[arg(long, value_name = "PATH")]
    pub write: Vec<String>,
    /// Hard predecessor id (repeatable).
    #[arg(long, value_name = "ID")]
    pub hard: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum PlanCmd {
    /// Register a new DRAFT plan.
    Add(PlanAddArgs),
    /// Show a plan node.
    Show(PlanShowArgs),
    /// Prepare or reuse the registered linked worktree.
    Prepare(PlanShowArgs),
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
}

#[derive(Args, Debug)]
pub struct PlanShowArgs {
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct PlanStartArgs {
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Owner label (default: "default").
    #[arg(long, value_name = "OWNER")]
    pub owner: Option<String>,
    /// Run id (default: "default-run").
    #[arg(long = "run-id", value_name = "ID")]
    pub run_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct PlanImplementedArgs {
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
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
    /// Review report path.
    #[arg(long, value_name = "PATH")]
    pub review: Option<String>,
}

#[derive(Args, Debug)]
pub struct PlanRejectArgs {
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
    /// Plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct PlanRewireCompensationArgs {
    /// Rejected plan id.
    #[arg(long, value_name = "ID")]
    pub id: String,
}

#[derive(Subcommand, Debug)]
pub enum DesignCmd {
    /// Create a timestamped design backup.
    Backup,
    /// Validate the design knowledge base.
    Validate,
    /// Show design status.
    Status,
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
