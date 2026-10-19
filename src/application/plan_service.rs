//! Application service for plan lifecycle transitions (`add`/`show`/`start`/
//! `implemented`/`accept`/`reject`).
//!
//! Per `docs/design/system/component-architecture.md`, the CLI and MCP
//! adapters call the **same** application services. This module owns the
//! state-machine transition checks, predecessor validation, successor
//! release, and evidence recording. It uses [`GraphService::mutate`] for the
//! lock→reload→recheck-revision→atomic-write→render transaction; it does
//! **not** reimplement persistence, locking, or revision handling.
//!
//! State-machine rules come from `docs/design/execution-graph/
//! state-machine-and-algorithms.md` and the domain
//! `PlanStatus::validate_transition` / `validation::hard_predecessors_accepted`.

use std::collections::HashSet;
use std::path::Path;

use crate::application::graph_service::{
    GraphService, PlanAcceptRequest, PlanAddRequest, PlanImplementedRequest, PlanRejectRequest,
    PlanStartRequest,
};
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{PlanNode, PlanWorkspace};
use crate::domain::plan_id::{
    CanonicalPlanId, registered_plan_branch, validate_compensation_replacement,
};
use crate::domain::status::PlanStatus;
use crate::domain::validation;
use crate::infrastructure::git::WorktreeInfo;

fn refusal(detail: impl Into<String>) -> MineError {
    MineError::GraphInvalid {
        detail: detail.into(),
    }
}

/// Untrusted selectors/declarations, never caller-authored approval facts.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticRegistrationInput {
    pub policy_path: crate::domain::graph::RepoPath,
    pub approving_adr: String,
    pub ownership: crate::domain::ownership::OwnershipContract,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticClarificationInput {
    pub policy_path: crate::domain::graph::RepoPath,
    pub approving_adr: String,
    pub plan: crate::domain::graph::PlanIdentity,
    pub actor: String,
    pub reason: String,
    pub kind: crate::domain::ownership::ClarificationKind,
    pub expected_tail: Option<crate::domain::graph::Sha256>,
    pub physical_mapping: Vec<crate::domain::graph::RepoPath>,
    pub generators: Vec<crate::domain::ownership::GeneratorTransaction>,
    /// Present only for explicit first activation; never replaces a baseline.
    #[serde(default)]
    pub baseline: Option<crate::domain::ownership::OwnershipContract>,
}

pub struct PlanClarifyRequest {
    pub id: String,
    pub expected_revision: u64,
    pub input: String,
    pub updated_at: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyBinding {
    schema_version: u32,
    policy_path: crate::domain::graph::RepoPath,
    policy_sha256: crate::domain::graph::Sha256,
}

struct ObservedOwnershipPolicy {
    policy: crate::domain::ownership::ApprovedOwnershipPolicy,
    provenance: crate::domain::graph::OwnershipPolicyProvenance,
    registry_sha256: crate::domain::graph::Sha256,
    approving_entry: crate::domain::adr::AdrEntry,
    scope: String,
}

fn contained_bytes(root: &Path, relative: &crate::domain::graph::RepoPath) -> MineResult<Vec<u8>> {
    let path = crate::infrastructure::operation_store::regular_path(root, relative)?;
    crate::infrastructure::operation_store::read_bounded(&path)
        .map_err(|error| refusal(format!("contained evidence unavailable: {error}")))
}

pub(crate) fn root_configuration(root: &Path) -> MineResult<crate::domain::config::MineConfig> {
    use crate::domain::{design_marker, graph::RepoPath};
    let relative = RepoPath::new(".mine/config.toml").expect("constant relative path");
    let bytes = contained_bytes(root, &relative)?;
    let config = crate::domain::config::MineConfig::parse(
        &root.join(relative.as_str()),
        std::str::from_utf8(&bytes).map_err(|_| refusal("configuration encoding"))?,
    )?;
    config.layout()?;
    uuid::Uuid::parse_str(&config.repository_id).map_err(|_| refusal("repository UUID invalid"))?;
    let marker_relative =
        RepoPath::new(config.design.marker.clone()).map_err(|e| refusal(e.to_string()))?;
    let marker_bytes = contained_bytes(root, &marker_relative)?;
    let marker_path = root.join(marker_relative.as_str());
    let marker = design_marker::DesignMarker::parse(
        &marker_path,
        std::str::from_utf8(&marker_bytes).map_err(|_| refusal("Design marker encoding"))?,
    )?;
    let design_index =
        RepoPath::new(config.design.root.clone()).map_err(|e| refusal(e.to_string()))?;
    let index = crate::infrastructure::operation_store::regular_path(root, &design_index)?;
    if !index.is_file() {
        return Err(refusal("Design index missing"));
    }
    design_marker::classify(
        index
            .parent()
            .ok_or_else(|| refusal("Design root invalid"))?,
        &marker_path,
        true,
        true,
        Some(&marker),
        Some(&config.repository_id),
    )?;
    Ok(config)
}

pub(crate) fn observe_planning_readiness(
    root: &Path,
    scope: &str,
    checked_at: &str,
) -> MineResult<crate::domain::plan_release::ReleaseReadinessAudit> {
    use crate::domain::graph::RepoPath;
    let config_relative = RepoPath::new(".mine/config.toml").expect("constant path");
    let mut registry_revision = 0;
    if root.join(config_relative.as_str()).exists() {
        let bytes = contained_bytes(root, &config_relative)?;
        let config = crate::domain::config::MineConfig::parse(
            &root.join(config_relative.as_str()),
            std::str::from_utf8(&bytes).map_err(|_| refusal("configuration encoding"))?,
        )?;
        let registry =
            RepoPath::new(config.adr.registry.clone()).map_err(|e| refusal(e.to_string()))?;
        let path = crate::infrastructure::operation_store::regular_path(root, &registry)?;
        if path.exists() {
            let service = crate::application::adr_service::AdrService::new(root, &config);
            service.require_planning_ready(scope)?;
            registry_revision = service.validate()?.revision;
        }
    }
    Ok(crate::domain::plan_release::ReleaseReadinessAudit {
        planning_scope: scope.to_string(),
        checked_at: checked_at.to_string(),
        adr_registry_revision: registry_revision,
    })
}

fn ownership_policy_binding(body: &[u8]) -> MineResult<PolicyBinding> {
    use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
    let text = std::str::from_utf8(body).map_err(|_| refusal("ADR body encoding"))?;
    let mut binding = None;
    let mut capture: Option<String> = None;
    // Parse Markdown events, not marker substrings inside quoted examples or
    // larger literal fences. Only the exact fenced-block info string binds.
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if info.as_ref() == "mine-ownership-policy" =>
            {
                if binding.is_some() || capture.is_some() {
                    return Err(refusal("duplicate ownership policy binding"));
                }
                capture = Some(String::new());
            }
            Event::Text(content) if capture.is_some() => {
                capture
                    .as_mut()
                    .expect("capture checked")
                    .push_str(&content);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(json) = capture.take() {
                    let parsed: PolicyBinding = serde_json::from_str(&json)
                        .map_err(|_| refusal("invalid ownership policy binding keys/schema"))?;
                    if parsed.schema_version != 1 {
                        return Err(refusal("unsupported ownership policy binding version"));
                    }
                    binding = Some(parsed);
                }
            }
            _ => {}
        }
    }
    binding.ok_or_else(|| refusal("approved ADR has no exact ownership policy binding"))
}

fn observe_ownership_policy(
    root: &Path,
    workspace: &PlanWorkspace,
    policy_path: &crate::domain::graph::RepoPath,
    approving_adr: &str,
    scope: &str,
    persisted: Option<&crate::domain::graph::OwnershipPolicyProvenance>,
) -> MineResult<ObservedOwnershipPolicy> {
    use crate::domain::{adr::AdrStatus, graph::*, ownership::*};
    use crate::infrastructure::graph_checkpoint::digest;
    let config = root_configuration(root)?;
    let design_index =
        RepoPath::new(config.design.root.clone()).map_err(|e| refusal(e.to_string()))?;
    let design_dir = Path::new(design_index.as_str())
        .parent()
        .and_then(Path::to_str)
        .ok_or_else(|| refusal("Design root invalid"))?;
    if !crate::domain::path::is_within(design_dir, policy_path.as_str()) {
        return Err(refusal("ownership policy must be within configured Design"));
    }
    let registry_path =
        RepoPath::new(config.adr.registry.clone()).map_err(|e| refusal(e.to_string()))?;
    let registry_before = contained_bytes(root, &registry_path)?;
    let registry = crate::infrastructure::adr_store::AdrStore::new(root, &config.adr).load()?;
    let entry = registry
        .get(approving_adr)
        .ok_or_else(|| refusal("policy approving ADR missing"))?;
    if entry.status != AdrStatus::APPROVED || !entry.applies_to(Some(scope)) {
        return Err(refusal(
            "policy ADR is not currently approved for this scope",
        ));
    }
    let adr_path = RepoPath::new(entry.path.clone()).map_err(|e| refusal(e.to_string()))?;
    if !crate::domain::path::is_within(design_dir, adr_path.as_str()) {
        return Err(refusal("approving ADR must be within configured Design"));
    }
    let body = contained_bytes(root, &adr_path)?;
    if crate::domain::adr::sha256_hex(&body) != entry.content_sha256 {
        return Err(refusal("policy ADR body differs from frozen approval"));
    }
    let binding = ownership_policy_binding(&body)?;
    if &binding.policy_path != policy_path {
        return Err(refusal(
            "selected policy path differs from approved binding",
        ));
    }
    let bytes = contained_bytes(root, policy_path)?;
    if digest(&bytes) != binding.policy_sha256 {
        return Err(refusal("policy bytes differ from approved ADR binding"));
    }
    let observed = OwnershipPolicyProvenance {
        repository_id: config.repository_id,
        workspace_id: workspace.workspace_id.clone(),
        policy_path: policy_path.clone(),
        policy_sha256: binding.policy_sha256.clone(),
        adr_id: entry.id.clone(),
        adr_path: adr_path.clone(),
        adr_body_sha256: digest(&body),
        adr_registry_revision: registry.revision,
        approval_actor: entry.approved_by.clone(),
        admitted_graph_revision: workspace.revision,
    };
    let provenance = if let Some(original) = persisted {
        let mut current = observed;
        if original.admitted_graph_revision > workspace.revision
            || original.adr_registry_revision > registry.revision
        {
            return Err(refusal(
                "policy provenance has a future graph/ADR observation",
            ));
        }
        current.admitted_graph_revision = original.admitted_graph_revision;
        current.adr_registry_revision = original.adr_registry_revision;
        if &current != original {
            return Err(refusal("approved policy provenance changed"));
        }
        original.clone()
    } else {
        observed
    };
    let identity = PolicyIdentity::new(PolicyIdentityFields {
        design_references: vec![adr_path],
        sha256: binding.policy_sha256,
    })
    .map_err(|e| refusal(e.to_string()))?;
    let policy = ApprovedOwnershipPolicy::verify(
        &bytes,
        &PolicyApproval {
            identity,
            actor: entry.approved_by.clone(),
            observed_revision: provenance.admitted_graph_revision,
        },
        workspace.revision,
    )
    .map_err(|e| refusal(e.to_string()))?;
    if registry_before != contained_bytes(root, &registry_path)?
        || body != contained_bytes(root, &provenance.adr_path)?
        || bytes != contained_bytes(root, &provenance.policy_path)?
    {
        return Err(refusal("policy approval observation changed"));
    }
    Ok(ObservedOwnershipPolicy {
        policy,
        provenance,
        registry_sha256: digest(&registry_before),
        approving_entry: entry.clone(),
        scope: scope.to_string(),
    })
}

fn observe_node_authority(
    root: &Path,
    workspace: &PlanWorkspace,
    node: &PlanNode,
    repository_id: &str,
    cached_policy: Option<&ObservedOwnershipPolicy>,
) -> MineResult<(
    crate::domain::ownership::EffectiveAuthority,
    Option<ObservedOwnershipPolicy>,
)> {
    use crate::domain::{graph::*, ownership::*};
    let identity = PlanIdentity::new(PlanIdentityFields {
        repository_id: repository_id.to_string(),
        workspace_id: workspace.workspace_id.clone(),
        plan_id: node.id.clone(),
        plan_path: RepoPath::new(node.path.clone()).map_err(|e| refusal(e.to_string()))?,
        body_sha256: node.body_sha256.clone(),
        lineage_id: node.lineage_id.clone(),
        branch: node.branch.clone(),
    })
    .map_err(|e| refusal(e.to_string()))?;
    if node.ownership.is_none() && node.ownership_policy.is_none() && node.clarifications.is_empty()
    {
        let body = contained_bytes(root, &identity.fields().plan_path)?;
        let mapping = node
            .write_scope()?
            .into_iter()
            .map(RepoPath::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| refusal(e.to_string()))?;
        let authority = EffectiveAuthority::observe_conservative(identity, &body, mapping)
            .map_err(|e| refusal(e.to_string()))?;
        return Ok((authority, None));
    }
    let original = node
        .ownership
        .as_ref()
        .ok_or_else(|| refusal("semantic provenance has no immutable declaration baseline"))?;
    let provenance = node
        .ownership_policy
        .as_ref()
        .ok_or_else(|| refusal("semantic provenance has no approved policy binding"))?;
    let scope = if node.planning_scope.is_empty() {
        Path::new(&workspace.design_root)
            .parent()
            .and_then(Path::to_str)
            .ok_or_else(|| refusal("Design planning scope missing"))?
    } else {
        &node.planning_scope
    };
    let discovered = if cached_policy.is_some_and(|cached| &cached.provenance == provenance) {
        None
    } else {
        Some(observe_ownership_policy(
            root,
            workspace,
            &provenance.policy_path,
            &provenance.adr_id,
            scope,
            Some(provenance),
        )?)
    };
    let observed = discovered
        .as_ref()
        .or(cached_policy)
        .expect("policy freshly observed or cached");
    if !observed.approving_entry.applies_to(Some(scope)) {
        return Err(refusal("approved policy does not apply to this Plan scope"));
    }
    let body = contained_bytes(root, &identity.fields().plan_path)?;
    let mut current = identity.fields().clone();
    current.body_sha256 = Some(crate::infrastructure::graph_checkpoint::digest(&body));
    let current = PlanIdentity::new(current).map_err(|e| refusal(e.to_string()))?;
    let authority = AuthorityResolver::resolve(
        original,
        &body,
        &current,
        &observed.policy,
        workspace.revision,
        &node.clarifications,
    )
    .map_err(|e| refusal(e.to_string()))?;
    Ok((authority, discovered))
}

pub(crate) fn authority_observation(
    snapshot: &crate::infrastructure::graph_snapshot::GraphSnapshot,
    id: &str,
) -> MineResult<crate::domain::ownership::EffectiveAuthority> {
    authority_from_workspace(&snapshot.repository_root, snapshot.workspace(), id)
}

pub(crate) fn authority_from_workspace(
    root: &Path,
    workspace: &PlanWorkspace,
    id: &str,
) -> MineResult<crate::domain::ownership::EffectiveAuthority> {
    let config = root_configuration(root)?;
    let node = workspace.get(id).ok_or_else(|| MineError::PlanNotFound {
        plan_id: id.to_string(),
    })?;
    Ok(observe_node_authority(root, workspace, node, &config.repository_id, None)?.0)
}

pub(crate) fn ownership_snapshot(
    snapshot: &crate::infrastructure::graph_snapshot::GraphSnapshot,
    guard: &crate::infrastructure::file_lock::FileLock,
) -> MineResult<crate::domain::scheduling::OwnershipWave> {
    let observed = ownership_observation(
        &snapshot.repository_root,
        snapshot.workspace(),
        crate::domain::graph::Sha256::new(snapshot.source_sha256.clone())
            .map_err(|e| refusal(e.to_string()))?,
        guard,
    )?;
    observed
        .evaluator(snapshot.workspace())?
        .wave()
        .map_err(|e| refusal(e.to_string()))
}

struct OwnershipObservation {
    repository_id: String,
    source_sha256: crate::domain::graph::Sha256,
    nodes: Vec<crate::domain::scheduling::NodeOwnership>,
    policy: Option<ObservedOwnershipPolicy>,
    pending: Option<crate::domain::scheduling::PendingPublication>,
}
impl OwnershipObservation {
    fn evaluator<'a>(
        &'a self,
        workspace: &'a PlanWorkspace,
    ) -> MineResult<crate::domain::scheduling::SchedulingKernel<'a>> {
        crate::domain::scheduling::SchedulingKernel::new(
            &self.repository_id,
            workspace,
            self.source_sha256.clone(),
            self.policy.as_ref().map(|p| &p.policy),
            self.nodes.clone(),
            self.pending.clone(),
        )
        .map_err(|e| refusal(e.to_string()))
    }
}

fn ownership_observation(
    root: &Path,
    workspace: &PlanWorkspace,
    source_sha256: crate::domain::graph::Sha256,
    guard: &crate::infrastructure::file_lock::FileLock,
) -> MineResult<OwnershipObservation> {
    use crate::domain::{graph::*, lifecycle_operation::*, scheduling::*};
    let config = root_configuration(root)?;
    let operations = crate::infrastructure::operation_store::OperationStore::under_lock(
        root,
        &config.repository_id,
        &workspace.workspace_id,
        guard,
    )
    .and_then(|store| store.observations())
    .map_err(|e| refusal(format!("operation evidence invalid: {e}")))?;
    let mut barriers = BarrierFacts::default();
    for record in &operations {
        barriers.observe(&record.operation);
    }
    let pending = barriers.global.first().map(|pending| {
        let operation = operations
            .iter()
            .find(|record| record.operation.fields().operation_id == pending.operation_id)
            .expect("observed operation")
            .operation
            .fields();
        let stage = match pending.stage {
            OperationStage::GraphCheckpoint(
                CheckpointStage::Intent | CheckpointStage::GitWorkspaceMutated,
            ) => PublicationStage::Graph,
            OperationStage::GraphCheckpoint(CheckpointStage::GraphPersisted) => {
                PublicationStage::Render
            }
            _ => PublicationStage::Checkpoint,
        };
        PendingPublication {
            operation_id: pending.operation_id.as_str().into(),
            owner: operation.actor.clone(),
            stage,
        }
    });
    let mut nodes = Vec::with_capacity(workspace.plans.len());
    let mut policy: Option<ObservedOwnershipPolicy> = None;
    let mut stopped_refs = Vec::new();
    for node in &workspace.plans {
        let (authority, observed) = observe_node_authority(
            root,
            workspace,
            node,
            &config.repository_id,
            policy.as_ref(),
        )?;
        if let Some(observed) = observed {
            if policy
                .as_ref()
                .is_some_and(|p| p.policy.identity() != observed.policy.identity())
            {
                return Err(refusal(
                    "snapshot contains incompatible approved ownership policies",
                ));
            }
            if policy.is_none() {
                policy = Some(observed);
            }
        }
        let unfinished = operations.iter().find(|record| {
            !terminal(&record.operation)
                && record
                    .operation
                    .fields()
                    .plan
                    .as_ref()
                    .is_some_and(|p| p.fields().plan_id == node.id)
        });
        let closure = if node.status == PlanStatus::Rejected {
            let completed = operations.iter().find(|record| {
                terminal(&record.operation)
                    && record.operation.fields().kind == OperationKind::RejectedHandoff
                    && record.operation.fields().outcome == OperationOutcome::Committed
                    && record
                        .operation
                        .fields()
                        .plan
                        .as_ref()
                        .is_some_and(|p| p == authority.plan())
            });
            if let Some(record) = completed {
                let fields = record.operation.fields();
                if fields.integrated_commit.is_some()
                    || fields.cleanup.is_some()
                    || fields.checkpoint.is_some()
                    || !fields.verification_evidence.is_empty()
                {
                    return Err(refusal("rejected handoff contains foreign closure facts"));
                }
                let Some(ClosureBinding::RejectedHandoff { compensation }) = &record.closure else {
                    return Err(refusal("rejected handoff binding missing"));
                };
                let review = record
                    .operation
                    .fields()
                    .review
                    .as_ref()
                    .ok_or_else(|| refusal("stopped handoff review evidence missing"))?;
                stopped_refs.push((node.branch.clone(), review.reviewed_tip.clone()));
                let report_bytes = contained_bytes(root, &review.report.path)?;
                if crate::infrastructure::graph_checkpoint::digest(&report_bytes)
                    != review.report.sha256
                    || report_bytes.len() as u64 != review.report.bytes
                {
                    return Err(refusal("stopped handoff report evidence changed"));
                }
                ClosureFacts::RejectedHandoff {
                    evidence: Box::new(StoppedHandoff {
                        rejected_plan: authority.plan().clone(),
                        compensation: compensation.clone(),
                        report: review.report.clone(),
                        stopped: true,
                        observed_revision: record.operation.fields().observed_graph_revision,
                    }),
                }
            } else {
                ClosureFacts::RejectionPending
            }
        } else if unfinished
            .is_some_and(|record| record.operation.fields().kind == OperationKind::Acceptance)
        {
            ClosureFacts::ReviewPending
        } else {
            ClosureFacts::Settled
        };
        nodes.push(NodeOwnership {
            authority,
            worktree: RepoPath::new(node.worktree_path.clone())
                .map_err(|e| refusal(e.to_string()))?,
            mutable_outputs: vec![
                RepoPath::new(format!(".mine/build/{}", node.id))
                    .map_err(|e| refusal(e.to_string()))?,
            ],
            closure,
        });
    }
    if let Some(observed) = &policy {
        let fresh = observe_ownership_policy(
            root,
            workspace,
            &observed.provenance.policy_path,
            &observed.provenance.adr_id,
            &observed.scope,
            Some(&observed.provenance),
        )?;
        if fresh.registry_sha256 != observed.registry_sha256 {
            return Err(refusal("policy observation changed during snapshot"));
        }
    }
    let mut stopped_git = crate::infrastructure::graph_checkpoint::checkpoint_git(root, None)
        .map_err(|e| refusal(e.to_string()))?;
    crate::infrastructure::graph_checkpoint::plan_git::preserved_references(
        &mut stopped_git,
        &stopped_refs,
    )
    .map_err(|e| refusal(e.to_string()))?;
    Ok(OwnershipObservation {
        repository_id: config.repository_id,
        source_sha256,
        nodes,
        policy,
        pending,
    })
}

fn validate_review_report(root: &Path, node: &PlanNode, requested: &str) -> MineResult<String> {
    let requested = crate::domain::path::normalize_repo_relative(requested)?;
    let expected = crate::domain::plan_id::report_path(&node.path, "review")?;
    if requested != expected {
        return Err(refusal(format!(
            "rejection review report must be the canonical path {expected}"
        )));
    }
    if requested == node.implementation_report {
        return Err(refusal(
            "rejection review report must not be the implementation report",
        ));
    }
    let report_path = root.join(&requested);
    let metadata = report_path.symlink_metadata().map_err(|_| {
        refusal(format!(
            "rejection review report does not exist at {expected}"
        ))
    })?;
    if !metadata.file_type().is_file() {
        return Err(refusal("rejection review report must be a regular file"));
    }
    let canonical_root = root.canonicalize()?;
    let canonical_report = report_path.canonicalize()?;
    if !canonical_report.starts_with(&canonical_root) {
        return Err(refusal("rejection review report escapes repository Root"));
    }
    Ok(expected)
}

/// Shared plan lifecycle service. Constructed over a [`GraphService`] and used
/// by both the CLI `plan.*` handlers and the MCP `mine_plan_*` tools.
pub struct PlanService<'a> {
    graph: &'a GraphService<'a>,
}

/// Runtime-enriched Plan lookup DTO. Persisted fields remain flattened for
/// compatibility; absolute/caller/observed locations are read-only evidence.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanLookup {
    #[serde(flatten)]
    pub node: PlanNode,
    pub effective_authority: crate::domain::ownership::EffectiveAuthority,
    pub registered_worktree_path: String,
    pub resolved_worktree_path: String,
    pub observed_worktree: Option<WorktreeInfo>,
    pub caller_location: String,
    pub authoritative_root: String,
    pub authoritative_revision: u64,
    pub pending_operations: Vec<crate::infrastructure::operation_store::OperationObservation>,
}

/// Result of deterministic linked-worktree preparation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanPreparation {
    #[serde(flatten)]
    pub lookup: PlanLookup,
    pub action: String,
    pub revision_before: u64,
    pub revision_after: u64,
    pub request_operation_id: crate::domain::lifecycle_operation::OperationId,
    pub request_actor: String,
    pub generation: Option<crate::domain::graph::PreparationGeneration>,
    pub checkpoint: Option<crate::domain::lifecycle_operation::LifecycleOperation>,
    pub replay: bool,
}

#[derive(Debug, Clone)]
pub struct PlanPrepareRequest {
    pub id: String,
    pub expected_revision: u64,
    pub operation_id: crate::domain::lifecycle_operation::OperationId,
    pub actor: String,
    pub mode: crate::domain::graph::PreparationMode,
}

impl<'a> PlanService<'a> {
    #[must_use]
    pub fn new(graph: &'a GraphService<'a>) -> Self {
        Self { graph }
    }

    /// Looks up a plan by id. Read-only.
    pub fn show(&self, id: &str) -> MineResult<(u64, PlanNode)> {
        let ws = self.graph.validate()?;
        let node = ws.get(id).cloned().ok_or_else(|| MineError::PlanNotFound {
            plan_id: id.to_string(),
        })?;
        Ok((ws.revision, node))
    }

    /// Looks up a Plan and enriches it with authoritative Root and linked
    /// worktree evidence gathered from Git's stable porcelain inventory.
    pub fn show_with_locations(
        &self,
        id: &str,
        caller_root: &std::path::Path,
    ) -> MineResult<PlanLookup> {
        let ws = self.graph.validate()?;
        let node = ws.get(id).cloned().ok_or_else(|| MineError::PlanNotFound {
            plan_id: id.to_string(),
        })?;
        let lineage_branches: HashSet<&str> = ws
            .plans
            .iter()
            .filter(|candidate| candidate.lineage_id == node.lineage_id)
            .map(|candidate| candidate.branch.as_str())
            .collect();
        let observed_worktree = self
            .graph
            .worktrees()?
            .into_iter()
            .skip(1)
            .find(|worktree| {
                worktree
                    .branch
                    .as_deref()
                    .is_some_and(|branch| lineage_branches.contains(branch))
            });
        let resolved_worktree_path = observed_worktree.as_ref().map_or_else(
            || {
                self.graph
                    .repo_root()
                    .join(&node.worktree_path)
                    .display()
                    .to_string()
            },
            |worktree| worktree.path.clone(),
        );
        let pending_operations = self
            .graph
            .pending_operations()?
            .into_iter()
            .filter(|record| {
                record
                    .operation
                    .fields()
                    .plan
                    .as_ref()
                    .is_some_and(|plan| plan.fields().plan_id == node.id)
            })
            .collect();
        Ok(PlanLookup {
            effective_authority: self.graph.effective_authority(id)?,
            registered_worktree_path: node.worktree_path.clone(),
            resolved_worktree_path,
            observed_worktree,
            caller_location: caller_root.display().to_string(),
            authoritative_root: self.graph.repo_root().display().to_string(),
            authoritative_revision: ws.revision,
            pending_operations,
            node,
        })
    }

    /// Creates or inspects the exact registered workspace through native intent.
    pub fn prepare(
        &self,
        request: PlanPrepareRequest,
        caller_root: &Path,
    ) -> MineResult<PlanPreparation> {
        super::native_plan_workspace::NativePlanWorkspace::new(self.graph).prepare(
            request,
            caller_root,
            &mut crate::domain::lifecycle_operation::NoInterruption,
        )
    }

    /// Adds a new plan node (status `DRAFT`).
    pub fn add(&self, req: PlanAddRequest) -> MineResult<PlanWorkspace> {
        let expected = req.expected_revision;
        let actor = req.actor.clone();
        let worktrees_root = self.graph.worktrees_root()?;
        let root = self.graph.repo_root().to_path_buf();
        self.graph.mutate(expected, &actor, move |mut w| {
            if w.get(&req.id).is_some() {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan id {} already exists", req.id),
                });
            }
            // The planning scope is recorded verbatim (ADR-0013) but must be
            // a non-empty, safe repository-relative scope.
            if req.planning_scope.trim().is_empty() {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} requires a non-empty planning scope", req.id),
                });
            }
            crate::domain::path::normalize_repo_relative(&req.planning_scope)?;
            let planning_scope = req.planning_scope.clone();
            observe_planning_readiness(&root, &planning_scope, "")?;
            let parsed = CanonicalPlanId::parse(&req.id)?;
            let _slug = root_configuration(&root)?
                .layout()?
                .validate_plan_path(&req.id, &req.path)?;
            let (branch, lineage_id, worktree_path) = (
                registered_plan_branch(&w.workspace_id, &req.id, &req.path)?,
                parsed.lineage_id(),
                parsed.worktree_path_under(&worktrees_root),
            );
            if req.design_references.is_empty() {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} has no design references", req.id),
                });
            }
            for dr in &req.design_references {
                if dr.is_empty() {
                    return Err(MineError::GraphInvalid {
                        detail: "design-reference must not be empty".to_string(),
                    });
                }
                crate::domain::path::normalize_repo_relative(dr)?;
            }
            for hp in &req.hard_predecessors {
                if !w.ids().contains(hp.as_str()) {
                    return Err(MineError::GraphInvalid {
                        detail: format!("plan {} hard_predecessor {} not found", req.id, hp),
                    });
                }
            }
            let node = PlanNode {
                id: req.id,
                path: req.path,
                title: req.title,
                branch,
                branch_anchor: String::new(),
                status: PlanStatus::Draft,
                hard_predecessors: req.hard_predecessors,
                soft_predecessors: vec![],
                design_references: req.design_references,
                exclusive_write_paths: req.exclusive_write_paths,
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
                planning_scope,
                readiness_checked_at: String::new(),
                adr_registry_revision: 0,
                lineage_id,
                worktree_path,
                body_sha256: None,
                ownership: None,
                ownership_policy: None,
                clarifications: vec![],
                acceptance: None,
                preparations: vec![],
                started_preparation_generation: None,
            };
            let added_id = node.id.clone();
            let is_compensation =
                CanonicalPlanId::parse(&added_id).is_ok_and(|parsed| parsed.is_compensation());
            w.plans.push(node);
            if is_compensation {
                validation::declared_compensation_predecessor(&w, &added_id)?;
            }
            // New canonical registrations capture actual contained body bytes.
            // Legacy migration never derives this historical fact retrospectively.
            if CanonicalPlanId::parse(&added_id).is_ok() {
                use crate::domain::graph::{PlanIdentity, PlanIdentityFields, RepoPath};
                use crate::domain::ownership::AuthorityResolver;
                let added = w.get(&added_id).expect("just registered").clone();
                let body_path =
                    RepoPath::new(added.path.clone()).map_err(|e| refusal(e.to_string()))?;
                let body = contained_bytes(&root, &body_path)?;
                crate::infrastructure::governance_git::freeze_body(&root, &body_path, &body)?;
                let body_sha256 = crate::infrastructure::graph_checkpoint::digest(&body);
                w.get_mut(&added_id).expect("just registered").body_sha256 =
                    Some(body_sha256.clone());
                if let Some(input_path) = req.ownership_input {
                    let input_path =
                        RepoPath::new(input_path).map_err(|e| refusal(e.to_string()))?;
                    let input: SemanticRegistrationInput =
                        serde_json::from_slice(&contained_bytes(&root, &input_path)?)
                            .map_err(|_| refusal("invalid typed semantic registration input"))?;
                    let observed = observe_ownership_policy(
                        &root,
                        &w,
                        &input.policy_path,
                        &input.approving_adr,
                        &added.planning_scope,
                        w.plans
                            .iter()
                            .filter_map(|node| node.ownership_policy.as_ref())
                            .find(|binding| {
                                binding.policy_path == input.policy_path
                                    && binding.adr_id == input.approving_adr
                            }),
                    )?;
                    let identity = PlanIdentity::new(PlanIdentityFields {
                        repository_id: observed.provenance.repository_id.clone(),
                        workspace_id: w.workspace_id.clone(),
                        plan_id: added.id,
                        plan_path: body_path,
                        body_sha256: Some(body_sha256),
                        lineage_id: added.lineage_id,
                        branch: added.branch,
                    })
                    .map_err(|e| refusal(e.to_string()))?;
                    if input.ownership.fields().plan != identity {
                        return Err(refusal("semantic declaration Plan/body identity differs"));
                    }
                    AuthorityResolver::resolve(
                        &input.ownership,
                        &body,
                        &identity,
                        &observed.policy,
                        expected,
                        &[],
                    )
                    .map_err(|e| refusal(e.to_string()))?;
                    let fresh = observe_ownership_policy(
                        &root,
                        &w,
                        &input.policy_path,
                        &input.approving_adr,
                        &added.planning_scope,
                        Some(&observed.provenance),
                    )?;
                    if fresh.registry_sha256 != observed.registry_sha256 {
                        return Err(refusal("approval registry changed before registration"));
                    }
                    let node = w.get_mut(&added_id).expect("just registered");
                    node.ownership = Some(input.ownership);
                    node.ownership_policy = Some(observed.provenance);
                }
            } else if req.ownership_input.is_some() {
                return Err(refusal(
                    "semantic registration requires canonical Plan identity",
                ));
            }
            w.revision = expected + 1;
            // DRAFT defers execution admission, not declaration validity. Use
            // the public reader's authority resolver before any graph/checkpoint
            // publication; never persist an unqueryable empty conservative scope.
            authority_from_workspace(&root, &w, &added_id)?;
            Ok(w)
        })
    }

    /// Append an owner-authorized clarification under the one coordinator.
    pub fn clarify(
        &self,
        req: PlanClarifyRequest,
    ) -> MineResult<(PlanWorkspace, crate::domain::ownership::EffectiveAuthority)> {
        use crate::domain::{graph::*, ownership::*};
        let root = self.graph.repo_root().to_path_buf();
        // Caller-contained input supplies attribution, never stored owner/Git.
        // Re-read and compare inside the transaction before publishing intent.
        let actor_path = RepoPath::new(req.input.clone()).map_err(|e| refusal(e.to_string()))?;
        let declared: SemanticClarificationInput =
            serde_json::from_slice(&contained_bytes(&root, &actor_path)?)
                .map_err(|_| refusal("invalid typed clarification input"))?;
        let actor = declared.actor;
        let mut result = None;
        let workspace = self.graph.mutate_locked(
            req.expected_revision,
            &actor,
            |mut workspace, guard, source_sha256| {
                let node =
                    workspace
                        .get(&req.id)
                        .cloned()
                        .ok_or_else(|| MineError::PlanNotFound {
                            plan_id: req.id.clone(),
                        })?;
                if node.status.is_terminal() {
                    return Err(refusal("terminal Plan authority cannot be clarified"));
                }
                let input_path =
                    RepoPath::new(req.input.clone()).map_err(|e| refusal(e.to_string()))?;
                let input: SemanticClarificationInput =
                    serde_json::from_slice(&contained_bytes(&root, &input_path)?)
                        .map_err(|_| refusal("invalid typed clarification input"))?;
                if input.actor != actor {
                    return Err(refusal("clarification actor changed before admission"));
                }
                let first_activation = node.ownership.is_none();
                if first_activation != (input.kind == ClarificationKind::BaselineAttestation)
                    || first_activation != input.baseline.is_some()
                {
                    return Err(refusal(
                        "baseline declarations are allowed only for explicit first activation",
                    ));
                }
                let baseline = node
                    .ownership
                    .clone()
                    .or(input.baseline)
                    .ok_or_else(|| refusal("first activation declaration baseline missing"))?;
                let scope = if node.planning_scope.is_empty() {
                    Path::new(&workspace.design_root)
                        .parent()
                        .and_then(Path::to_str)
                        .ok_or_else(|| refusal("Design planning scope missing"))?
                } else {
                    &node.planning_scope
                };
                let persisted = node.ownership_policy.as_ref().or_else(|| {
                    workspace
                        .plans
                        .iter()
                        .filter_map(|node| node.ownership_policy.as_ref())
                        .find(|binding| {
                            binding.policy_path == input.policy_path
                                && binding.adr_id == input.approving_adr
                        })
                });
                if node.ownership_policy.as_ref().is_some_and(|binding| {
                    binding.policy_path != input.policy_path
                        || binding.adr_id != input.approving_adr
                }) {
                    return Err(refusal(
                        "clarification cannot switch its original approved policy binding",
                    ));
                }
                let observed = observe_ownership_policy(
                    &root,
                    &workspace,
                    &input.policy_path,
                    &input.approving_adr,
                    scope,
                    persisted,
                )?;
                let body = contained_bytes(
                    &root,
                    &RepoPath::new(node.path.clone()).map_err(|e| refusal(e.to_string()))?,
                )?;
                let effective = AuthorityResolver::append(
                    &baseline,
                    &body,
                    &observed.policy,
                    req.expected_revision,
                    &node.clarifications,
                    ClarificationRequest {
                        plan: input.plan,
                        policy: observed.policy.identity().clone(),
                        expected_revision: req.expected_revision,
                        expected_tail: input.expected_tail,
                        actor: input.actor,
                        reason: input.reason,
                        kind: input.kind,
                        physical_mapping: input.physical_mapping,
                        generators: input.generators,
                    },
                )
                .map_err(|e| refusal(e.to_string()))?;
                let mut ownership =
                    ownership_observation(&root, &workspace, source_sha256.clone(), guard)?;
                if ownership
                    .policy
                    .as_ref()
                    .is_some_and(|p| p.policy.identity() != observed.policy.identity())
                {
                    return Err(refusal(
                        "clarification policy differs from other semantic owners",
                    ));
                }
                ownership.policy = Some(observed);
                let admission = ownership
                    .evaluator(&workspace)?
                    .admit_clarification(&req.id, effective.clone(), req.expected_revision)
                    .map_err(|e| refusal(e.to_string()))?;
                if !admission.admissible() {
                    return Err(refusal(format!(
                        "clarification ownership blocked: {:?}",
                        admission.blockers
                    )));
                }
                let observed = ownership.policy.as_ref().expect("explicit verified policy");
                let fresh = observe_ownership_policy(
                    &root,
                    &workspace,
                    &observed.provenance.policy_path,
                    &observed.provenance.adr_id,
                    scope,
                    Some(&observed.provenance),
                )?;
                if fresh.registry_sha256 != observed.registry_sha256 {
                    return Err(refusal("clarification approval observation changed"));
                }
                let node = workspace
                    .get_mut(&req.id)
                    .expect("Plan observed under lock");
                node.ownership = Some(baseline);
                node.ownership_policy = Some(observed.provenance.clone());
                node.clarifications = effective.clarifications().to_vec();
                node.updated_at = req.updated_at.clone();
                workspace.revision = req.expected_revision + 1;
                result = Some(effective);
                Ok(workspace)
            },
        )?;
        Ok((
            workspace,
            result.expect("clarification result recorded on success"),
        ))
    }

    /// Starts a plan: requires `READY` + hard predecessors `ACCEPTED`.
    pub fn start(&self, req: PlanStartRequest) -> MineResult<PlanWorkspace> {
        let expected = req.expected_revision;
        let actor = req.owner.clone();
        let root = self.graph.repo_root().to_path_buf();
        self.graph
            .mutate_locked(expected, &actor, move |mut w, guard, source_sha256| {
                let start_node =
                    w.get(&req.id)
                        .cloned()
                        .ok_or_else(|| MineError::PlanNotFound {
                            plan_id: req.id.clone(),
                        })?;
                let current = start_node.status;
                if current != PlanStatus::Ready {
                    return Err(MineError::InvalidTransition {
                        plan_id: req.id.clone(),
                        from: current.as_str().to_string(),
                        to: PlanStatus::InProgress.as_str().to_string(),
                    });
                }
                crate::domain::error::contract_text("owner", &req.owner)
                    .map_err(|e| refusal(e.to_string()))?;
                crate::domain::error::contract_text("run_id", &req.run_id)
                    .map_err(|e| refusal(e.to_string()))?;
                if !validation::hard_predecessors_accepted(&w, &req.id)? {
                    let preds = w
                        .get(&req.id)
                        .map(|n| n.hard_predecessors.clone())
                        .unwrap_or_default();
                    let unaccepted = preds
                        .into_iter()
                        .find(|p| w.get(p).is_some_and(|n| n.status != PlanStatus::Accepted))
                        .unwrap_or_default();
                    return Err(MineError::PredecessorNotAccepted {
                        plan_id: req.id.clone(),
                        predecessor_id: unaccepted,
                        predecessor_status: "not accepted".to_string(),
                    });
                }
                let ownership = ownership_observation(&root, &w, source_sha256.clone(), guard)?;
                let admission = ownership
                    .evaluator(&w)?
                    .admit(&req.id, expected)
                    .map_err(|e| refusal(e.to_string()))?;
                if !admission.admissible() {
                    return Err(refusal(format!(
                        "ownership admission blocked: {:?}",
                        admission.blockers
                    )));
                }
                let scope = if start_node.planning_scope.is_empty() {
                    Path::new(&w.design_root)
                        .parent()
                        .and_then(Path::to_str)
                        .ok_or_else(|| refusal("Design planning scope missing"))?
                } else {
                    &start_node.planning_scope
                };
                observe_planning_readiness(&root, scope, &req.started_at)?;
                use crate::infrastructure::graph_checkpoint::{
                    CheckpointKernel, checkpoint_git, digest, plan_git::{self, CheckoutState, RefState},
                    preparation_history, preparation_intent,
                };
                use crate::domain::graph::{FileIdentity, RepoPath};
                let generation = start_node.preparations.last().ok_or_else(||refusal("READY Plan requires completed preparation; use explicit legacy recovery when applicable"))?;
                let kernel = CheckpointKernel::under_lock(&root,guard).map_err(|e|refusal(e.to_string()))?;
                let (common,head) = kernel.preparation_root().map_err(|e|refusal(e.to_string()))?;
                let handoffs = kernel.store().observations().map_err(|e|refusal(e.to_string()))?;
                super::native_lifecycle::observe_compensation_handoff(&root,&w,&start_node,&handoffs)?;
                let config = root_configuration(&root)?;
                let mut git = checkpoint_git(&root,None).map_err(|e|refusal(e.to_string()))?;
                let path = RepoPath::new(start_node.path.clone()).map_err(|e|refusal(e.to_string()))?;
                let body = contained_bytes(&root,&path)?;
                if generation.plan.fields().body_sha256.as_ref() != Some(&digest(&body)) {
                    return Err(refusal("start body differs from frozen preparation"));
                }
                let mut decisions = Vec::new();
                for reference in &start_node.design_references {
                    let path = RepoPath::new(reference.clone()).map_err(|e|refusal(e.to_string()))?;
                    let bytes = contained_bytes(&root,&path)?;
                    decisions.push(FileIdentity {path,bytes:bytes.len() as u64,sha256:digest(&bytes)});
                }
                let mut files = decisions.clone();
                files.push(FileIdentity {path,bytes:body.len() as u64,sha256:digest(&body)});
                super::native_evidence::committed_files(&root,&head,&files)?;
                let location = RepoPath::new(start_node.worktree_path.clone()).map_err(|e|refusal(e.to_string()))?;
                let reference = plan_git::reference(&mut git,&start_node.branch).map_err(|e|refusal(e.to_string()))?;
                let checkout = plan_git::checkout(&root,&common,&location,&start_node.branch,&mut git).map_err(|e|refusal(e.to_string()))?;
                let expected_checkout = preparation_intent::completed_checkout(&kernel,generation).map_err(|e|refusal(e.to_string()))?;
                if reference != (RefState::Present {tip:generation.prepared_branch_commit.clone()})
                    || !matches!(checkout,CheckoutState::Present {proof} if proof == expected_checkout) {
                    return Err(refusal("READY start requires clean exact prepared ref and checkout"));
                }
                if CanonicalPlanId::parse(&start_node.id)?.is_compensation() {
                    let rejected = validation::declaring_rejected_predecessor(&w,&start_node.id)?;
                    if plan_git::reference(&mut git,&rejected.branch).map_err(|e|refusal(e.to_string()))?
                        != (RefState::Present {tip:generation.prepared_branch_commit.clone()}) {
                        return Err(refusal("preserved rejected predecessor advanced after preparation"));
                    }
                }
                preparation_history::prove(&mut git,&config,&w,&start_node,&head,&decisions).map_err(|e|refusal(e.to_string()))?;
                let expected_anchor = generation.prepared_branch_commit.as_str().to_owned();
                let ordinal = generation.ordinal;
                let node = w.get_mut(&req.id).expect("checked present above");
                node.status
                    .validate_transition(&req.id, PlanStatus::InProgress)?;
                node.status = PlanStatus::InProgress;
                node.branch_anchor = expected_anchor;
                node.started_preparation_generation = Some(ordinal);
                node.owner = req.owner;
                node.run_id = req.run_id;
                node.started_at = req.started_at.clone();
                node.updated_at = req.started_at;
                w.revision = expected + 1;
                Ok(w)
            })
    }

    /// Marks a plan implemented: records the report + commits.
    pub fn mark_implemented(&self, req: PlanImplementedRequest) -> MineResult<PlanWorkspace> {
        let expected = req.expected_revision;
        let actor = req.actor.clone();
        self.graph.mutate(expected, &actor, move |mut w| {
            let node = w.get_mut(&req.id).ok_or_else(|| MineError::PlanNotFound {
                plan_id: req.id.clone(),
            })?;
            node.status
                .validate_transition(&req.id, PlanStatus::Implemented)?;
            node.status = PlanStatus::Implemented;
            node.implementation_report = req.report;
            node.implementation_commits = req.commits;
            node.updated_at = req.updated_at;
            w.revision = expected + 1;
            Ok(w)
        })
    }

    /// Accepts a plan: requires `IMPLEMENTED`; releases eligible BLOCKED
    /// successors whose hard predecessors are all now accepted.
    pub fn accept(&self, req: PlanAcceptRequest) -> MineResult<PlanWorkspace> {
        super::native_lifecycle::NativeLifecycle::new(self.graph).finalize_for_plan(
            req.expected_revision,
            &req.input,
            &req.id,
            &req.operation_id,
        )
    }

    /// Rejects a plan: records reason + compensating plan; downstream stays
    /// blocked.
    pub fn reject(&self, req: PlanRejectRequest) -> MineResult<PlanWorkspace> {
        let expected = req.expected_revision;
        let actor = req.actor.clone();
        let root = self.graph.repo_root().to_path_buf();
        self.graph
            .mutate_locked(expected, &actor, move |mut w, guard, _| {
                let current = w
                    .get(&req.id)
                    .cloned()
                    .ok_or_else(|| MineError::PlanNotFound {
                        plan_id: req.id.clone(),
                    })?;
                if !matches!(
                    current.status,
                    PlanStatus::InProgress | PlanStatus::Implemented
                ) {
                    return Err(MineError::InvalidTransition {
                        plan_id: req.id.clone(),
                        from: current.status.as_str().to_string(),
                        to: PlanStatus::Rejected.as_str().to_string(),
                    });
                }
                current
                    .status
                    .validate_transition(&req.id, PlanStatus::Rejected)?;
                if req.reason.trim().is_empty() {
                    return Err(MineError::GraphInvalid {
                        detail: "rejection requires a non-empty reason".to_string(),
                    });
                }
                if req.compensating_plan.is_empty() {
                    return Err(MineError::GraphInvalid {
                        detail: "rejection requires a compensating plan".to_string(),
                    });
                }
                if req.id.starts_with("plan") {
                    validate_compensation_replacement(&req.id, &req.compensating_plan)?;
                }
                let review_report = validate_review_report(&root, &current, &req.review_report)?;
                super::native_lifecycle::observe_rejection_intent(
                    &root,
                    &w,
                    &req,
                    &review_report,
                    guard,
                )?;
                let node = w.get_mut(&req.id).expect("checked present above");
                node.status = PlanStatus::Rejected;
                node.rejection_reason = req.reason;
                node.review_report = review_report;
                node.compensating_plan = req.compensating_plan;
                node.updated_at = req.updated_at;
                w.revision = expected + 1;
                Ok(w)
            })
    }
}

/// A request to release a DRAFT plan to the startable frontier (shared by CLI
/// and MCP).
#[derive(Debug, Clone)]
pub struct PlanReleaseRequest {
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    pub expected_revision: u64,
    pub id: String,
    pub updated_at: String,
}

/// A request to rewire downstream successors off a REJECTED plan onto its
/// registered compensating plan (shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanRewireRequest {
    pub actor: String,
    /// Graph revision observed by the caller, never refreshed internally.
    pub expected_revision: u64,
    pub id: String,
    pub updated_at: String,
}

/// A request to withdraw an unstarted plan into terminal `WITHDRAWN`
/// (ADR-0012; shared by CLI and MCP).
#[derive(Debug, Clone)]
pub struct PlanWithdrawRequest {
    /// Graph revision observed by the caller, never refreshed internally.
    pub expected_revision: u64,
    pub id: String,
    pub reason: String,
    pub operation_id: crate::domain::lifecycle_operation::OperationId,
    pub actor: String,
}

/// Deterministic withdrawal outcome shared by the CLI envelope and the MCP
/// `mine_plan_withdraw` tool.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanWithdrawal {
    pub id: String,
    pub status_before: String,
    pub status_after: String,
    pub withdrawn_at: String,
    pub cleaned_git_artifacts: bool,
    pub revision_before: u64,
    pub revision_after: u64,
    pub operation: crate::domain::lifecycle_operation::LifecycleOperation,
    pub replay: bool,
}

impl<'a> PlanService<'a> {
    /// Withdraws an unstarted plan (`DRAFT`/`BLOCKED`/`READY`) into terminal
    /// `WITHDRAWN` (ADR-0012). Under the store lock: reload, revision check,
    /// status gate, non-empty reason, provably-lossless Git cleanup (refusing
    /// typed on diverged/dirty/missing evidence), then one revision recording
    /// `WITHDRAWN` + reason + timestamp. No review report and no compensation
    /// are required or accepted. Returns the saved workspace and the
    /// deterministic withdrawal DTO.
    pub fn withdraw(
        &self,
        req: PlanWithdrawRequest,
    ) -> MineResult<(PlanWorkspace, PlanWithdrawal)> {
        super::native_plan_workspace::NativePlanWorkspace::new(self.graph)
            .withdraw(req, &mut crate::domain::lifecycle_operation::NoInterruption)
    }

    /// Releases a `DRAFT` plan: `DRAFT -> READY` when every hard predecessor is
    /// `ACCEPTED` (incl. no preds), else `DRAFT -> BLOCKED`. Returns the saved
    /// workspace and the unsatisfied predecessors (in stable order) for
    /// deterministic reporting.
    pub fn release(&self, req: PlanReleaseRequest) -> MineResult<PlanWorkspace> {
        let expected = req.expected_revision;
        let root = self.graph.repo_root().to_path_buf();
        self.graph.mutate(expected, &req.actor, |mut w| {
            let node = w.get(&req.id).ok_or_else(|| MineError::PlanNotFound {
                plan_id: req.id.clone(),
            })?;
            let scope = if node.planning_scope.is_empty() {
                Path::new(&w.design_root)
                    .parent()
                    .and_then(Path::to_str)
                    .ok_or_else(|| refusal("Design scope missing"))?
                    .to_string()
            } else {
                node.planning_scope.clone()
            };
            let readiness = observe_planning_readiness(&root, &scope, &req.updated_at)?;
            crate::domain::plan_release::release_plan(
                &mut w,
                &req.id,
                &req.updated_at,
                Some(&readiness),
            )?;
            w.revision = expected + 1;
            Ok(w)
        })
    }

    /// Rewires a REJECTED plan's downstream successors onto its registered
    /// compensating plan. The replacement id is derived from the rejected plan's
    /// `compensating_plan` field (the single source of truth). Returns the saved
    /// workspace and the affected successor ids (in stable insertion order); on
    /// the idempotent no-op (no successor still references the rejected id), the
    /// revision is NOT bumped and the affected list is empty.
    pub fn rewire_compensation(
        &self,
        req: PlanRewireRequest,
    ) -> MineResult<(PlanWorkspace, Vec<String>)> {
        let expected = req.expected_revision;
        let affected_cell = std::cell::RefCell::new(Vec::<String>::new());
        let saved = self.graph.mutate(expected, &req.actor, |mut w| {
            let affected =
                crate::domain::rewire::rewire_compensation(&mut w, &req.id, &req.updated_at)?;
            if !affected.is_empty() {
                w.revision = expected + 1;
            }
            *affected_cell.borrow_mut() = affected;
            Ok(w)
        })?;
        Ok((saved, affected_cell.into_inner()))
    }
}
