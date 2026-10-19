use super::*;
use crate::domain::graph::Sha256;
use sha2::{Digest, Sha256 as Hasher};

/// Approval facts supplied by the authoritative ADR/Design adapter, not inferred
/// from policy content. The pure kernel cannot authenticate an external actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyApproval {
    pub identity: PolicyIdentity,
    pub actor: String,
    pub observed_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsibilityMapping {
    pub responsibility: String,
    pub namespaces: Vec<NamespaceClaim>,
    pub contracts: Vec<String>,
    pub physical_prefixes: Vec<RepoPath>,
    pub shared_entries: Vec<SharedEntryKey>,
    pub generator_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedEntryKey {
    pub path: RepoPath,
    pub entry: String,
}

/// Exact declared directory overlap; it does not authorize arbitrary shared
/// file edits below that directory. Such edits need SharedDeclaration intents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryPartition {
    pub directory: RepoPath,
    pub responsibilities: Vec<String>,
    pub integration_owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipPolicyDocument {
    pub schema_version: u32,
    pub clarification_actors: Vec<String>,
    pub mappings: Vec<ResponsibilityMapping>,
    pub partitions: Vec<DirectoryPartition>,
    pub shared_declarations: Vec<SharedDeclaration>,
    pub generators: Vec<GeneratorTransaction>,
}

/// Not deserializable as already-approved authority. Construction compares
/// actual policy bytes with externally observed approval and validates all rules.
#[derive(Debug, Clone)]
pub struct ApprovedOwnershipPolicy {
    identity: PolicyIdentity,
    document: OwnershipPolicyDocument,
    approved_revision: u64,
}
impl ApprovedOwnershipPolicy {
    pub fn verify(bytes: &[u8], approval: &PolicyApproval, revision: u64) -> ContractResult<Self> {
        contract_text("policy.approval.actor", &approval.actor)?;
        if approval.observed_revision > revision
            || hash_bytes(bytes) != approval.identity.fields().sha256
        {
            return Err(ContractError::new(
                "policy.approval",
                "unapproved, future or mismatched policy identity",
            ));
        }
        let document: OwnershipPolicyDocument = serde_json::from_slice(bytes).map_err(|_| {
            ContractError::new("policy", "malformed or unsupported policy document")
        })?;
        contract_version(document.schema_version)?;
        text_set("policy.actors", &document.clarification_actors)?;
        if document.clarification_actors.is_empty() || document.mappings.is_empty() {
            return Err(ContractError::new(
                "policy",
                "actors and responsibility mappings are required",
            ));
        }
        let mut responsibilities = HashSet::new();
        for mapping in &document.mappings {
            contract_text("policy.responsibility", &mapping.responsibility)?;
            if !responsibilities.insert(mapping.responsibility.as_str())
                || mapping.namespaces.is_empty()
                || mapping.physical_prefixes.is_empty()
            {
                return Err(ContractError::new(
                    "policy.mappings",
                    "duplicate or incomplete responsibility mapping",
                ));
            }
            validate_claims(&mapping.namespaces)?;
            text_set("policy.contracts", &mapping.contracts)?;
            text_set("policy.generator_names", &mapping.generator_names)?;
            distinct_paths("policy.physical_prefixes", &mapping.physical_prefixes)?;
        }
        let mut partitions = HashSet::new();
        for partition in &document.partitions {
            text_set(
                "policy.partition.responsibilities",
                &partition.responsibilities,
            )?;
            contract_text(
                "policy.partition.integration_owner",
                &partition.integration_owner,
            )?;
            if partition.responsibilities.len() < 2
                || !partitions.insert(&partition.directory)
                || partition
                    .responsibilities
                    .iter()
                    .any(|r| !responsibilities.contains(r.as_str()))
            {
                return Err(ContractError::new(
                    "policy.partitions",
                    "unknown responsibility, duplicate or incomplete partition",
                ));
            }
            for responsibility in &partition.responsibilities {
                let mapping = document
                    .mappings
                    .iter()
                    .find(|m| &m.responsibility == responsibility)
                    .expect("validated mapping");
                if !mapping
                    .physical_prefixes
                    .iter()
                    .any(|p| path_contains(p, &partition.directory))
                {
                    return Err(ContractError::new(
                        "policy.partitions",
                        "partition is outside responsibility mapping",
                    ));
                }
            }
        }
        let mut entries = HashSet::new();
        for entry in &document.shared_declarations {
            contract_text("policy.shared.entry", &entry.entry)?;
            contract_text("policy.shared.integration_owner", &entry.integration_owner)?;
            if !entries.insert((&entry.path, &entry.entry)) {
                return Err(ContractError::new(
                    "policy.shared",
                    "duplicate shared entry",
                ));
            }
            if document.shared_declarations.iter().any(|other| {
                other.path == entry.path && other.integration_owner != entry.integration_owner
            }) {
                return Err(ContractError::new(
                    "policy.shared",
                    "one shared path requires one integration owner",
                ));
            }
        }
        let mut generators = HashSet::new();
        for generator in &document.generators {
            validate_generator(generator)?;
            if !generators.insert(generator.name.as_str()) {
                return Err(ContractError::new(
                    "policy.generators",
                    "duplicate generator",
                ));
            }
        }
        if document
            .mappings
            .iter()
            .flat_map(|m| &m.generator_names)
            .any(|g| !generators.contains(g.as_str()))
        {
            return Err(ContractError::new(
                "policy.generators",
                "mapping names an undeclared generator",
            ));
        }
        for mapping in &document.mappings {
            if mapping.shared_entries.iter().collect::<HashSet<_>>().len()
                != mapping.shared_entries.len()
                || mapping.shared_entries.iter().any(|key| {
                    !document
                        .shared_declarations
                        .iter()
                        .any(|d| d.path == key.path && d.entry == key.entry)
                })
            {
                return Err(ContractError::new(
                    "policy.shared",
                    "mapping names a duplicate or undeclared shared entry",
                ));
            }
        }
        Ok(Self {
            identity: approval.identity.clone(),
            document,
            approved_revision: approval.observed_revision,
        })
    }
    pub fn identity(&self) -> &PolicyIdentity {
        &self.identity
    }
    pub fn document(&self) -> &OwnershipPolicyDocument {
        &self.document
    }
    pub fn approved_revision(&self) -> u64 {
        self.approved_revision
    }

    pub(crate) fn validate_scope(&self, fields: &OwnershipFields) -> ContractResult<()> {
        if fields.policy != self.identity {
            return Err(ContractError::new(
                "ownership.policy",
                "policy identity mismatch",
            ));
        }
        let mapping = self
            .document
            .mappings
            .iter()
            .find(|m| m.responsibility == fields.responsibility)
            .ok_or_else(|| {
                ContractError::new(
                    "ownership.responsibility",
                    "responsibility is not mapped by approved policy",
                )
            })?;
        for claim in &fields.namespace_claims {
            if !mapping
                .namespaces
                .iter()
                .any(|allowed| claim_subset(claim, allowed))
            {
                return Err(ContractError::new(
                    "ownership.namespaces",
                    "claim exceeds approved namespace mapping",
                ));
            }
        }
        if fields
            .owned_contracts
            .iter()
            .any(|c| !mapping.contracts.contains(c))
        {
            return Err(ContractError::new(
                "ownership.contracts",
                "contract exceeds approved responsibility",
            ));
        }
        if fields.physical_mapping.iter().any(|p| {
            !mapping
                .physical_prefixes
                .iter()
                .any(|prefix| path_contains(prefix, p))
        }) {
            return Err(ContractError::new(
                "ownership.physical_mapping",
                "physical mapping exceeds approved responsibility",
            ));
        }
        if fields.shared_declarations.iter().any(|d| {
            !self.document.shared_declarations.contains(d)
                || !mapping
                    .shared_entries
                    .iter()
                    .any(|key| key.path == d.path && key.entry == d.entry)
        }) {
            return Err(ContractError::new(
                "ownership.shared",
                "undeclared shared entry, class or integration owner",
            ));
        }
        for generator in &fields.generators {
            if !mapping.generator_names.contains(&generator.name)
                || !self.document.generators.contains(generator)
            {
                return Err(ContractError::new(
                    "ownership.generators",
                    "generator transaction is not the complete approved closure",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn allows_directory_overlap(
        &self,
        left: &OwnershipFields,
        right: &OwnershipFields,
        a: &RepoPath,
        b: &RepoPath,
    ) -> bool {
        self.document.partitions.iter().any(|p| {
            &p.directory == a
                && a == b
                && p.responsibilities.contains(&left.responsibility)
                && p.responsibilities.contains(&right.responsibility)
                && left.responsibility != right.responsibility
        })
    }
}

pub fn hash_bytes(bytes: &[u8]) -> Sha256 {
    Sha256::new(format!("{:x}", Hasher::digest(bytes))).expect("SHA-256 encoding")
}

pub(crate) fn text_set(field: &'static str, values: &[String]) -> ContractResult<()> {
    let mut set = HashSet::new();
    for value in values {
        contract_text(field, value)?;
        if !set.insert(value) {
            return Err(ContractError::new(field, "duplicate declaration"));
        }
    }
    Ok(())
}

fn validate_claims(claims: &[NamespaceClaim]) -> ContractResult<()> {
    let mut set = HashSet::new();
    for claim in claims {
        if !set.insert(&claim.subtree)
            || claim.excluded.iter().collect::<HashSet<_>>().len() != claim.excluded.len()
            || claim
                .excluded
                .iter()
                .any(|e| e == &claim.subtree || !claim.subtree.contains(e))
            || claims.iter().any(|other| {
                claim.subtree != other.subtree && claim.subtree.contains(&other.subtree)
            })
            || claims
                .iter()
                .any(|other| claim.excluded.iter().any(|e| e.contains(&other.subtree)))
        {
            return Err(ContractError::new(
                "policy.namespaces",
                "contradictory or invalid namespace claims",
            ));
        }
    }
    Ok(())
}
fn claim_subset(claim: &NamespaceClaim, allowed: &NamespaceClaim) -> bool {
    allowed.subtree.contains(&claim.subtree)
        && allowed.excluded.iter().all(|excluded| {
            !excluded.contains(&claim.subtree)
                && (!claim.subtree.contains(excluded)
                    || claim.excluded.iter().any(|e| e.contains(excluded)))
        })
}
fn validate_generator(g: &GeneratorTransaction) -> ContractResult<()> {
    contract_text("policy.generator.name", &g.name)?;
    distinct_paths("policy.generator.inputs", &g.inputs)?;
    distinct_paths("policy.generator.outputs", &g.outputs)?;
    distinct_paths("policy.generator.seals", &g.seals)?;
    if g.inputs.is_empty()
        || g.outputs.is_empty()
        || !g.inputs.contains(&g.machine_source)
        || g.seals.iter().any(|s| !g.outputs.contains(s))
    {
        return Err(ContractError::new(
            "policy.generator",
            "incomplete generator transaction",
        ));
    }
    Ok(())
}
