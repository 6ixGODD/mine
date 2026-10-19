//! The `PlanWorkspace` aggregate root and `PlanNode` entity.
//!
//! Implements `docs/design/execution-graph/domain-model.md`. The TOML
//! serialization model matches the live fact source
//! `docs/plan/execution-graph.toml` byte-for-byte (flat string arrays for
//! design references and path ownership), so the persistence layer can
//! round-trip the existing graph without drift.

use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::domain::design_reference::{DesignReference, from_flat_paths};
use crate::domain::error::{MineError, MineResult};
use crate::domain::path::{is_within, normalize_repo_relative};
use crate::domain::status::PlanStatus;

/// Native ownership (v5), acceptance (v6), and durable preparation (v7).
/// Earlier readers refuse this marker rather than discard unknown authority.
pub const CURRENT_GRAPH_SCHEMA_VERSION: u32 = 7;

/// The immediately preceding wire schema: accepted for reading and upgraded
/// to the current schema by the next lifecycle mutation (no field derivation
/// is needed — the new fields are optional with defaults).
pub const PREVIOUS_GRAPH_SCHEMA_VERSION: u32 = 6;

/// Readers retain all supported historical facts; a read never invents evidence.
pub fn is_supported_graph_schema(version: u32) -> bool {
    (3..=CURRENT_GRAPH_SCHEMA_VERSION).contains(&version)
}

/// The fail-closed provenance schema upgraded by Plan branch-anchor repair.
pub const ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION: u32 = 2;

/// The numeric bootstrap schema accepted only by the provenance repair path.
pub const LEGACY_GRAPH_SCHEMA_VERSION: u32 = 1;

/// Full SHA-256 identity, not proof that any particular producer was trusted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256(String);

impl Sha256 {
    pub fn new(value: impl Into<String>) -> crate::domain::error::ContractResult<Self> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(crate::domain::error::ContractError::new(
                "sha256",
                "expected 64 lowercase hexadecimal digits",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for Sha256 {
    type Error = crate::domain::error::ContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<Sha256> for String {
    fn from(value: Sha256) -> Self {
        value.0
    }
}

/// Normalized, lexical repository-relative path. IO consumers must additionally
/// check real filesystem containment (including links) immediately before IO.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoPath(String);
impl RepoPath {
    pub fn new(value: impl Into<String>) -> crate::domain::error::ContractResult<Self> {
        let value = value.into();
        let normalized = normalize_repo_relative(&value).map_err(|_| {
            crate::domain::error::ContractError::new(
                "path",
                "expected safe repository-relative path",
            )
        })?;
        if value != normalized || value.chars().any(char::is_control) || value.contains(':') {
            return Err(crate::domain::error::ContractError::new(
                "path",
                "expected canonical forward-slash relative path",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for RepoPath {
    type Error = crate::domain::error::ContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<RepoPath> for String {
    fn from(value: RepoPath) -> Self {
        value.0
    }
}

/// Declared file evidence. Only the IO owner verifies these bytes on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    pub path: RepoPath,
    pub bytes: u64,
    pub sha256: Sha256,
}

/// Exact native-sized OID syntax. Only fresh Git evidence proves object/type
/// and repository-native width; this value alone never grants authority.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommitId(String);
impl CommitId {
    pub fn new(value: impl Into<String>) -> crate::domain::error::ContractResult<Self> {
        let value = value.into();
        if !matches!(value.len(), 40 | 64)
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(crate::domain::error::ContractError::new(
                "commit_id",
                "expected full lowercase SHA-1 or SHA-256 object ID",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for CommitId {
    type Error = crate::domain::error::ContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<CommitId> for String {
    fn from(value: CommitId) -> Self {
        value.0
    }
}

/// RFC3339 timestamp with an explicit UTC offset; preserves observed text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UtcTimestamp(String);
impl UtcTimestamp {
    pub fn new(value: impl Into<String>) -> crate::domain::error::ContractResult<Self> {
        let value = value.into();
        let parsed =
            time::OffsetDateTime::parse(&value, &time::format_description::well_known::Rfc3339)
                .map_err(|_| {
                    crate::domain::error::ContractError::new(
                        "timestamp",
                        "expected RFC3339 UTC timestamp",
                    )
                })?;
        if parsed.offset() != time::UtcOffset::UTC || value.ends_with("-00:00") {
            return Err(crate::domain::error::ContractError::new(
                "timestamp",
                "timestamp must use UTC",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn instant(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::parse(&self.0, &time::format_description::well_known::Rfc3339)
            .expect("validated timestamp")
    }
}
impl TryFrom<String> for UtcTimestamp {
    type Error = crate::domain::error::ContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<UtcTimestamp> for String {
    fn from(value: UtcTimestamp) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanIdentityFields {
    pub repository_id: String,
    pub workspace_id: String,
    pub plan_id: String,
    pub plan_path: RepoPath,
    /// None is explicitly absent historical evidence, never an inferred digest.
    pub body_sha256: Option<Sha256>,
    pub lineage_id: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PlanIdentityFields")]
pub struct PlanIdentity(PlanIdentityFields);
impl PlanIdentity {
    pub fn new(fields: PlanIdentityFields) -> crate::domain::error::ContractResult<Self> {
        use crate::domain::error::{ContractError, contract_text};
        contract_text("repository_id", &fields.repository_id)?;
        contract_text("workspace_id", &fields.workspace_id)?;
        let branch = crate::domain::plan_id::registered_plan_branch(
            &fields.workspace_id,
            &fields.plan_id,
            fields.plan_path.as_str(),
        )
        .map_err(|_| ContractError::new("plan_identity", "invalid exact Plan ID/path binding"))?;
        let lineage = if crate::domain::plan_id::is_exact_bootstrap_node(
            &fields.workspace_id,
            &fields.plan_id,
            fields.plan_path.as_str(),
        ) {
            // mine-release-allow-plan-reference: canonical bootstrap identity
            "plan01".to_string()
        } else {
            crate::domain::plan_id::CanonicalPlanId::parse(&fields.plan_id)
                .map_err(|_| ContractError::new("plan_id", "invalid canonical Plan ID"))?
                .lineage_id()
        };
        if branch != fields.branch || lineage != fields.lineage_id {
            return Err(ContractError::new(
                "plan_identity",
                "branch or lineage does not match exact Plan identity",
            ));
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &PlanIdentityFields {
        &self.0
    }
    /// Relative evidence namespace derived from this exact frozen Plan path.
    /// IO admission still verifies that path against the configured layout.
    pub fn verification_directory(&self) -> RepoPath {
        let (root, _) = self
            .0
            .plan_path
            .as_str()
            .rsplit_once('/')
            .expect("validated Plan parent");
        RepoPath::new(format!("{root}/reports/verification/{}", self.0.plan_id))
            .expect("derived from validated Plan identity")
    }
}
impl TryFrom<PlanIdentityFields> for PlanIdentity {
    type Error = crate::domain::error::ContractError;
    fn try_from(fields: PlanIdentityFields) -> Result<Self, Self::Error> {
        Self::new(fields)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyIdentityFields {
    pub design_references: Vec<RepoPath>,
    pub sha256: Sha256,
}

/// Identity of a policy; approval and actual digest matching remain a resolver's
/// responsibility, not a flag callers can set on this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PolicyIdentityFields")]
pub struct PolicyIdentity(PolicyIdentityFields);
impl PolicyIdentity {
    pub fn new(fields: PolicyIdentityFields) -> crate::domain::error::ContractResult<Self> {
        let unique: HashSet<_> = fields.design_references.iter().collect();
        if unique.is_empty() || unique.len() != fields.design_references.len() {
            return Err(crate::domain::error::ContractError::new(
                "policy.design_references",
                "expected nonempty distinct Design references",
            ));
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &PolicyIdentityFields {
        &self.0
    }
}
impl TryFrom<PolicyIdentityFields> for PolicyIdentity {
    type Error = crate::domain::error::ContractError;
    fn try_from(fields: PolicyIdentityFields) -> Result<Self, Self::Error> {
        Self::new(fields)
    }
}

/// The execution-graph aggregate root for an active plan workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanWorkspace {
    /// Graph schema version. Version 2 is deliberately encoded as a string so
    /// schema-v1 binaries, whose field type was `u32`, fail during parsing
    /// before they can rewrite and discard fields they do not understand.
    #[serde(
        serialize_with = "serialize_graph_schema_version",
        deserialize_with = "deserialize_graph_schema_version"
    )]
    pub schema_version: u32,
    /// Monotonically increasing revision; incremented once per successful
    /// mutation.
    pub revision: u64,
    /// Stable project identifier.
    pub project_id: String,
    /// Generated internal workspace identifier (not a product version).
    pub workspace_id: String,
    /// Stable branch detected by `mine init`.
    pub stable_branch: String,
    /// Managed integration branch.
    pub integration_branch: String,
    /// Stable baseline commit this workspace was opened from.
    pub stable_baseline_commit: String,
    /// Repository-relative design root index path.
    pub design_root: String,
    /// Whether the workspace is ephemeral.
    pub ephemeral_workspace: bool,
    /// Whether the plan workspace must be purged before stable release.
    pub purge_before_stable_release: bool,
    /// Plan nodes, in stable insertion order.
    #[serde(default)]
    pub plans: Vec<PlanNode>,
}

/// A plan node within the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanNode {
    /// Stable plan identifier within the active workspace.
    pub id: String,
    /// Plan Markdown path under `docs/plan/`.
    pub path: String,
    /// Human-readable title.
    pub title: String,
    /// Registered managed branch derived from the canonical id and plan slug.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branch: String,
    /// Immutable creation commit captured atomically when execution starts.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branch_anchor: String,
    /// Lifecycle status.
    pub status: PlanStatus,
    /// Hard predecessors that must be `ACCEPTED` before this plan can start.
    #[serde(default)]
    pub hard_predecessors: Vec<String>,
    /// Soft predecessors that do not block readiness.
    #[serde(default)]
    pub soft_predecessors: Vec<String>,
    /// Design document paths governing this plan (flat TOML form).
    #[serde(default)]
    pub design_references: Vec<String>,
    /// Exclusive write paths owned by this plan.
    #[serde(default)]
    pub exclusive_write_paths: Vec<String>,
    /// Read-only context paths.
    #[serde(default)]
    pub read_only_paths: Vec<String>,
    /// Reserved shared paths requiring serialized access.
    #[serde(default)]
    pub reserved_shared_paths: Vec<String>,
    /// Implementation report path.
    #[serde(default)]
    pub implementation_report: String,
    /// Independent review report path.
    #[serde(default)]
    pub review_report: String,
    /// Implementation commit hashes recorded as evidence.
    #[serde(default)]
    pub implementation_commits: Vec<String>,
    /// Owner assigned on start.
    #[serde(default)]
    pub owner: String,
    /// Run identifier assigned on start.
    #[serde(default)]
    pub run_id: String,
    /// Timestamp the plan was started.
    #[serde(default)]
    pub started_at: String,
    /// Timestamp of the last mutation.
    #[serde(default)]
    pub updated_at: String,
    /// Reviewer rejection reason.
    #[serde(default)]
    pub rejection_reason: String,
    /// Compensation plan registered on rejection.
    #[serde(default)]
    pub compensating_plan: String,
    /// Withdrawal reason recorded by `mine plan withdraw` (ADR-0012); empty
    /// unless the node is `WITHDRAWN`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub withdraw_reason: String,
    /// UTC timestamp of withdrawal; empty unless the node is `WITHDRAWN`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub withdrawn_at: String,
    /// Non-empty planning scope recorded at registration (ADR-0013), defaulting
    /// to the configured design root directory; immutable afterwards. Empty
    /// only on legacy pre-v4 nodes, which read with design-root semantics.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub planning_scope: String,
    /// UTC timestamp of the release-time planning-readiness re-check
    /// (ADR-0013); empty before the node is released.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub readiness_checked_at: String,
    /// ADR registry revision that satisfied the node's planning scope at
    /// release; 0 for legacy nodes and registries without bootstrap.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub adr_registry_revision: u64,
    /// Derived canonical base lineage identity.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lineage_id: String,
    /// Repository-relative lineage worktree locator.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub worktree_path: String,
    /// Registration-time immutable identity; None is absent historical evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_sha256: Option<Sha256>,
    /// Immutable semantic baseline. Explicit legacy activation retains the
    /// absent historical body and original conservative mapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership: Option<crate::domain::ownership::OwnershipContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership_policy: Option<OwnershipPolicyProvenance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clarifications: Vec<crate::domain::ownership::ClarificationRecord>,
    /// Portable terminal evidence, separate from the recoverable local mirror.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<crate::domain::lifecycle_operation::LifecycleOperation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preparations: Vec<PreparationGeneration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_preparation_generation: Option<u64>,
}

/// An explicit preparation request mode; not a new Plan lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationMode {
    Initial,
    Reprepare,
    Recover,
}

/// Append-only completed preparation. Absolute machine paths are not authority.
/// Root and branch commits are distinct, especially for compensation lineages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationGeneration {
    pub ordinal: u64,
    pub operation_id: crate::domain::lifecycle_operation::OperationId,
    pub actor: String,
    pub prepared_at: UtcTimestamp,
    pub plan: PlanIdentity,
    pub policy_sha256: Sha256,
    pub worktree: RepoPath,
    pub prepared_branch_commit: CommitId,
    pub observed_root_commit: CommitId,
    pub observed_graph_revision: u64,
}

/// Exact approved policy observation. ADR and graph revisions are independent.
/// Decoding this record is not approval: adapters re-observe authoritative ADR,
/// body, policy bytes and Root ownership before using it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipPolicyProvenance {
    pub repository_id: String,
    pub workspace_id: String,
    pub policy_path: RepoPath,
    pub policy_sha256: Sha256,
    pub adr_id: String,
    pub adr_path: RepoPath,
    pub adr_body_sha256: Sha256,
    pub adr_registry_revision: u64,
    pub approval_actor: String,
    pub admitted_graph_revision: u64,
}

fn serialize_graph_schema_version<S>(version: &u32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if *version == LEGACY_GRAPH_SCHEMA_VERSION {
        serializer.serialize_u32(*version)
    } else {
        serializer.serialize_str(&format!("mine-graph-v{version}"))
    }
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

fn deserialize_graph_schema_version<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WireVersion {
        Legacy(u32),
        Versioned(String),
    }

    match WireVersion::deserialize(deserializer)? {
        WireVersion::Legacy(LEGACY_GRAPH_SCHEMA_VERSION) => Ok(LEGACY_GRAPH_SCHEMA_VERSION),
        WireVersion::Legacy(version) => Err(serde::de::Error::custom(format!(
            "numeric graph schema marker is valid only for legacy v1, got {version}"
        ))),
        WireVersion::Versioned(value) => value
            .strip_prefix("mine-graph-v")
            .ok_or_else(|| serde::de::Error::custom("invalid graph schema marker"))?
            .parse::<u32>()
            .map_err(serde::de::Error::custom),
    }
}

impl PlanNode {
    /// Returns the structured design references (parsed and validated).
    ///
    /// # Errors
    /// Returns [`MineError::GraphInvalid`] if any design path is unsafe or the
    /// list is empty.
    pub fn design_references_structured(&self) -> MineResult<Vec<DesignReference>> {
        from_flat_paths(&self.design_references)
    }

    /// Returns the write scope of this plan: exclusive write paths plus
    /// reserved shared paths. All entries are normalized.
    ///
    /// # Errors
    /// Returns [`MineError::GraphInvalid`] if any owned path is unsafe.
    pub fn write_scope(&self) -> MineResult<Vec<String>> {
        let mut scope = Vec::new();
        for p in &self.exclusive_write_paths {
            scope.push(normalize_repo_relative(p)?);
        }
        for p in &self.reserved_shared_paths {
            scope.push(normalize_repo_relative(p)?);
        }
        Ok(scope)
    }

    /// Returns `true` if this plan's write scope overlaps `other`'s write scope.
    ///
    /// Two scopes overlap when any path in one is contained by any path in the
    /// other (prefix match) in either direction.
    pub fn write_scope_overlaps(&self, other: &PlanNode) -> MineResult<bool> {
        let a = self.write_scope()?;
        let b = other.write_scope()?;
        for pa in &a {
            for pb in &b {
                if is_within(pa, pb) || is_within(pb, pa) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Returns `true` if this plan has a hard-dependency ancestor relationship
    /// with `other` (one is reachable from the other through hard
    /// predecessors).
    pub fn has_ancestor_relationship(
        &self,
        other_id: &str,
        workspace: &PlanWorkspace,
    ) -> MineResult<bool> {
        Ok(workspace.is_hard_ancestor(&self.id, other_id)?
            || workspace.is_hard_ancestor(other_id, &self.id)?)
    }
}

impl PlanWorkspace {
    /// Looks up a plan node by id.
    #[must_use]
    pub fn get(&self, plan_id: &str) -> Option<&PlanNode> {
        self.plans.iter().find(|p| p.id == plan_id)
    }

    /// Looks up a mutable plan node by id.
    pub fn get_mut(&mut self, plan_id: &str) -> Option<&mut PlanNode> {
        self.plans.iter_mut().find(|p| p.id == plan_id)
    }

    /// Returns the set of plan IDs.
    #[must_use]
    pub fn ids(&self) -> HashSet<&str> {
        self.plans.iter().map(|p| p.id.as_str()).collect()
    }

    /// Returns `true` if `ancestor_id` is a hard-predecessor ancestor of
    /// `descendant_id`.
    ///
    /// # Errors
    /// Returns [`MineError::PlanNotFound`] if either id is unknown.
    pub fn is_hard_ancestor(&self, ancestor_id: &str, descendant_id: &str) -> MineResult<bool> {
        GraphIndex::new(self)?.is_hard_ancestor(ancestor_id, descendant_id)
    }
}

/// Declaration-ranked adjacency/index membership, built once per algorithm.
/// This is derived data, never serialized into the authoritative graph.
pub struct GraphIndex {
    ids: Vec<String>,
    positions: std::collections::HashMap<String, usize>,
    predecessors: Vec<Vec<usize>>,
    children: Vec<Vec<usize>>,
    ancestry: std::cell::RefCell<std::collections::HashMap<(usize, usize), bool>>,
}

impl GraphIndex {
    pub fn new(workspace: &PlanWorkspace) -> MineResult<Self> {
        let ids: Vec<_> = workspace.plans.iter().map(|node| node.id.clone()).collect();
        let positions: std::collections::HashMap<_, _> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        if positions.len() != ids.len() {
            return Err(MineError::GraphInvalid {
                detail: "duplicate plan id".to_string(),
            });
        }
        let mut predecessors = vec![Vec::new(); ids.len()];
        let mut children = vec![Vec::new(); ids.len()];
        for (child, node) in workspace.plans.iter().enumerate() {
            for predecessor in &node.hard_predecessors {
                let parent =
                    *positions
                        .get(predecessor)
                        .ok_or_else(|| MineError::PlanNotFound {
                            plan_id: predecessor.clone(),
                        })?;
                predecessors[child].push(parent);
                children[parent].push(child);
            }
        }
        Ok(Self {
            ids,
            positions,
            predecessors,
            children,
            ancestry: std::cell::RefCell::new(std::collections::HashMap::new()),
        })
    }

    pub fn position(&self, id: &str) -> MineResult<usize> {
        self.positions
            .get(id)
            .copied()
            .ok_or_else(|| MineError::PlanNotFound {
                plan_id: id.to_string(),
            })
    }

    pub fn topological_sort(&self) -> MineResult<Vec<String>> {
        let mut degrees: Vec<_> = self.predecessors.iter().map(Vec::len).collect();
        let mut queue: std::collections::VecDeque<_> = degrees
            .iter()
            .enumerate()
            .filter_map(|(i, d)| (*d == 0).then_some(i))
            .collect();
        let mut emitted = vec![false; degrees.len()];
        let mut order = Vec::with_capacity(degrees.len());
        while let Some(parent) = queue.pop_front() {
            emitted[parent] = true;
            order.push(self.ids[parent].clone());
            // Children were inserted in declaration order, matching the old
            // FIFO/newly-zero semantics without scanning the entire graph.
            for child in &self.children[parent] {
                degrees[*child] -= 1;
                if degrees[*child] == 0 {
                    queue.push_back(*child);
                }
            }
        }
        if order.len() != self.ids.len() {
            return Err(MineError::GraphCycle {
                cycle: self
                    .ids
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !emitted[*i])
                    .map(|(_, id)| id.as_str())
                    .collect::<Vec<_>>()
                    .join(" -> "),
            });
        }
        Ok(order)
    }

    pub fn is_hard_ancestor(&self, ancestor: &str, descendant: &str) -> MineResult<bool> {
        let pair = (self.position(ancestor)?, self.position(descendant)?);
        if let Some(found) = self.ancestry.borrow().get(&pair) {
            return Ok(*found);
        }
        let mut visited = vec![false; self.ids.len()];
        let mut stack = vec![pair.1];
        let mut found = false;
        while let Some(node) = stack.pop() {
            if visited[node] {
                continue;
            }
            visited[node] = true;
            for predecessor in &self.predecessors[node] {
                if *predecessor == pair.0 {
                    found = true;
                    break;
                }
                stack.push(*predecessor);
            }
            if found {
                break;
            }
        }
        self.ancestry.borrow_mut().insert(pair, found);
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::MineError;

    fn node(id: &str, hard: &[&str]) -> PlanNode {
        PlanNode {
            id: id.to_string(),
            path: format!("docs/plan/{id}.md"),
            title: id.to_string(),
            branch: format!("plan/{id}"),
            branch_anchor: String::new(),
            status: PlanStatus::Blocked,
            hard_predecessors: hard.iter().map(|s| s.to_string()).collect(),
            soft_predecessors: vec![],
            design_references: vec!["docs/design/principles.md".to_string()],
            exclusive_write_paths: vec![format!("src/{id}/")],
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
            lineage_id: String::new(),
            worktree_path: String::new(),
            body_sha256: None,
            ownership: None,
            ownership_policy: None,
            clarifications: vec![],
            acceptance: None,
            preparations: vec![],
            started_preparation_generation: None,
        }
    }

    #[test]
    fn write_scope_overlap_within_prefix() -> Result<(), MineError> {
        let a = node("01", &[]);
        let mut b = node("02", &["01"]);
        b.exclusive_write_paths = vec!["src/01/main.rs".to_string()];
        assert!(a.write_scope_overlaps(&b)?);
        Ok(())
    }

    #[test]
    fn write_scope_no_overlap_disjoint() -> Result<(), MineError> {
        let mut a = node("01", &[]);
        a.exclusive_write_paths = vec!["src/domain/".to_string()];
        let mut b = node("02", &["01"]);
        b.exclusive_write_paths = vec!["tests/".to_string()];
        assert!(!a.write_scope_overlaps(&b)?);
        Ok(())
    }

    #[test]
    fn reserved_shared_path_overlaps() -> Result<(), MineError> {
        let mut a = node("01", &[]);
        a.exclusive_write_paths = vec![];
        a.reserved_shared_paths = vec!["docs/plan/execution-graph.toml".to_string()];
        let mut b = node("02", &["01"]);
        b.exclusive_write_paths = vec![];
        b.reserved_shared_paths = vec!["docs/plan/execution-graph.toml".to_string()];
        assert!(a.write_scope_overlaps(&b)?);
        Ok(())
    }

    #[test]
    fn ancestor_walks_hard_predecessors() -> Result<(), MineError> {
        let ws = PlanWorkspace {
            schema_version: CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 0,
            project_id: "p".to_string(),
            workspace_id: "w".to_string(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: String::new(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![node("01", &[]), node("02", &["01"]), node("03", &["02"])],
        };
        assert!(ws.is_hard_ancestor("01", "03")?);
        assert!(!ws.is_hard_ancestor("03", "01")?);
        Ok(())
    }
}
