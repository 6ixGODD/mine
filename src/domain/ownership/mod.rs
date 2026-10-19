//! Validated semantic authority and append-only, body-bound clarification.
//! IO, approval observation and graph persistence belong to the integration adapter.

mod policy;
mod resolver;
pub use policy::*;
pub use resolver::*;

use crate::domain::error::{ContractError, ContractResult, contract_text, contract_version};
use crate::domain::graph::{PlanIdentity, PolicyIdentity, RepoPath};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Case-sensitive dot-separated semantic segments, never a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Namespace(String);
impl Namespace {
    pub fn new(value: impl Into<String>) -> ContractResult<Self> {
        let value = value.into();
        if value.split('.').any(|segment| {
            segment.is_empty()
                || !segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        }) {
            return Err(ContractError::new(
                "namespace",
                "expected nonempty dot-separated ASCII name segments",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn contains(&self, other: &Self) -> bool {
        other.0 == self.0
            || other
                .0
                .strip_prefix(&self.0)
                .is_some_and(|tail| tail.starts_with('.'))
    }
}
impl TryFrom<String> for Namespace {
    type Error = ContractError;
    fn try_from(value: String) -> ContractResult<Self> {
        Self::new(value)
    }
}
impl From<Namespace> for String {
    fn from(value: Namespace) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamespaceClaim {
    pub subtree: Namespace,
    pub excluded: Vec<Namespace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationClass {
    Import,
    Export,
    Registration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedDeclaration {
    pub path: RepoPath,
    pub entry: String,
    pub class: DeclarationClass,
    pub integration_owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorTransaction {
    pub name: String,
    pub inputs: Vec<RepoPath>,
    pub machine_source: RepoPath,
    pub outputs: Vec<RepoPath>,
    /// Seals/digest refreshes are members of the complete declared output set.
    pub seals: Vec<RepoPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipFields {
    pub schema_version: u32,
    pub plan: PlanIdentity,
    pub policy: PolicyIdentity,
    pub responsibility: String,
    pub namespace_claims: Vec<NamespaceClaim>,
    pub owned_contracts: Vec<String>,
    pub excluded_responsibilities: Vec<String>,
    pub excluded_contracts: Vec<String>,
    pub physical_mapping: Vec<RepoPath>,
    pub shared_declarations: Vec<SharedDeclaration>,
    pub generators: Vec<GeneratorTransaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "OwnershipFields")]
pub struct OwnershipContract(OwnershipFields);
impl OwnershipContract {
    pub fn new(fields: OwnershipFields) -> ContractResult<Self> {
        contract_version(fields.schema_version)?;
        contract_text("responsibility", &fields.responsibility)?;
        if fields.namespace_claims.is_empty() || fields.physical_mapping.is_empty() {
            return Err(ContractError::new(
                "ownership",
                "namespace claims and physical mapping must be nonempty",
            ));
        }
        let mut claims = HashSet::new();
        for claim in &fields.namespace_claims {
            if !claims.insert(&claim.subtree) {
                return Err(ContractError::new("namespace_claims", "duplicate subtree"));
            }
            let mut exclusions = HashSet::new();
            for excluded in &claim.excluded {
                if excluded == &claim.subtree
                    || !claim.subtree.contains(excluded)
                    || !exclusions.insert(excluded)
                {
                    return Err(ContractError::new(
                        "namespace_claims.excluded",
                        "expected distinct strict descendant subtrees",
                    ));
                }
            }
        }
        for (name, values) in [
            ("owned_contracts", &fields.owned_contracts),
            ("excluded_contracts", &fields.excluded_contracts),
            (
                "excluded_responsibilities",
                &fields.excluded_responsibilities,
            ),
        ] {
            let mut unique = HashSet::new();
            for value in values {
                contract_text(name, value)?;
                if !unique.insert(value) {
                    return Err(ContractError::new(name, "duplicate declaration"));
                }
            }
        }
        if fields
            .excluded_responsibilities
            .contains(&fields.responsibility)
        {
            return Err(ContractError::new(
                "responsibility",
                "responsibility is explicitly excluded",
            ));
        }
        for claim in &fields.namespace_claims {
            for other in &fields.namespace_claims {
                if claim.subtree != other.subtree && claim.subtree.contains(&other.subtree) {
                    return Err(ContractError::new(
                        "namespace_claims",
                        "overlapping claims or excluded subtree reclamation",
                    ));
                }
                if claim
                    .excluded
                    .iter()
                    .any(|excluded| excluded.contains(&other.subtree))
                {
                    return Err(ContractError::new(
                        "namespace_claims",
                        "an excluded subtree cannot be reclaimed",
                    ));
                }
            }
        }
        distinct_paths("physical_mapping", &fields.physical_mapping)?;
        if fields
            .owned_contracts
            .iter()
            .any(|owned| fields.excluded_contracts.contains(owned))
        {
            return Err(ContractError::new(
                "owned_contracts",
                "a contract cannot be both owned and excluded",
            ));
        }
        let mut entries = HashSet::new();
        for declaration in &fields.shared_declarations {
            contract_text("shared_declarations.entry", &declaration.entry)?;
            contract_text(
                "shared_declarations.integration_owner",
                &declaration.integration_owner,
            )?;
            if !entries.insert((&declaration.path, &declaration.entry)) {
                return Err(ContractError::new(
                    "shared_declarations",
                    "duplicate shared entry",
                ));
            }
        }
        let mut generators = HashSet::new();
        for generator in &fields.generators {
            contract_text("generators.name", &generator.name)?;
            if !generators.insert(&generator.name)
                || generator.inputs.is_empty()
                || generator.outputs.is_empty()
            {
                return Err(ContractError::new(
                    "generators",
                    "expected distinct names and nonempty input/output closure",
                ));
            }
            distinct_paths("generators.inputs", &generator.inputs)?;
            distinct_paths("generators.seals", &generator.seals)?;
            if !generator.inputs.contains(&generator.machine_source) {
                return Err(ContractError::new(
                    "generators.machine_source",
                    "machine source must belong to the input closure",
                ));
            }
            let outputs: HashSet<_> = generator.outputs.iter().collect();
            if outputs.len() != generator.outputs.len()
                || generator.seals.iter().any(|seal| !outputs.contains(seal))
            {
                return Err(ContractError::new(
                    "generators.outputs",
                    "outputs must be distinct and include all seals",
                ));
            }
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &OwnershipFields {
        &self.0
    }
}

pub(crate) fn distinct_paths(field: &'static str, paths: &[RepoPath]) -> ContractResult<()> {
    if paths.iter().collect::<HashSet<_>>().len() != paths.len() {
        return Err(ContractError::new(field, "duplicate physical path"));
    }
    Ok(())
}

/// Lexical boundary only; filesystem containment must be rechecked by IO owners.
pub fn path_contains(prefix: &RepoPath, path: &RepoPath) -> bool {
    crate::domain::path::is_within(prefix.as_str(), path.as_str())
}
impl TryFrom<OwnershipFields> for OwnershipContract {
    type Error = ContractError;
    fn try_from(fields: OwnershipFields) -> ContractResult<Self> {
        Self::new(fields)
    }
}
