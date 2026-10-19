//! Local journal invariants, separate from graph lifecycle authority.
use super::*;

#[derive(Debug, thiserror::Error)]
pub enum OperationError {
    #[error("operation refused: {0}")]
    Refused(&'static str),
    #[error("operation evidence unavailable: {0}")]
    Evidence(&'static str),
    #[error("operation stage conflict: expected {expected}, actual {actual}")]
    Revision { expected: u64, actual: u64 },
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error(transparent)]
    Mine(#[from] crate::domain::error::MineError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid operation journal")]
    Journal,
    #[error("injected interruption at {0:?}")]
    Interrupted(Boundary),
}
pub type OperationResult<T> = Result<T, OperationError>;

/// Extra immutable inputs not present in the initial shared observation spine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClosureBinding {
    Acceptance,
    RejectedHandoff {
        compensation: PlanIdentity,
    },
    Release {
        baseline: CommitId,
        candidate: CommitId,
    },
}
impl ClosureBinding {
    pub fn kind(&self) -> OperationKind {
        match self {
            Self::Acceptance => OperationKind::Acceptance,
            Self::RejectedHandoff { .. } => OperationKind::RejectedHandoff,
            Self::Release { .. } => OperationKind::Release,
        }
    }
}

/// Bounded effect boundaries, not a shell/command injection interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    Intent,
    PlanGitAttempted,
    PlanGitReturned,
    PlanGitRecorded,
    PlanCheckoutObserved,
    ViewRestorePrepared,
    ViewRestorePublished,
    GraphWritten,
    GraphRecorded,
    ViewWritten,
    ViewRecorded,
    PrivateIndexCreated,
    CandidateCreated,
    CandidateRecorded,
    LockCreated,
    LockClaimed,
    LockWritten,
    LockRecorded,
    CasRecorded,
    CasReturned,
    RefRecorded,
    PostRefObserved,
    IndexPublished,
    IndexRecorded,
    Complete,
    Response,
}
pub trait BoundaryObserver {
    fn reached(&mut self, boundary: Boundary) -> OperationResult<()>;
}
pub struct NoInterruption;
impl BoundaryObserver for NoInterruption {
    fn reached(&mut self, _: Boundary) -> OperationResult<()> {
        Ok(())
    }
}

pub fn initial_stage(kind: OperationKind) -> OperationStage {
    match kind {
        OperationKind::Acceptance => OperationStage::Acceptance(AcceptanceStage::Reviewed),
        OperationKind::RejectedHandoff => OperationStage::RejectedHandoff(RejectionStage::Intent),
        OperationKind::Release => OperationStage::Release(ReleaseStage::Intent),
        OperationKind::GraphCheckpoint => OperationStage::GraphCheckpoint(CheckpointStage::Intent),
    }
}
pub fn stage_number(stage: OperationStage) -> u8 {
    match stage {
        OperationStage::Acceptance(s) => s as u8,
        OperationStage::RejectedHandoff(s) => s as u8,
        OperationStage::Release(s) => s as u8,
        OperationStage::GraphCheckpoint(s) => s as u8,
    }
}
pub fn terminal(op: &LifecycleOperation) -> bool {
    let f = op.fields();
    (f.outcome == OperationOutcome::Refused && f.stage == initial_stage(f.kind))
        || (f.outcome == OperationOutcome::Committed
            && matches!(
                f.stage,
                OperationStage::Acceptance(AcceptanceStage::Accepted)
                    | OperationStage::RejectedHandoff(RejectionStage::Complete)
                    | OperationStage::Release(ReleaseStage::Complete)
                    | OperationStage::GraphCheckpoint(CheckpointStage::Complete)
            ))
}

/// Persisted binding never changes on a stage retry. Produced facts only accrue.
pub fn validate_progress(
    old: &LifecycleOperation,
    new: &LifecycleOperation,
) -> OperationResult<()> {
    let a = old.fields();
    let b = new.fields();
    if terminal(old) {
        return Err(OperationError::Refused("terminal operation is immutable"));
    }
    if a.operation_id != b.operation_id
        || a.kind != b.kind
        || a.actor != b.actor
        || a.repository_id != b.repository_id
        || a.workspace_id != b.workspace_id
        || a.plan != b.plan
        || a.registered_worktree != b.registered_worktree
        || a.review != b.review
        || a.observed_graph_revision != b.observed_graph_revision
    {
        return Err(OperationError::Refused("operation input binding changed"));
    }
    if a.stage_revision.checked_add(1) != Some(b.stage_revision) {
        return Err(OperationError::Revision {
            expected: a.stage_revision.saturating_add(1),
            actual: b.stage_revision,
        });
    }
    let before = stage_number(a.stage);
    let after = stage_number(b.stage);
    if after < before || after > before + 1 {
        return Err(OperationError::Refused("stage skipped or moved backwards"));
    }
    if a.integrated_commit.is_some() && a.integrated_commit != b.integrated_commit
        || a.cleanup.is_some() && a.cleanup != b.cleanup
        || !a
            .verification_evidence
            .iter()
            .all(|v| b.verification_evidence.contains(v))
    {
        return Err(OperationError::Refused("produced evidence changed"));
    }
    if let (Some(old), Some(new)) = (&a.checkpoint, &b.checkpoint) {
        let old = old.fields();
        let new = new.fields();
        if old.original_head != new.original_head
            || old.original_index_sha256 != new.original_index_sha256
            || old.revision_before != new.revision_before
            || old.revision_after.is_some() && old.revision_after != new.revision_after
            || old.source_sha256.is_some() && old.source_sha256 != new.source_sha256
            || old.view_sha256.is_some() && old.view_sha256 != new.view_sha256
            || old.candidate_tree.is_some() && old.candidate_tree != new.candidate_tree
            || old.candidate_commit.is_some() && old.candidate_commit != new.candidate_commit
            || old.generated_index_sha256.is_some()
                && old.generated_index_sha256 != new.generated_index_sha256
            || old.owned_lock_sha256.is_some() && old.owned_lock_sha256 != new.owned_lock_sha256
        {
            return Err(OperationError::Refused("checkpoint evidence changed"));
        }
    } else if a.checkpoint.is_some() != b.checkpoint.is_some() {
        return Err(OperationError::Refused("checkpoint binding changed"));
    }
    if b.outcome == OperationOutcome::Committed && !terminal(new) {
        return Err(OperationError::Refused("committed requires final stage"));
    }
    if b.outcome == OperationOutcome::Refused
        && (a.kind == OperationKind::GraphCheckpoint
            || a.stage != initial_stage(a.kind)
            || matches!(
                a.outcome,
                OperationOutcome::Unknown | OperationOutcome::CommittedRenderRepair
            ))
    {
        return Err(OperationError::Refused(
            "uncertain/produced effects cannot be called refused",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingOperation {
    pub operation_id: OperationId,
    pub stage: OperationStage,
    pub outcome: OperationOutcome,
    pub graph_revision: u64,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BarrierFacts {
    pub global: Vec<PendingOperation>,
    pub held_plans: Vec<String>,
}
impl BarrierFacts {
    pub fn observe(&mut self, op: &LifecycleOperation) {
        if terminal(op) {
            return;
        }
        let f = op.fields();
        if matches!(
            f.kind,
            OperationKind::GraphCheckpoint | OperationKind::Release
        ) || matches!(
            f.outcome,
            OperationOutcome::Unknown | OperationOutcome::CommittedRenderRepair
        ) {
            self.global.push(PendingOperation {
                operation_id: f.operation_id.clone(),
                stage: f.stage,
                outcome: f.outcome,
                graph_revision: f
                    .checkpoint
                    .as_ref()
                    .and_then(|c| c.fields().revision_after)
                    .unwrap_or(f.observed_graph_revision),
            });
        }
        if let Some(plan) = &f.plan {
            self.held_plans.push(plan.fields().plan_id.clone());
        }
    }
}
