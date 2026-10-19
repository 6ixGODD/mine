//! Native prepare composition, sharing the parent's single Root transaction.
use super::*;
use crate::application::{
    native_evidence,
    plan_service::{PlanLookup, PlanPreparation, PlanPrepareRequest},
};
use crate::infrastructure::graph_checkpoint::{
    checkpoint_git, digest,
    plan_git::{self, CheckoutState, RefState},
    preparation_history, preparation_intent,
};

impl NativePlanWorkspace<'_, '_> {
    pub fn prepare(
        &self,
        request: PlanPrepareRequest,
        caller: &std::path::Path,
        observer: &mut impl BoundaryObserver,
    ) -> MineResult<PlanPreparation> {
        contract_text("operation.actor", &request.actor).map_err(|e| refused(e.to_string()))?;
        let root = self.graph.repo_root();
        let result = self.graph.reconcile_operation_locked(request.expected_revision, &request.operation_id,
            |snapshot, guard, existing| {
                let kernel = CheckpointKernel::under_lock(root, guard).map_err(|e| refused(e.to_string()))?;
                let observations=kernel.store().observations().map_err(|e|refused(e.to_string()))?;
                let mut barriers=BarrierFacts::default();
                for record in &observations {barriers.observe(&record.operation);}
                if barriers.global.iter().any(|p| p.operation_id != request.operation_id) {
                    return Err(refused("another pending operation blocks preparation"));
                }
                let before = snapshot.workspace();
                let node = before.get(&request.id).ok_or_else(|| crate::domain::error::MineError::PlanNotFound {plan_id:request.id.clone()})?;
                let config = super::super::plan_service::root_configuration(root)?;
                let authority = super::super::plan_service::authority_observation(snapshot, &request.id)?;
                let mut git = checkpoint_git(root,None).map_err(|e| refused(e.to_string()))?;
                let mut workspace = before.clone();
                let mut operation = None;
                let mut replay = false;
                let (action, generation) = if existing {
                    let record = kernel.store().query(&request.operation_id).map_err(|e| refused(e.to_string()))?;
                    let runtime = record.checkpoint.as_ref().ok_or_else(|| refused("preparation runtime missing"))?;
                    let PlanGitEffects::Preparation {intent, ..} = &runtime.plan_git else {return Err(refused("operation is not this preparation"));};
                    if record.operation.fields().actor != request.actor
                        || record.operation.fields().observed_graph_revision != request.expected_revision
                        || intent.generation.plan.fields().plan_id != request.id || intent.mode != request.mode
                    {return Err(refused("same-operation preparation request identity changed"));}
                    let generation = intent.generation.clone();
                    if !runtime.plan_git.complete() {
                        super::super::native_lifecycle::observe_compensation_handoff(root,before,node,&observations)?;
                    }
                    replay = terminal(&record.operation);
                    let completed = if replay {
                        kernel.replay_completed(&record).map_err(|e| operation_error(&kernel,&request.operation_id,e))?;
                        record
                    } else {kernel.resume(&request.operation_id,observer).map_err(|e| operation_error(&kernel,&request.operation_id,e))?};
                    workspace = toml::from_str(std::str::from_utf8(&bytes(root,&crate::domain::graph::RepoPath::new(config.graph.source.clone()).map_err(|e| refused(e.to_string()))?)?).map_err(|_|refused("graph encoding"))?).map_err(|_|refused("completed preparation graph"))?;
                    let ordinal = generation.ordinal.checked_sub(1).and_then(|n|usize::try_from(n).ok()).ok_or_else(||refused("generation ordinal outside addressable range"))?;
                    if workspace.get(&request.id).and_then(|n|n.preparations.get(ordinal)) != Some(&generation) {
                        return Err(refused("completed generation is not retained in current graph"));
                    }
                    super::super::native_lifecycle::observe_compensation_handoff(root,&workspace,workspace.get(&request.id).ok_or_else(||refused("prepared node missing"))?,&observations)?;
                    if workspace.get(&request.id).is_some_and(|node|node.status==PlanStatus::Ready && node.preparations.last()==Some(&generation)) {
                        let runtime=completed.checkpoint.as_ref().ok_or_else(||refused("completed preparation runtime missing"))?;
                        let PlanGitEffects::Preparation {after_checkout:Some(expected),..}=&runtime.plan_git else {return Err(refused("completed preparation checkout proof missing"));};
                        let checkout=plan_git::checkout(root,&runtime.common_dir,&generation.worktree,&generation.plan.fields().branch,&mut git).map_err(|e|refused(e.to_string()))?;
                        let reference=plan_git::reference(&mut git,&generation.plan.fields().branch).map_err(|e|refused(e.to_string()))?;
                        if checkout!=(CheckoutState::Present {proof:*expected.clone()})
                            || reference!=(RefState::Present {tip:generation.prepared_branch_commit.clone()}) {
                            return Err(refused("READY completed preparation resources changed or were replaced"));
                        }
                    }
                    operation = Some(completed.operation);
                    (if replay {"replayed"} else {"recovered"},Some(generation))
                } else {
                    if !matches!(node.status,PlanStatus::Ready|PlanStatus::InProgress|PlanStatus::Implemented) {
                        return Err(crate::domain::error::MineError::InvalidTransition {plan_id:request.id.clone(),from:node.status.as_str().into(),to:"PREPARED".into()});
                    }
                    // This probe shares the kernel's actual Root identity/index/
                    // cleanliness gates, including no pending publication.
                    let (common, head) = kernel.preparation_root().map_err(|e| refused(e.to_string()))?;
                    super::super::native_lifecycle::observe_compensation_handoff(root,before,node,&observations)?;
                    let path = RepoPath::new(node.path.clone()).map_err(|e|refused(e.to_string()))?;
                    let body_bytes = bytes(root,&path)?;
                    let mut governed = node.design_references.iter().map(String::as_str).collect::<Vec<_>>();
                    governed.push(path.as_str());
                    crate::infrastructure::governance_git::consume_bodies(root, &governed)?;
                    let body = digest(&body_bytes);
                    let mut decisions = Vec::new();
                    for reference in &node.design_references {
                        let path = RepoPath::new(reference.clone()).map_err(|e|refused(e.to_string()))?;
                        let actual = bytes(root,&path)?;
                        decisions.push(FileIdentity {path,bytes:actual.len() as u64,sha256:digest(&actual)});
                    }
                    let mut files = decisions.clone();
                    files.push(FileIdentity {path:path.clone(),bytes:body_bytes.len() as u64,sha256:body.clone()});
                    native_evidence::committed_files(root,&head,&files)?;
                    let location = RepoPath::new(node.worktree_path.clone()).map_err(|e|refused(e.to_string()))?;
                    if matches!(node.status,PlanStatus::InProgress|PlanStatus::Implemented) {
                        if request.mode != PreparationMode::Initial {return Err(refused("active Plan inspection cannot create a preparation generation"));}
                        let tip = active_checkout(root,&common,node,&location,&mut git,&kernel)?;
                        let source = root.join(location.as_str());
                        let actual_body = bytes(&source,&path)?;
                        if digest(&actual_body) != body {return Err(refused("active checkout body differs from authoritative Plan"));}
                        native_evidence::committed_bytes(&source,&tip,&path,&actual_body)?;
                        if node.preparations.last().and_then(|g|g.plan.fields().body_sha256.as_ref()).is_some_and(|frozen| frozen != &body) {return Err(refused("active preparation body changed"));}
                        ("reused",node.preparations.last().cloned())
                    } else if !node.preparations.is_empty() && request.mode == PreparationMode::Initial {
                        let generation = node.preparations.last().expect("not empty");
                        if generation.plan.fields().body_sha256.as_ref() != Some(&body) {return Err(refused("prepared body changed"));}
                        let reference = plan_git::reference(&mut git,&node.branch).map_err(|e|refused(e.to_string()))?;
                        let checkout = plan_git::checkout(root,&common,&location,&node.branch,&mut git).map_err(|e|refused(e.to_string()))?;
                        let expected_checkout = preparation_intent::completed_checkout(&kernel,generation).map_err(|e|refused(e.to_string()))?;
                        if reference != (RefState::Present {tip:generation.prepared_branch_commit.clone()})
                            || !matches!(&checkout,CheckoutState::Present {proof} if *proof == expected_checkout)
                        {return Err(refused("READY checkout must remain exactly at its prepared tip"));}
                        preparation_history::prove(&mut git,&config,before,node,&head,&decisions).map_err(|e|refused(e.to_string()))?;
                        ("reused",Some(generation.clone()))
                    } else {
                        if barriers.held_plans.contains(&request.id) {return Err(refused("Plan has an active closure"));}
                        if !crate::domain::validation::hard_predecessors_accepted(before,&request.id)? {return Err(refused("preparation hard predecessor not accepted"));}
                        let scope = if node.planning_scope.is_empty() {
                            std::path::Path::new(&before.design_root).parent().and_then(std::path::Path::to_str).ok_or_else(||refused("planning scope missing"))?
                        } else {&node.planning_scope};
                        super::super::plan_service::observe_planning_readiness(root,scope,"")?;
                        let prepared = if crate::domain::plan_id::CanonicalPlanId::parse(&node.id)?.is_compensation() {
                            let rejected = crate::domain::validation::declaring_rejected_predecessor(before,&node.id)?;
                            let RefState::Present {tip} = plan_git::reference(&mut git,&rejected.branch).map_err(|e|refused(e.to_string()))? else {return Err(refused("rejected predecessor ref missing"));};
                            tip
                        } else {head.clone()};
                        let plan = PlanIdentity::new(PlanIdentityFields {repository_id:config.repository_id.clone(),workspace_id:before.workspace_id.clone(),plan_id:node.id.clone(),plan_path:path,body_sha256:Some(body.clone()),lineage_id:node.lineage_id.clone(),branch:node.branch.clone()}).map_err(|e|refused(e.to_string()))?;
                        let generation = PreparationGeneration {
                            ordinal:u64::try_from(node.preparations.len()).map_err(|_|refused("generation count overflow"))?.checked_add(1).ok_or_else(||refused("generation ordinal overflow"))?,
                            operation_id:request.operation_id.clone(),actor:request.actor.clone(),prepared_at:UtcTimestamp::new(SystemClock.now_utc_rfc3339()).map_err(|e|refused(e.to_string()))?,
                            plan,policy_sha256:preparation_history::policy(&config,before,node,&body,&decisions).map_err(|e|refused(e.to_string()))?,worktree:location,
                            prepared_branch_commit:prepared,observed_root_commit:head,observed_graph_revision:request.expected_revision,
                        };
                        let target = workspace.get_mut(&request.id).expect("checked");
                        target.preparations.push(generation.clone());
                        target.updated_at = generation.prepared_at.as_str().into();
                        workspace.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
                        workspace.revision = before.revision.checked_add(1).ok_or_else(||refused("revision overflow"))?;
                        crate::domain::validation::validate_native_successor(before,&workspace)?;
                        kernel.begin_with_preparation(request.operation_id.clone(),request.expected_revision,&request.actor,&workspace,
                            preparation_intent::PrepareInput {generation:generation.clone(),mode:request.mode,decisions},observer).map_err(|e|operation_error(&kernel,&request.operation_id,e))?;
                        let completed = kernel.resume(&request.operation_id,observer).map_err(|e|operation_error(&kernel,&request.operation_id,e))?;
                        operation = Some(completed.operation);
                        (match request.mode {PreparationMode::Initial=>"created",PreparationMode::Recover=>"recovered_preparation",PreparationMode::Reprepare=>"reprepared"},Some(generation))
                    }
                };
                git.refresh_observations();
                let node = workspace.get(&request.id).cloned().ok_or_else(||refused("prepared Plan missing"))?;
                let branches = workspace.plans.iter().filter(|p|p.lineage_id == node.lineage_id).map(|p|p.branch.as_str()).collect::<std::collections::HashSet<_>>();
                let observed = git.worktrees()?.into_iter().skip(1).find(|w|w.branch.as_deref().is_some_and(|b|branches.contains(b)));
                let pending = observations.into_iter().filter(|r|r.operation.fields().operation_id!=request.operation_id && !terminal(&r.operation) && r.operation.fields().plan.as_ref().is_some_and(|p|p.fields().plan_id == request.id)).collect();
                Ok(PlanPreparation {
                    lookup:PlanLookup {effective_authority:authority,registered_worktree_path:node.worktree_path.clone(),resolved_worktree_path:observed.as_ref().map_or_else(||root.join(&node.worktree_path).display().to_string(),|w|w.path.clone()),observed_worktree:observed,caller_location:caller.display().to_string(),authoritative_root:root.display().to_string(),authoritative_revision:workspace.revision,pending_operations:pending,node},
                    action:action.into(),revision_before:before.revision,revision_after:workspace.revision,
                    request_operation_id:request.operation_id.clone(),request_actor:request.actor.clone(),generation,checkpoint:operation,replay,
                })
            });
        self.graph.invalidate_observation();
        result
    }
}

fn active_checkout(
    root: &std::path::Path,
    common: &std::path::Path,
    node: &PlanNode,
    location: &RepoPath,
    git: &mut crate::infrastructure::git_evidence::GitRequest,
    kernel: &CheckpointKernel,
) -> MineResult<CommitId> {
    let RefState::Present { tip } =
        plan_git::reference(git, &node.branch).map_err(|e| refused(e.to_string()))?
    else {
        return Err(refused("active Plan branch missing"));
    };
    let checkout = plan_git::checkout(root, common, location, &node.branch, git)
        .map_err(|e| refused(e.to_string()))?;
    if let (Some(generation), CheckoutState::Present { proof }) =
        (node.preparations.last(), &checkout)
    {
        let expected = preparation_intent::completed_checkout(kernel, generation)
            .map_err(|e| refused(e.to_string()))?;
        if !preparation_intent::same_admin(&expected, proof) {
            return Err(refused(
                "active generation checkout admin identity was replaced",
            ));
        }
    }
    if !matches!(checkout,CheckoutState::Present {proof} if proof.head == tip)
        || node.branch_anchor.is_empty()
        || !git.is_ancestor(&node.branch_anchor, tip.as_str())?
    {
        return Err(refused(
            "active Plan must retain its immutable started anchor and clean exact checkout",
        ));
    }
    Ok(tip)
}
