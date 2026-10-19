//! Concrete Root/graph/Git/report bridge and phase-aware native closure dispatch.
//! Local handoff assertions are attributable evidence, never an OS writer fence.
use super::native_gate::{GateAdmission, GateInput, GateProof};
use super::{
    graph_service::GraphService,
    native_evidence::{bytes, committed_bytes, committed_files, input, one_fenced_json, refused},
    operation_service::*,
};
use crate::domain::{
    error::{MineResult, contract_text, contract_version},
    graph::*,
    lifecycle_operation::*,
    status::PlanStatus,
};
use crate::infrastructure::{
    graph_checkpoint::{checkpoint_git, digest},
    operation_store::OperationStore,
};
use serde::Deserialize;
use std::path::Path;
pub(crate) mod finalize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosureAssertion {
    pub schema_version: u32,
    pub plan: PlanIdentity,
    pub writer_owner: String,
    pub writer_run_id: String,
    pub reviewer: String,
    pub independent_review: bool,
    pub writer_stopped: bool,
    pub stopped_at: UtcTimestamp,
    pub stop_basis: String,
}

/// Shared attributable assertion check; not authentication or an OS fence.
pub(crate) fn validate_closure_assertion(
    assertion: &ClosureAssertion,
    node: &PlanNode,
    operation: &LifecycleOperation,
) -> OperationResult<()> {
    let plan = operation
        .fields()
        .plan
        .as_ref()
        .ok_or(OperationError::Journal)?;
    let operation = operation.fields();
    contract_version(assertion.schema_version)?;
    contract_text("writer_owner", &assertion.writer_owner)?;
    contract_text("writer_run_id", &assertion.writer_run_id)?;
    contract_text("reviewer", &assertion.reviewer)?;
    if assertion.plan != *plan
        || assertion.writer_owner != node.owner
        || assertion.writer_run_id != node.run_id
        || assertion.reviewer != operation.actor
        || assertion.reviewer == assertion.writer_owner
        || !assertion.independent_review
        || !assertion.writer_stopped
        || assertion.stop_basis.trim().is_empty()
        || assertion.stop_basis.len() > 4096
        || assertion
            .stop_basis
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(OperationError::Refused(
            "independent review or stopped-writer binding differs",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClosureIntent {
    Release {
        candidate: CommitId,
        candidate_root: RepoPath,
        attribution: Vec<String>,
        policy_path: RepoPath,
    },
    Acceptance {
        plan_id: String,
        review: ReviewEvidence,
    },
    RejectedHandoff {
        plan_id: String,
        review: ReviewEvidence,
        compensation: PlanIdentity,
    },
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginInput {
    pub schema_version: u32,
    pub actor: String,
    pub intent: ClosureIntent,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageInput {
    pub schema_version: u32,
    pub expected_stage_revision: u64,
    pub operation: LifecycleOperation,
    pub verification: Option<GateInput>,
}
#[derive(Debug, serde::Serialize)]
pub struct StageOutcome {
    pub operation: LifecycleOperation,
    pub checks: Vec<GateAdmission>,
}

struct ClosureProof<'a> {
    root: &'a Path,
    workspace: &'a PlanWorkspace,
    operation: &'a LifecycleOperation,
    gate: Option<&'a GateProof<'a>>,
}
impl VerificationEvidenceProvider for ClosureProof<'_> {
    fn validate(
        &self,
        evidence: &FileIdentity,
        operation: &LifecycleOperation,
    ) -> OperationResult<()> {
        if operation != self.operation {
            return Err(OperationError::Refused("foreign gate operation"));
        }
        self.gate
            .ok_or(OperationError::Evidence(
                "current verification gate input required",
            ))?
            .history(evidence)
    }
    fn writer_stopped(
        &self,
        plan: &PlanIdentity,
        committed: Option<&CommittedFilesProof>,
    ) -> OperationResult<()> {
        let operation = self.operation.fields();
        if operation.plan.as_ref() != Some(plan) {
            return Err(OperationError::Refused("foreign stopped-writer Plan"));
        }
        let node = self
            .workspace
            .get(&plan.fields().plan_id)
            .ok_or(OperationError::Evidence("writer node missing"))?;
        let review = operation
            .review
            .as_ref()
            .ok_or(OperationError::Evidence("pinned review missing"))?;
        let report_root = if operation.kind == OperationKind::RejectedHandoff
            || operation.integrated_commit.is_some()
        {
            self.root.to_path_buf()
        } else {
            crate::infrastructure::operation_store::regular_path(
                self.root,
                operation
                    .registered_worktree
                    .as_ref()
                    .ok_or(OperationError::Journal)?,
            )?
        };
        let body = bytes(&report_root, &review.report.path)?;
        if body.len() as u64 != review.report.bytes || digest(&body) != review.report.sha256 {
            return Err(OperationError::Refused("pinned closure report changed"));
        }
        let assertion: ClosureAssertion = one_fenced_json(&body, "mine-review-closure")?;
        let report_commit = if operation.kind == OperationKind::RejectedHandoff {
            let mut git = checkpoint_git(self.root, None)?;
            CommitId::new(git.text(&["rev-parse", "--verify", "HEAD^{commit}"])?)?
        } else {
            review.reviewed_tip.clone()
        };
        let plan_root = if operation.integrated_commit.is_some() {
            self.root.to_path_buf()
        } else {
            crate::infrastructure::operation_store::regular_path(
                self.root,
                operation
                    .registered_worktree
                    .as_ref()
                    .ok_or(OperationError::Journal)?,
            )?
        };
        let plan_bytes = bytes(&plan_root, &plan.fields().plan_path)?;
        if Some(&digest(&plan_bytes)) != plan.fields().body_sha256.as_ref() {
            return Err(OperationError::Refused(
                "reviewed checkout Plan body differs",
            ));
        }
        committed_bytes(
            &plan_root,
            &review.reviewed_tip,
            &plan.fields().plan_path,
            &plan_bytes,
        )?;
        let mut root_git = checkpoint_git(self.root, None)?;
        let root_head =
            CommitId::new(root_git.text(&["rev-parse", "--verify", "HEAD^{commit}"])?)?;
        let root_plan_bytes = bytes(self.root, &plan.fields().plan_path)?;
        let root_files = [
            review.report.clone(),
            FileIdentity {
                path: plan.fields().plan_path.clone(),
                bytes: root_plan_bytes.len() as u64,
                sha256: digest(&root_plan_bytes),
            },
        ];
        if let Some(committed) = committed {
            if report_root != self.root || report_commit != root_head {
                return Err(OperationError::Refused(
                    "committed report observation changed",
                ));
            }
            // Reuse immutable tree membership only within this observe call.
            // Contained bytes, attributes and native storage hashes are fresh.
            committed.reobserve(self.root, &root_head, &root_files)?;
        } else if report_root == self.root && report_commit == root_head {
            // One fresh proof for this exact Root/commit only. The reviewed
            // checkout above remains a separate proof at its own pinned tip;
            // no observation survives an observer, Git effect or request.
            committed_files(self.root, &root_head, &root_files)?;
        } else {
            committed_bytes(&report_root, &report_commit, &review.report.path, &body)?;
            committed_bytes(
                self.root,
                &root_head,
                &plan.fields().plan_path,
                &root_plan_bytes,
            )?;
        }
        validate_closure_assertion(&assertion, node, self.operation)?;
        // After integration, ancestry alone cannot detect contrary activity on
        // the still-present original Plan branch or its registered checkout.
        if operation.integrated_commit.is_some() {
            let mut git = checkpoint_git(self.root, None)?;
            if let Some(branch) = git
                .refs()?
                .iter()
                .find(|r| r.name == format!("refs/heads/{}", node.branch))
            {
                if branch.oid != review.reviewed_tip.as_str() || branch.object_type != "commit" {
                    return Err(OperationError::Refused(
                        "Plan branch advanced after independent review",
                    ));
                }
            } else if !matches!(
                operation.stage,
                OperationStage::Acceptance(AcceptanceStage::Cleaned | AcceptanceStage::Accepted)
            ) {
                return Err(OperationError::Refused(
                    "Plan branch removed before cleanup evidence",
                ));
            }
            let target = crate::infrastructure::operation_store::regular_path(
                self.root,
                operation
                    .registered_worktree
                    .as_ref()
                    .ok_or(OperationError::Journal)?,
            )?;
            if target.try_exists()? {
                let canonical_target = target.canonicalize()?;
                let inventory = git.worktrees()?;
                if !inventory.iter().any(|w| {
                    Path::new(&w.path).canonicalize().ok().as_ref() == Some(&canonical_target)
                        && w.branch.as_deref() == Some(node.branch.as_str())
                        && !w.detached
                        && !w.bare
                        && !w.prunable
                }) {
                    return Err(OperationError::Refused(
                        "foreign or replaced review checkout",
                    ));
                }
                let mut checkout = checkpoint_git(&target, None)?;
                if !checkout
                    .run(
                        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
                        b"",
                    )?
                    .is_empty()
                {
                    return Err(OperationError::Refused(
                        "review checkout changed after integration",
                    ));
                }
            }
        }
        Ok(())
    }
}

pub struct NativeLifecycle<'a, 'store> {
    graph: &'a GraphService<'store>,
}

/// Lineage-local preparation gate. It observes a completed handoff afresh but
/// neither performs global scheduling admission nor manufactures legacy proof.
pub(crate) fn observe_compensation_handoff(
    root: &Path,
    workspace: &PlanWorkspace,
    node: &PlanNode,
    observations: &[crate::infrastructure::operation_store::OperationObservation],
) -> MineResult<()> {
    if node.status != PlanStatus::Ready
        || !crate::domain::plan_id::CanonicalPlanId::parse(&node.id)?.is_compensation()
    {
        return Ok(());
    }
    let rejected = crate::domain::validation::declaring_rejected_predecessor(workspace, &node.id)?;
    let mut candidates = observations.iter().filter(|record| {
        record.operation.fields().kind == OperationKind::RejectedHandoff
            && record
                .operation
                .fields()
                .plan
                .as_ref()
                .is_some_and(|p| p.fields().plan_id == rejected.id)
    });
    let record=candidates.find(|record|record.operation.fields().stage==OperationStage::RejectedHandoff(RejectionStage::Complete)
        && record.operation.fields().outcome==OperationOutcome::Committed)
        .ok_or_else(||refused("READY compensation requires a completed current stopped handoff; query/resume its operation or use explicit legacy handoff observation"))?;
    if candidates.any(|record| {
        record.operation.fields().stage == OperationStage::RejectedHandoff(RejectionStage::Complete)
            && record.operation.fields().outcome == OperationOutcome::Committed
    }) {
        return Err(refused("ambiguous completed rejection handoff"));
    }
    let rejected_authority =
        super::plan_service::authority_from_workspace(root, workspace, &rejected.id)?;
    let compensation_authority =
        super::plan_service::authority_from_workspace(root, workspace, &node.id)?;
    let fields = record.operation.fields();
    let Some(ClosureBinding::RejectedHandoff { compensation }) = &record.closure else {
        return Err(refused("completed handoff binding missing"));
    };
    if fields.plan.as_ref() != Some(rejected_authority.plan())
        || compensation != compensation_authority.plan()
        || fields
            .review
            .as_ref()
            .is_none_or(|r| r.report.path.as_str() != rejected.review_report)
    {
        return Err(refused("completed handoff body/report identity changed"));
    }
    let proof = ClosureProof {
        root,
        workspace,
        operation: &record.operation,
        gate: None,
    };
    let mut provider = FilesystemOperationEvidence::new(root, proof);
    provider
        .observe(
            &record.operation,
            record
                .closure
                .as_ref()
                .ok_or_else(|| refused("handoff closure missing"))?,
        )
        .map_err(|e| refused(e.to_string()))
}

/// The ordinary reject transition consumes an already pinned independent
/// handoff intent. A Root-visible report string alone cannot stop a writer.
pub(crate) fn observe_rejection_intent(
    root: &Path,
    workspace: &PlanWorkspace,
    request: &super::graph_service::PlanRejectRequest,
    report_path: &str,
    guard: &crate::infrastructure::file_lock::FileLock,
) -> MineResult<()> {
    use crate::infrastructure::graph_checkpoint::plan_git::{self, CheckoutState, RefState};
    let config = super::plan_service::root_configuration(root)?;
    let store =
        OperationStore::under_lock(root, &config.repository_id, &workspace.workspace_id, guard)
            .map_err(|e| refused(e.to_string()))?;
    let observations = store.observations().map_err(|e| refused(e.to_string()))?;
    let mut matching = observations.iter().filter(|record| {
        let fields = record.operation.fields();
        fields.kind == OperationKind::RejectedHandoff
            && !terminal(&record.operation)
            && fields
                .plan
                .as_ref()
                .is_some_and(|p| p.fields().plan_id == request.id)
    });
    let record = matching.next().ok_or_else(|| {
        refused("rejection requires pinned independent operation begin before its graph transition")
    })?;
    if matching.next().is_some() {
        return Err(refused("ambiguous active rejection handoff"));
    }
    let fields = record.operation.fields();
    let plan = fields
        .plan
        .as_ref()
        .ok_or_else(|| refused("handoff Plan identity missing"))?;
    let node = workspace
        .get(&request.id)
        .ok_or_else(|| refused("rejection node missing"))?;
    let review = fields
        .review
        .as_ref()
        .ok_or_else(|| refused("pinned rejection review missing"))?;
    let Some(ClosureBinding::RejectedHandoff { compensation }) = &record.closure else {
        return Err(refused("rejection handoff compensation missing"));
    };
    let comp = compensation.fields();
    if fields.stage != OperationStage::RejectedHandoff(RejectionStage::Intent)
        || fields.outcome != OperationOutcome::Pending
        || fields.integrated_commit.is_some()
        || fields.cleanup.is_some()
        || fields.checkpoint.is_some()
        || !fields.verification_evidence.is_empty()
        || fields.actor != request.actor
        || fields.observed_graph_revision > workspace.revision
        || plan.fields().plan_path.as_str() != node.path
        || plan.fields().branch != node.branch
        || plan.fields().lineage_id != node.lineage_id
        || fields.registered_worktree.as_ref().map(RepoPath::as_str)
            != Some(node.worktree_path.as_str())
        || review.report.path.as_str() != report_path
        || comp.plan_id != request.compensating_plan
        || comp.repository_id != config.repository_id
        || comp.workspace_id != workspace.workspace_id
        || comp.lineage_id != node.lineage_id
        || comp.branch
            != crate::domain::plan_id::registered_plan_branch(
                &workspace.workspace_id,
                &comp.plan_id,
                comp.plan_path.as_str(),
            )?
    {
        return Err(refused(
            "rejection request differs from frozen independent handoff",
        ));
    }
    let root_report = bytes(root, &review.report.path)?;
    let compensation_body = bytes(root, &comp.plan_path)?;
    if root_report.len() as u64 != review.report.bytes
        || digest(&root_report) != review.report.sha256
        || comp.body_sha256.as_ref() != Some(&digest(&compensation_body))
    {
        return Err(refused(
            "Root review/compensation bytes differ from handoff",
        ));
    }
    let proof = ClosureProof {
        root,
        workspace,
        operation: &record.operation,
        gate: None,
    };
    proof
        .writer_stopped(plan, None)
        .map_err(|e| refused(e.to_string()))?;
    let mut git = checkpoint_git(root, None).map_err(|e| refused(e.to_string()))?;
    let head = CommitId::new(git.text(&["rev-parse", "--verify", "HEAD^{commit}"])?)
        .map_err(|e| refused(e.to_string()))?;
    committed_files(
        root,
        &head,
        &[
            review.report.clone(),
            FileIdentity {
                path: comp.plan_path.clone(),
                bytes: compensation_body.len() as u64,
                sha256: digest(&compensation_body),
            },
        ],
    )?;
    let common = std::path::PathBuf::from(git.text(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?)
    .canonicalize()?;
    let location = RepoPath::new(node.worktree_path.clone()).map_err(|e| refused(e.to_string()))?;
    let reference =
        plan_git::reference(&mut git, &node.branch).map_err(|e| refused(e.to_string()))?;
    let checkout = plan_git::checkout(root, &common, &location, &node.branch, &mut git)
        .map_err(|e| refused(e.to_string()))?;
    if reference
        != (RefState::Present {
            tip: review.reviewed_tip.clone(),
        })
        || !matches!(checkout,CheckoutState::Present {proof} if proof.head==review.reviewed_tip)
    {
        return Err(refused(
            "stopped rejection checkout/ref changed after review",
        ));
    }
    Ok(())
}
impl<'a, 'store> NativeLifecycle<'a, 'store> {
    pub fn new(graph: &'a GraphService<'store>) -> Self {
        Self { graph }
    }

    /// Observe one typed next stage. Skills own the actual merge/check/cleanup.
    /// Terminal graph publication is deliberately reserved for finalize.
    pub fn stage(&self, expected_revision: u64, path: &RepoPath) -> MineResult<StageOutcome> {
        let root = self.graph.repo_root();
        let request: StageInput = input(root, path)?;
        contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
        if request.operation.fields().kind == OperationKind::Release {
            return super::release_service::retained::stage(root, expected_revision, request);
        }
        self.graph.observe_locked(expected_revision, |snapshot, guard| {
            let request: StageInput = input(root, path)?;
            contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
            let workspace = snapshot.workspace();
            let config = super::plan_service::root_configuration(root)?;
            let store = OperationStore::under_lock(root, &config.repository_id, &workspace.workspace_id, guard)
                .map_err(|e| refused(e.to_string()))?;
            let operation = request.operation;
            let f = operation.fields();
            let query = OperationQuery::new(config.repository_id, workspace.workspace_id.clone(), f.operation_id.clone())
                .map_err(|e| refused(e.to_string()))?;
            let service = OperationService::new(&store);
            let old = service.query(&query).map_err(|e| refused(e.to_string()))?;
            if old != operation {
                if old.fields().stage_revision != request.expected_stage_revision {
                    return Err(refused(OperationError::Revision {expected:request.expected_stage_revision, actual:old.fields().stage_revision}.to_string()));
                }
                validate_progress(&old, &operation).map_err(|e| refused(e.to_string()))?;
            } else if request.expected_stage_revision != f.stage_revision
                && request.expected_stage_revision.checked_add(1) != Some(f.stage_revision) {
                return Err(refused("replay stage revision differs"));
            }
            if terminal(&operation) || matches!(f.stage, OperationStage::Acceptance(AcceptanceStage::Accepted)
                | OperationStage::RejectedHandoff(RejectionStage::Complete) | OperationStage::Release(_) | OperationStage::GraphCheckpoint(_)) {
                return Err(refused("terminal/release/checkpoint progression requires its native finalize adapter"));
            }
            if store.barriers().map_err(|e| refused(e.to_string()))?.global.iter().any(|pending| pending.operation_id != f.operation_id) {
                return Err(refused("another pending workspace publication blocks stage"));
            }
            let plan = f.plan.as_ref().ok_or_else(|| refused("closure Plan missing"))?;
            let id = &plan.fields().plan_id;
            super::plan_service::authority_observation(snapshot, id)?;
            let node = workspace.get(id).ok_or_else(|| refused("closure node missing"))?;
            let needs_gate = matches!(f.stage, OperationStage::Acceptance(AcceptanceStage::Verified | AcceptanceStage::Cleaned));
            let gate = if needs_gate {Some(GateProof::new(root,&operation,expected_revision).map_err(|e|refused(e.to_string()))?)} else {None};
            let checks = if needs_gate {
                gate.as_ref().expect("defined verification gate").admit(request.verification.as_ref().ok_or_else(|| refused("complete current verification selection required"))?, &old, expected_revision, &node.owner, None)
                    .map_err(|e| refused(e.to_string()))?
            } else {
                if request.verification.is_some() || !f.verification_evidence.is_empty() {
                    return Err(refused("verification evidence is admitted only at a defined check gate"));
                }
                vec![]
            };
            let proof = ClosureProof {root, workspace, operation:&operation, gate:gate.as_ref()};
            let mut provider = FilesystemOperationEvidence::new(root, proof);
            let saved = service.stage(&query, request.expected_stage_revision, operation.clone(), &mut provider)
                .map_err(|e| refused(e.to_string()))?;
            Ok(StageOutcome {operation:saved, checks})
        })
    }

    pub fn begin(&self, expected_revision: u64, path: &RepoPath) -> MineResult<LifecycleOperation> {
        let root = self.graph.repo_root();
        self.graph
            .observe_locked(expected_revision, |snapshot, guard| {
                let request: BeginInput = input(root, path)?;
                contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
                contract_text("operation.actor", &request.actor)
                    .map_err(|e| refused(e.to_string()))?;
                let config = super::plan_service::root_configuration(root)?;
                let workspace = snapshot.workspace();
                let store = OperationStore::under_lock(
                    root,
                    &config.repository_id,
                    &workspace.workspace_id,
                    guard,
                )
                .map_err(|e| refused(e.to_string()))?;
                if !store
                    .barriers()
                    .map_err(|e| refused(e.to_string()))?
                    .global
                    .is_empty()
                {
                    return Err(refused("pending workspace publication blocks new closure"));
                }
                let (id, review, binding) = match request.intent {
                    ClosureIntent::Release { candidate, candidate_root, attribution, policy_path } => {
                        return super::release_service::retained::begin_under_lock(
                            root, workspace, guard, &store,
                            super::release_service::retained::Admission {
                                actor: request.actor, candidate, candidate_root: &candidate_root, attribution: &attribution, policy_path: &policy_path,
                            },
                        ).map_err(|error| refused(error.to_string()));
                    }
                    ClosureIntent::Acceptance { plan_id, review } => {
                        (plan_id, review, ClosureBinding::Acceptance)
                    }
                    ClosureIntent::RejectedHandoff {
                        plan_id,
                        review,
                        compensation,
                    } => (
                        plan_id,
                        review,
                        ClosureBinding::RejectedHandoff { compensation },
                    ),
                };
                let node = workspace.get(&id).ok_or_else(|| {
                    crate::domain::error::MineError::PlanNotFound {
                        plan_id: id.clone(),
                    }
                })?;
                // Approval failure is never downgraded to conservative authority.
                super::plan_service::authority_observation(snapshot, &id)?;
                let legacy_observation = binding.kind()==OperationKind::RejectedHandoff && node.status==PlanStatus::Rejected;
                if legacy_observation {
                    let observations=store.observations().map_err(|e|refused(e.to_string()))?;
                    if let Some(existing)=observations.iter().find(|record|record.operation.fields().plan.as_ref().is_some_and(|p|p.fields().plan_id==id)) {
                        return Err(refused(format!("existing node operation {} must be queried/resumed, not replaced",existing.operation.fields().operation_id.as_str())));
                    }
                    let ClosureBinding::RejectedHandoff {compensation}=&binding else {return Err(refused("legacy handoff kind differs"));};
                    let comp=workspace.get(&compensation.fields().plan_id).ok_or_else(||refused("declared READY compensation missing"))?;
                    let declared=crate::domain::validation::declared_compensation_predecessor(workspace,&comp.id)?;
                    let authority=super::plan_service::authority_observation(snapshot,&comp.id)?;
                    if declared.id!=id || comp.status!=PlanStatus::Ready || authority.plan()!=compensation
                        || workspace.plans.iter().any(|successor|successor.id!=comp.id && successor.hard_predecessors.contains(&id)) {
                        return Err(refused("legacy handoff requires exact declared READY/re-wired compensation before begin"));
                    }
                    if store.barriers().map_err(|e|refused(e.to_string()))?.held_plans.iter().any(|held|held==&comp.id) {
                        return Err(refused("declared compensation has a conflicting operation hold"));
                    }
                }
                if node.status != PlanStatus::Implemented
                    && !(binding.kind() == OperationKind::RejectedHandoff
                        && node.status == PlanStatus::InProgress)
                    && !legacy_observation
                {
                    return Err(refused(
                        "closure requires an implemented or explicitly rejected active Plan",
                    ));
                }
                let body_path =
                    RepoPath::new(node.path.clone()).map_err(|e| refused(e.to_string()))?;
                let body = digest(&bytes(root, &body_path)?);
                if node
                    .body_sha256
                    .as_ref()
                    .is_some_and(|known| known != &body)
                {
                    return Err(refused("immutable Root Plan body changed"));
                }
                let plan = PlanIdentity::new(PlanIdentityFields {
                    repository_id: config.repository_id,
                    workspace_id: workspace.workspace_id.clone(),
                    plan_id: id,
                    plan_path: body_path,
                    body_sha256: Some(body),
                    lineage_id: node.lineage_id.clone(),
                    branch: node.branch.clone(),
                })
                .map_err(|e| refused(e.to_string()))?;
                let operation = LifecycleOperation::new(OperationFields {
                    schema_version: 1,
                    operation_id: OperationId::generate(),
                    stage_revision: 0,
                    kind: binding.kind(),
                    stage: initial_stage(binding.kind()),
                    outcome: OperationOutcome::Pending,
                    repository_id: plan.fields().repository_id.clone(),
                    workspace_id: workspace.workspace_id.clone(),
                    actor: request.actor,
                    observed_graph_revision: expected_revision,
                    plan: Some(plan),
                    registered_worktree: Some(
                        RepoPath::new(node.worktree_path.clone())
                            .map_err(|e| refused(e.to_string()))?,
                    ),
                    review: Some(review),
                    integrated_commit: None,
                    verification_evidence: vec![],
                    cleanup: None,
                    checkpoint: None,
                    diagnostics: vec![],
                })
                .map_err(|e| refused(e.to_string()))?;
                let proof = ClosureProof {
                    root,
                    workspace,
                    operation: &operation,
                    gate: None,
                };
                let mut provider = FilesystemOperationEvidence::new(root, proof);
                OperationService::new(&store)
                    .begin(operation.clone(), binding, &mut provider)
                    .map_err(|e| refused(e.to_string()))
            })
    }
}
