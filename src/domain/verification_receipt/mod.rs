//! Attributable observations and conservative, command-bound input policies.

mod policy;
pub use policy::*;

use crate::domain::error::{ContractError, ContractResult, contract_text, contract_version};
pub use crate::domain::graph::FileIdentity as ArtifactIdentity;
use crate::domain::graph::{PlanIdentity, PolicyIdentity, RepoPath, Sha256, UtcTimestamp};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ReceiptId(String);
impl ReceiptId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
    pub fn parse(value: impl Into<String>) -> ContractResult<Self> {
        let value = value.into();
        let id = uuid::Uuid::parse_str(&value)
            .map_err(|_| ContractError::new("receipt_id", "expected canonical UUID v4"))?;
        if id.get_version_num() != 4
            || id.get_variant() != uuid::Variant::RFC4122
            || id.to_string() != value
        {
            return Err(ContractError::new(
                "receipt_id",
                "expected canonical UUID v4",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ReceiptId {
    type Error = ContractError;
    fn try_from(value: String) -> ContractResult<Self> {
        Self::parse(value)
    }
}
impl From<ReceiptId> for String {
    fn from(value: ReceiptId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandFields {
    /// Ordered argv including the executable. Never a shell command string.
    pub argv: Vec<String>,
    pub cwd: CommandDirectory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "path",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CommandDirectory {
    Root,
    Relative(RepoPath),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CommandFields")]
pub struct CheckCommand(CommandFields);
impl CheckCommand {
    pub fn new(fields: CommandFields) -> ContractResult<Self> {
        let executable = fields
            .argv
            .first()
            .ok_or(ContractError::new("command.argv", "executable is required"))?;
        contract_text("command.executable", executable)?;
        if fields.argv.iter().any(|arg| arg.contains('\0')) {
            return Err(ContractError::new(
                "command.argv",
                "arguments must not contain NUL",
            ));
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &CommandFields {
        &self.0
    }
}
impl TryFrom<CommandFields> for CheckCommand {
    type Error = ContractError;
    fn try_from(fields: CommandFields) -> ContractResult<Self> {
        Self::new(fields)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Passed,
    Failed,
    Skipped,
    TimedOut,
    WarningDisqualified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentIdentity {
    pub name: String,
    /// A nonsecret identity, never the environment value itself.
    pub sha256: Option<Sha256>,
    pub observable: bool,
    pub secret_dependent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptFields {
    pub schema_version: u32,
    pub receipt_id: ReceiptId,
    pub plan: PlanIdentity,
    pub actor: String,
    pub command: CheckCommand,
    pub started_at: UtcTimestamp,
    pub ended_at: UtcTimestamp,
    pub exit_code: Option<i32>,
    pub outcome: CheckOutcome,
    pub policy: PolicyIdentity,
    pub input_manifest_sha256: Sha256,
    pub toolchains: BTreeMap<String, Sha256>,
    pub dependency_locks: BTreeMap<RepoPath, Sha256>,
    pub platform: String,
    pub architecture: String,
    pub features: Vec<String>,
    pub environment: Vec<EnvironmentIdentity>,
    pub artifacts: Vec<ArtifactIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ReceiptFields")]
pub struct VerificationReceipt(ReceiptFields);
impl VerificationReceipt {
    pub fn new(fields: ReceiptFields) -> ContractResult<Self> {
        contract_version(fields.schema_version)?;
        contract_text("actor", &fields.actor)?;
        contract_text("platform", &fields.platform)?;
        contract_text("architecture", &fields.architecture)?;
        if fields.plan.fields().body_sha256.is_none() {
            return Err(ContractError::new(
                "plan.body_sha256",
                "receipt requires an attributable Plan body",
            ));
        }
        if fields.ended_at.instant() < fields.started_at.instant() {
            return Err(ContractError::new("ended_at", "end precedes start"));
        }
        if matches!(fields.outcome, CheckOutcome::Passed) && fields.exit_code != Some(0)
            || matches!(fields.outcome, CheckOutcome::Failed)
                && fields.exit_code.is_none_or(|exit| exit == 0)
            || matches!(fields.outcome, CheckOutcome::Skipped) && fields.exit_code.is_some()
        {
            return Err(ContractError::new(
                "outcome",
                "outcome contradicts observed exit code",
            ));
        }
        let mut features = HashSet::new();
        for feature in &fields.features {
            contract_text("features", feature)?;
            if !features.insert(feature) {
                return Err(ContractError::new("features", "duplicate feature"));
            }
        }
        for tool in fields.toolchains.keys() {
            contract_text("toolchains", tool)?;
        }
        if fields.toolchains.is_empty() {
            return Err(ContractError::new(
                "toolchains",
                "at least one toolchain identity is required",
            ));
        }
        let mut environment = HashSet::new();
        for identity in &fields.environment {
            contract_text("environment.name", &identity.name)?;
            if !environment.insert(&identity.name)
                || ((identity.secret_dependent || !identity.observable)
                    && identity.sha256.is_some())
            {
                return Err(ContractError::new(
                    "environment",
                    "duplicate name or unsafe fabricated/secret identity",
                ));
            }
        }
        let mut artifacts = HashSet::new();
        for artifact in &fields.artifacts {
            if !artifacts.insert(&artifact.path) {
                return Err(ContractError::new("artifacts", "duplicate artifact path"));
            }
        }
        Ok(Self(fields))
    }
    pub fn fields(&self) -> &ReceiptFields {
        &self.0
    }
    /// Structural reason only: a true result still needs the policy/input and
    /// real artifact checks implemented by the subsequent receipt service.
    pub fn has_unobservable_inputs(&self) -> bool {
        self.0
            .environment
            .iter()
            .any(|e| e.secret_dependent || !e.observable || e.sha256.is_none())
    }
}
impl TryFrom<ReceiptFields> for VerificationReceipt {
    type Error = ContractError;
    fn try_from(fields: ReceiptFields) -> ContractResult<Self> {
        Self::new(fields)
    }
}
