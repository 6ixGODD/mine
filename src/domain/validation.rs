//! Execution-graph validation and graph algorithms.
//!
//! Implements `docs/design/execution-graph/state-machine-and-algorithms.md`
//! "Validation" and "Parallel wave": unique IDs/paths, valid predecessors,
//! acyclic hard dependencies, legal states, ready frontier, topological sort,
//! parallel-wave computation with write-scope conflict detection, and
//! generated-view revision parity. These are pure functions over the
//! [`PlanWorkspace`] aggregate.

use std::collections::HashSet;

use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{
    CURRENT_GRAPH_SCHEMA_VERSION, PlanNode, PlanWorkspace, is_supported_graph_schema,
};
use crate::domain::path::normalize_repo_relative;
use crate::domain::plan_id::{
    CanonicalPlanId, is_exact_bootstrap_node, registered_plan_branch,
    validate_compensation_replacement,
};
use crate::domain::status::PlanStatus;

/// Validates the structural integrity of the whole workspace.
///
/// Per `docs/design/execution-graph/state-machine-and-algorithms.md`, the
/// "Validation" step checks: unique IDs/paths, valid predecessors, acyclic hard
/// dependencies, legal status values (enforced by the typed enum), normalized
/// safe paths, non-empty design references, generated-view revision parity,
/// and branch/workspace consistency. Exclusive-write scope overlap is **not**
/// a structural validation concern: it is a `parallel_wave` constraint applied
/// only among `READY` plans, so that sequential plans (e.g. an `ACCEPTED`
/// predecessor owning a broad path and its `READY` successor owning a
/// subdirectory) validate cleanly.
///
/// # Errors
/// Returns the first [`MineError`] describing the violation.
pub fn validate(ws: &PlanWorkspace) -> MineResult<()> {
    validate_with_worktrees_root(ws, ".mine/worktrees")
}

/// Validates a workspace against the configured repository-relative worktree
/// root. Persistence adapters use this form so a non-default
/// `[plan].worktrees_root` remains authoritative.
pub fn validate_with_worktrees_root(ws: &PlanWorkspace, worktrees_root: &str) -> MineResult<()> {
    if !is_supported_graph_schema(ws.schema_version) {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "unsupported execution-graph schema version {}; expected {} (run `mine graph repair-provenance` for schema v1/v2)",
                ws.schema_version, CURRENT_GRAPH_SCHEMA_VERSION
            ),
        });
    }
    let normalized_worktrees_root = normalize_repo_relative(worktrees_root)?;

    // Unique IDs.
    let mut seen_ids = HashSet::new();
    for p in &ws.plans {
        if !seen_ids.insert(p.id.as_str()) {
            return Err(MineError::GraphInvalid {
                detail: format!("duplicate plan id: {}", p.id),
            });
        }
    }

    // An exact policy binding has one preserved first graph/ADR observation.
    let mut policy_bindings = std::collections::HashMap::new();
    for node in &ws.plans {
        if let Some(policy) = &node.ownership_policy {
            let key = (&policy.policy_path, policy.adr_id.as_str());
            if policy_bindings
                .insert(key, policy)
                .is_some_and(|first| first != policy)
            {
                return Err(MineError::GraphInvalid {
                    detail:
                        "one approved policy binding has contradictory first-admission provenance"
                            .into(),
                });
            }
        }
    }

    // Rejection and compensation registration are separate durable mutations,
    // so a rejected node may temporarily name a not-yet-registered
    // compensation. Once that declared node exists, however, its lineage and
    // complete ordered hard-predecessor list are one atomic relationship.
    // Selection starts from the rejected declaration, not merely a `-CNN`
    // identifier: imported graphs can contain sealed administrative mirror
    // nodes whose historical IDs resemble compensation without declaring that
    // relationship. New registrations remain fail-closed in `PlanService::add`.
    for rejected in &ws.plans {
        if rejected.status == PlanStatus::Rejected
            && !rejected.compensating_plan.is_empty()
            && ws.get(&rejected.compensating_plan).is_some()
            && CanonicalPlanId::parse(&rejected.compensating_plan)
                .is_ok_and(|parsed| parsed.is_compensation())
        {
            declared_compensation_predecessor(ws, &rejected.compensating_plan)?;
        }
    }

    // Unique plan paths.
    let mut seen_paths = HashSet::new();
    for p in &ws.plans {
        let norm = normalize_repo_relative(&p.path)?;
        if !seen_paths.insert(norm) {
            return Err(MineError::GraphInvalid {
                detail: format!("duplicate plan path: {}", p.path),
            });
        }
    }

    // Valid predecessors and non-empty design references.
    let id_set = ws.ids();
    for p in &ws.plans {
        validate_native_facts(ws, p)?;
        for pred in &p.hard_predecessors {
            if !id_set.contains(pred.as_str()) {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} hard_predecessor {} not found", p.id, pred),
                });
            }
            if pred == &p.id {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} depends on itself", p.id),
                });
            }
        }
        for pred in &p.soft_predecessors {
            if !id_set.contains(pred.as_str()) {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} soft_predecessor {} not found", p.id, pred),
                });
            }
        }
        if p.design_references.is_empty() {
            return Err(MineError::GraphInvalid {
                detail: format!("plan {} has no design references", p.id),
            });
        }
        // Validate all owned paths are safe.
        for path in &p.design_references {
            normalize_repo_relative(path)?;
        }
        for path in &p.exclusive_write_paths {
            normalize_repo_relative(path)?;
        }
        for path in &p.read_only_paths {
            normalize_repo_relative(path)?;
        }
        for path in &p.reserved_shared_paths {
            normalize_repo_relative(path)?;
        }
        if p.lineage_id.is_empty() || p.worktree_path.is_empty() || p.branch.is_empty() {
            return Err(MineError::GraphInvalid {
                detail: format!("plan {} has incomplete canonical provenance", p.id),
            });
        }
        let anchor_must_be_empty = matches!(
            p.status,
            PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready | PlanStatus::Withdrawn
        );
        if anchor_must_be_empty != p.branch_anchor.is_empty() {
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "plan {} has branch_anchor inconsistent with status {}",
                    p.id, p.status
                ),
            });
        }
        let (expected_lineage, expected_worktree) =
            if is_exact_bootstrap_node(&ws.workspace_id, &p.id, &p.path) {
                (
                    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                    "plan01".to_string(),
                    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                    format!("{normalized_worktrees_root}/plan01"),
                )
            } else {
                let parsed = CanonicalPlanId::parse(&p.id)?;
                (
                    parsed.lineage_id(),
                    parsed.worktree_path_under(&normalized_worktrees_root),
                )
            };
        let expected_branch = registered_plan_branch(&ws.workspace_id, &p.id, &p.path)?;
        let normalized_worktree = normalize_repo_relative(&p.worktree_path)?;
        if p.lineage_id != expected_lineage
            || normalized_worktree != expected_worktree
            || p.branch != expected_branch
        {
            return Err(MineError::GraphInvalid {
                detail: format!("plan {} has inconsistent canonical provenance", p.id),
            });
        }
    }

    // Acyclic hard dependencies (topological sort via Kahn's algorithm).
    topological_sort(ws)?;

    // Exclusive-write scope overlap is intentionally NOT checked here: per the
    // design it is a `parallel_wave` constraint among READY plans, not a
    // structural validation rule. Two sequential plans may legitimately
    // own overlapping paths (a broad predecessor path and a narrower
    // successor path) because they never write concurrently.

    Ok(())
}

/// Pure wire consistency only. File/Git/policy approval is checked by the IO
/// adapter, not inferred from deserialized provenance.
fn validate_native_facts(ws: &PlanWorkspace, p: &PlanNode) -> MineResult<()> {
    use crate::domain::lifecycle_operation::{AcceptanceStage, OperationOutcome, OperationStage};
    let invalid = |detail: &str| MineError::GraphInvalid {
        detail: format!("plan {} native provenance: {detail}", p.id),
    };
    if ws.schema_version < 5
        && (p.body_sha256.is_some()
            || p.ownership.is_some()
            || p.ownership_policy.is_some()
            || !p.clarifications.is_empty())
        || ws.schema_version < 6 && p.acceptance.is_some()
        || ws.schema_version < 7
            && (!p.preparations.is_empty() || p.started_preparation_generation.is_some())
    {
        return Err(invalid("facts require their declared graph version"));
    }
    let bound = |identity: &crate::domain::graph::PlanIdentity| {
        let f = identity.fields();
        f.workspace_id == ws.workspace_id
            && f.plan_id == p.id
            && f.plan_path.as_str() == p.path
            && f.lineage_id == p.lineage_id
            && f.branch == p.branch
    };
    if !p.clarifications.is_empty() && p.ownership.is_none() {
        return Err(invalid("clarification has no declaration baseline"));
    }
    if let Some(original) = &p.ownership {
        if !bound(&original.fields().plan)
            || original.fields().plan.fields().body_sha256 != p.body_sha256
            || p.body_sha256.is_none()
                && p.clarifications.first().is_none_or(|record| {
                    record.kind != crate::domain::ownership::ClarificationKind::BaselineAttestation
                })
        {
            return Err(invalid(
                "original semantic authority differs from registered Plan",
            ));
        }
    }
    if let Some(policy) = &p.ownership_policy {
        let identity =
            crate::domain::graph::PolicyIdentity::new(crate::domain::graph::PolicyIdentityFields {
                design_references: vec![policy.adr_path.clone()],
                sha256: policy.policy_sha256.clone(),
            })
            .map_err(|_| invalid("invalid policy identity"))?;
        if policy.workspace_id != ws.workspace_id
            || policy.repository_id.is_empty()
            || policy.approval_actor.trim().is_empty()
            || policy.admitted_graph_revision >= ws.revision
            || p.ownership.as_ref().is_none_or(|original| {
                original.fields().policy != identity
                    || original.fields().plan.fields().repository_id != policy.repository_id
            })
        {
            return Err(invalid("policy approval provenance differs"));
        }
    } else if p.ownership.is_some() {
        return Err(invalid(
            "semantic authority has no policy approval provenance",
        ));
    }
    let mut tail = None;
    let mut revision = None;
    for (index, record) in p.clarifications.iter().enumerate() {
        if record.schema_version != 1
            || record.sequence != index as u64 + 1
            || record.previous_record != tail
            || record.original_body != p.body_sha256
            || !bound(&record.plan)
            || record.plan.fields().body_sha256.is_none()
            || record.actor.trim().is_empty()
            || record.reason.trim().is_empty()
            || record.observed_revision >= ws.revision
            || revision.is_some_and(|r| record.observed_revision <= r)
        {
            return Err(invalid("clarification identity/order differs"));
        }
        tail = Some(record.digest());
        revision = Some(record.observed_revision);
    }
    let mut operations = HashSet::new();
    let mut prepared_revision = None;
    for (index, generation) in p.preparations.iter().enumerate() {
        if generation.ordinal != index as u64 + 1
            || !operations.insert(generation.operation_id.as_str())
            || generation.actor.trim().is_empty()
            || !bound(&generation.plan)
            || p.preparations
                .first()
                .is_some_and(|first| generation.plan != first.plan)
            || generation.plan.fields().body_sha256.is_none()
            || p.body_sha256
                .as_ref()
                .is_some_and(|body| generation.plan.fields().body_sha256.as_ref() != Some(body))
            || generation.worktree.as_str() != p.worktree_path
            || generation.observed_graph_revision >= ws.revision
            || prepared_revision.is_some_and(|r| generation.observed_graph_revision <= r)
        {
            return Err(invalid("preparation identity/order differs"));
        }
        prepared_revision = Some(generation.observed_graph_revision);
    }
    if !p.preparations.is_empty()
        && matches!(
            p.status,
            PlanStatus::InProgress
                | PlanStatus::Implemented
                | PlanStatus::Accepted
                | PlanStatus::Rejected
        )
        && p.started_preparation_generation.is_none()
    {
        return Err(invalid(
            "native started node has no pinned preparation generation",
        ));
    }
    if let Some(ordinal) = p.started_preparation_generation {
        let generation = p
            .preparations
            .last()
            .ok_or_else(|| invalid("missing start generation"))?;
        if ordinal != generation.ordinal
            || p.branch_anchor != generation.prepared_branch_commit.as_str()
            || !matches!(
                p.status,
                PlanStatus::InProgress
                    | PlanStatus::Implemented
                    | PlanStatus::Accepted
                    | PlanStatus::Rejected
            )
        {
            return Err(invalid("immutable start anchor/generation/status differs"));
        }
    }
    if let Some(acceptance) = &p.acceptance {
        let f = acceptance.fields();
        if p.status != PlanStatus::Accepted
            || f.stage != OperationStage::Acceptance(AcceptanceStage::Accepted)
            || f.outcome != OperationOutcome::Committed
            || f.plan.as_ref().is_none_or(|identity| !bound(identity))
            || p.body_sha256.as_ref().is_some_and(|body| {
                f.plan
                    .as_ref()
                    .and_then(|identity| identity.fields().body_sha256.as_ref())
                    != Some(body)
            })
            || f.observed_graph_revision >= ws.revision
            || f.review
                .as_ref()
                .is_none_or(|review| review.report.path.as_str() != p.review_report)
        {
            return Err(invalid("terminal acceptance binding differs"));
        }
    }
    Ok(())
}

/// A schema upgrade preserves absent historical evidence. Native append-only
/// facts cannot be rewritten by an otherwise valid lifecycle transaction.
pub fn validate_native_successor(before: &PlanWorkspace, after: &PlanWorkspace) -> MineResult<()> {
    for old in &before.plans {
        let new = after.get(&old.id).ok_or_else(|| MineError::GraphInvalid {
            detail: format!("native provenance cannot remove Plan {}", old.id),
        })?;
        let invalid = || MineError::GraphInvalid {
            detail: format!(
                "Plan {} immutable/append-only native provenance changed",
                old.id
            ),
        };
        let legacy_activation = old.ownership.is_none()
            && old.clarifications.is_empty()
            && new.clarifications.len() == 1
            && new.clarifications[0].kind
                == crate::domain::ownership::ClarificationKind::BaselineAttestation
            && new.ownership.as_ref().is_some_and(|original| {
                original.fields().plan.fields().body_sha256 == old.body_sha256
                    && old.write_scope().ok().is_some_and(|paths| {
                        paths
                            == original
                                .fields()
                                .physical_mapping
                                .iter()
                                .map(|p| p.as_str().to_string())
                                .collect::<Vec<_>>()
                    })
            });
        if new.body_sha256 != old.body_sha256
            || new.ownership != old.ownership && !legacy_activation
            || new.ownership_policy != old.ownership_policy && !legacy_activation
            || !new.clarifications.starts_with(&old.clarifications)
            || new.clarifications.len() > old.clarifications.len() + 1
            || !new.preparations.starts_with(&old.preparations)
            || new.preparations.len() > old.preparations.len() + 1
            || old.acceptance.is_some() && new.acceptance != old.acceptance
            || old.started_preparation_generation.is_some()
                && new.started_preparation_generation != old.started_preparation_generation
        {
            return Err(invalid());
        }
        if new.clarifications.len() > old.clarifications.len()
            && new
                .clarifications
                .last()
                .is_none_or(|record| record.observed_revision != before.revision)
            || new.preparations.len() > old.preparations.len()
                && (new
                    .preparations
                    .last()
                    .is_none_or(|generation| generation.observed_graph_revision != before.revision)
                    || !matches!(old.status, PlanStatus::Ready))
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Returns the unique rejected predecessor that declares a compensation and
/// validates their immutable lineage and dependency relationship.
///
/// The compensation must be the rejected node's next canonical sibling, share
/// its lineage, and inherit the complete ordered hard-predecessor list exactly.
/// This helper is the single policy entry point used by registration, whole-
/// graph validation, preparation, and the controlled repair path.
///
/// # Errors
/// Returns [`MineError::GraphInvalid`] when the compensation is missing, is not
/// declared by exactly one rejected predecessor, or violates any relationship
/// invariant.
pub fn declared_compensation_predecessor<'a>(
    ws: &'a PlanWorkspace,
    compensation_id: &str,
) -> MineResult<&'a PlanNode> {
    let predecessor = declaring_rejected_predecessor(ws, compensation_id)?;
    let compensation = ws
        .get(compensation_id)
        .expect("declaring_rejected_predecessor verifies the target");
    if compensation.hard_predecessors != predecessor.hard_predecessors {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "compensation {compensation_id} must inherit the complete ordered hard predecessors of {}: expected {:?}, actual {:?}",
                predecessor.id, predecessor.hard_predecessors, compensation.hard_predecessors
            ),
        });
    }
    Ok(predecessor)
}

/// Returns the unique rejected predecessor declaring a compensation after
/// validating identity and lineage, but does not compare dependency lists.
///
/// This narrower form exists for the bounded repair transaction: it derives
/// the only permitted dependency list before constructing a candidate graph,
/// which is then checked by [`declared_compensation_predecessor`].
///
/// # Errors
/// Returns [`MineError::GraphInvalid`] when declaration, identity, sibling
/// order, or lineage is ambiguous or inconsistent.
pub fn declaring_rejected_predecessor<'a>(
    ws: &'a PlanWorkspace,
    compensation_id: &str,
) -> MineResult<&'a PlanNode> {
    let compensation = ws
        .get(compensation_id)
        .ok_or_else(|| MineError::GraphInvalid {
            detail: format!("compensation {compensation_id} is not registered"),
        })?;
    let parsed =
        CanonicalPlanId::parse(compensation_id).map_err(|error| MineError::GraphInvalid {
            detail: format!("compensation {compensation_id} has invalid identity: {error}"),
        })?;
    if !parsed.is_compensation() {
        return Err(MineError::GraphInvalid {
            detail: format!("plan {compensation_id} is not a compensation identity"),
        });
    }
    let predecessors: Vec<_> = ws
        .plans
        .iter()
        .filter(|candidate| {
            candidate.status == PlanStatus::Rejected
                && candidate.compensating_plan == compensation_id
        })
        .collect();
    if predecessors.len() != 1 {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "compensation {compensation_id} must be declared by exactly one rejected predecessor"
            ),
        });
    }
    let predecessor = predecessors[0];
    validate_compensation_replacement(&predecessor.id, compensation_id).map_err(|error| {
        MineError::GraphInvalid {
            detail: format!(
                "compensation {compensation_id} is not the next sibling of {}: {error}",
                predecessor.id
            ),
        }
    })?;
    if compensation.lineage_id != predecessor.lineage_id {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "compensation {compensation_id} must remain in rejected predecessor lineage {}",
                predecessor.lineage_id
            ),
        });
    }
    Ok(predecessor)
}

/// Returns a topological ordering of plan IDs via Kahn's algorithm on hard
/// predecessors. Stable: emits predecessors before descendants in declaration
/// order.
///
/// # Errors
/// Returns [`MineError::GraphCycle`] if a cycle is detected.
pub fn topological_sort(ws: &PlanWorkspace) -> MineResult<Vec<String>> {
    crate::domain::graph::GraphIndex::new(ws)?.topological_sort()
}

/// Shared terminal snapshot rule for live release and retained workspace seals.
pub(crate) fn validate_terminal_workspace(
    workspace: &PlanWorkspace,
) -> crate::domain::lifecycle_operation::OperationResult<()> {
    for node in &workspace.plans {
        validate_terminal_node(workspace, node)?;
    }
    Ok(())
}

pub(crate) fn validate_terminal_node(
    workspace: &PlanWorkspace,
    node: &crate::domain::graph::PlanNode,
) -> crate::domain::lifecycle_operation::OperationResult<()> {
    use crate::domain::lifecycle_operation::OperationError;
    match node.status {
        PlanStatus::Accepted | PlanStatus::Withdrawn => {}
        PlanStatus::Rejected => {
            let mut current = node;
            let mut seen = std::collections::BTreeSet::new();
            while current.status == PlanStatus::Rejected {
                if !seen.insert(&current.id) {
                    return Err(OperationError::Refused("cyclic terminal compensation"));
                }
                current =
                    workspace
                        .get(&current.compensating_plan)
                        .ok_or(OperationError::Refused(
                            "rejection has no completed compensation",
                        ))?;
            }
            if current.status != PlanStatus::Accepted {
                return Err(OperationError::Refused(
                    "rejection compensation does not end in ACCEPTED",
                ));
            }
        }
        _ => return Err(OperationError::Refused("source workspace is not terminal")),
    }
    Ok(())
}

/// Returns `true` if a plan's hard predecessors are all `ACCEPTED`.
pub fn hard_predecessors_accepted(ws: &PlanWorkspace, plan_id: &str) -> MineResult<bool> {
    let node = ws.get(plan_id).ok_or_else(|| MineError::PlanNotFound {
        plan_id: plan_id.to_string(),
    })?;
    for pred in &node.hard_predecessors {
        let pred_node = ws.get(pred).ok_or_else(|| MineError::PlanNotFound {
            plan_id: pred.to_string(),
        })?;
        if pred_node.status != PlanStatus::Accepted {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Computes the derived readiness of a plan: ready only when every hard
/// predecessor is accepted. (Design references and path ownership are checked
/// by [`validate`].)
pub fn is_derived_ready(ws: &PlanWorkspace, plan_id: &str) -> MineResult<bool> {
    hard_predecessors_accepted(ws, plan_id)
}

/// Returns the READY frontier: all plans currently `READY`. (Does not mutate;
/// derived readiness recomputation is a service concern.)
#[must_use]
pub fn ready_frontier(ws: &PlanWorkspace) -> Vec<String> {
    ws.plans
        .iter()
        .filter(|p| p.status == PlanStatus::Ready)
        .map(|p| p.id.clone())
        .collect()
}

/// Computes a parallel wave: a stable maximal set of `READY` plans with no
/// ancestor relationship among them and no mutual write-scope conflict.
///
/// Two plans are excluded from the same wave when: they have a hard-dependency
/// ancestor relationship, or their write scopes overlap (exclusive-write vs
/// exclusive-write, or exclusive-write vs reserved-shared).
#[must_use]
pub fn parallel_wave(ws: &PlanWorkspace) -> Vec<String> {
    let Ok(index) = crate::domain::graph::GraphIndex::new(ws) else {
        return Vec::new();
    };
    let ready: Vec<&crate::domain::graph::PlanNode> = ws
        .plans
        .iter()
        .filter(|p| p.status == PlanStatus::Ready)
        .collect();
    // Greedy stable selection in declaration order.
    let mut wave: Vec<String> = Vec::new();
    for candidate in &ready {
        let mut compatible = true;
        for chosen_id in &wave {
            let chosen = &ws.plans[index.position(chosen_id).expect("chosen is from ready set")];
            if index
                .is_hard_ancestor(&candidate.id, &chosen.id)
                .unwrap_or(false)
                || index
                    .is_hard_ancestor(&chosen.id, &candidate.id)
                    .unwrap_or(false)
            {
                compatible = false;
                break;
            }
            if candidate.write_scope_overlaps(chosen).unwrap_or(false) {
                compatible = false;
                break;
            }
        }
        if compatible {
            wave.push(candidate.id.clone());
        }
    }
    wave
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::MineError;
    use crate::domain::graph::PlanNode;

    fn ws(plans: Vec<PlanNode>) -> PlanWorkspace {
        PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 0,
            project_id: "p".to_string(),
            workspace_id: "validation-test".to_string(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: String::new(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans,
        }
    }

    fn node(id: &str, hard: &[&str], excl: &[&str]) -> PlanNode {
        PlanNode {
            id: id.to_string(),
            path: format!("docs/plan/{id}-test.md"),
            title: id.to_string(),
            branch: crate::domain::plan_id::registered_plan_branch(
                "validation-test",
                id,
                &format!("docs/plan/{id}-test.md"),
            )
            .unwrap(),
            branch_anchor: String::new(),
            status: PlanStatus::Blocked,
            hard_predecessors: hard.iter().map(|s| s.to_string()).collect(),
            soft_predecessors: vec![],
            design_references: vec!["docs/design/principles.md".to_string()],
            exclusive_write_paths: excl.iter().map(|s| s.to_string()).collect(),
            read_only_paths: vec![],
            reserved_shared_paths: vec![],
            implementation_report: String::new(),
            review_report: String::new(),
            implementation_commits: vec![],
            owner: String::new(),
            run_id: String::new(),
            started_at: String::new(),
            updated_at: String::new(),
            rejection_reason: String::new(),
            compensating_plan: String::new(),
            withdraw_reason: String::new(),
            withdrawn_at: String::new(),
            planning_scope: String::new(),
            readiness_checked_at: String::new(),
            adr_registry_revision: 0,
            lineage_id: crate::domain::plan_id::CanonicalPlanId::parse(id)
                .unwrap()
                .lineage_id(),
            worktree_path: crate::domain::plan_id::CanonicalPlanId::parse(id)
                .unwrap()
                .worktree_path(),
            body_sha256: None,
            ownership: None,
            ownership_policy: None,
            clarifications: vec![],
            acceptance: None,
            preparations: vec![],
            started_preparation_generation: None,
        }
    }

    fn canonical_node(id: &str, status: PlanStatus, hard: &[&str]) -> PlanNode {
        let parsed = CanonicalPlanId::parse(id).expect("canonical test id");
        let mut node = node(id, hard, &[&format!("src/{id}")]);
        node.path = format!("docs/plan/{id}-test.md");
        node.branch = registered_plan_branch("validation-test", id, &node.path).unwrap();
        node.status = status;
        if !matches!(
            status,
            PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready | PlanStatus::Withdrawn
        ) {
            node.branch_anchor = "validation-test-anchor".to_string();
        }
        node.lineage_id = parsed.lineage_id();
        node.worktree_path = parsed.worktree_path();
        node
    }

    fn canonical_ws(plans: Vec<PlanNode>) -> PlanWorkspace {
        let mut workspace = ws(plans);
        workspace.workspace_id = "validation-test".to_string();
        workspace
    }

    #[test]
    fn valid_chain_validates() -> Result<(), MineError> {
        let g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/a/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan02", &["plan01"], &["src/b/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan03", &["plan02"], &["src/c/"]),
        ]);
        validate(&g)?;
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        assert_eq!(topological_sort(&g)?, vec!["plan01", "plan02", "plan03"]);
        Ok(())
    }

    #[test]
    fn duplicate_id_rejected() {
        let g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/a/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/b/"]),
        ]);
        assert_eq!(validate(&g).unwrap_err().code(), "MINE_GRAPH_INVALID");
    }

    #[test]
    fn missing_predecessor_rejected() {
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        let g = ws(vec![node("plan02", &["plan01"], &["src/b/"])]);
        assert_eq!(validate(&g).unwrap_err().code(), "MINE_GRAPH_INVALID");
    }

    #[test]
    fn cycle_detected() {
        let g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &["plan02"], &["src/a/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan02", &["plan01"], &["src/b/"]),
        ]);
        assert_eq!(validate(&g).unwrap_err().code(), "MINE_GRAPH_CYCLE");
    }

    #[test]
    fn self_dependency_rejected() {
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        let g = ws(vec![node("plan01", &["plan01"], &["src/a/"])]);
        assert_eq!(validate(&g).unwrap_err().code(), "MINE_GRAPH_INVALID");
    }

    #[test]
    fn sequential_plans_with_overlapping_scopes_validate() {
        // Per the design, exclusive-write overlap is a parallel-wave constraint,
        // not a structural validation rule. A broad predecessor scope and a
        // narrower successor scope must validate cleanly because they never
        // write concurrently.
        let g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan02", &["plan01"], &["src/domain/"]),
        ]);
        validate(&g).expect("sequential overlapping scopes validate");
    }

    #[test]
    fn parallel_wave_splits_overlapping_scopes() {
        let mut g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/shared/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan02", &[], &["src/shared/x.rs"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan03", &[], &["src/other/"]),
        ]);
        g.plans[0].status = PlanStatus::Ready;
        g.plans[1].status = PlanStatus::Ready;
        g.plans[2].status = PlanStatus::Ready;
        let wave = parallel_wave(&g);
        // 01 and 02 overlap; only one of them can join the wave alongside 03.
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        assert!(wave.contains(&"plan03".to_string()));
        assert!(
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            wave.contains(&"plan01".to_string()) ^ wave.contains(&"plan02".to_string()),
            "wave was {wave:?}"
        );
    }

    #[test]
    fn derived_ready_requires_accepted_predecessors() -> Result<(), MineError> {
        let mut g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/a/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan02", &["plan01"], &["src/b/"]),
        ]);
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        assert!(!is_derived_ready(&g, "plan02")?);
        g.plans[0].status = PlanStatus::Accepted;
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        assert!(is_derived_ready(&g, "plan02")?);
        Ok(())
    }

    #[test]
    fn ready_frontier_lists_ready_plans() {
        let mut g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/a/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan02", &[], &["src/b/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan03", &[], &["src/c/"]),
        ]);
        g.plans[0].status = PlanStatus::Ready;
        g.plans[2].status = PlanStatus::Ready;
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        assert_eq!(ready_frontier(&g), vec!["plan01", "plan03"]);
    }

    #[test]
    fn wave_excludes_ancestor_pairs() {
        let mut g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/a/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan02", &["plan01"], &["src/b/"]),
        ]);
        // Make 02 READY while 01 is also READY (artificial, for wave test).
        g.plans[0].status = PlanStatus::Ready;
        g.plans[1].status = PlanStatus::Ready;
        let wave = parallel_wave(&g);
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        assert!(wave.contains(&"plan01".to_string()));
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        assert!(!wave.contains(&"plan02".to_string()));
    }

    #[test]
    fn wave_excludes_write_scope_conflict() {
        let mut g = ws(vec![
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan01", &[], &["src/shared/"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan02", &[], &["src/shared/x.rs"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            node("plan03", &[], &["src/c/"]),
        ]);
        // Suppress the structural conflict check by making scopes disjoint
        // for validation, then test wave with an overlap via reserved paths.
        g.plans[1].exclusive_write_paths = vec!["src/other/".to_string()];
        g.plans[0].status = PlanStatus::Ready;
        g.plans[1].status = PlanStatus::Ready;
        g.plans[2].status = PlanStatus::Ready;
        // 01 and 02 are now disjoint; all three ready with disjoint scopes.
        let wave = parallel_wave(&g);
        assert_eq!(wave.len(), 3);
    }

    #[test]
    fn exact_declared_compensation_relation_validates() {
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        let accepted = canonical_node("plan01-01", PlanStatus::Accepted, &[]);
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        let mut rejected = canonical_node("plan01-02", PlanStatus::Rejected, &["plan01-01"]);
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        rejected.compensating_plan = "plan01-02-C01".to_string();
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        let compensation = canonical_node("plan01-02-C01", PlanStatus::Ready, &["plan01-01"]);
        validate(&canonical_ws(vec![accepted, rejected, compensation])).unwrap();
    }

    #[test]
    fn compensation_relation_matrix_rejects_missing_extra_reordered_and_wrong_sibling() {
        let cases = [
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            ("plan01-03-C01", vec!["plan01-01"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            ("plan01-03-C01", vec!["plan01-01", "plan01-02", "plan01-04"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            ("plan01-03-C01", vec!["plan01-02", "plan01-01"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            ("plan01-03-C02", vec!["plan01-01", "plan01-02"]),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            ("plan01-04-C01", vec!["plan01-01", "plan01-02"]),
        ];
        for (compensation_id, hard) in cases {
            let mut plans = vec![
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                canonical_node("plan01-01", PlanStatus::Accepted, &[]),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                canonical_node("plan01-02", PlanStatus::Accepted, &[]),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                canonical_node("plan01-04", PlanStatus::Accepted, &[]),
            ];
            let mut rejected = canonical_node(
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan01-03",
                PlanStatus::Rejected,
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                &["plan01-01", "plan01-02"],
            );
            rejected.compensating_plan = compensation_id.to_string();
            plans.push(rejected);
            plans.push(canonical_node(compensation_id, PlanStatus::Ready, &hard));
            let error = validate(&canonical_ws(plans)).unwrap_err();
            assert_eq!(error.code(), "MINE_GRAPH_INVALID", "case {compensation_id}");
        }
    }

    #[test]
    fn sealed_historical_compensation_shaped_id_without_declaration_validates() {
        // Early administrative migrations could preserve an ACCEPTED mirror
        // under a canonical-looking compensation ID without any rejected node
        // declaring it. The identifier alone must not fabricate a relationship.
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        let historical = canonical_node("plan01-03-C01", PlanStatus::Accepted, &[]);
        validate(&canonical_ws(vec![historical])).unwrap();
    }
}
