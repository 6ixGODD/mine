//! Setup-only repository initialization service.
//!
//! Implements the deterministic `mine init` behavior defined by the CLI
//! contract (`docs/design/interfaces/cli-contract.md`):
//!
//! - discover the repository root and record the stable branch;
//! - initialize or validate `.mine/config.toml`;
//! - create a repository UUID when unmanaged, preserving existing managed
//!   values;
//! - create the `docs/design/` scaffold and `.mine-design.toml` when absent;
//! - auto-back-up an unmarked or foreign-owned existing `docs/design/`;
//! - create the MINE section in `AGENTS.md` without erasing unrelated content;
//! - initialize the repository version from existing MINE state, reliable root
//!   version evidence, or `0.1.0`.
//!
//! The service performs **no** source scan, architecture generation, plan
//! creation, agent invocation, business-code change, branch mutation, commit,
//! merge, or release. File writes use `std::fs` directly; workspace lifecycle is a
//! the atomic-write and file-lock infrastructure for the execution-graph paths
//! it will own, at which point this service is refactored to use them.
//!
//! Real stable-branch discovery via Git is wired with the Git infrastructure in
//! separate concern. The MINE-managed default for this repository is recorded
//! (`master`) and preserves any stable branch already recorded in an existing
//! configuration.

use std::path::{Path, PathBuf};

use crate::domain::config::{
    AdrConfig, BranchesConfig, DesignConfig, GraphConfig, MineConfig, PlanConfig,
};
use crate::domain::design_marker::{
    DESIGN_MARKER_FILE, DESIGN_ROOT_INDEX, DesignMarker, DesignRootState, classify,
};
use crate::domain::error::{MineError, MineResult};
use crate::domain::ports::{Clock, UuidSource};
use crate::domain::repository_identity::{RepositoryIdentity, root_version_from_cargo_manifest};
use crate::infrastructure::adr_store::AdrStore;
use crate::infrastructure::git;

/// Default managed integration branch.
const DEFAULT_INTEGRATION_BRANCH: &str = "dev";

/// Marker comment embedded in `AGENTS.md` so initialization is idempotent and
/// never erases existing agent agreements.
const AGENTS_MINE_MARKER: &str = "<!-- mine-managed-agents -->";

/// `.mine/.gitignore` content: MINE runtime, locks, and mutable build outputs
/// are local diagnostics, not fact sources, and must not be committed.
const MINE_GITIGNORE: &str = "# MINE runtime, locks, and mutable build outputs are local diagnostics, not fact sources.\nruntime/\nlocks/\nbuild/\n";

/// Repository-relative design root index path.
const DESIGN_ROOT_RELATIVE: &str = "docs/design/index.md";
/// Repository-relative design marker path.
const DESIGN_MARKER_RELATIVE: &str = "docs/design/.mine-design.toml";

/// Minimal `AGENTS.md` written only when `AGENTS.md` is absent. The full
/// durable governance is authored by the `mine-arch` skill.
const AGENTS_MINE_STUB: &str = "<!-- mine-managed-agents -->\n# Agent Working Agreement\n\nThis repository is managed by MINE. The durable governance, source-of-truth\npaths, quality gates, and branch authorization are authored by the `mine-arch`\nskill and recorded here.\n\n- Design root: `docs/design/index.md`\n- Execution graph machine source: `docs/plan/execution-graph.toml`\n- Execution graph generated view: `docs/plan/execution-graph.md`\n- All execution-graph state transitions must go through the `mine` MCP tools or\n  `mine --format json` CLI. Never edit either graph file directly.\n\nRun `mine init` to validate this configuration. Run `mine-arch` to expand\nrepository governance.\n";

/// MINE section appended to an existing `AGENTS.md` that lacks the MINE marker.
const AGENTS_MINE_SECTION: &str = "\n<!-- mine-managed-agents -->\n## MINE governance\n\nThis repository is managed by MINE. See `docs/design/index.md` for the design\nroot and `docs/plan/execution-graph.toml` for the execution graph machine\nsource. All execution-graph state transitions must go through the `mine` MCP\ntools or `mine --format json` CLI; never edit either graph file directly.\n";

/// Minimal design root index written when `docs/design/` is absent.
const DESIGN_INDEX_STUB: &str = "<!-- mine-managed-design -->\n# Design Knowledge Base\n\nThis design root is owned by MINE. The marker `docs/design/.mine-design.toml`\nrecords repository ownership. Use the `mine-arch` skill to author the modular\ntarget design and `mine-sync` to reconcile it with repository reality.\n";

/// Summary of the classified design root in an [`InitOutcome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesignRootSummary {
    /// The design root was absent and has been created.
    Absent,
    /// The design root was already MINE-managed and has been preserved.
    Managed,
}

/// A single action taken by `mine init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitAction {
    /// A file was created.
    Created(PathBuf),
    /// An existing file was validated and preserved unchanged.
    Preserved(PathBuf),
    /// A MINE section was appended to an existing file.
    CreatedSection(PathBuf),
    /// A non-MINE docs/design/ was moved to a timestamped backup before
    /// creating a fresh MINE-managed design root.
    BackedUpDesign { backup_path: PathBuf },
    /// A stale `branches.stable` recorded by an older MINE version was
    /// repaired to the actually detected stable branch.
    RepairedStableBranch {
        path: PathBuf,
        from: String,
        to: String,
    },
}

/// The structured outcome of `mine init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitOutcome {
    /// Resolved repository identifier.
    pub repository_id: String,
    /// Resolved MINE code-repository version.
    pub mine_code_version: String,
    /// Summary of the design root state after initialization.
    pub design_root: DesignRootSummary,
    /// Actions taken, in order.
    pub actions: Vec<InitAction>,
}

/// The setup-only initialization service.
///
/// Constructed with injected [`UuidSource`] and [`Clock`] ports so that
/// behavior is deterministic and unit-testable. Use
/// [`crate::infrastructure::system::SystemUuidSource`] and
/// [`crate::infrastructure::system::SystemClock`] for production.
pub struct InitService<'a> {
    uuid_source: &'a dyn UuidSource,
    clock: &'a dyn Clock,
}

impl<'a> InitService<'a> {
    /// Creates a new initialization service with the given ports.
    #[must_use]
    pub fn new(uuid_source: &'a dyn UuidSource, clock: &'a dyn Clock) -> Self {
        Self { uuid_source, clock }
    }

    /// Runs setup-only initialization at `repo_root`.
    ///
    /// `stable_branch` is the detected stable branch (e.g. `main`, `master`,
    /// or a custom configured branch). It is persisted into
    /// `.mine/config.toml` under `[branches].stable` so all downstream
    /// services read the actual branch rather than a hardcoded default.
    ///
    /// # Errors
    /// - [`MineError::DesignOwnershipMismatch`] for a marker belonging to
    ///   another repository.
    /// - [`MineError::DesignMarkerInvalid`] for a malformed marker.
    /// - [`MineError::ConfigInvalid`] for an unparseable or incomplete existing
    ///   `.mine/config.toml`.
    /// - [`MineError::Io`] for filesystem failures.
    pub fn initialize(&self, repo_root: &Path, stable_branch: &str) -> MineResult<InitOutcome> {
        let mine_dir = repo_root.join(".mine");
        let config_path = mine_dir.join("config.toml");
        let design_dir = repo_root.join("docs").join("design");
        let marker_path = design_dir.join(DESIGN_MARKER_FILE);
        let cargo_path = repo_root.join("Cargo.toml");

        let mut actions: Vec<InitAction> = Vec::new();

        // Existing configuration identity, to preserve managed values.
        let existing_identity = read_existing_identity(&config_path)?;

        // Reliable root-version evidence from Cargo.toml, if present.
        let root_version = read_root_version(&cargo_path);

        // Classify the design namespace.
        let design_dir_exists = design_dir.exists();
        let marker_file_exists = marker_path.exists();
        let marker = if marker_file_exists {
            let content = std::fs::read_to_string(&marker_path)?;
            Some(DesignMarker::parse(&marker_path, &content)?)
        } else {
            None
        };

        let state = match classify(
            &design_dir,
            &marker_path,
            design_dir_exists,
            marker_file_exists,
            marker.as_ref(),
            existing_identity.as_ref().map(|i| i.repository_id.as_str()),
        ) {
            Ok(s) => s,
            Err(MineError::DesignNamespaceConflict { .. }) => {
                // The user asked for init to back up an existing non-MINE
                // docs/design/ rather than abort. Move it aside to a
                // timestamped backup, then proceed as if absent.
                let backup = backup_conflicting_design(&design_dir, self.clock)?;
                actions.push(InitAction::BackedUpDesign {
                    backup_path: backup,
                });
                // Re-classify: design_dir no longer exists.
                DesignRootState::Absent
            }
            Err(e) => return Err(e),
        };

        let marker_repository_id = match state {
            DesignRootState::Absent => {
                // Prefer an existing recorded identity; otherwise generate one.
                let id = existing_identity
                    .as_ref()
                    .map(|i| i.repository_id.clone())
                    .unwrap_or_else(|| self.uuid_source.new_repository_id());
                create_design_scaffold(&design_dir)?;
                let new_marker = DesignMarker::new(id.clone(), self.clock.now_utc_rfc3339());
                write_marker(&marker_path, &new_marker)?;
                actions.push(InitAction::Created(marker_path.clone()));
                id
            }
            DesignRootState::Managed(_) => {
                // Preserve the existing marker unchanged.
                actions.push(InitAction::Preserved(marker_path.clone()));
                marker
                    .expect("managed state implies a parsed marker")
                    .repository_id
                    .clone()
            }
        };

        // Resolve full identity, preserving version from config or root evidence.
        let identity = RepositoryIdentity::resolve(
            Some(&marker_repository_id),
            existing_identity.as_ref(),
            self.uuid_source,
            root_version.as_deref(),
        );

        // Initialize or validate configuration (idempotent: never rewrite an
        // existing valid configuration).
        if config_path.exists() {
            // The existing configuration was already parsed by
            // `read_existing_identity`; reaching here means it is valid. The
            // resolved identity preserves its recorded values, so no rewrite is
            // needed -- EXCEPT for a stale `branches.stable` recorded by an
            // older MINE version (e.g. `master`) that does not exist in this
            // repository while a different branch is detected. Silently
            // preserving that value would make doctor/workspace/release
            // consume a wrong branch forever. Repair it explicitly and record
            // the action; never guess when the recorded branch exists.
            let existing_config = read_existing_config(&config_path)?;
            let recorded_stable = existing_config.as_ref().map(|c| c.branches.stable.clone());
            let stale = match (&recorded_stable, stable_branch) {
                (Some(recorded), detected) if recorded != detected => {
                    // Only repair when the recorded branch provably does not
                    // exist in git. If it exists (even if we are on another
                    // branch right now), it may be intentional and is kept.
                    let recorded_exists = git::branch_exists(repo_root, recorded);
                    let detected_exists = git::branch_exists(repo_root, detected);
                    !recorded_exists && detected_exists
                }
                _ => false,
            };
            if stale {
                let mut config = existing_config.expect("stale implies parsed config");
                config.branches.stable = stable_branch.to_string();
                let content = toml::to_string(&config).map_err(|e| MineError::ConfigInvalid {
                    path: config_path.clone(),
                    detail: format!("serialization failed: {e}"),
                })?;
                std::fs::write(&config_path, content)?;
                actions.push(InitAction::RepairedStableBranch {
                    path: config_path.clone(),
                    from: recorded_stable.expect("stale implies recorded"),
                    to: stable_branch.to_string(),
                });
            } else {
                actions.push(InitAction::Preserved(config_path.clone()));
            }
        } else {
            std::fs::create_dir_all(&mine_dir)?;
            let config = build_default_config(&identity, stable_branch);
            let content = toml::to_string(&config).map_err(|e| MineError::ConfigInvalid {
                path: config_path.clone(),
                detail: format!("serialization failed: {e}"),
            })?;
            std::fs::write(&config_path, content)?;
            actions.push(InitAction::Created(config_path.clone()));
        }

        ensure_mine_gitignore(&mine_dir)?;
        actions.push(ensure_agents_md(repo_root)?);

        // The current MINE repository carries ADR-0001..0009 as its
        // pre-registry bootstrap evidence. Import them exactly once when all
        // nine bodies are present. Ordinary user repositories without that
        // bootstrap set are left untouched until they create ADRs normally.
        if has_complete_adr_bootstrap(repo_root) {
            let config = read_existing_config(&config_path)?.expect("config exists after init");
            let _ = AdrStore::new(repo_root, &config.adr)
                .bootstrap_if_absent(&self.clock.now_utc_rfc3339())?;
        }

        Ok(InitOutcome {
            repository_id: identity.repository_id,
            mine_code_version: identity.mine_code_version,
            design_root: if matches!(state, DesignRootState::Absent) {
                DesignRootSummary::Absent
            } else {
                DesignRootSummary::Managed
            },
            actions,
        })
    }
}

fn has_complete_adr_bootstrap(repo_root: &Path) -> bool {
    let decisions = repo_root.join("docs/design/decisions");
    (1..=9).all(|number| {
        let prefix = format!("{number:04}-");
        std::fs::read_dir(&decisions)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".md"))
            })
    })
}

fn read_existing_identity(config_path: &Path) -> MineResult<Option<RepositoryIdentity>> {
    if !config_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(config_path)?;
    let config: MineConfig = toml::from_str(&content).map_err(|e| MineError::ConfigInvalid {
        path: config_path.to_path_buf(),
        detail: e.to_string(),
    })?;
    Ok(Some(RepositoryIdentity {
        repository_id: config.repository_id,
        mine_code_version: config.mine_code_version,
    }))
}

/// Reads the full existing configuration (including branch settings), if any.
fn read_existing_config(config_path: &Path) -> MineResult<Option<MineConfig>> {
    if !config_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(config_path)?;
    let config: MineConfig = toml::from_str(&content).map_err(|e| MineError::ConfigInvalid {
        path: config_path.to_path_buf(),
        detail: e.to_string(),
    })?;
    Ok(Some(config))
}

fn read_root_version(cargo_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(cargo_path).ok()?;
    root_version_from_cargo_manifest(&content)
}

/// Moves an existing non-MINE `docs/design/` aside to a timestamped backup
/// so initialization can create a fresh MINE-managed design root.
///
/// The backup destination is `docs/design-backup-<UTC timestamp>/` next to
/// the design directory. If a name collision occurs, a numeric suffix is
/// appended. The whole directory is moved (renamed) to preserve all contents
/// atomically.
fn backup_conflicting_design(design_dir: &Path, clock: &dyn Clock) -> MineResult<PathBuf> {
    let parent = design_dir
        .parent()
        .ok_or_else(|| std::io::Error::other("design directory has no parent"))?;
    let timestamp = clock.now_utc_rfc3339();
    // RFC3339 contains ':' which is invalid in Windows filenames; sanitize.
    let safe_ts = timestamp.replace(':', "-");
    let mut backup = parent.join(format!("design-backup-{safe_ts}"));
    let mut suffix = 1;
    while backup.exists() {
        backup = parent.join(format!("design-backup-{safe_ts}-{suffix}"));
        suffix += 1;
    }
    std::fs::rename(design_dir, &backup).map_err(MineError::Io)?;
    Ok(backup)
}

fn create_design_scaffold(design_dir: &Path) -> MineResult<()> {
    std::fs::create_dir_all(design_dir)?;
    let index_path = design_dir.join(DESIGN_ROOT_INDEX);
    if !index_path.exists() {
        std::fs::write(&index_path, DESIGN_INDEX_STUB)?;
    }
    Ok(())
}

fn write_marker(path: &Path, marker: &DesignMarker) -> MineResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, marker.to_toml())?;
    Ok(())
}

fn build_default_config(identity: &RepositoryIdentity, stable_branch: &str) -> MineConfig {
    MineConfig {
        schema_version: 1,
        repository_id: identity.repository_id.clone(),
        mine_code_version: identity.mine_code_version.clone(),
        branches: BranchesConfig {
            stable: stable_branch.to_string(),
            integration: DEFAULT_INTEGRATION_BRANCH.to_string(),
        },
        design: DesignConfig {
            root: DESIGN_ROOT_RELATIVE.to_string(),
            marker: DESIGN_MARKER_RELATIVE.to_string(),
            language: "en".to_string(),
            index_soft_limit_lines: 250,
            leaf_soft_limit_lines: 400,
        },
        plan: PlanConfig {
            root: "docs/plan".to_string(),
            ephemeral: true,
            purge_before_stable_release: true,
            worktrees_root: ".mine/worktrees".to_string(),
            shared_environment_root: ".".to_string(),
        },
        graph: GraphConfig {
            source: "docs/plan/execution-graph.toml".to_string(),
            rendered: "docs/plan/execution-graph.md".to_string(),
            lock_timeout_ms: 5000,
        },
        adr: AdrConfig::default(),
    }
}

fn ensure_mine_gitignore(mine_dir: &Path) -> MineResult<()> {
    let path = mine_dir.join(".gitignore");
    if !path.exists() {
        std::fs::create_dir_all(mine_dir)?;
        std::fs::write(&path, MINE_GITIGNORE)?;
    }
    Ok(())
}

fn ensure_agents_md(repo_root: &Path) -> MineResult<InitAction> {
    let path = repo_root.join("AGENTS.md");
    if !path.exists() {
        std::fs::write(&path, AGENTS_MINE_STUB)?;
        return Ok(InitAction::Created(path));
    }
    let content = std::fs::read_to_string(&path)?;
    if content.contains(AGENTS_MINE_MARKER) {
        return Ok(InitAction::Preserved(path));
    }
    // Append the MINE section without erasing existing content.
    let mut updated = content;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(AGENTS_MINE_SECTION);
    std::fs::write(&path, updated)?;
    Ok(InitAction::CreatedSection(path))
}
