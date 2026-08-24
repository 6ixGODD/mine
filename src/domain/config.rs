//! Project configuration model for `.mine/config.toml`.
//!
//! The schema follows the operations design
//! (`docs/design/operations/configuration-security-observability.md`).
//! `.mine/config.toml` is long-lived, may exist on the stable branch, and is
//! the authoritative source for branch names, design root/marker paths,
//! document-size soft limits, plan workspace policy, and execution-graph
//! persistence paths. `mine init` initializes it when absent and validates it
//! when present; it never silently rewrites an existing configuration.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::domain::error::{MineError, MineResult};

/// Top-level `.mine/config.toml` model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MineConfig {
    /// Configuration schema version.
    pub schema_version: u32,
    /// Stable repository identifier (UUID). Matches the design marker.
    pub repository_id: String,
    /// MINE code-repository version, persisted across releases.
    pub mine_code_version: String,
    /// Managed branch names.
    pub branches: BranchesConfig,
    /// Design knowledge-base root and marker configuration.
    pub design: DesignConfig,
    /// Plan workspace policy.
    pub plan: PlanConfig,
    /// Execution-graph persistence configuration.
    pub graph: GraphConfig,
    /// ADR registry persistence configuration.
    #[serde(default)]
    pub adr: AdrConfig,
}

/// ADR registry persistence configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdrConfig {
    #[serde(default = "default_adr_registry")]
    pub registry: String,
    #[serde(default = "default_adr_index")]
    pub rendered_index: String,
    #[serde(default = "default_adr_lock_timeout")]
    pub lock_timeout_ms: u32,
}

impl Default for AdrConfig {
    fn default() -> Self {
        Self {
            registry: default_adr_registry(),
            rendered_index: default_adr_index(),
            lock_timeout_ms: default_adr_lock_timeout(),
        }
    }
}

fn default_adr_registry() -> String {
    "docs/design/decisions/adr-registry.toml".to_string()
}

fn default_adr_index() -> String {
    "docs/design/decisions/index.md".to_string()
}

fn default_adr_lock_timeout() -> u32 {
    5000
}

impl MineConfig {
    /// Parses a configuration from TOML.
    ///
    /// # Errors
    /// Returns [`MineError::ConfigInvalid`] if the content is not valid
    /// configuration TOML or is missing required fields.
    pub fn parse(path: &Path, content: &str) -> MineResult<Self> {
        toml::from_str(content).map_err(|e| MineError::ConfigInvalid {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })
    }

    /// Serializes the configuration to TOML. Serialization of the declared
    /// schema is infallible in practice.
    #[must_use]
    pub fn to_toml(&self) -> String {
        toml::to_string(self)
            .expect("MineConfig serialization is infallible for the declared schema")
    }
}

/// Managed branch configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchesConfig {
    /// Stable branch detected and persisted by `mine init` (e.g. `main`,
    /// `master`, or a custom configured branch). All downstream services read
    /// this value rather than a hardcoded default.
    pub stable: String,
    /// Temporary integration branch.
    pub integration: String,
}

/// Design knowledge-base configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignConfig {
    /// Repository-relative path to the design root index.
    pub root: String,
    /// Repository-relative path to the design ownership marker.
    pub marker: String,
    /// Design documentation language.
    pub language: String,
    /// Soft limit in lines for index documents.
    pub index_soft_limit_lines: u32,
    /// Soft limit in lines for leaf documents.
    pub leaf_soft_limit_lines: u32,
}

/// Plan workspace configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanConfig {
    /// Repository-relative path to the plan workspace root.
    pub root: String,
    /// Whether the plan workspace is ephemeral.
    pub ephemeral: bool,
    /// Whether the plan workspace must be purged before stable release.
    pub purge_before_stable_release: bool,
    /// Repository-relative root for canonical Plan linked worktrees.
    #[serde(default = "default_worktrees_root")]
    pub worktrees_root: String,
    /// Repository-relative root that owns shared language environments/caches.
    #[serde(default = "default_shared_environment_root")]
    pub shared_environment_root: String,
}

fn default_worktrees_root() -> String {
    ".mine/worktrees".to_string()
}

fn default_shared_environment_root() -> String {
    ".".to_string()
}

/// Execution-graph persistence configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphConfig {
    /// Repository-relative path to the execution-graph machine source.
    pub source: String,
    /// Repository-relative path to the generated execution-graph view.
    pub rendered: String,
    /// Exclusive lock wait timeout in milliseconds.
    pub lock_timeout_ms: u32,
}
