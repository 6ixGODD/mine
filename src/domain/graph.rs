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

/// The only graph schema accepted by ordinary reads and lifecycle writes.
pub const CURRENT_GRAPH_SCHEMA_VERSION: u32 = 3;

/// The fail-closed provenance schema upgraded by Plan branch-anchor repair.
pub const PREVIOUS_GRAPH_SCHEMA_VERSION: u32 = 2;

/// The numeric bootstrap schema accepted only by the provenance repair path.
pub const LEGACY_GRAPH_SCHEMA_VERSION: u32 = 1;

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
    /// Derived canonical base lineage identity.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lineage_id: String,
    /// Repository-relative lineage worktree locator.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub worktree_path: String,
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
        if self.get(ancestor_id).is_none() {
            return Err(MineError::PlanNotFound {
                plan_id: ancestor_id.to_string(),
            });
        }
        let mut visited = HashSet::new();
        let mut stack = vec![descendant_id];
        while let Some(cur) = stack.pop() {
            if !visited.insert(cur) {
                continue;
            }
            let node = self.get(cur).ok_or_else(|| MineError::PlanNotFound {
                plan_id: cur.to_string(),
            })?;
            for pred in &node.hard_predecessors {
                if pred == ancestor_id {
                    return Ok(true);
                }
                stack.push(pred);
            }
        }
        Ok(false)
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
            lineage_id: String::new(),
            worktree_path: String::new(),
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
