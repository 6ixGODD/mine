//! Reviewed independently before use; not linked into the current product.
#![forbid(unsafe_code)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! uuid_id {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({"type":"string", "pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$", "not":{"const":"00000000-0000-0000-0000-000000000000"}})
            }
        }
        impl TryFrom<String> for $name {
            type Error = &'static str;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                let id = uuid::Uuid::parse_str(&value).map_err(|_| "invalid UUID")?;
                if id.is_nil() || id.hyphenated().to_string() != value {
                    return Err("expected non-nil canonical lowercase UUID");
                }
                Ok(Self(value))
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self { value.0 }
        }
        impl $name { pub fn as_str(&self) -> &str { &self.0 } }
    )+};
}
uuid_id!(
    ProjectId,
    PrincipalId,
    AttemptId,
    OperationId,
    WorkNodeId,
    MessageId,
    SnapshotId
);

macro_rules! counter {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(u64);
        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({"type":"string", "pattern":"^(0|[1-9][0-9]{0,18})$", "maxLength":19, "description":"Canonical decimal 0..9223372036854775807; consumers additionally enforce this numeric bound."})
            }
        }
        impl $name {
            pub const fn new(value: u64) -> Result<Self, ErrorCode> {
                if value > i64::MAX as u64 { Err(ErrorCode::InvalidInput) } else { Ok(Self(value)) }
            }
            pub const fn get(self) -> u64 { self.0 }
            pub fn checked_next(self) -> Result<Self, ErrorCode> {
                Self::new(self.0 + 1)
            }
        }
        impl TryFrom<String> for $name {
            type Error = &'static str;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                let number = value.parse::<u64>().map_err(|_| "invalid unsigned counter")?;
                if number.to_string() != value { return Err("noncanonical counter"); }
                Self::new(number).map_err(|_| "counter exceeds SQLite signed64 range")
            }
        }
        impl From<$name> for String { fn from(value: $name) -> Self { value.0.to_string() } }
    )+};
}
counter!(EntityRevision, AssignmentEpoch, EventCursor);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest(String);
impl JsonSchema for Digest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Digest".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type":"string", "pattern":"^[0-9a-f]{64}$"})
    }
}
impl TryFrom<String> for Digest {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("expected lowercase SHA-256 hex");
        }
        Ok(Self(value))
    }
}
impl From<Digest> for String {
    fn from(value: Digest) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Architect,
    Executor,
    Reviewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    Unauthenticated,
    Forbidden,
    NotFound,
    RevisionConflict,
    StaleEpoch,
    IdempotencyConflict,
    Busy,
    Pending,
    OutcomeUnknown,
    CorruptStore,
    VersionUnsupported,
    IoFailure,
    CapabilityStale,
    SelectionUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandMeta {
    pub operation_id: OperationId,
    pub project_id: ProjectId,
    pub expected_revision: EntityRevision,
    pub assignment_epoch: Option<AssignmentEpoch>,
    pub input_digest: Digest,
}

/// Global commands (including first registration) must not invent a Project ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ControlCommandMeta {
    pub operation_id: OperationId,
    pub expected_revision: EntityRevision,
    pub input_digest: Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommitDisposition {
    Applied,
    Replayed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommitReceipt {
    pub operation_id: OperationId,
    pub disposition: CommitDisposition,
    pub revision: EntityRevision,
    pub cursor: EventCursor,
}

/// Authentication and authorization happen before this pure concurrency check.
pub fn check_concurrency(
    meta: &CommandMeta,
    revision: EntityRevision,
    epoch: Option<AssignmentEpoch>,
) -> Result<(), ErrorCode> {
    if meta.expected_revision != revision {
        return Err(ErrorCode::RevisionConflict);
    }
    if meta.assignment_epoch != epoch {
        return Err(ErrorCode::StaleEpoch);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Codex,
    Claude,
    Pi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RequestedValue {
    RuntimeDefault,
    Exact(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ObservedValue {
    Observed(String),
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "values",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Choices {
    Supported(Vec<String>),
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelCapability {
    pub model: String,
    pub effort: Choices,
    pub speed: Choices,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Discovery {
    pub project_id: ProjectId,
    pub snapshot_id: SnapshotId,
    pub provider: Provider,
    pub runtime_version: String,
    pub config_fingerprint: Digest,
    pub models: Vec<ModelCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub project_id: ProjectId,
    pub snapshot_id: SnapshotId,
    pub provider: Provider,
    pub model: RequestedValue,
    pub effort: RequestedValue,
    pub speed: RequestedValue,
}

fn supported(value: &RequestedValue, choices: &Choices) -> bool {
    match value {
        RequestedValue::RuntimeDefault => true,
        RequestedValue::Exact(value) => {
            !value.trim().is_empty()
                && matches!(choices, Choices::Supported(values) if values.contains(value))
        }
    }
}

/// Capability validation is not proof of quota, credentials, or effective selection.
pub fn validate_selection(selection: &Selection, discovery: &Discovery) -> Result<(), ErrorCode> {
    validate_discovery(discovery)?;
    if selection.project_id != discovery.project_id
        || selection.snapshot_id != discovery.snapshot_id
        || selection.provider != discovery.provider
    {
        return Err(ErrorCode::CapabilityStale);
    }
    match &selection.model {
        RequestedValue::RuntimeDefault => {
            // An unresolved default model cannot authorize model-specific options.
            if selection.effort != RequestedValue::RuntimeDefault
                || selection.speed != RequestedValue::RuntimeDefault
            {
                return Err(ErrorCode::SelectionUnsupported);
            }
        }
        RequestedValue::Exact(model) => {
            let capability = discovery
                .models
                .iter()
                .find(|c| c.model == *model && !model.trim().is_empty())
                .ok_or(ErrorCode::SelectionUnsupported)?;
            if !supported(&selection.effort, &capability.effort)
                || !supported(&selection.speed, &capability.speed)
            {
                return Err(ErrorCode::SelectionUnsupported);
            }
        }
    }
    Ok(())
}

/// Reject ambiguous capabilities before array order can affect a selection.
pub fn validate_discovery(discovery: &Discovery) -> Result<(), ErrorCode> {
    let mut names = std::collections::HashSet::new();
    if discovery.runtime_version.trim().is_empty() {
        return Err(ErrorCode::InvalidInput);
    }
    for model in &discovery.models {
        if model.model.trim().is_empty() || !names.insert(&model.model) {
            return Err(ErrorCode::InvalidInput);
        }
        for choices in [&model.effort, &model.speed] {
            if let Choices::Supported(values) = choices {
                let mut options = std::collections::HashSet::new();
                if values
                    .iter()
                    .any(|value| value.trim().is_empty() || !options.insert(value))
                {
                    return Err(ErrorCode::InvalidInput);
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Runnable,
    Running,
    WaitingPeer,
    WaitingDecision,
    WaitingResource,
    Pausing,
    Paused,
    Recovering,
    Failed,
    Done,
}

impl WorkState {
    /// Necessary, not sufficient: caller also checks persisted correlation/dedup/CAS.
    pub const fn can_wake(self, project_paused: bool) -> bool {
        !project_paused
            && matches!(
                self,
                Self::WaitingPeer | Self::WaitingDecision | Self::WaitingResource
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Inform,
    RequestResult,
    Commit,
    Decline,
    Progress,
    Result,
    Failed,
    DecisionRequired,
    Decision,
    ResourceRequest,
    Proposal,
    Cancel,
    Cancelled,
    Handoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EffectOutcome {
    NotSent,
    Rejected,
    Accepted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrontierNode {
    pub id: WorkNodeId,
    pub title: String,
    pub state: WorkState,
    pub owner: PrincipalId,
    pub waiting_on: Option<PrincipalId>,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrontierSnapshot {
    pub project_id: ProjectId,
    pub cursor: EventCursor,
    pub paused: bool,
    pub nodes: Vec<FrontierNode>,
}

/// Root for schema generation; not a network request or a duplicated state store.
#[derive(JsonSchema)]
pub struct ContractSchema {
    pub command: CommandMeta,
    pub control_command: ControlCommandMeta,
    pub receipt: CommitReceipt,
    pub selection: Selection,
    pub discovery: Discovery,
    pub frontier: FrontierSnapshot,
    pub observed_value: ObservedValue,
    pub error: ErrorCode,
    pub role: Role,
    pub message_kind: MessageKind,
    pub effect_outcome: EffectOutcome,
}
