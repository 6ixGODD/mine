use super::*;
use crate::domain::graph::{PlanIdentity, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationKind {
    BaselineAttestation,
    Mapping,
}

/// Persisted as an append-only chain by the future Root adapter. All identity
/// fields are checked again when resolving, never trusted because they decoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClarificationRecord {
    pub schema_version: u32,
    pub sequence: u64,
    pub previous_record: Option<Sha256>,
    pub original_body: Option<Sha256>,
    pub plan: PlanIdentity,
    pub policy: PolicyIdentity,
    pub observed_revision: u64,
    pub actor: String,
    pub reason: String,
    pub kind: ClarificationKind,
    pub physical_mapping: Vec<RepoPath>,
    pub generators: Vec<GeneratorTransaction>,
}
impl ClarificationRecord {
    pub fn digest(&self) -> Sha256 {
        hash_bytes(&serde_json::to_vec(self).expect("serializable clarification"))
    }
}

#[derive(Debug, Clone)]
pub struct ClarificationRequest {
    pub plan: PlanIdentity,
    pub policy: PolicyIdentity,
    pub expected_revision: u64,
    pub expected_tail: Option<Sha256>,
    pub actor: String,
    pub reason: String,
    pub kind: ClarificationKind,
    pub physical_mapping: Vec<RepoPath>,
    pub generators: Vec<GeneratorTransaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OriginalScope {
    Semantic {
        contract: Box<OwnershipContract>,
    },
    /// Historical paths remain conservative and never imply semantic authority.
    Conservative {
        plan: PlanIdentity,
        physical_mapping: Vec<RepoPath>,
    },
}

/// Immutable validated result. No Deserialize bypass into effective authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveAuthority {
    original: OriginalScope,
    effective: Option<OwnershipContract>,
    plan: PlanIdentity,
    policy: Option<PolicyIdentity>,
    physical_mapping: Vec<RepoPath>,
    clarifications: Vec<ClarificationRecord>,
    reserved_shared_paths: Vec<RepoPath>,
    reserved_generated_paths: Vec<RepoPath>,
}
impl EffectiveAuthority {
    pub fn original(&self) -> &OriginalScope {
        &self.original
    }
    pub fn semantic(&self) -> Option<&OwnershipContract> {
        self.effective.as_ref()
    }
    pub fn plan(&self) -> &PlanIdentity {
        &self.plan
    }
    pub fn policy(&self) -> Option<&PolicyIdentity> {
        self.policy.as_ref()
    }
    pub fn physical_mapping(&self) -> &[RepoPath] {
        &self.physical_mapping
    }
    pub fn clarifications(&self) -> &[ClarificationRecord] {
        &self.clarifications
    }

    pub fn conservative(
        plan: PlanIdentity,
        physical_mapping: Vec<RepoPath>,
    ) -> ContractResult<Self> {
        if physical_mapping.is_empty() {
            return Err(ContractError::new(
                "legacy.mapping",
                "conservative mapping is required",
            ));
        }
        distinct_paths("legacy.mapping", &physical_mapping)?;
        Ok(Self {
            original: OriginalScope::Conservative {
                plan: plan.clone(),
                physical_mapping: physical_mapping.clone(),
            },
            effective: None,
            plan,
            policy: None,
            physical_mapping,
            clarifications: vec![],
            reserved_shared_paths: vec![],
            reserved_generated_paths: vec![],
        })
    }

    /// Bind current raw bytes without inventing missing historical identity or
    /// granting semantic authority. The IO adapter supplies contained Root bytes.
    pub fn observe_conservative(
        original: PlanIdentity,
        raw_body: &[u8],
        physical_mapping: Vec<RepoPath>,
    ) -> ContractResult<Self> {
        let observed = hash_bytes(raw_body);
        if original
            .fields()
            .body_sha256
            .as_ref()
            .is_some_and(|known| known != &observed)
        {
            return Err(ContractError::new(
                "legacy.body",
                "current bytes differ from known immutable body",
            ));
        }
        let mut result = Self::conservative(original, physical_mapping)?;
        let mut effective = result.plan.fields().clone();
        effective.body_sha256 = Some(observed);
        result.plan = PlanIdentity::new(effective)?;
        Ok(result)
    }

    pub fn owns_namespace(&self, namespace: &Namespace) -> bool {
        self.semantic().is_some_and(|contract| {
            contract.fields().namespace_claims.iter().any(|claim| {
                claim.subtree.contains(namespace)
                    && !claim.excluded.iter().any(|e| e.contains(namespace))
            })
        })
    }

    pub fn authorize_internal(
        &self,
        namespace: &Namespace,
        contract: &str,
        path: &RepoPath,
    ) -> ContractResult<()> {
        let fields = self
            .semantic()
            .ok_or_else(|| {
                ContractError::new(
                    "authority",
                    "conservative paths cannot grant semantic authority",
                )
            })?
            .fields();
        if !self.owns_namespace(namespace)
            || !fields.owned_contracts.iter().any(|c| c == contract)
            || fields.excluded_contracts.iter().any(|c| c == contract)
        {
            return Err(ContractError::new(
                "write.contract",
                "namespace or behavior is outside effective authority",
            ));
        }
        if !fields
            .physical_mapping
            .iter()
            .any(|p| path_contains(p, path))
        {
            return Err(ContractError::new(
                "write.path",
                "path is outside effective physical mapping",
            ));
        }
        if self
            .reserved_shared_paths
            .iter()
            .any(|p| path_contains(p, path) || path_contains(path, p))
        {
            return Err(ContractError::new(
                "write.shared",
                "shared entry requires an exact declaration intent",
            ));
        }
        if self
            .reserved_generated_paths
            .iter()
            .any(|p| path_contains(p, path) || path_contains(path, p))
        {
            return Err(ContractError::new(
                "write.generator",
                "generated writes require the complete transaction",
            ));
        }
        Ok(())
    }

    pub fn authorize_shared(&self, declaration: &SharedDeclaration) -> ContractResult<()> {
        if !self
            .semantic()
            .is_some_and(|c| c.fields().shared_declarations.contains(declaration))
        {
            return Err(ContractError::new(
                "write.shared",
                "entry, declaration class or integrator is not authorized",
            ));
        }
        Ok(())
    }

    pub fn authorize_generation(&self, name: &str, outputs: &[RepoPath]) -> ContractResult<()> {
        distinct_paths("write.generator.outputs", outputs)?;
        let generator = self
            .semantic()
            .and_then(|c| c.fields().generators.iter().find(|g| g.name == name))
            .ok_or_else(|| ContractError::new("write.generator", "generator is not authorized"))?;
        if outputs.len() != generator.outputs.len()
            || outputs.iter().any(|p| !generator.outputs.contains(p))
        {
            return Err(ContractError::new(
                "write.generator.outputs",
                "whole output and seal closure is required",
            ));
        }
        Ok(())
    }
}

pub struct AuthorityResolver;
impl AuthorityResolver {
    /// Current bytes are compared to the persisted baseline, not merely hashed
    /// and treated as permission. Historical absence needs a fresh attestation.
    pub fn resolve(
        original: &OwnershipContract,
        current_body: &[u8],
        request_plan: &PlanIdentity,
        policy: &ApprovedOwnershipPolicy,
        current_revision: u64,
        records: &[ClarificationRecord],
    ) -> ContractResult<EffectiveAuthority> {
        let original_plan = &original.fields().plan;
        if policy.approved_revision() > current_revision {
            return Err(ContractError::new(
                "policy.revision",
                "policy approval is newer than the observed graph",
            ));
        }
        let actual = hash_bytes(current_body);
        verify_identity(original_plan, request_plan, &actual)?;
        if original.fields().policy != *policy.identity() {
            return Err(ContractError::new(
                "clarification.policy",
                "original policy is not the approved policy",
            ));
        }
        if let Some(baseline) = &original_plan.fields().body_sha256 {
            if baseline != &actual {
                return Err(ContractError::new(
                    "plan.body",
                    "current bytes differ from the immutable registered baseline",
                ));
            }
        } else if !matches!(
            records.first().map(|r| r.kind),
            Some(ClarificationKind::BaselineAttestation)
        ) {
            return Err(ContractError::new(
                "plan.body",
                "absent historical baseline requires explicit fresh attestation",
            ));
        }
        let mut fields = original.fields().clone();
        let mut tail = None;
        let mut previous_revision = 0;
        for (index, record) in records.iter().enumerate() {
            contract_version(record.schema_version)?;
            contract_text("clarification.actor", &record.actor)?;
            contract_text("clarification.reason", &record.reason)?;
            verify_identity(original_plan, &record.plan, &actual)?;
            if record.sequence != index as u64 + 1
                || record.previous_record != tail
                || record.original_body != original_plan.fields().body_sha256
                || record.policy != *policy.identity()
                || record.observed_revision > current_revision
                || record.observed_revision < policy.approved_revision()
                || (index > 0 && record.observed_revision <= previous_revision)
                || !policy
                    .document()
                    .clarification_actors
                    .contains(&record.actor)
            {
                return Err(ContractError::new(
                    "clarification.provenance",
                    "forged, stale, reordered or unauthorized clarification",
                ));
            }
            if record.kind == ClarificationKind::BaselineAttestation
                && (index != 0
                    || record.physical_mapping != fields.physical_mapping
                    || record.generators != fields.generators)
            {
                return Err(ContractError::new(
                    "clarification.baseline",
                    "first attestation must preserve the known or absent original baseline and scope",
                ));
            }
            fields.plan = record.plan.clone();
            fields.physical_mapping = record.physical_mapping.clone();
            fields.generators = record.generators.clone();
            let resolved = OwnershipContract::new(fields.clone())?;
            policy.validate_scope(resolved.fields())?;
            tail = Some(record.digest());
            previous_revision = record.observed_revision;
        }
        if records.is_empty() {
            fields.plan = request_plan.clone();
        }
        let effective = OwnershipContract::new(fields)?;
        policy.validate_scope(effective.fields())?;
        Ok(EffectiveAuthority {
            original: OriginalScope::Semantic {
                contract: Box::new(original.clone()),
            },
            plan: effective.fields().plan.clone(),
            policy: Some(policy.identity().clone()),
            physical_mapping: effective.fields().physical_mapping.clone(),
            effective: Some(effective),
            clarifications: records.to_vec(),
            reserved_shared_paths: reserved_shared_paths(policy),
            reserved_generated_paths: reserved_generated_paths(policy),
        })
    }

    /// Return a new chain; never mutate the original contract or prior records.
    /// The Root adapter must persist with revision CAS and use the scheduling
    /// evaluator to refuse conflicts with other active owners before publication.
    pub fn append(
        original: &OwnershipContract,
        current_body: &[u8],
        policy: &ApprovedOwnershipPolicy,
        current_revision: u64,
        records: &[ClarificationRecord],
        request: ClarificationRequest,
    ) -> ContractResult<EffectiveAuthority> {
        if request.expected_revision != current_revision
            || request.expected_tail != records.last().map(ClarificationRecord::digest)
        {
            return Err(ContractError::new(
                "clarification.revision",
                "stale observer revision or chain tail",
            ));
        }
        if request.policy != *policy.identity() {
            return Err(ContractError::new(
                "clarification.policy",
                "request policy mismatch",
            ));
        }
        let record = ClarificationRecord {
            schema_version: 1,
            sequence: records.len() as u64 + 1,
            previous_record: request.expected_tail,
            original_body: original.fields().plan.fields().body_sha256.clone(),
            plan: request.plan.clone(),
            policy: request.policy,
            observed_revision: current_revision,
            actor: request.actor,
            reason: request.reason,
            kind: request.kind,
            physical_mapping: request.physical_mapping,
            generators: request.generators,
        };
        let mut next = records.to_vec();
        next.push(record);
        Self::resolve(
            original,
            current_body,
            &request.plan,
            policy,
            current_revision,
            &next,
        )
    }
}

// An omitted transaction/entry is not an ordinary-file permission. Preserve the
// complete approved policy's reserved paths, including those owned elsewhere.
fn reserved_shared_paths(policy: &ApprovedOwnershipPolicy) -> Vec<RepoPath> {
    policy
        .document()
        .shared_declarations
        .iter()
        .map(|entry| entry.path.clone())
        .collect()
}
fn reserved_generated_paths(policy: &ApprovedOwnershipPolicy) -> Vec<RepoPath> {
    policy
        .document()
        .generators
        .iter()
        .flat_map(|g| g.outputs.iter().chain(&g.seals))
        .cloned()
        .collect()
}

fn verify_identity(
    original: &PlanIdentity,
    request: &PlanIdentity,
    actual: &Sha256,
) -> ContractResult<()> {
    let mut expected = original.fields().clone();
    expected.body_sha256 = Some(actual.clone());
    if request.fields() != &expected {
        return Err(ContractError::new(
            "clarification.plan",
            "exact repository, workspace, Plan path, branch, lineage or body identity mismatch",
        ));
    }
    Ok(())
}
