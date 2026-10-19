use super::*;
use crate::domain::graph::{CommitId, Sha256};
use crate::domain::lifecycle_operation::*;
use serde::{Deserialize, Serialize};

/// Original graph authority is explicit. An absent source is never a fake
/// revision-zero graph or an empty string accepted after a parse failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphBefore {
    Present {
        source: String,
        view: String,
    },
    Absent {
        predecessors: Vec<WorkspacePredecessor>,
    },
}

/// Frozen release authority consumed by one successor workspace. These are
/// declared identities; initial admission must verify their retained evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspacePredecessor {
    pub workspace_id: String,
    pub release_operation_id: OperationId,
    pub terminal_stage_revision: u64,
    pub manifest: crate::domain::graph::FileIdentity,
}

impl GraphBefore {
    pub fn present(&self) -> OperationResult<(&str, &str)> {
        match self {
            Self::Present { source, view } => Ok((source, view)),
            Self::Absent { .. } => Err(OperationError::Refused(
                "operation requires an existing original graph",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointRuntime {
    pub root: PathBuf,
    pub common_dir: PathBuf,
    pub index_path: PathBuf,
    pub integration_ref: String,
    pub config_sha256: Sha256,
    pub source_path: RepoPath,
    pub view_path: RepoPath,
    pub before: GraphBefore,
    pub source_after: String,
    pub view_after: String,
    pub original_index: Vec<u8>,
    pub generated_index: Option<Vec<u8>>,
    pub candidate_tree: Option<CommitId>,
    pub candidate_commit: Option<CommitId>,
    /// True is persisted BEFORE invoking CAS, including uncertain failures.
    pub cas_attempted: bool,
    /// A hard-link witness is created from our exclusively created lock. Its
    /// filesystem identity and bytes must agree before any recovery write.
    pub lock_claimed: bool,
    pub lock_sha256: Option<Sha256>,
    /// Required explicit tag; missing intent never decodes as safe no-effects.
    pub plan_git: super::PlanGitEffects,
}
impl CheckpointRuntime {
    pub fn validate(&self, op: &LifecycleOperation) -> OperationResult<()> {
        self.validate_metadata(op)?;
        self.validate_graph_bindings(op)?;
        self.plan_git.validate(self, op)?;
        Ok(())
    }
    fn validate_metadata(&self, op: &LifecycleOperation) -> OperationResult<()> {
        let f = op.fields();
        let c = f
            .checkpoint
            .as_ref()
            .ok_or(OperationError::Journal)?
            .fields();
        if self.source_path == self.view_path
            || self.original_index.is_empty()
            || digest(&self.original_index) != c.original_index_sha256
            || c.candidate_tree != self.candidate_tree
            || c.candidate_commit != self.candidate_commit
            || c.generated_index_sha256 != self.generated_index.as_ref().map(|b| digest(b))
            || c.owned_lock_sha256 != self.lock_sha256
            || self.generated_index.is_some() != self.candidate_commit.is_some()
            || self.candidate_commit.is_some() != self.candidate_tree.is_some()
            || self.cas_attempted
                && (!self.lock_claimed
                    || self.candidate_commit.is_none()
                    || self.lock_sha256.is_none())
            || self.lock_claimed && self.candidate_commit.is_none()
            || c.source_sha256
                .as_ref()
                .is_some_and(|v| *v != digest(self.source_after.as_bytes()))
            || c.view_sha256
                .as_ref()
                .is_some_and(|v| *v != digest(self.view_after.as_bytes()))
        {
            return Err(OperationError::Journal);
        }
        Ok(())
    }
    pub(crate) fn validate_reusing(
        &self,
        op: &LifecycleOperation,
        witness: Option<&super::validation::RuntimeValidationWitness>,
    ) -> OperationResult<super::validation::RuntimeValidationWitness> {
        self.validate_metadata(op)?;
        let binding = super::validation::RuntimeValidationWitness::binding(self, op)?;
        if let Some(known) = witness.filter(|known| known.matches(&binding)) {
            self.plan_git.validate_progress(self, op)?;
            return Ok(known.clone());
        }
        self.validate_graph_bindings(op)?;
        self.plan_git.validate(self, op)?;
        Ok(super::validation::RuntimeValidationWitness::validated(
            binding,
        ))
    }
    /// Historical observation cannot defer original workspace/revision/view
    /// consistency to a later live-Git resume (that source may be retired).
    /// Validate bounded persisted facts without reading today's graph/files.
    fn validate_graph_bindings(&self, op: &LifecycleOperation) -> OperationResult<()> {
        use crate::domain::graph::{
            CURRENT_GRAPH_SCHEMA_VERSION, PlanWorkspace, is_supported_graph_schema,
        };
        use crate::infrastructure::toml_store::render_markdown;
        let after: PlanWorkspace =
            toml::from_str(&self.source_after).map_err(|_| OperationError::Journal)?;
        let fields = op.fields();
        match &self.before {
            GraphBefore::Present { source, view } => {
                let before: PlanWorkspace =
                    toml::from_str(source).map_err(|_| OperationError::Journal)?;
                if !(1..=CURRENT_GRAPH_SCHEMA_VERSION).contains(&before.schema_version)
                    || before.workspace_id != fields.workspace_id
                    || before.revision != fields.observed_graph_revision
                    || render_markdown(&before)? != *view
                {
                    return Err(OperationError::Journal);
                }
            }
            GraphBefore::Absent { predecessors } => {
                if fields.observed_graph_revision != 0
                    || after.revision != 1
                    || after.schema_version != CURRENT_GRAPH_SCHEMA_VERSION
                    || !after.plans.is_empty()
                    || self.plan_git != super::PlanGitEffects::None
                    || fields.plan.is_some()
                    || fields.registered_worktree.is_some()
                {
                    return Err(OperationError::Journal);
                }
                let mut workspaces = std::collections::BTreeSet::new();
                let mut releases = std::collections::BTreeSet::new();
                for predecessor in predecessors {
                    if predecessor.workspace_id.is_empty()
                        || predecessor.workspace_id == fields.workspace_id
                        || predecessor.terminal_stage_revision == 0
                        || predecessor.manifest.bytes == 0
                        || !workspaces.insert(&predecessor.workspace_id)
                        || !releases.insert(predecessor.release_operation_id.as_str())
                    {
                        return Err(OperationError::Journal);
                    }
                }
            }
        }
        // Known old numeric formats may occur only as the explicit pre-repair
        // source. Published candidates use the supported current/earlier wire.
        if !is_supported_graph_schema(after.schema_version)
            || after.workspace_id != fields.workspace_id
            || fields.observed_graph_revision.checked_add(1) != Some(after.revision)
            || self.integration_ref != format!("refs/heads/{}", after.integration_branch)
            || render_markdown(&after)? != self.view_after
            || toml::to_string(&after).map_err(|_| OperationError::Journal)? != self.source_after
        {
            return Err(OperationError::Journal);
        }
        Ok(())
    }
    pub fn source_before(&self) -> OperationResult<&str> {
        Ok(self.before.present()?.0)
    }
    pub fn view_before(&self) -> OperationResult<&str> {
        Ok(self.before.present()?.1)
    }
    pub fn validate_successor(&self, before: &Self) -> OperationResult<()> {
        let mut immutable = self.clone();
        immutable.generated_index = before.generated_index.clone();
        immutable.candidate_tree = before.candidate_tree.clone();
        immutable.candidate_commit = before.candidate_commit.clone();
        immutable.cas_attempted = before.cas_attempted;
        immutable.lock_claimed = before.lock_claimed;
        immutable.lock_sha256 = before.lock_sha256.clone();
        immutable.plan_git = before.plan_git.clone();
        self.plan_git.validate_successor(&before.plan_git)?;
        if immutable != *before
            || before.cas_attempted && !self.cas_attempted
            || before.lock_claimed && !self.lock_claimed
            || before.candidate_commit.is_some()
                && (before.candidate_commit != self.candidate_commit
                    || before.candidate_tree != self.candidate_tree
                    || before.generated_index != self.generated_index)
            || before.lock_sha256.is_some() && before.lock_sha256 != self.lock_sha256
        {
            return Err(OperationError::Refused(
                "checkpoint produced identity changed",
            ));
        }
        Ok(())
    }
}
pub fn digest(bytes: &[u8]) -> Sha256 {
    use sha2::Digest;
    Sha256::new(format!("{:x}", sha2::Sha256::digest(bytes))).expect("SHA-256 encoding")
}
