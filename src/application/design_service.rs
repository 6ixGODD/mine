//! Application service for design validation/status (the read-only design
//! tools shared by the CLI and MCP).
//!
//! Per `docs/design/interfaces/mcp-contract.md` and the component-architecture
//! design, design *backup*, workspace open/close, init, install, and release
//! mutations remain **CLI-only** because they change local environment or
//! delete owned temporary state. Only the read-only `mine_design_validate` is
//! exposed over MCP; `mine_design_status` is a CLI-only helper that this
//! service also backs so the CLI and MCP share one validation path.

use crate::domain::config::MineConfig;
use crate::domain::design_marker::DesignMarker;
use crate::domain::error::MineResult;
use crate::infrastructure::git;

/// A design-validate outcome (shared DTO for CLI and MCP).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DesignValidateResult {
    pub valid: bool,
    pub warnings: Vec<DesignWarning>,
}

/// A design-validation warning.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DesignWarning {
    pub code: String,
    pub message: String,
}

/// Read-only design validation/status service.
pub struct DesignService;

impl DesignService {
    /// Validates the design namespace against the config: marker exists and
    /// matches repository id; `docs/design/index.md` exists; warns if plan
    /// workspace is on the stable branch.
    pub fn validate(
        repo_root: &std::path::Path,
        config: &MineConfig,
    ) -> MineResult<DesignValidateResult> {
        let mut warnings: Vec<DesignWarning> = Vec::new();
        let marker_path = repo_root.join(&config.design.marker);
        let marker_ok = marker_path.exists();
        let index_ok = repo_root.join("docs/design/index.md").exists();
        let mut ok = true;
        if !marker_ok {
            // An absent marker is either a namespace conflict (design dir
            // exists) or simply uninitialized; surface as invalid.
            ok = false;
        }
        if !index_ok {
            warnings.push(DesignWarning {
                code: "MINE_DESIGN_INDEX_MISSING".to_string(),
                message: "docs/design/index.md missing".to_string(),
            });
            ok = false;
        }
        // Stable-branch release hygiene: warn if docs/plan lives on the stable
        // branch (read-only Git evidence; never mutates Git).
        if git::current_branch(repo_root).ok().flatten().as_deref()
            == Some(config.branches.stable.as_str())
            && repo_root
                .join("docs/plan")
                .join("execution-graph.toml")
                .exists()
        {
            warnings.push(DesignWarning {
                code: "MINE_PLANS_ON_STABLE".to_string(),
                message: "docs/plan found on the stable branch".to_string(),
            });
        }
        Ok(DesignValidateResult {
            valid: ok,
            warnings,
        })
    }

    /// Reports managed/unmanaged design status. Read-only.
    pub fn status(repo_root: &std::path::Path, config: &MineConfig) -> MineResult<DesignStatus> {
        let marker_path = repo_root.join(&config.design.marker);
        let marker = marker_path.exists().then(|| {
            std::fs::read_to_string(&marker_path)
                .ok()
                .and_then(|c| DesignMarker::parse(&marker_path, &c).ok())
        });
        Ok(DesignStatus {
            managed: marker.is_some(),
            repository_id: marker
                .as_ref()
                .and_then(|m| m.as_ref().map(|m| m.repository_id.clone())),
            created_at: marker
                .as_ref()
                .and_then(|m| m.as_ref().map(|m| m.created_at.clone())),
            design_root: config.design.root.clone(),
        })
    }
}

/// Design status DTO (CLI-only `mine design status`; MCP exposes only validate).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DesignStatus {
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    pub design_root: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::{
        AdrConfig, BranchesConfig, DesignConfig, GraphConfig, MineConfig, PlanConfig,
    };

    fn cfg(repo_id: &str) -> MineConfig {
        MineConfig {
            schema_version: 1,
            repository_id: repo_id.to_string(),
            mine_code_version: "0.1.0".to_string(),
            branches: BranchesConfig {
                stable: "master".to_string(),
                integration: "dev".to_string(),
            },
            design: DesignConfig {
                root: "docs/design/index.md".to_string(),
                marker: "docs/design/.mine-design.toml".to_string(),
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

    #[test]
    fn validate_flags_missing_marker_and_index() {
        let dir = tempfile::tempdir().unwrap();
        // No marker, no index -> invalid.
        let res = DesignService::validate(dir.path(), &cfg("repo")).unwrap();
        assert!(!res.valid);
        assert!(
            res.warnings
                .iter()
                .any(|w| w.code == "MINE_DESIGN_INDEX_MISSING")
        );
    }

    #[test]
    fn validate_ok_when_marker_and_index_present() {
        let dir = tempfile::tempdir().unwrap();
        let design = dir.path().join("docs/design");
        std::fs::create_dir_all(&design).unwrap();
        let marker = DesignMarker::new("repo".to_string(), "2026-07-23T00:00:00Z".to_string());
        std::fs::write(design.join(".mine-design.toml"), marker.to_toml()).unwrap();
        std::fs::write(design.join("index.md"), "# Design\n").unwrap();
        let res = DesignService::validate(dir.path(), &cfg("repo")).unwrap();
        assert!(res.valid);
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn status_reports_managed_marker() {
        let dir = tempfile::tempdir().unwrap();
        let design = dir.path().join("docs/design");
        std::fs::create_dir_all(&design).unwrap();
        let marker = DesignMarker::new("repo-7".to_string(), "2026-07-23T00:00:00Z".to_string());
        std::fs::write(design.join(".mine-design.toml"), marker.to_toml()).unwrap();
        let st = DesignService::status(dir.path(), &cfg("repo-7")).unwrap();
        assert!(st.managed);
        assert_eq!(st.repository_id.as_deref(), Some("repo-7"));
    }
}
