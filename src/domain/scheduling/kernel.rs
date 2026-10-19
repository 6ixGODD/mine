use super::{Blocker, BlockingReason, Resource};
use crate::domain::error::{ContractError, ContractResult, contract_text};
use crate::domain::graph::{
    FileIdentity, GraphIndex, PlanIdentity, PlanWorkspace, RepoPath, Sha256,
};
use crate::domain::ownership::{
    ApprovedOwnershipPolicy, ClarificationKind, EffectiveAuthority, OriginalScope, OwnershipFields,
    path_contains,
};
use crate::domain::status::PlanStatus;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoppedHandoff {
    pub rejected_plan: PlanIdentity,
    pub compensation: PlanIdentity,
    pub report: FileIdentity,
    pub stopped: bool,
    pub observed_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClosureFacts {
    Settled,
    ReviewPending,
    RejectionPending,
    RejectedHandoff { evidence: Box<StoppedHandoff> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeOwnership {
    pub authority: EffectiveAuthority,
    pub worktree: RepoPath,
    pub mutable_outputs: Vec<RepoPath>,
    pub closure: ClosureFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationStage {
    Graph,
    Render,
    Checkpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingPublication {
    pub operation_id: String,
    pub owner: String,
    pub stage: PublicationStage,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotNode {
    pub ownership: NodeOwnership,
    pub status: PlanStatus,
    pub hard_predecessors: Vec<String>,
    pub owner: String,
    pub run_id: String,
}

/// Complete pure snapshot, not a persisted graph schema or a new CLI DTO.
#[derive(Debug, Clone, Serialize)]
pub struct OwnershipWave {
    pub repository_id: String,
    pub workspace_id: String,
    pub revision: u64,
    pub graph_sha256: Sha256,
    pub policy: Option<crate::domain::graph::PolicyIdentity>,
    pub authority_mode: AuthorityMode,
    pub nodes: Vec<SnapshotNode>,
    pub held: Vec<String>,
    pub candidates: Vec<String>,
    pub eligible: Vec<String>,
    pub selected: Vec<String>,
    pub blockers: Vec<Blocker>,
    pub pending_publication: Option<PendingPublication>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityMode {
    Conservative,
    Semantic,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdmissionDecision {
    pub plan_id: String,
    pub revision: u64,
    pub blockers: Vec<Blocker>,
}
impl AdmissionDecision {
    /// Eligibility at this exact input only; never an atomic execution grant.
    pub fn admissible(&self) -> bool {
        self.blockers.is_empty()
    }
}

/// Membership/ancestry indexed once, held owners derived once. Both advisory
/// wave and locked-start consumers use this evaluator over the same facts.
pub struct SchedulingKernel<'a> {
    repository_id: &'a str,
    graph: &'a PlanWorkspace,
    index: GraphIndex,
    by_id: HashMap<String, usize>,
    nodes: Vec<NodeOwnership>,
    resource_index: Vec<Vec<(RepoPath, AreaKind)>>,
    held: Vec<usize>,
    reservations: Vec<usize>,
    policy: Option<&'a ApprovedOwnershipPolicy>,
    graph_sha256: Sha256,
    pending: Option<PendingPublication>,
}

impl<'a> SchedulingKernel<'a> {
    pub fn new(
        repository_id: &'a str,
        graph: &'a PlanWorkspace,
        graph_sha256: Sha256,
        policy: Option<&'a ApprovedOwnershipPolicy>,
        nodes: Vec<NodeOwnership>,
        pending: Option<PendingPublication>,
    ) -> ContractResult<Self> {
        contract_text("scheduling.repository_id", repository_id)?;
        if policy.is_some_and(|p| p.approved_revision() > graph.revision) {
            return Err(ContractError::new(
                "scheduling.policy",
                "policy approval is newer than the graph",
            ));
        }
        let index = GraphIndex::new(graph)
            .map_err(|_| ContractError::new("scheduling.graph", "invalid graph membership"))?;
        index
            .topological_sort()
            .map_err(|_| ContractError::new("scheduling.graph", "cyclic graph"))?;
        if nodes.len() != graph.plans.len() {
            return Err(ContractError::new(
                "scheduling.nodes",
                "complete ownership for every graph node is required",
            ));
        }
        let by_id: HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.authority.plan().fields().plan_id.clone(), i))
            .collect();
        if by_id.len() != nodes.len() {
            return Err(ContractError::new(
                "scheduling.nodes",
                "duplicate ownership node",
            ));
        }
        let mut ordered = Vec::with_capacity(nodes.len());
        for plan in &graph.plans {
            if plan.hard_predecessors.iter().collect::<HashSet<_>>().len()
                != plan.hard_predecessors.len()
            {
                return Err(ContractError::new(
                    "scheduling.dependencies",
                    "duplicate hard predecessor",
                ));
            }
            let node = &nodes[*by_id
                .get(&plan.id)
                .ok_or_else(|| ContractError::new("scheduling.nodes", "missing ownership node"))?];
            let identity = node.authority.plan().fields();
            if identity.repository_id != repository_id
                || identity.workspace_id != graph.workspace_id
                || identity.plan_path.as_str() != plan.path
                || identity.branch != plan.branch
                || identity.lineage_id != plan.lineage_id
                || node.worktree.as_str() != plan.worktree_path
                || node.authority.semantic().is_some()
                    && (node.authority.policy().is_none()
                        || node.authority.policy() != policy.map(|p| p.identity()))
                || node.authority.semantic().is_none() && node.authority.policy().is_some()
            {
                return Err(ContractError::new(
                    "scheduling.identity",
                    "graph, ownership or policy identity mismatch",
                ));
            }
            if node.mutable_outputs.is_empty()
                || node.mutable_outputs.iter().collect::<HashSet<_>>().len()
                    != node.mutable_outputs.len()
            {
                return Err(ContractError::new(
                    "scheduling.outputs",
                    "distinct mutable output ownership is required",
                ));
            }
            if matches!(
                plan.status,
                PlanStatus::InProgress | PlanStatus::Implemented
            ) || matches!(
                node.closure,
                ClosureFacts::ReviewPending | ClosureFacts::RejectionPending
            ) {
                contract_text("scheduling.owner", &plan.owner)?;
                contract_text("scheduling.run_id", &plan.run_id)?;
            }
            match (&node.closure, plan.status) {
                (ClosureFacts::RejectedHandoff { evidence }, PlanStatus::Rejected) => {
                    let rejected = evidence.rejected_plan.fields();
                    let compensation = evidence.compensation.fields();
                    crate::domain::plan_id::validate_plan_path(
                        &identity.plan_id,
                        identity.plan_path.as_str(),
                    )
                    .map_err(|_| {
                        ContractError::new("scheduling.handoff", "invalid rejected Plan path")
                    })?;
                    crate::domain::plan_id::validate_report_path(
                        identity.plan_path.as_str(),
                        evidence.report.path.as_str(),
                        "review",
                    )
                    .map_err(|_| {
                        ContractError::new(
                            "scheduling.handoff",
                            "review report is not bound to the rejected Plan",
                        )
                    })?;
                    crate::domain::plan_id::validate_compensation_replacement(
                        &identity.plan_id,
                        &compensation.plan_id,
                    )
                    .map_err(|_| {
                        ContractError::new(
                            "scheduling.handoff",
                            "not the next contiguous sibling compensation",
                        )
                    })?;
                    let next = graph
                        .plans
                        .iter()
                        .find(|p| p.id == compensation.plan_id)
                        .ok_or_else(|| {
                            ContractError::new(
                                "scheduling.handoff",
                                "compensation is not registered",
                            )
                        })?;
                    if rejected != identity
                        || !evidence.stopped
                        || rejected.body_sha256.is_none()
                        || compensation.body_sha256.is_none()
                        || evidence.observed_revision > graph.revision
                        || evidence.report.bytes == 0
                        || compensation.plan_id != plan.compensating_plan
                        || compensation.lineage_id != identity.lineage_id
                        || compensation.workspace_id != identity.workspace_id
                        || compensation.repository_id != identity.repository_id
                        || next.path != compensation.plan_path.as_str()
                        || next.branch != compensation.branch
                        || next.hard_predecessors != plan.hard_predecessors
                        || next.worktree_path != plan.worktree_path
                        || nodes[*by_id.get(&compensation.plan_id).ok_or_else(|| {
                            ContractError::new(
                                "scheduling.handoff",
                                "missing compensation authority",
                            )
                        })?]
                        .authority
                        .plan()
                            != &evidence.compensation
                    {
                        return Err(ContractError::new(
                            "scheduling.handoff",
                            "invalid stopped rejection handoff or compensation binding",
                        ));
                    }
                }
                (ClosureFacts::RejectionPending, PlanStatus::Rejected) => {}
                (_, PlanStatus::Rejected)
                | (ClosureFacts::RejectedHandoff { .. } | ClosureFacts::RejectionPending, _) => {
                    return Err(ContractError::new(
                        "scheduling.handoff",
                        "rejected state requires explicit pending or stopped handoff facts",
                    ));
                }
                (ClosureFacts::ReviewPending, PlanStatus::Implemented | PlanStatus::Accepted) => {}
                (ClosureFacts::ReviewPending, _) => {
                    return Err(ContractError::new(
                        "scheduling.review",
                        "review hold has inconsistent lifecycle state",
                    ));
                }
                _ => {}
            }
            ordered.push(node.clone());
        }
        if let Some(pending) = &pending {
            contract_text("scheduling.pending.operation_id", &pending.operation_id)?;
            contract_text("scheduling.pending.owner", &pending.owner)?;
        }
        let held = graph
            .plans
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                (matches!(p.status, PlanStatus::InProgress | PlanStatus::Implemented)
                    || matches!(
                        ordered[i].closure,
                        ClosureFacts::ReviewPending | ClosureFacts::RejectionPending
                    ))
                .then_some(i)
            })
            .collect();
        let reservations = ordered
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                matches!(n.closure, ClosureFacts::RejectedHandoff { .. }).then_some(i)
            })
            .collect();
        let by_id = graph
            .plans
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
        let resource_index = ordered.iter().map(|n| areas(&n.authority)).collect();
        Ok(Self {
            repository_id,
            graph,
            index,
            by_id,
            nodes: ordered,
            resource_index,
            held,
            reservations,
            policy,
            graph_sha256,
            pending,
        })
    }

    pub fn wave(&self) -> ContractResult<OwnershipWave> {
        let mut selected = Vec::new();
        let mut candidates = Vec::new();
        let mut eligible = Vec::new();
        let mut blockers = Vec::new();
        for (i, plan) in self.graph.plans.iter().enumerate() {
            if plan.status != PlanStatus::Ready {
                continue;
            }
            candidates.push(plan.id.clone());
            let readiness = self.readiness(i);
            if readiness.is_empty() {
                eligible.push(plan.id.clone());
            }
            let mut reasons = readiness;
            reasons.extend(self.conflicts_with_holds(i, &self.nodes[i])?);
            for chosen in &selected {
                reasons.extend(self.pair(i, &self.nodes[i], *chosen, &self.nodes[*chosen])?);
            }
            if reasons.is_empty() {
                selected.push(i);
            } else {
                blockers.extend(reasons);
            }
        }
        Ok(OwnershipWave {
            repository_id: self.repository_id.into(),
            workspace_id: self.graph.workspace_id.clone(),
            revision: self.graph.revision,
            graph_sha256: self.graph_sha256.clone(),
            policy: self.policy.map(|p| p.identity().clone()),
            authority_mode: match (
                self.nodes.iter().any(|n| n.authority.semantic().is_some()),
                self.nodes.iter().any(|n| n.authority.semantic().is_none()),
            ) {
                (true, true) => AuthorityMode::Mixed,
                (true, false) => AuthorityMode::Semantic,
                _ => AuthorityMode::Conservative,
            },
            nodes: self
                .graph
                .plans
                .iter()
                .zip(&self.nodes)
                .map(|(p, n)| SnapshotNode {
                    ownership: n.clone(),
                    status: p.status,
                    hard_predecessors: p.hard_predecessors.clone(),
                    owner: p.owner.clone(),
                    run_id: p.run_id.clone(),
                })
                .collect(),
            held: self
                .held
                .iter()
                .chain(&self.reservations)
                .map(|i| self.graph.plans[*i].id.clone())
                .collect(),
            candidates,
            eligible,
            selected: selected
                .iter()
                .map(|i| self.graph.plans[*i].id.clone())
                .collect(),
            blockers,
            pending_publication: self.pending.clone(),
        })
    }

    /// The integration owner calls this under the Root lock after comparing the
    /// caller revision and freshly loading complete IO/closure/policy facts.
    pub fn admit(&self, id: &str, expected_revision: u64) -> ContractResult<AdmissionDecision> {
        self.check_revision(expected_revision)?;
        let i = self.position(id)?;
        let mut blockers = self.readiness(i);
        blockers.extend(self.conflicts_with_holds(i, &self.nodes[i])?);
        Ok(AdmissionDecision {
            plan_id: id.to_string(),
            revision: self.graph.revision,
            blockers,
        })
    }

    /// Clarification uses the very same held-owner evaluator, not a second
    /// permissive mapping policy. It must append exactly one validated record.
    pub fn admit_clarification(
        &self,
        id: &str,
        authority: EffectiveAuthority,
        expected_revision: u64,
    ) -> ContractResult<AdmissionDecision> {
        self.check_revision(expected_revision)?;
        let i = self.position(id)?;
        let old = &self.nodes[i].authority;
        let unchanged_original =
            authority.original() == old.original() && authority.plan() == old.plan();
        // Explicit first activation of legacy paths: preserve the original known
        // or absent body and conservative declarations, require owner attestation,
        // then evaluate the approved semantic mapping against every held owner.
        let legacy_activation = match (old.original(), authority.original()) {
            (
                OriginalScope::Conservative {
                    plan,
                    physical_mapping,
                },
                OriginalScope::Semantic { contract },
            ) => {
                contract.fields().plan == *plan
                    && contract.fields().physical_mapping == *physical_mapping
                    && contract.fields().shared_declarations.iter().all(|entry| {
                        physical_mapping
                            .iter()
                            .any(|prefix| path_contains(prefix, &entry.path))
                    })
                    && contract
                        .fields()
                        .generators
                        .iter()
                        .flat_map(|generator| generator.outputs.iter().chain(&generator.seals))
                        .all(|output| {
                            physical_mapping
                                .iter()
                                .any(|prefix| path_contains(prefix, output))
                        })
                    && authority
                        .clarifications()
                        .first()
                        .is_some_and(|r| r.kind == ClarificationKind::BaselineAttestation)
            }
            _ => false,
        };
        if !(unchanged_original || legacy_activation)
            || authority.policy() != old.policy() && !legacy_activation
            || authority.policy().is_none()
            || authority.policy() != self.policy.map(|p| p.identity())
            || authority
                .clarifications()
                .last()
                .is_some_and(|r| r.kind == ClarificationKind::BaselineAttestation)
                && !legacy_activation
            || authority.clarifications().len() != old.clarifications().len() + 1
            || !authority.clarifications().starts_with(old.clarifications())
            || authority
                .clarifications()
                .last()
                .is_none_or(|record| record.observed_revision != self.graph.revision)
        {
            return Err(ContractError::new(
                "scheduling.clarification",
                "clarification must append to the exact original effective authority",
            ));
        }
        let mut candidate = self.nodes[i].clone();
        candidate.authority = authority;
        let blockers = self.conflicts_with_holds(i, &candidate)?;
        Ok(AdmissionDecision {
            plan_id: id.to_string(),
            revision: self.graph.revision,
            blockers,
        })
    }

    /// Indexed pair evaluator exposed for compatibility-table tests/consumers.
    pub fn compatibility(&self, candidate: &str, owner: &str) -> ContractResult<Vec<Blocker>> {
        let a = self.position(candidate)?;
        let b = self.position(owner)?;
        if a == b {
            return Err(ContractError::new(
                "scheduling.pair",
                "distinct Plan nodes required",
            ));
        }
        self.pair(a, &self.nodes[a], b, &self.nodes[b])
    }

    fn check_revision(&self, expected: u64) -> ContractResult<()> {
        if expected != self.graph.revision {
            return Err(ContractError::new(
                "scheduling.revision",
                "stale observer revision",
            ));
        }
        Ok(())
    }
    fn position(&self, id: &str) -> ContractResult<usize> {
        self.by_id
            .get(id)
            .copied()
            .ok_or_else(|| ContractError::new("scheduling.plan", "unknown Plan identity"))
    }
    fn block(&self, a: usize, b: usize, resource: Resource, reason: BlockingReason) -> Blocker {
        Blocker {
            candidate: self.graph.plans[a].id.clone(),
            owner: self.graph.plans[b].id.clone(),
            resource,
            reason,
        }
    }
    fn readiness(&self, i: usize) -> Vec<Blocker> {
        let mut blockers = vec![];
        let plan = &self.graph.plans[i];
        if plan.status != PlanStatus::Ready {
            blockers.push(self.block(
                i,
                i,
                Resource::Plan(plan.id.clone()),
                BlockingReason::NotReady,
            ));
        }
        for pred in &plan.hard_predecessors {
            let p = self.by_id[pred];
            if self.graph.plans[p].status != PlanStatus::Accepted {
                blockers.push(self.block(
                    i,
                    p,
                    Resource::Plan(pred.clone()),
                    BlockingReason::PredecessorNotAccepted,
                ));
            }
        }
        blockers
    }
    fn conflicts_with_holds(
        &self,
        i: usize,
        candidate: &NodeOwnership,
    ) -> ContractResult<Vec<Blocker>> {
        if let Some(pending) = &self.pending {
            return Ok(vec![Blocker {
                candidate: self.graph.plans[i].id.clone(),
                owner: pending.owner.clone(),
                resource: Resource::WorkspaceCheckpoint(pending.operation_id.clone()),
                reason: BlockingReason::PendingCheckpoint,
            }]);
        }
        let mut blockers = vec![];
        for held in &self.held {
            if *held != i {
                blockers.extend(self.pair(i, candidate, *held, &self.nodes[*held])?);
            }
        }
        for reserved in &self.reservations {
            if *reserved == i {
                continue;
            }
            let node = &self.nodes[*reserved];
            let ClosureFacts::RejectedHandoff { evidence } = &node.closure else {
                unreachable!()
            };
            if candidate.authority.plan() == &evidence.compensation {
                continue;
            }
            if candidate.authority.plan().fields().lineage_id
                == node.authority.plan().fields().lineage_id
            {
                blockers.push(self.block(
                    i,
                    *reserved,
                    Resource::Lineage(node.authority.plan().fields().lineage_id.clone()),
                    BlockingReason::RejectedHandoff,
                ));
            }
            if intersection(&candidate.worktree, &node.worktree).is_some() {
                blockers.push(self.block(
                    i,
                    *reserved,
                    Resource::Worktree(node.worktree.clone()),
                    BlockingReason::RejectedHandoff,
                ));
            }
        }
        Ok(blockers)
    }

    fn pair(
        &self,
        a: usize,
        left: &NodeOwnership,
        b: usize,
        right: &NodeOwnership,
    ) -> ContractResult<Vec<Blocker>> {
        let mut blockers = vec![];
        // The common snapshot path borrows precomputed resource closures. Only
        // an append-only clarification candidate needs a fresh local index.
        let changed_left;
        let left_areas = if std::ptr::eq(left, &self.nodes[a]) {
            &self.resource_index[a]
        } else {
            changed_left = areas(&left.authority);
            &changed_left
        };
        let changed_right;
        let right_areas = if std::ptr::eq(right, &self.nodes[b]) {
            &self.resource_index[b]
        } else {
            changed_right = areas(&right.authority);
            &changed_right
        };
        let l = left.authority.plan().fields();
        let r = right.authority.plan().fields();
        if self
            .index
            .is_hard_ancestor(&l.plan_id, &r.plan_id)
            .map_err(|_| ContractError::new("scheduling.ancestry", "invalid ancestry query"))?
            || self
                .index
                .is_hard_ancestor(&r.plan_id, &l.plan_id)
                .map_err(|_| ContractError::new("scheduling.ancestry", "invalid ancestry query"))?
        {
            blockers.push(self.block(
                a,
                b,
                Resource::Plan(r.plan_id.clone()),
                BlockingReason::Ancestor,
            ));
        }
        if l.lineage_id == r.lineage_id {
            blockers.push(self.block(
                a,
                b,
                Resource::Lineage(r.lineage_id.clone()),
                BlockingReason::SameLineage,
            ));
        }
        if intersection(&left.worktree, &right.worktree).is_some() {
            blockers.push(self.block(
                a,
                b,
                Resource::Worktree(right.worktree.clone()),
                BlockingReason::SameWorktree,
            ));
        }
        for x in &left.mutable_outputs {
            for y in &right.mutable_outputs {
                if let Some(path) = intersection(x, y) {
                    blockers.push(self.block(
                        a,
                        b,
                        Resource::MutableOutput(path),
                        BlockingReason::SharedOutput,
                    ));
                }
            }
        }
        let lf = left.authority.semantic().map(|c| c.fields());
        let rf = right.authority.semantic().map(|c| c.fields());
        if let (Some(lf), Some(rf)) = (lf, rf) {
            for lc in &lf.namespace_claims {
                for rc in &rf.namespace_claims {
                    let common = if lc.subtree.contains(&rc.subtree) {
                        Some(&rc.subtree)
                    } else if rc.subtree.contains(&lc.subtree) {
                        Some(&lc.subtree)
                    } else {
                        None
                    };
                    if let Some(common) = common {
                        if !lc
                            .excluded
                            .iter()
                            .chain(&rc.excluded)
                            .any(|e| e.contains(common))
                        {
                            blockers.push(self.block(
                                a,
                                b,
                                Resource::Namespace(common.clone()),
                                BlockingReason::NamespaceOverlap,
                            ));
                        }
                    }
                }
            }
            for contract in &lf.owned_contracts {
                if rf.owned_contracts.contains(contract) {
                    blockers.push(self.block(
                        a,
                        b,
                        Resource::Contract(contract.clone()),
                        BlockingReason::ContractOverlap,
                    ));
                }
            }
            if lf.responsibility == rf.responsibility {
                blockers.push(self.block(
                    a,
                    b,
                    Resource::Contract(lf.responsibility.clone()),
                    BlockingReason::ContractOverlap,
                ));
            }
            for generator in &lf.generators {
                if rf.generators.iter().any(|g| g.name == generator.name) {
                    blockers.push(self.block(
                        a,
                        b,
                        Resource::Generator(generator.name.clone()),
                        BlockingReason::GeneratorTransaction,
                    ));
                }
            }
        }
        // Mutable outputs are exclusive against source/shared/generated areas as
        // well, not just other output-directory declarations.
        for (outputs, indexed_areas) in [
            (&left.mutable_outputs, right_areas),
            (&right.mutable_outputs, left_areas),
        ] {
            for output in outputs {
                for (path, _) in indexed_areas {
                    if let Some(path) = intersection(output, path) {
                        blockers.push(self.block(
                            a,
                            b,
                            Resource::MutableOutput(path),
                            BlockingReason::SharedOutput,
                        ));
                    }
                }
            }
        }
        for (x, x_kind) in left_areas {
            for (y, y_kind) in right_areas {
                let Some(path) = intersection(x, y) else {
                    continue;
                };
                let allowed = match (*x_kind, *y_kind, lf, rf) {
                    (AreaKind::Mapping, AreaKind::Mapping, Some(lf), Some(rf)) => self
                        .policy
                        .is_some_and(|p| p.allows_directory_overlap(lf, rf, x, y)),
                    (AreaKind::Declaration, AreaKind::Declaration, Some(lf), Some(rf)) => {
                        shared_overlap_allowed(lf, rf, x, y)
                    }
                    _ => false,
                };
                if !allowed {
                    let reason = if matches!(x_kind, AreaKind::Generator)
                        || matches!(y_kind, AreaKind::Generator)
                    {
                        BlockingReason::GeneratorTransaction
                    } else {
                        BlockingReason::PhysicalOverlap
                    };
                    blockers.push(self.block(a, b, Resource::PhysicalPath(path), reason));
                }
            }
        }
        // Stable, deterministic diagnostics without duplicate resource reasons.
        let mut unique = Vec::with_capacity(blockers.len());
        for blocker in blockers {
            if !unique.contains(&blocker) {
                unique.push(blocker);
            }
        }
        Ok(unique)
    }
}

#[derive(Clone, Copy)]
enum AreaKind {
    Mapping,
    Declaration,
    Generator,
}
fn areas(authority: &EffectiveAuthority) -> Vec<(RepoPath, AreaKind)> {
    let mut areas: Vec<_> = authority
        .physical_mapping()
        .iter()
        .cloned()
        .map(|p| (p, AreaKind::Mapping))
        .collect();
    if let Some(contract) = authority.semantic() {
        let f = contract.fields();
        areas.extend(
            f.shared_declarations
                .iter()
                .map(|d| (d.path.clone(), AreaKind::Declaration)),
        );
        for g in &f.generators {
            areas.extend(
                g.inputs
                    .iter()
                    .chain(&g.outputs)
                    .chain(&g.seals)
                    .chain(std::iter::once(&g.machine_source))
                    .cloned()
                    .map(|p| (p, AreaKind::Generator)),
            );
        }
    }
    areas
}
fn intersection(a: &RepoPath, b: &RepoPath) -> Option<RepoPath> {
    if path_contains(a, b) {
        Some(b.clone())
    } else if path_contains(b, a) {
        Some(a.clone())
    } else {
        None
    }
}
fn shared_overlap_allowed(
    l: &OwnershipFields,
    r: &OwnershipFields,
    a: &RepoPath,
    b: &RepoPath,
) -> bool {
    if a != b {
        return false;
    }
    let left: Vec<_> = l
        .shared_declarations
        .iter()
        .filter(|d| &d.path == a)
        .collect();
    let right: Vec<_> = r
        .shared_declarations
        .iter()
        .filter(|d| &d.path == b)
        .collect();
    !left.is_empty()
        && !right.is_empty()
        && left.iter().all(|x| {
            right
                .iter()
                .all(|y| x.entry != y.entry && x.integration_owner == y.integration_owner)
        })
}
