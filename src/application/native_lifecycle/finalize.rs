//! Acceptance publication binds the existing closure to one native checkpoint.
//! The source transition itself is evidence, not a reason to mint new receipts.
use super::*;
use crate::application::native_gate::TerminalDelta;
use crate::domain::{ports::Clock, validation};
use crate::infrastructure::{
    graph_checkpoint::{CheckpointKernel, CheckpointRuntime},
    operation_store::JournalRecord,
    system::SystemClock,
    toml_store::render_markdown,
};

#[derive(Debug, serde::Serialize)]
pub struct FinalizeOutcome {
    #[serde(skip)]
    pub(crate) workspace: Option<PlanWorkspace>,
    pub operation: LifecycleOperation,
    /// Journal-only handoff completion has no new graph checkpoint to claim.
    pub checkpoint_operation_id: Option<OperationId>,
    pub revision_before: u64,
    pub revision_after: u64,
    pub checks: Vec<GateAdmission>,
    pub replay: bool,
}

fn candidate(
    before: &PlanWorkspace,
    operation: &LifecycleOperation,
    updated_at: &str,
) -> OperationResult<PlanWorkspace> {
    let id = &operation
        .fields()
        .plan
        .as_ref()
        .ok_or(OperationError::Journal)?
        .fields()
        .plan_id;
    let mut after = before.clone();
    let target = after.get_mut(id).ok_or(OperationError::Journal)?;
    if target.status != PlanStatus::Implemented {
        return Err(OperationError::Refused(
            "acceptance requires IMPLEMENTED input",
        ));
    }
    target.status = PlanStatus::Accepted;
    target.review_report = operation
        .fields()
        .review
        .as_ref()
        .ok_or(OperationError::Journal)?
        .report
        .path
        .as_str()
        .into();
    target.acceptance = Some(operation.clone());
    target.updated_at = updated_at.into();
    let accepted = after
        .plans
        .iter()
        .filter(|p| p.status == PlanStatus::Accepted)
        .map(|p| p.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for successor in &mut after.plans {
        if successor.status == PlanStatus::Blocked
            && !successor.hard_predecessors.is_empty()
            && successor
                .hard_predecessors
                .iter()
                .all(|p| accepted.contains(p))
        {
            successor.status = PlanStatus::Ready;
            successor.updated_at = updated_at.into();
        }
    }
    after.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
    after.revision = before
        .revision
        .checked_add(1)
        .ok_or(OperationError::Journal)?;
    validation::validate_native_successor(before, &after)?;
    Ok(after)
}

fn validate_delta(
    record: &JournalRecord,
    operation: &LifecycleOperation,
    config: &crate::domain::config::MineConfig,
    root: &Path,
) -> OperationResult<()> {
    let runtime = record.checkpoint.as_ref().ok_or(OperationError::Journal)?;
    let before: PlanWorkspace =
        toml::from_str(runtime.source_before()?).map_err(|_| OperationError::Journal)?;
    let after: PlanWorkspace =
        toml::from_str(&runtime.source_after).map_err(|_| OperationError::Journal)?;
    validation::validate_with_worktrees_root(&before, &config.plan.worktrees_root)?;
    validation::validate_with_worktrees_root(&after, &config.plan.worktrees_root)?;
    let id = &operation
        .fields()
        .plan
        .as_ref()
        .ok_or(OperationError::Journal)?
        .fields()
        .plan_id;
    let target = after.get(id).ok_or(OperationError::Journal)?;
    UtcTimestamp::new(target.updated_at.clone())?;
    if record.operation.fields().actor != operation.fields().actor
        || record.operation.fields().repository_id != operation.fields().repository_id
        || record.operation.fields().workspace_id != operation.fields().workspace_id
        || before.workspace_id != operation.fields().workspace_id
        || before.revision != record.operation.fields().observed_graph_revision
        || candidate(&before, operation, &target.updated_at)? != after
        || render_markdown(&before)? != runtime.view_before()?
        || render_markdown(&after)? != runtime.view_after
        || runtime.root != root.canonicalize()?
        || runtime.source_path.as_str() != config.graph.source
        || runtime.view_path.as_str() != config.graph.rendered
        || runtime.integration_ref != format!("refs/heads/{}", config.branches.integration)
        || before.integration_branch != config.branches.integration
        || before.stable_branch != config.branches.stable
        || runtime.config_sha256 != digest(&bytes(root, &RepoPath::new(".mine/config.toml")?)?)
    {
        return Err(OperationError::Refused(
            "foreign or broader terminal graph delta",
        ));
    }
    Ok(())
}

fn find_checkpoint(
    store: &OperationStore<'_>,
    operation: &LifecycleOperation,
    config: &crate::domain::config::MineConfig,
) -> OperationResult<Option<JournalRecord>> {
    let found = store.checkpoint_for_acceptance(operation)?;
    if let Some(record) = &found {
        validate_delta(record, operation, config, store.root())?;
    }
    Ok(found)
}

impl NativeLifecycle<'_, '_> {
    pub fn finalize(&self, expected_revision: u64, path: &RepoPath) -> MineResult<FinalizeOutcome> {
        self.finalize_subject(expected_revision, path, None)
    }
    pub(crate) fn finalize_for_plan(
        &self,
        expected_revision: u64,
        path: &RepoPath,
        id: &str,
        operation_id: &OperationId,
    ) -> MineResult<PlanWorkspace> {
        self.finalize_subject(expected_revision, path, Some((id, operation_id)))
            .and_then(|result| {
                result
                    .workspace
                    .ok_or_else(|| refused("Plan acceptance requires a live graph result"))
            })
    }
    fn finalize_subject(
        &self,
        expected_revision: u64,
        path: &RepoPath,
        subject: Option<(&str, &OperationId)>,
    ) -> MineResult<FinalizeOutcome> {
        let root = self.graph.repo_root();
        if subject.is_none() {
            let request: StageInput = input(root, path)?;
            contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
            if request.operation.fields().kind == OperationKind::Release {
                return super::super::release_service::retained::finalize(
                    root,
                    expected_revision,
                    request,
                );
            }
        }
        // The first checkpoint can be pending before a graph snapshot exists.
        // Inspect only its exact typed journal route; ordinary Plan closures
        // still require their live workspace snapshot and existing admission.
        if subject.is_none()
            && matches!(
                self.graph.observed_revision(),
                Err(crate::domain::error::MineError::GraphNotInitialized { .. })
            )
        {
            let proposed: StageInput = input(root, path)?;
            if proposed.operation.fields().kind == OperationKind::GraphCheckpoint {
                if let Some(result) = self.finalize_initial(
                    expected_revision,
                    path,
                    &proposed.operation.fields().operation_id,
                )? {
                    return Ok(result);
                }
            }
        }
        let result = self
            .graph
            .observe_locked(expected_revision, |snapshot, guard| {
                let request: StageInput = input(root, path)?;
                contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
                let config = super::super::plan_service::root_configuration(root)?;
                let store = OperationStore::under_lock(
                    root,
                    &config.repository_id,
                    &snapshot.workspace().workspace_id,
                    guard,
                )
                .map_err(|e| refused(e.to_string()))?;
                let service = OperationService::new(&store);
                let operation = request.operation;
                let fields = operation.fields();
                if subject.is_some_and(|(id, operation_id)| {
                    &fields.operation_id != operation_id
                        || fields
                            .plan
                            .as_ref()
                            .is_none_or(|plan| plan.fields().plan_id != id)
                }) {
                    return Err(refused("plan accept subject or operation identity differs"));
                }
                let query = OperationQuery::new(
                    config.repository_id.clone(),
                    snapshot.workspace().workspace_id.clone(),
                    fields.operation_id.clone(),
                )
                .map_err(|e| refused(e.to_string()))?;
                let old = service.query(&query).map_err(|e| refused(e.to_string()))?;
                if fields.kind == OperationKind::GraphCheckpoint {
                    if subject.is_some() || request.verification.is_some()
                        || old != operation || request.expected_stage_revision != old.fields().stage_revision {
                        return Err(refused("checkpoint recovery requires exact current operation and stage, without replacement input"));
                    }
                    let kernel = CheckpointKernel::under_lock(root,guard).map_err(|e|refused(e.to_string()))?;
                    if kernel.store().barriers().map_err(|e|refused(e.to_string()))?.global.iter()
                        .any(|pending|pending.operation_id != fields.operation_id) {
                        return Err(refused("another pending publication blocks checkpoint recovery"));
                    }
                    let record = kernel.store().query(&fields.operation_id).map_err(|e|refused(e.to_string()))?;
                    if record.checkpoint.is_none() {return Err(refused("native checkpoint runtime missing"));}
                    let replay = terminal(&record.operation);
                    let completed = if replay {
                        kernel.replay_completed(&record).map_err(|e|refused(e.to_string()))?;
                        record
                    } else {
                        kernel.resume(&fields.operation_id,&mut NoInterruption).map_err(|e| {
                            match kernel.store().query(&fields.operation_id) {
                                Ok(current)=>crate::domain::error::MineError::GraphCheckpointPending {operation:Box::new(current.operation),
                                    revision_after:std::fs::read_to_string(root.join(&config.graph.source)).ok().and_then(|s|toml::from_str::<PlanWorkspace>(&s).ok()).map(|w|w.revision),detail:e.to_string()},
                                Err(query)=>refused(format!("checkpoint outcome unknown: {e}; journal unavailable: {query}")),
                            }
                        })?
                    };
                    let current: PlanWorkspace = toml::from_str(std::str::from_utf8(&bytes(root,&RepoPath::new(config.graph.source.clone()).map_err(|e|refused(e.to_string()))?)?).map_err(|_|refused("checkpoint graph encoding"))?).map_err(|_|refused("checkpoint graph unavailable"))?;
                    return Ok(FinalizeOutcome {revision_before:snapshot.workspace().revision,revision_after:current.revision,
                        workspace:Some(current),checkpoint_operation_id:Some(fields.operation_id.clone()),operation:completed.operation,checks:vec![],replay});
                }
                if fields.kind == OperationKind::RejectedHandoff {
                    if subject.is_some() || request.verification.is_some()
                        || fields.stage != OperationStage::RejectedHandoff(RejectionStage::Complete)
                        || fields.outcome != OperationOutcome::Committed {
                        return Err(refused("handoff finalize requires exact terminal rejection progress, without acceptance or check input"));
                    }
                    let replay = old == operation;
                    if !replay {
                        if old.fields().stage != OperationStage::RejectedHandoff(RejectionStage::Released)
                            || old.fields().stage_revision != request.expected_stage_revision {
                            return Err(refused("handoff finalize requires exact released-stage revision"));
                        }
                        validate_progress(&old,&operation).map_err(|e|refused(e.to_string()))?;
                    } else if request.expected_stage_revision != fields.stage_revision
                        && request.expected_stage_revision.checked_add(1) != Some(fields.stage_revision) {
                        return Err(refused("completed handoff replay stage revision differs"));
                    }
                    if !store.barriers().map_err(|e|refused(e.to_string()))?.global.is_empty() {
                        return Err(refused("pending graph publication blocks handoff completion"));
                    }
                    let plan = fields.plan.as_ref().ok_or_else(||refused("handoff Plan identity missing"))?;
                    super::super::plan_service::authority_observation(snapshot,&plan.fields().plan_id)?;
                    let node = snapshot.workspace().get(&plan.fields().plan_id).ok_or_else(||refused("handoff Plan missing"))?;
                    if fields.review.as_ref().is_none_or(|r|node.review_report != r.report.path.as_str()) {
                        return Err(refused("rejected graph report differs from frozen handoff"));
                    }
                    let proof = ClosureProof {root,workspace:snapshot.workspace(),operation:&operation,gate:None};
                    let mut provider = FilesystemOperationEvidence::new(root,proof);
                    let saved = service.stage(&query,request.expected_stage_revision,operation.clone(),&mut provider).map_err(|e|refused(e.to_string()))?;
                    return Ok(FinalizeOutcome {workspace:Some(snapshot.workspace().clone()),operation:saved,
                        checkpoint_operation_id:None,revision_before:expected_revision,revision_after:expected_revision,checks:vec![],replay});
                }
                if fields.stage != OperationStage::Acceptance(AcceptanceStage::Accepted)
                    || fields.outcome != OperationOutcome::Committed
                {
                    return Err(refused(
                        "this finalize adapter requires terminal acceptance",
                    ));
                }
                let replay = old == operation;
                if !replay {
                    if old.fields().stage != OperationStage::Acceptance(AcceptanceStage::Cleaned)
                        || old.fields().stage_revision != request.expected_stage_revision
                    {
                        return Err(refused(
                            "acceptance finalize requires exact cleaned-stage revision",
                        ));
                    }
                    validate_progress(&old, &operation).map_err(|e| refused(e.to_string()))?;
                } else if request.expected_stage_revision != fields.stage_revision
                    && request.expected_stage_revision.checked_add(1) != Some(fields.stage_revision)
                {
                    return Err(refused("terminal replay revision differs"));
                }
                let subject = fields
                    .plan
                    .as_ref()
                    .ok_or_else(|| refused("acceptance Plan missing"))?;
                super::super::plan_service::authority_observation(
                    snapshot,
                    &subject.fields().plan_id,
                )?;
                let node = snapshot
                    .workspace()
                    .get(&subject.fields().plan_id)
                    .ok_or_else(|| refused("acceptance node missing"))?;
                let existing = find_checkpoint(&store, &operation, &config)
                    .map_err(|e| refused(e.to_string()))?;
                let barriers = store.barriers().map_err(|e| refused(e.to_string()))?;
                if barriers.global.iter().any(|pending| {
                    existing.as_ref().is_none_or(|record| {
                        pending.operation_id != record.operation.fields().operation_id
                    })
                }) {
                    return Err(refused("foreign pending publication blocks finalize"));
                }
                let proof = GateProof::new(root, &operation, expected_revision)
                    .map_err(|e| refused(e.to_string()))?;
                if replay {
                    let checkpoint =
                        existing.ok_or_else(|| refused("terminal checkpoint history missing"))?;
                    if !terminal(&checkpoint.operation)
                        || node.acceptance.as_ref() != Some(&operation)
                    {
                        return Err(refused(
                            "terminal replay differs from committed graph/checkpoint",
                        ));
                    }
                    historical_checkpoint(root, &checkpoint).map_err(|e| refused(e.to_string()))?;
                    let provider_proof = ClosureProof {
                        root,
                        workspace: snapshot.workspace(),
                        operation: &operation,
                        gate: Some(&proof),
                    };
                    let mut provider = FilesystemOperationEvidence::new(root, provider_proof);
                    provider
                        .observe(&operation, &ClosureBinding::Acceptance)
                        .map_err(|e| refused(e.to_string()))?;
                    return Ok(FinalizeOutcome {
                        workspace: Some(snapshot.workspace().clone()),
                        operation: operation.clone(),
                        checkpoint_operation_id: Some(checkpoint.operation.fields().operation_id.clone()),
                        revision_before: expected_revision,
                        revision_after: expected_revision,
                        checks: vec![],
                        replay: true,
                    });
                }
                let gate = request.verification.as_ref().ok_or_else(|| {
                    refused("current verification selections required at finalize")
                })?;
                let delta = existing
                    .as_ref()
                    .and_then(|r| r.checkpoint.as_ref())
                    .map(TerminalDelta);
                let mut checks = proof
                    .admit(gate, &old, expected_revision, &node.owner, delta.as_ref())
                    .map_err(|e| refused(e.to_string()))?;
                if existing.is_some() {
                    for check in &mut checks {
                        check.retain_admission();
                    }
                }
                // Recheck pinned stop/body/report before resuming any known effect.
                let stopped = ClosureProof {
                    root,
                    workspace: snapshot.workspace(),
                    operation: &old,
                    gate: Some(&proof),
                };
                stopped
                    .writer_stopped(subject, None)
                    .map_err(|e| refused(e.to_string()))?;
                let kernel = CheckpointKernel::under_lock(root, guard)
                    .map_err(|e| refused(e.to_string()))?;
                let checkpoint = if let Some(record) = existing {
                    record
                } else {
                    let provider_proof = ClosureProof {
                        root,
                        workspace: snapshot.workspace(),
                        operation: &old,
                        gate: Some(&proof),
                    };
                    // The provider observes the pre-terminal graph, not a fictional
                    // already-accepted status. Its evidence identities are frozen.
                    let old_proof = GateProof::new(root, &old, expected_revision)
                        .map_err(|e| refused(e.to_string()))?;
                    let provider_proof = ClosureProof {
                        gate: Some(&old_proof),
                        ..provider_proof
                    };
                    let mut provider = FilesystemOperationEvidence::new(root, provider_proof);
                    provider
                        .observe(&old, &ClosureBinding::Acceptance)
                        .map_err(|e| refused(e.to_string()))?;
                    let after = candidate(
                        snapshot.workspace(),
                        &operation,
                        &SystemClock.now_utc_rfc3339(),
                    )
                    .map_err(|e| refused(e.to_string()))?;
                    kernel
                        .begin(
                            OperationId::generate(),
                            expected_revision,
                            &fields.actor,
                            &after,
                            &mut NoInterruption,
                        )
                        .map_err(|e| refused(e.to_string()))?
                };
                let completed = kernel
                    .resume(
                        &checkpoint.operation.fields().operation_id,
                        &mut NoInterruption,
                    )
                    .map_err(|e| refused(e.to_string()))?;
                validate_delta(&completed, &operation, &config, root)
                    .map_err(|e| refused(e.to_string()))?;
                let runtime = completed
                    .checkpoint
                    .as_ref()
                    .ok_or_else(|| refused("completed checkpoint runtime missing"))?;
                let current: PlanWorkspace = toml::from_str(
                    &String::from_utf8(bytes(root, &runtime.source_path)?)
                        .map_err(|_| refused("graph encoding"))?,
                )
                .map_err(|_| refused("completed graph parse"))?;
                let post_proof = GateProof::new(root, &operation, current.revision)
                    .map_err(|e| refused(e.to_string()))?;
                post_proof
                    .admit(
                        gate,
                        &old,
                        current.revision,
                        &node.owner,
                        Some(&TerminalDelta(runtime)),
                    )
                    .map_err(|e| refused(e.to_string()))?;
                let provider_proof = ClosureProof {
                    root,
                    workspace: &current,
                    operation: &operation,
                    gate: Some(&post_proof),
                };
                let mut provider = FilesystemOperationEvidence::new(root, provider_proof);
                let saved = service
                    .stage(
                        &query,
                        request.expected_stage_revision,
                        operation.clone(),
                        &mut provider,
                    )
                    .map_err(|e| refused(e.to_string()))?;
                Ok(FinalizeOutcome {
                    operation: saved,
                    checkpoint_operation_id: Some(completed.operation.fields().operation_id.clone()),
                    revision_before: expected_revision,
                    revision_after: current.revision,
                    checks,
                    replay: false,
                    workspace: Some(current),
                })
            });
        // Effects may have been produced even on an uncertain error.
        self.graph.invalidate_observation();
        result
    }

    fn finalize_initial(
        &self,
        expected_revision: u64,
        path: &RepoPath,
        id: &OperationId,
    ) -> MineResult<Option<FinalizeOutcome>> {
        use crate::infrastructure::{
            graph_checkpoint::GraphBefore, operation_store::RepositoryJournal,
            toml_store::TomlStore,
        };
        let root = self.graph.repo_root();
        let config = super::super::plan_service::root_configuration(root)?;
        let configured = TomlStore::from_configuration(root, &config)?;
        configured.with_coordinator_lock(|guard| {
            let journal = RepositoryJournal::under_lock(root, &config.repository_id, guard).map_err(|e| refused(e.to_string()))?;
            journal.observations().map_err(|e| refused(e.to_string()))?;
            let record = journal.query(id).map_err(|e| refused(e.to_string()))?;
            if !matches!(record.checkpoint.as_ref().map(|r| &r.before), Some(GraphBefore::Absent { .. })) {
                return Ok(None);
            }
            let actual = match configured.observed_revision() {
                Ok(revision) => revision,
                Err(crate::domain::error::MineError::GraphNotInitialized { .. }) => 0,
                Err(error) => return Err(error),
            };
            if actual != expected_revision {
                return Err(crate::domain::error::MineError::RevisionConflict { expected: expected_revision, actual });
            }
            let request: StageInput = input(root, path)?;
            contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
            if request.operation != record.operation || request.expected_stage_revision != record.operation.fields().stage_revision
                || request.verification.is_some()
            {
                return Err(refused("initial recovery requires the exact current operation/stage without replacement input"));
            }
            let kernel = CheckpointKernel::initial_intent_under_lock(root, guard, id).map_err(|e| refused(e.to_string()))?;
            let replay = terminal(&record.operation);
            let completed = if replay {
                kernel.replay_completed(&record).map_err(|e| refused(e.to_string()))?;
                record
            } else {
                kernel.resume(id, &mut NoInterruption).map_err(|error| {
                    match kernel.store().query(id) {
                        Ok(current) => crate::domain::error::MineError::GraphCheckpointPending {
                            operation: Box::new(current.operation), revision_after: configured.observed_revision().ok(), detail: error.to_string(),
                        },
                        Err(query) => refused(format!("initial outcome unknown: {error}; journal unavailable: {query}")),
                    }
                })?
            };
            let workspace = configured.load()?;
            Ok(Some(FinalizeOutcome {
                revision_before: actual, revision_after: workspace.revision, workspace:Some(workspace),
                operation: completed.operation, checkpoint_operation_id: Some(id.clone()), checks: Vec::new(), replay,
            }))
        })
    }
}

/// A completed checkpoint may be an ancestor after other accepted work. This
/// validates historical production, without pretending it matches current keys.
fn historical_checkpoint(root: &Path, record: &JournalRecord) -> OperationResult<()> {
    let runtime: &CheckpointRuntime = record.checkpoint.as_ref().ok_or(OperationError::Journal)?;
    let commit = runtime
        .candidate_commit
        .as_ref()
        .ok_or(OperationError::Journal)?;
    let mut git = checkpoint_git(root, None)?;
    git.validate_commits(&[
        commit.as_str().into(),
        record
            .operation
            .fields()
            .checkpoint
            .as_ref()
            .ok_or(OperationError::Journal)?
            .fields()
            .original_head
            .as_str()
            .into(),
    ])?;
    let head = git.text(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    let original = &record
        .operation
        .fields()
        .checkpoint
        .as_ref()
        .ok_or(OperationError::Journal)?
        .fields()
        .original_head;
    let parents = git.text(&["rev-list", "--parents", "-n", "1", commit.as_str()])?;
    let changed = git.run(
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            original.as_str(),
            commit.as_str(),
            "--",
        ],
        b"",
    )?;
    let changed = changed
        .strip_suffix(&[0])
        .ok_or(OperationError::Evidence("terminal delta framing"))?
        .split(|v| *v == 0)
        .collect::<std::collections::BTreeSet<_>>();
    let allowed = [
        runtime.source_path.as_str().as_bytes(),
        runtime.view_path.as_str().as_bytes(),
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    if parents != format!("{} {}", commit.as_str(), original.as_str()) || changed != allowed {
        return Err(OperationError::Refused(
            "terminal checkpoint has an unauthorized tree delta or parent",
        ));
    }
    if !git.is_ancestor(commit.as_str(), &head)?
        || git.text(&["rev-parse", &format!("{}^{{tree}}", commit.as_str())])?
            != runtime
                .candidate_tree
                .as_ref()
                .ok_or(OperationError::Journal)?
                .as_str()
    {
        return Err(OperationError::Refused(
            "terminal checkpoint no longer bound to integration history",
        ));
    }
    committed_bytes(
        root,
        commit,
        &runtime.source_path,
        runtime.source_after.as_bytes(),
    )?;
    committed_bytes(
        root,
        commit,
        &runtime.view_path,
        runtime.view_after.as_bytes(),
    )?;
    committed_bytes(
        root,
        original,
        &runtime.source_path,
        runtime.source_before()?.as_bytes(),
    )?;
    committed_bytes(
        root,
        original,
        &runtime.view_path,
        runtime.view_before()?.as_bytes(),
    )?;
    Ok(())
}
