//! Concrete governance paths derived from the existing configuration schema.
//! No filesystem access, second registry, or writable legacy alias lives here.

use super::MineConfig;
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::RepoPath;
use crate::domain::path::is_within;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceLayout {
    design_index: RepoPath,
    design_dir: RepoPath,
    design_marker: RepoPath,
    plan_dir: RepoPath,
    graph_source: RepoPath,
    graph_view: RepoPath,
    adr_registry: RepoPath,
    adr_index: RepoPath,
    decisions_dir: RepoPath,
    reports_dir: RepoPath,
}

impl GovernanceLayout {
    pub const DESIGN_DIR: &str = ".mine/design";
    pub const PLAN_DIR: &str = ".mine/plan";
    pub const BACKUP_DIR: &str = ".mine/runtime/design-backups";

    /// The one fresh-layout contract; existing configured source layouts are
    /// resolved separately for bounded read/terminal-release observations.
    #[must_use]
    pub fn canonical() -> Self {
        let fixed = |value: String| RepoPath::new(value).expect("canonical governance path");
        Self {
            design_index: fixed(format!("{}/index.md", Self::DESIGN_DIR)),
            design_dir: fixed(Self::DESIGN_DIR.into()),
            design_marker: fixed(format!("{}/.mine-design.toml", Self::DESIGN_DIR)),
            plan_dir: fixed(Self::PLAN_DIR.into()),
            graph_source: fixed(format!("{}/execution-graph.toml", Self::PLAN_DIR)),
            graph_view: fixed(format!("{}/execution-graph.md", Self::PLAN_DIR)),
            adr_registry: fixed(format!("{}/decisions/adr-registry.toml", Self::DESIGN_DIR)),
            adr_index: fixed(format!("{}/decisions/index.md", Self::DESIGN_DIR)),
            decisions_dir: fixed(format!("{}/decisions", Self::DESIGN_DIR)),
            reports_dir: fixed(format!("{}/reports", Self::PLAN_DIR)),
        }
    }

    pub fn from_config(config: &MineConfig) -> MineResult<Self> {
        let design_index = path(&config.design.root)?;
        let design_dir = parent(&design_index)?;
        let design_marker = path(&config.design.marker)?;
        let plan_dir = path(&config.plan.root)?;
        let graph_source = path(&config.graph.source)?;
        let graph_view = path(&config.graph.rendered)?;
        let adr_registry = path(&config.adr.registry)?;
        let adr_index = path(&config.adr.rendered_index)?;
        let decisions_dir = parent(&adr_registry)?;
        let reports_dir = path(&format!("{}/reports", plan_dir.as_str()))?;
        for (directory, child) in [
            (&design_dir, &design_marker),
            (&design_dir, &decisions_dir),
            (&decisions_dir, &adr_index),
            (&plan_dir, &graph_source),
            (&plan_dir, &graph_view),
        ] {
            if directory == child || !is_within(directory.as_str(), child.as_str()) {
                return Err(invalid(
                    "configured governance paths have inconsistent ownership",
                ));
            }
        }
        if is_within(design_dir.as_str(), plan_dir.as_str())
            || is_within(plan_dir.as_str(), design_dir.as_str())
        {
            return Err(invalid("durable Design and ephemeral Plan roots overlap"));
        }
        let files = [
            &design_index,
            &design_marker,
            &graph_source,
            &graph_view,
            &adr_registry,
            &adr_index,
        ];
        for (index, file) in files.iter().enumerate() {
            if files[index + 1..].iter().any(|other| other == file) {
                return Err(invalid("configured governance files collide"));
            }
        }
        for directory in [&design_dir, &plan_dir] {
            if [
                ".git",
                ".mine/runtime",
                ".mine/locks",
                ".mine/build",
                config.plan.worktrees_root.as_str(),
            ]
            .iter()
            .any(|reserved| {
                is_within(reserved, directory.as_str()) || is_within(directory.as_str(), reserved)
            }) {
                return Err(invalid(
                    "governance root overlaps Git or mutable runtime storage",
                ));
            }
        }
        Ok(Self {
            design_index,
            design_dir,
            design_marker,
            plan_dir,
            graph_source,
            graph_view,
            adr_registry,
            adr_index,
            decisions_dir,
            reports_dir,
        })
    }

    pub fn design_index(&self) -> &RepoPath {
        &self.design_index
    }
    pub fn design_dir(&self) -> &RepoPath {
        &self.design_dir
    }
    pub fn design_marker(&self) -> &RepoPath {
        &self.design_marker
    }
    pub fn plan_dir(&self) -> &RepoPath {
        &self.plan_dir
    }
    pub fn graph_source(&self) -> &RepoPath {
        &self.graph_source
    }
    pub fn graph_view(&self) -> &RepoPath {
        &self.graph_view
    }
    pub fn adr_registry(&self) -> &RepoPath {
        &self.adr_registry
    }
    pub fn adr_index(&self) -> &RepoPath {
        &self.adr_index
    }
    pub fn decisions_dir(&self) -> &RepoPath {
        &self.decisions_dir
    }
    pub fn reports_dir(&self) -> &RepoPath {
        &self.reports_dir
    }

    /// Filename grammar is pure; configured namespace admission lives here.
    pub fn validate_plan_path(&self, id: &str, path: &str) -> MineResult<String> {
        if path.rsplit_once('/').map(|pair| pair.0) != Some(self.plan_dir.as_str()) {
            return Err(invalid(
                "Plan body must be directly inside the configured Plan root",
            ));
        }
        crate::domain::plan_id::validate_plan_path(id, path)
    }

    pub fn paths(&self) -> [&RepoPath; 8] {
        [
            self.design_index(),
            self.design_marker(),
            self.plan_dir(),
            self.graph_source(),
            self.graph_view(),
            self.adr_registry(),
            self.adr_index(),
            self.reports_dir(),
        ]
    }
}

fn parent(path: &RepoPath) -> MineResult<RepoPath> {
    let (parent, _) = path
        .as_str()
        .rsplit_once('/')
        .ok_or_else(|| invalid("governance file needs an owned parent directory"))?;
    self::path(parent)
}

fn path(value: &str) -> MineResult<RepoPath> {
    RepoPath::new(value).map_err(|error| invalid(&error.to_string()))
}

fn invalid(detail: &str) -> MineError {
    MineError::ConfigInvalid {
        path: ".mine/config.toml".into(),
        detail: detail.into(),
    }
}
