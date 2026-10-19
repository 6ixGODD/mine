//! Native Plan checkout effects and their exact Root checkpoint composition.
#[path = "native_preparation.rs"]
mod preparation;
use super::{
    graph_service::GraphService,
    native_evidence::{bytes, refused},
    plan_service::{PlanWithdrawRequest, PlanWithdrawal},
};
use crate::domain::{
    error::{MineResult, contract_text},
    graph::*,
    lifecycle_operation::*,
    ports::Clock,
    status::PlanStatus,
};
use crate::infrastructure::{
    graph_checkpoint::{CheckpointKernel, PlanGitEffects, plan_git::WithdrawInput},
    system::SystemClock,
    toml_store::render_markdown,
};

pub struct NativePlanWorkspace<'a, 'store> {
    graph: &'a GraphService<'store>,
}
impl<'a, 'store> NativePlanWorkspace<'a, 'store> {
    pub fn new(graph: &'a GraphService<'store>) -> Self {
        Self { graph }
    }
    pub fn withdraw(
        &self,
        request: PlanWithdrawRequest,
        observer: &mut impl BoundaryObserver,
    ) -> MineResult<(PlanWorkspace, PlanWithdrawal)> {
        contract_text("operation.actor", &request.actor).map_err(|e| refused(e.to_string()))?;
        if request.actor.trim().is_empty() || request.reason.trim().is_empty() {
            return Err(refused("actor and reason must be non-whitespace"));
        }
        let root = self.graph.repo_root();
        let result = self.graph.reconcile_operation_locked(
            request.expected_revision,
            &request.operation_id,
            |snapshot, guard, existing| {
                let kernel = CheckpointKernel::under_lock(root, guard)
                    .map_err(|e| refused(e.to_string()))?;
                let barriers = kernel
                    .store()
                    .barriers()
                    .map_err(|e| refused(e.to_string()))?;
                if barriers
                    .global
                    .iter()
                    .any(|p| p.operation_id != request.operation_id)
                {
                    return Err(refused("another pending operation blocks withdrawal"));
                }
                let revision_before = snapshot.workspace().revision;
                let record = if existing {
                    let record = kernel
                        .store()
                        .query(&request.operation_id)
                        .map_err(|e| refused(e.to_string()))?;
                    let intent = match &record
                        .checkpoint
                        .as_ref()
                        .ok_or_else(|| refused("withdrawal checkpoint missing"))?
                        .plan_git
                    {
                        PlanGitEffects::Withdrawal { intent, .. } => intent,
                        _ => return Err(refused("operation is not this exact withdrawal")),
                    };
                    if record.operation.fields().actor != request.actor
                        || record.operation.fields().observed_graph_revision
                            != request.expected_revision
                        || intent.plan.fields().plan_id != request.id
                        || intent.reason != request.reason
                    {
                        return Err(refused(
                            "same-operation withdrawal request identity changed",
                        ));
                    }
                    record
                } else {
                    if barriers.held_plans.contains(&request.id) {
                        return Err(refused("Plan has an active closure"));
                    }
                    let before = snapshot.workspace();
                    let node = before.get(&request.id).ok_or_else(|| {
                        crate::domain::error::MineError::PlanNotFound {
                            plan_id: request.id.clone(),
                        }
                    })?;
                    if !matches!(
                        node.status,
                        PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready
                    ) {
                        return Err(crate::domain::error::MineError::InvalidTransition {
                            plan_id: request.id.clone(),
                            from: node.status.as_str().into(),
                            to: "WITHDRAWN".into(),
                        });
                    }
                    super::plan_service::authority_observation(snapshot, &request.id)?;
                    let at = UtcTimestamp::new(SystemClock.now_utc_rfc3339())
                        .map_err(|e| refused(e.to_string()))?;
                    let mut after = before.clone();
                    let target = after.get_mut(&request.id).expect("subject checked");
                    target.status = PlanStatus::Withdrawn;
                    target.withdraw_reason = request.reason.clone();
                    target.withdrawn_at = at.as_str().into();
                    target.updated_at = at.as_str().into();
                    after.revision = request
                        .expected_revision
                        .checked_add(1)
                        .ok_or_else(|| refused("graph revision overflow"))?;
                    after.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
                    crate::domain::validation::validate_native_successor(before, &after)?;
                    kernel
                        .begin_with_withdrawal(
                            request.operation_id.clone(),
                            request.expected_revision,
                            &request.actor,
                            &after,
                            WithdrawInput {
                                id: request.id.clone(),
                                reason: request.reason.clone(),
                                at,
                            },
                            observer,
                        )
                        .map_err(|e| operation_error(&kernel, &request.operation_id, e))?
                };
                let replay = terminal(&record.operation);
                let completed = if replay {
                    kernel
                        .replay_completed(&record)
                        .map_err(|e| operation_error(&kernel, &request.operation_id, e))?;
                    record
                } else {
                    kernel
                        .resume(&request.operation_id, observer)
                        .map_err(|e| operation_error(&kernel, &request.operation_id, e))?
                };
                let r = completed
                    .checkpoint
                    .as_ref()
                    .ok_or_else(|| refused("withdrawal runtime missing"))?;
                let current: PlanWorkspace = toml::from_str(
                    std::str::from_utf8(&bytes(root, &r.source_path)?)
                        .map_err(|_| refused("graph encoding"))?,
                )
                .map_err(|_| refused("withdrawal graph parse"))?;
                let frozen: PlanWorkspace = toml::from_str(&r.source_after)
                    .map_err(|_| refused("withdrawal candidate parse"))?;
                if current.get(&request.id) != frozen.get(&request.id)
                    || bytes(root, &r.view_path)? != render_markdown(&current)?.as_bytes()
                {
                    return Err(refused(
                        "withdrawal result differs from exact committed candidate",
                    ));
                }
                let PlanGitEffects::Withdrawal { intent, .. } = &r.plan_git else {
                    return Err(refused("withdrawal intent lost"));
                };
                let before: PlanWorkspace = toml::from_str(
                    r.source_before()
                        .map_err(|_| refused("withdrawal requires original graph"))?,
                )
                .map_err(|_| refused("withdrawal before parse"))?;
                let withdrawal = PlanWithdrawal {
                    id: request.id.clone(),
                    status_before: before
                        .get(&request.id)
                        .ok_or_else(|| refused("withdrawal original Plan missing"))?
                        .status
                        .as_str()
                        .into(),
                    status_after: "WITHDRAWN".into(),
                    withdrawn_at: intent.withdrawn_at.as_str().into(),
                    cleaned_git_artifacts: !matches!(
                        intent.before_ref,
                        crate::infrastructure::graph_checkpoint::plan_git::RefState::Absent
                    ),
                    revision_before,
                    revision_after: current.revision,
                    operation: completed.operation,
                    replay,
                };
                Ok((current, withdrawal))
            },
        );
        self.graph.invalidate_observation();
        result
    }
}
fn operation_error(
    kernel: &CheckpointKernel<'_>,
    id: &OperationId,
    error: OperationError,
) -> crate::domain::error::MineError {
    // The deadline never grants rollback. A durable intent/attempt may already
    // exist even when the graph revision is unchanged; query without launching
    // another child and expose that exact pending identity to CLI/MCP recovery.
    if let Ok(record) = kernel.store().query(id) {
        let revision_after = kernel.observed_revision();
        return crate::domain::error::MineError::GraphCheckpointPending {
            operation: Box::new(record.operation),
            revision_after,
            detail: error.to_string(),
        };
    }
    match error {
        OperationError::Mine(error) => error,
        other => refused(format!(
            "operation {} requires exact query/recovery: {other}",
            id.as_str()
        )),
    }
}
