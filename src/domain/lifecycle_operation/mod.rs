//! Journal/checkpoint observation contracts. No stage executes effects or grants
//! acceptance; the operation owner must reobserve actual graph/Git/artifacts.

use crate::domain::error::{ContractError, ContractResult, contract_text, contract_version};
use crate::domain::graph::{CommitId, FileIdentity, PlanIdentity, RepoPath, Sha256};
use serde::{Deserialize, Serialize};

mod journal;
pub use journal::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OperationId(String);
impl OperationId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
    pub fn parse(value: impl Into<String>) -> ContractResult<Self> {
        let value = value.into();
        let id = uuid::Uuid::parse_str(&value)
            .map_err(|_| ContractError::new("operation_id", "expected canonical UUID v4"))?;
        if id.get_version_num() != 4
            || id.get_variant() != uuid::Variant::RFC4122
            || id.to_string() != value
        {
            return Err(ContractError::new(
                "operation_id",
                "expected canonical UUID v4",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for OperationId {
    type Error = ContractError;
    fn try_from(value: String) -> ContractResult<Self> {
        Self::parse(value)
    }
}
impl From<OperationId> for String {
    fn from(value: OperationId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Acceptance,
    RejectedHandoff,
    Release,
    GraphCheckpoint,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceStage {
    Reviewed,
    Integrated,
    Verified,
    Cleaned,
    Accepted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionStage {
    Intent,
    Rejected,
    CompensationRegistered,
    Rewired,
    Released,
    Complete,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStage {
    Intent,
    Preflight,
    Integrated,
    Complete,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStage {
    Intent,
    GitWorkspaceMutated,
    GraphPersisted,
    RenderComplete,
    CandidatePrepared,
    RefAdvanced,
    IndexPublished,
    Complete,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "stage",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OperationStage {
    Acceptance(AcceptanceStage),
    RejectedHandoff(RejectionStage),
    Release(ReleaseStage),
    GraphCheckpoint(CheckpointStage),
}
impl OperationStage {
    pub fn kind(self) -> OperationKind {
        match self {
            Self::Acceptance(_) => OperationKind::Acceptance,
            Self::RejectedHandoff(_) => OperationKind::RejectedHandoff,
            Self::Release(_) => OperationKind::Release,
            Self::GraphCheckpoint(_) => OperationKind::GraphCheckpoint,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    Refused,
    Pending,
    Committed,
    Unknown,
    CommittedRenderRepair,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewEvidence {
    pub reviewed_tip: CommitId,
    pub report: FileIdentity,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupEvidence {
    pub worktree: RepoPath,
    pub branch: String,
    pub worktree_removed: bool,
    pub branch_removed: bool,
}

/// Produced checkpoint identities are optional until their creation boundary.
/// No service may invent a candidate or lock identity before that boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointFields {
    pub schema_version: u32,
    pub stage: CheckpointStage,
    pub revision_before: u64,
    pub revision_after: Option<u64>,
    pub source_sha256: Option<Sha256>,
    pub view_sha256: Option<Sha256>,
    pub original_head: CommitId,
    pub original_index_sha256: Sha256,
    pub candidate_tree: Option<CommitId>,
    pub candidate_commit: Option<CommitId>,
    pub generated_index_sha256: Option<Sha256>,
    pub owned_lock_sha256: Option<Sha256>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CheckpointFields")]
pub struct CheckpointFacts(CheckpointFields);
impl CheckpointFacts {
    pub fn new(fields: CheckpointFields) -> ContractResult<Self> {
        contract_version(fields.schema_version)?;
        if fields.stage >= CheckpointStage::GraphPersisted
            && (fields.revision_after.is_none()
                || fields.revision_after != fields.revision_before.checked_add(1)
                || fields.source_sha256.is_none())
        {
            return Err(ContractError::new(
                "checkpoint.graph",
                "persisted stage requires actual next revision and source identity",
            ));
        }
        if fields.stage >= CheckpointStage::RenderComplete && fields.view_sha256.is_none() {
            return Err(ContractError::new(
                "checkpoint.view",
                "rendered stage requires view identity",
            ));
        }
        if fields.stage >= CheckpointStage::CandidatePrepared
            && (fields.candidate_tree.is_none()
                || fields.candidate_commit.is_none()
                || fields.generated_index_sha256.is_none())
        {
            return Err(ContractError::new(
                "checkpoint.candidate",
                "prepared stage requires produced tree, commit and index identities",
            ));
        }
        if fields.stage >= CheckpointStage::RefAdvanced && fields.owned_lock_sha256.is_none() {
            return Err(ContractError::new(
                "checkpoint.lock",
                "publication stage requires owned lock identity",
            ));
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &CheckpointFields {
        &self.0
    }
}
impl TryFrom<CheckpointFields> for CheckpointFacts {
    type Error = ContractError;
    fn try_from(fields: CheckpointFields) -> ContractResult<Self> {
        Self::new(fields)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationFields {
    pub schema_version: u32,
    pub operation_id: OperationId,
    pub stage_revision: u64,
    pub kind: OperationKind,
    pub stage: OperationStage,
    pub outcome: OperationOutcome,
    pub repository_id: String,
    pub workspace_id: String,
    pub actor: String,
    pub observed_graph_revision: u64,
    /// Workspace-level checkpoint/release may not name an existing Plan.
    pub plan: Option<PlanIdentity>,
    pub registered_worktree: Option<RepoPath>,
    pub review: Option<ReviewEvidence>,
    pub integrated_commit: Option<CommitId>,
    pub verification_evidence: Vec<FileIdentity>,
    pub cleanup: Option<CleanupEvidence>,
    pub checkpoint: Option<CheckpointFacts>,
    /// Bounded machine codes only, never child output or credentials.
    pub diagnostics: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "OperationFields")]
pub struct LifecycleOperation(OperationFields);
impl LifecycleOperation {
    pub fn new(fields: OperationFields) -> ContractResult<Self> {
        contract_version(fields.schema_version)?;
        contract_text("repository_id", &fields.repository_id)?;
        contract_text("workspace_id", &fields.workspace_id)?;
        contract_text("actor", &fields.actor)?;
        if fields.stage.kind() != fields.kind {
            return Err(ContractError::new(
                "stage",
                "stage kind does not match operation kind",
            ));
        }
        if let Some(plan) = &fields.plan {
            if plan.fields().repository_id != fields.repository_id
                || plan.fields().workspace_id != fields.workspace_id
            {
                return Err(ContractError::new(
                    "plan",
                    "operation repository/workspace binding mismatch",
                ));
            }
        }
        if matches!(
            fields.kind,
            OperationKind::Acceptance | OperationKind::RejectedHandoff
        ) && fields
            .plan
            .as_ref()
            .is_none_or(|plan| plan.fields().body_sha256.is_none())
        {
            return Err(ContractError::new(
                "plan",
                "closure requires exact Plan body identity",
            ));
        }
        if let OperationStage::Acceptance(stage) = fields.stage {
            let plan = fields
                .plan
                .as_ref()
                .expect("closure identity checked")
                .fields();
            let review = fields.review.as_ref().ok_or(ContractError::new(
                "review",
                "acceptance requires reviewed tip/report evidence",
            ))?;
            if crate::domain::plan_id::validate_report_path(
                plan.plan_path.as_str(),
                review.report.path.as_str(),
                "review",
            )
            .is_err()
            {
                return Err(ContractError::new(
                    "review.report",
                    "review path is not the exact Plan review report",
                ));
            }
            if stage != AcceptanceStage::Reviewed && fields.integrated_commit.is_none() {
                return Err(ContractError::new(
                    "integrated_commit",
                    "integrated stage requires integration result",
                ));
            }
            if matches!(stage, AcceptanceStage::Cleaned | AcceptanceStage::Accepted) {
                let cleanup = fields.cleanup.as_ref().ok_or(ContractError::new(
                    "cleanup",
                    "cleaned stage requires cleanup observations",
                ))?;
                if !cleanup.worktree_removed
                    || !cleanup.branch_removed
                    || cleanup.branch != plan.branch
                    || fields.registered_worktree.as_ref() != Some(&cleanup.worktree)
                {
                    return Err(ContractError::new(
                        "cleanup",
                        "cleanup does not match exact registered worktree/branch removal",
                    ));
                }
            }
        }
        if let OperationStage::GraphCheckpoint(stage) = fields.stage {
            let facts = fields.checkpoint.as_ref().ok_or(ContractError::new(
                "checkpoint",
                "checkpoint operation requires checkpoint facts",
            ))?;
            if facts.fields().stage != stage
                || facts.fields().revision_before != fields.observed_graph_revision
            {
                return Err(ContractError::new(
                    "checkpoint",
                    "checkpoint stage/revision binding mismatch",
                ));
            }
        }
        if fields.diagnostics.len() > 64
            || fields.diagnostics.iter().any(|code| {
                code.is_empty()
                    || code.len() > 128
                    || !code
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
        {
            return Err(ContractError::new(
                "diagnostics",
                "expected at most 64 bounded machine codes",
            ));
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &OperationFields {
        &self.0
    }
}
impl TryFrom<OperationFields> for LifecycleOperation {
    type Error = ContractError;
    fn try_from(fields: OperationFields) -> ContractResult<Self> {
        Self::new(fields)
    }
}
