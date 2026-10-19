//! Common pure scheduling and admission kernel; actual locked mutation is an
//! integration responsibility, not granted by an advisory result.

mod kernel;
pub use kernel::*;

use crate::domain::error::{ContractError, ContractResult, contract_text, contract_version};
use crate::domain::graph::{PolicyIdentity, RepoPath, Sha256};
use crate::domain::ownership::{Namespace, OwnershipContract};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Occupancy {
    Executing,
    ImplementedReview,
    RejectedReservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipHold {
    pub authority: OwnershipContract,
    pub occupancy: Occupancy,
    pub owner: String,
    pub run_id: String,
    pub worktree: RepoPath,
    pub mutable_outputs: Vec<RepoPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "identity",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Resource {
    Namespace(Namespace),
    Contract(String),
    PhysicalPath(RepoPath),
    Lineage(String),
    Worktree(RepoPath),
    MutableOutput(RepoPath),
    Generator(String),
    WorkspaceCheckpoint(String),
    Plan(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockingReason {
    Ancestor,
    NamespaceOverlap,
    ContractOverlap,
    PhysicalOverlap,
    SameLineage,
    SameWorktree,
    SharedOutput,
    GeneratorTransaction,
    RejectedHandoff,
    PendingCheckpoint,
    NotReady,
    PredecessorNotAccepted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blocker {
    pub candidate: String,
    pub owner: String,
    pub resource: Resource,
    pub reason: BlockingReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulingFields {
    pub schema_version: u32,
    pub repository_id: String,
    pub workspace_id: String,
    pub revision: u64,
    pub graph_sha256: Sha256,
    pub policy: PolicyIdentity,
    pub held: Vec<OwnershipHold>,
    pub candidates: Vec<String>,
    pub eligible: Vec<String>,
    pub selected: Vec<String>,
    pub blockers: Vec<Blocker>,
    pub pending_checkpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SchedulingFields")]
pub struct SchedulingSnapshot(SchedulingFields);
impl SchedulingSnapshot {
    pub fn new(fields: SchedulingFields) -> ContractResult<Self> {
        contract_version(fields.schema_version)?;
        contract_text("repository_id", &fields.repository_id)?;
        contract_text("workspace_id", &fields.workspace_id)?;
        let candidates: HashSet<_> = fields.candidates.iter().collect();
        let eligible: HashSet<_> = fields.eligible.iter().collect();
        let selected: HashSet<_> = fields.selected.iter().collect();
        for id in &fields.candidates {
            contract_text("candidates", id)?;
        }
        if candidates.len() != fields.candidates.len()
            || eligible.len() != fields.eligible.len()
            || selected.len() != fields.selected.len()
            || !eligible.is_subset(&candidates)
            || !selected.is_subset(&eligible)
        {
            return Err(ContractError::new(
                "selected",
                "expected distinct selected subset of eligible candidate IDs",
            ));
        }
        let mut held = HashSet::new();
        for hold in &fields.held {
            contract_text("held.owner", &hold.owner)?;
            contract_text("held.run_id", &hold.run_id)?;
            let identity = hold.authority.fields().plan.fields();
            if identity.repository_id != fields.repository_id
                || identity.workspace_id != fields.workspace_id
                || hold.authority.fields().policy != fields.policy
            {
                return Err(ContractError::new(
                    "held.authority",
                    "repository, workspace or policy identity mismatch",
                ));
            }
            if !held.insert(&identity.plan_id) || selected.contains(&identity.plan_id) {
                return Err(ContractError::new(
                    "held",
                    "duplicate held owner or selected held Plan",
                ));
            }
        }
        for blocker in &fields.blockers {
            contract_text("blocker.owner", &blocker.owner)?;
            if !candidates.contains(&blocker.candidate) || selected.contains(&blocker.candidate) {
                return Err(ContractError::new(
                    "blockers",
                    "blocked Plan must be an unselected candidate",
                ));
            }
            match &blocker.resource {
                Resource::Contract(id)
                | Resource::Lineage(id)
                | Resource::Generator(id)
                | Resource::Plan(id)
                | Resource::WorkspaceCheckpoint(id) => contract_text("blocker.resource", id)?,
                _ => {}
            }
        }
        if let Some(operation) = &fields.pending_checkpoint {
            contract_text("pending_checkpoint", operation)?;
            if !fields.selected.is_empty() {
                return Err(ContractError::new(
                    "selected",
                    "pending checkpoint forbids grants",
                ));
            }
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &SchedulingFields {
        &self.0
    }
}
impl TryFrom<SchedulingFields> for SchedulingSnapshot {
    type Error = ContractError;
    fn try_from(fields: SchedulingFields) -> ContractResult<Self> {
        Self::new(fields)
    }
}
