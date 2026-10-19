use super::{CheckCommand, EnvironmentIdentity, VerificationReceipt};
use crate::domain::error::{ContractError, ContractResult, contract_text, contract_version};
use crate::domain::graph::{FileIdentity, PolicyIdentity, PolicyIdentityFields, RepoPath, Sha256};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Hasher};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error("verification filesystem I/O failed")]
    Io(#[from] std::io::Error),
    #[error("verification JSON is invalid")]
    Json(#[from] serde_json::Error),
    #[error("verification refused: {0}")]
    Refused(&'static str),
}
pub type VerificationResult<T> = Result<T, VerificationError>;

pub fn sha256(bytes: &[u8]) -> Sha256 {
    Sha256::new(format!("{:x}", Hasher::digest(bytes))).expect("SHA-256 encoding")
}

/// A reviewed dependency declaration is repository policy, not a claim that
/// arbitrary commands have been sandboxed. The IO adapter verifies the exact
/// audit bytes and their command/roots/generator/exclusion bindings each time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyScope {
    pub roots: Vec<RepoPath>,
    pub generator_inputs: Vec<RepoPath>,
    pub excluded: Vec<RepoPath>,
    pub audit: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputScope {
    Full,
    DependencyClosure { declaration: DependencyScope },
}

/// Machine-checkable, hash-bound audit produced/reviewed outside the receipt
/// service. It must enumerate the complete transitive inputs, including tools'
/// generators. The service never invents dependency analysis or executes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyAudit {
    pub schema_version: u32,
    pub command: CheckCommand,
    pub roots: Vec<RepoPath>,
    pub generator_inputs: Vec<RepoPath>,
    pub excluded: Vec<RepoPath>,
    pub reviewed_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentRequirement {
    pub name: String,
    pub secret_dependent: bool,
    pub observable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPolicy {
    pub name: String,
    pub command: CheckCommand,
    pub inputs: InputScope,
    pub tools: Vec<String>,
    pub dependency_locks: Vec<RepoPath>,
    /// Include HOME, network and external-data state here, even when they
    /// cannot be safely identified. No values or secret hashes are persisted.
    pub environment: Vec<EnvironmentRequirement>,
    pub features: Vec<String>,
    pub warnings_disqualify: bool,
    pub independent_required: bool,
    pub native_platform: Option<String>,
    pub retention_bytes: u64,
    pub retention_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyFields {
    pub schema_version: u32,
    pub design_references: Vec<RepoPath>,
    pub checks: Vec<CheckPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PolicyFields")]
pub struct VerificationPolicy(PolicyFields);

fn distinct<'a>(field: &'static str, items: impl Iterator<Item = &'a str>) -> ContractResult<()> {
    let mut seen = HashSet::new();
    for item in items {
        contract_text(field, item)?;
        if !seen.insert(item) {
            return Err(ContractError::new(field, "duplicate declaration"));
        }
    }
    Ok(())
}

pub fn within(parent: &RepoPath, child: &RepoPath) -> bool {
    child == parent || child.as_str().starts_with(&format!("{}/", parent.as_str()))
}

impl VerificationPolicy {
    pub fn new(fields: PolicyFields) -> ContractResult<Self> {
        contract_version(fields.schema_version)?;
        PolicyIdentity::new(PolicyIdentityFields {
            design_references: fields.design_references.clone(),
            sha256: sha256(b"validation"),
        })?;
        if fields.checks.is_empty() {
            return Err(ContractError::new(
                "checks",
                "at least one check is required",
            ));
        }
        distinct("checks", fields.checks.iter().map(|c| c.name.as_str()))?;
        for check in &fields.checks {
            distinct("tools", check.tools.iter().map(String::as_str))?;
            distinct("features", check.features.iter().map(String::as_str))?;
            distinct(
                "dependency_locks",
                check.dependency_locks.iter().map(RepoPath::as_str),
            )?;
            distinct(
                "environment",
                check.environment.iter().map(|e| e.name.as_str()),
            )?;
            if check.tools.is_empty()
                || check.retention_bytes == 0
                || check.retention_files == 0
                || check.retention_bytes > 16 * 1024 * 1024
                || check.retention_files > 64
            {
                return Err(ContractError::new(
                    "check",
                    "missing tool or unbounded retention",
                ));
            }
            if let Some(platform) = &check.native_platform {
                contract_text("native_platform", platform)?;
            }
            if let InputScope::DependencyClosure { declaration: d } = &check.inputs {
                if d.roots.is_empty() {
                    return Err(ContractError::new(
                        "inputs",
                        "dependency roots are required",
                    ));
                }
                distinct("roots", d.roots.iter().map(RepoPath::as_str))?;
                distinct(
                    "generator_inputs",
                    d.generator_inputs.iter().map(RepoPath::as_str),
                )?;
                distinct("excluded", d.excluded.iter().map(RepoPath::as_str))?;
                if d.excluded.iter().any(|e| {
                    d.roots
                        .iter()
                        .chain(&d.generator_inputs)
                        .chain(std::iter::once(&d.audit.path))
                        .any(|r| within(e, r) || within(r, e))
                }) {
                    return Err(ContractError::new(
                        "inputs",
                        "exclusion overlaps a proven input",
                    ));
                }
            }
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &PolicyFields {
        &self.0
    }
    pub fn identity(&self) -> PolicyIdentity {
        PolicyIdentity::new(PolicyIdentityFields {
            design_references: self.0.design_references.clone(),
            sha256: sha256(&serde_json::to_vec(self).expect("serializable policy")),
        })
        .expect("validated policy")
    }
    pub fn check(&self, name: &str) -> VerificationResult<&CheckPolicy> {
        self.0
            .checks
            .iter()
            .find(|c| c.name == name)
            .ok_or(VerificationError::Refused("check not declared by policy"))
    }
}
impl TryFrom<PolicyFields> for VerificationPolicy {
    type Error = ContractError;
    fn try_from(fields: PolicyFields) -> ContractResult<Self> {
        Self::new(fields)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputEntry {
    pub file: FileIdentity,
    pub directory: bool,
    /// Unix permission bits, or the Windows readonly attribute. No mtime.
    pub mode: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputManifest {
    pub schema_version: u32,
    pub policy_sha256: Sha256,
    pub command: CheckCommand,
    pub root_mode: u32,
    pub working_directory_mode: u32,
    pub entries: Vec<InputEntry>,
}
impl InputManifest {
    pub fn digest(&self) -> Sha256 {
        sha256(&serde_json::to_vec(self).expect("serializable manifest"))
    }
}

/// Values observed by the caller's actual runner. Never spawn a version probe
/// or read a real user's HOME while deciding applicability. Supplied identities
/// are local trusted evidence, not cryptographic execution attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationContext {
    pub toolchains: BTreeMap<String, Sha256>,
    pub platform: String,
    pub architecture: String,
    pub features: Vec<String>,
    pub environment: Vec<EnvironmentIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredVerification {
    pub schema_version: u32,
    pub check: String,
    pub receipt: VerificationReceipt,
    pub policy: VerificationPolicy,
    pub manifest: InputManifest,
    pub inputs_stable: bool,
}
impl StoredVerification {
    pub fn validate(&self) -> VerificationResult<()> {
        let fields = self.receipt.fields();
        let check = self.policy.check(&self.check)?;
        if self.schema_version != 1
            || self.manifest.schema_version != 1
            || fields.policy != self.policy.identity()
            || fields.command != check.command
            || self.manifest.command != check.command
            || self.manifest.policy_sha256 != fields.policy.fields().sha256
            || self.manifest.digest() != fields.input_manifest_sha256
            || self
                .manifest
                .entries
                .windows(2)
                .any(|pair| pair[0].file.path >= pair[1].file.path)
            || self.manifest.entries.iter().any(|entry| {
                entry.directory
                    && (entry.file.bytes != 0 || entry.file.sha256 != sha256(b"directory"))
            })
        {
            return Err(VerificationError::Refused(
                "stored verification bindings are invalid",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationDisposition {
    Run,
    Reused,
    Invalidated,
    NonReusable,
    NotRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationReason {
    ObservedRun,
    Applicable,
    NoReceipt,
    EvidenceUnavailable,
    IdentityChanged,
    PolicyChanged,
    InputsChanged,
    ContextChanged,
    ArtifactsChanged,
    UnobservableInputs,
    UnsuccessfulRun,
    IndependentRunRequired,
    NativePlatformRequired,
    InputsChangedDuringRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationDecision {
    pub disposition: VerificationDisposition,
    pub reason: VerificationReason,
    pub original: Option<VerificationReceipt>,
}
