//! Exact configured graph-pair locator and recoverable native checkpoint.

use crate::domain::config::MineConfig;
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::RepoPath;
pub use crate::domain::lifecycle_operation::{CheckpointFacts, CheckpointStage};
use crate::infrastructure::graph_snapshot::resolve_contained;
use std::path::{Path, PathBuf};

mod git;
pub(crate) use git::index_policy as observe_index_policy;
pub(crate) use git::request as checkpoint_git;
mod kernel;
pub(crate) mod plan_git;
pub(crate) mod preparation_history;
pub(crate) mod preparation_intent;
mod proofs;
mod runtime;
mod validation;
pub use kernel::{CheckpointKernel, GraphRenderOutcome};
pub use plan_git::{PlanGitEffects, PlanGitSummary};
pub use runtime::{CheckpointRuntime, GraphBefore, WorkspacePredecessor, digest};
pub(crate) use validation::RuntimeValidationWitness;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCheckpointPaths {
    root: PathBuf,
    source: RepoPath,
    view: RepoPath,
}
impl GraphCheckpointPaths {
    /// Configuration is the sole path source; there is no caller path whitelist.
    pub fn from_config(root: &Path, config: &MineConfig) -> MineResult<Self> {
        let bad = || MineError::GraphInvalid {
            detail: "configured graph paths must be distinct safe files within the Plan workspace"
                .into(),
        };
        let layout = config.layout()?;
        let source = layout.graph_source().clone();
        let view = layout.graph_view().clone();
        let plan_root = layout.plan_dir().clone();
        if source == view
            || source == plan_root
            || view == plan_root
            || !crate::domain::path::is_within(plan_root.as_str(), source.as_str())
            || !crate::domain::path::is_within(plan_root.as_str(), view.as_str())
        {
            return Err(bad());
        }
        let source_path = resolve_contained(root, &source)?;
        if source_path == resolve_contained(root, &view)? {
            return Err(bad());
        }
        Ok(Self {
            root: root.canonicalize()?,
            source,
            view,
        })
    }
    pub fn source(&self) -> &RepoPath {
        &self.source
    }
    pub fn view(&self) -> &RepoPath {
        &self.view
    }
    pub fn resolve(&self) -> MineResult<(PathBuf, PathBuf)> {
        Ok((
            crate::infrastructure::operation_store::regular_path(&self.root, &self.source)?,
            crate::infrastructure::operation_store::regular_path(&self.root, &self.view)?,
        ))
    }
}
