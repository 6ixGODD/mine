//! Isolated, NOT ACCEPTED runtime wire foundation. No provider or process effects.
#![forbid(unsafe_code)]

use mine_v1_contracts::{
    AssignmentEpoch, AttemptId, Digest, Discovery, EntityRevision, ErrorCode, ObservedValue,
    OperationId, PrincipalId, ProjectId, Provider, Selection, SnapshotId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "mine-runtime/1";
pub const MAX_FRAME_BYTES: usize = 1_048_576;
pub const MAX_PENDING_REQUESTS: u32 = 64;
pub const MAX_CONTROL_EVENTS: u32 = 256;
pub const MAX_DIAGNOSTIC_TAIL_BYTES: u32 = 65_536;

macro_rules! closed_struct {
    ($(#[$meta:meta])* $name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}

macro_rules! wire_enum {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),* }
    };
}

// Reuse the canonical UUID/counter parsers rather than introduce stringly IDs.
macro_rules! identifier {
    ($name:ident, $inner:ty) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub $inner);
    };
}
identifier!(RequestId, OperationId);
identifier!(ConnectionId, OperationId);
identifier!(AssignmentId, OperationId);
identifier!(DeliveryId, OperationId);
identifier!(Generation, EntityRevision);
identifier!(UtcMillis, EntityRevision);

closed_struct!(Limits {
    frame_bytes: u32,
    pending_requests: u32,
    control_events: u32,
    diagnostic_tail_bytes: u32,
});
impl Default for Limits {
    fn default() -> Self {
        Self {
            frame_bytes: MAX_FRAME_BYTES as u32,
            pending_requests: MAX_PENDING_REQUESTS,
            control_events: MAX_CONTROL_EVENTS,
            diagnostic_tail_bytes: MAX_DIAGNOSTIC_TAIL_BYTES,
        }
    }
}
wire_enum!(HandshakeKind { Hello, HelloAck });
closed_struct!(Handshake {
    protocol: String,
    kind: HandshakeKind,
    connection_id: ConnectionId,
    adapter_version: String,
    schema_digest: Digest,
    limits: Limits,
});
impl Handshake {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.protocol != PROTOCOL || self.limits != Limits::default() {
            return Err(ErrorCode::VersionUnsupported);
        }
        nonempty(&self.adapter_version)
    }
}

closed_struct!(AttemptScope {
    principal_id: PrincipalId,
    assignment_id: AssignmentId,
    assignment_revision: EntityRevision,
    attempt_id: AttemptId,
    generation: Generation,
    fencing_epoch: AssignmentEpoch,
});
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    Project,
    Attempt(AttemptScope),
}

closed_struct!(RuntimeContext {
    provider: Provider,
    runtime_profile_ref: String,
    credential_profile_ref: String,
    trusted_config_root: String,
    execution_cwd: String,
    config_fingerprint: Digest,
    policy_revision: EntityRevision,
});
impl RuntimeContext {
    /// Structural checks only. Caller must resolve paths and verify registered
    /// worktree identity, trust, credentials, policy and filesystem containment.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        nonempty(&self.runtime_profile_ref)?;
        nonempty(&self.credential_profile_ref)?;
        for path in [&self.trusted_config_root, &self.execution_cwd] {
            nonempty(path)?;
            if !absolute_path_syntax(path) {
                return Err(ErrorCode::InvalidInput);
            }
        }
        Ok(())
    }
}

closed_struct!(SessionBinding {
    provider: Provider,
    native_session_id: String,
    native_thread_id: Option<String>,
    native_storage_ref: Option<String>,
    attempt_id: AttemptId,
    generation: Generation,
});
impl SessionBinding {
    pub fn validate(&self, scope: &AttemptScope) -> Result<(), ErrorCode> {
        nonempty(&self.native_session_id)?;
        for value in [&self.native_thread_id, &self.native_storage_ref]
            .into_iter()
            .flatten()
        {
            nonempty(value)?;
        }
        if self.attempt_id != scope.attempt_id || self.generation != scope.generation {
            return Err(ErrorCode::StaleEpoch);
        }
        Ok(())
    }
}

wire_enum!(CapabilityState {
    Supported,
    Unsupported,
    Unknown
});
closed_struct!(MethodCapability {
    method: Method,
    state: CapabilityState,
    reason: Option<String>,
});
closed_struct!(ModelDetails {
    model: String,
    display_name: String,
    resolved_identity: Option<String>,
    is_runtime_default: bool,
    default_effort: ObservedValue,
    default_speed: ObservedValue,
    evidence_refs: Vec<String>,
});
closed_struct!(RuntimeCapabilitySnapshot {
    discovery: Discovery,
    adapter_version: String,
    protocol_schema_digest: Digest,
    credential_profile_ref: String,
    observed_at: UtcMillis,
    expires_at: UtcMillis,
    methods: Vec<MethodCapability>,
    model_details: Vec<ModelDetails>,
    diagnostics: Vec<String>,
});
impl RuntimeCapabilitySnapshot {
    pub fn validate(&self, now: &UtcMillis) -> Result<(), ErrorCode> {
        if self.observed_at.0 >= self.expires_at.0
            || now.0 < self.observed_at.0
            || now.0 >= self.expires_at.0
        {
            return Err(ErrorCode::CapabilityStale);
        }
        nonempty(&self.adapter_version)?;
        nonempty(&self.credential_profile_ref)?;
        mine_v1_contracts::validate_discovery(&self.discovery)?;
        for (index, capability) in self.methods.iter().enumerate() {
            if self.methods[..index]
                .iter()
                .any(|other| other.method == capability.method)
            {
                return Err(ErrorCode::InvalidInput);
            }
            if capability.state != CapabilityState::Supported {
                nonempty(
                    capability
                        .reason
                        .as_deref()
                        .ok_or(ErrorCode::InvalidInput)?,
                )?;
            }
        }
        for (index, details) in self.model_details.iter().enumerate() {
            nonempty(&details.display_name)?;
            if !self
                .discovery
                .models
                .iter()
                .any(|m| m.model == details.model)
                || self.model_details[..index]
                    .iter()
                    .any(|m| m.model == details.model)
            {
                return Err(ErrorCode::InvalidInput);
            }
        }
        Ok(())
    }
}

closed_struct!(ObservationDetails {
    reason: Option<String>,
    evidence_refs: Vec<String>,
    resolved_identity: Option<String>,
    availability_state: Option<String>,
});
closed_struct!(SelectionDetails {
    model: ObservationDetails,
    effort: ObservationDetails,
    speed: ObservationDetails,
});
closed_struct!(RuntimeEffectiveSelection {
    project_id: ProjectId,
    snapshot_id: SnapshotId,
    provider: Provider,
    model: ObservedValue,
    effort: ObservedValue,
    speed: ObservedValue,
    details: SelectionDetails,
});
impl RuntimeEffectiveSelection {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        for (observed, details) in [
            (&self.model, &self.details.model),
            (&self.effort, &self.details.effort),
            (&self.speed, &self.details.speed),
        ] {
            match observed {
                ObservedValue::Observed(value) => {
                    nonempty(value)?;
                    if details.evidence_refs.is_empty() {
                        return Err(ErrorCode::InvalidInput);
                    }
                }
                ObservedValue::Unsupported | ObservedValue::Unknown => {
                    nonempty(details.reason.as_deref().ok_or(ErrorCode::InvalidInput)?)?;
                }
            }
            for evidence in &details.evidence_refs {
                nonempty(evidence)?;
            }
        }
        Ok(())
    }
}

wire_enum!(Method {
    Discover,
    Create,
    Resume,
    Rename,
    Submit,
    Interrupt,
    Inspect,
    Shutdown
});
wire_enum!(SubmitMode {
    Start,
    Steer,
    FollowUp
});
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    Text { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "method",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Call {
    Discover {
        context: RuntimeContext,
    },
    Create {
        context: RuntimeContext,
        selection: Selection,
        display_name: String,
    },
    Resume {
        context: RuntimeContext,
        binding: SessionBinding,
        selection: Selection,
    },
    Rename {
        binding: SessionBinding,
        display_name: String,
    },
    Submit {
        binding: SessionBinding,
        delivery_id: DeliveryId,
        mode: SubmitMode,
        expected_native_turn_id: Option<String>,
        content: Vec<Content>,
    },
    Interrupt {
        binding: SessionBinding,
        expected_native_turn_id: Option<String>,
        reason: String,
        discard_pending: bool,
    },
    Inspect {
        binding: Option<SessionBinding>,
        create_operation_id: Option<OperationId>,
    },
    Shutdown {
        binding: SessionBinding,
        reason: String,
    },
}
impl Call {
    pub const fn method(&self) -> Method {
        match self {
            Self::Discover { .. } => Method::Discover,
            Self::Create { .. } => Method::Create,
            Self::Resume { .. } => Method::Resume,
            Self::Rename { .. } => Method::Rename,
            Self::Submit { .. } => Method::Submit,
            Self::Interrupt { .. } => Method::Interrupt,
            Self::Inspect { .. } => Method::Inspect,
            Self::Shutdown { .. } => Method::Shutdown,
        }
    }
}

// Nested call is deliberate: no serde flatten with deny_unknown_fields. The
// envelope and call are each closed and schema-checkable.
closed_struct!(Request {
    protocol: String,
    request_id: RequestId,
    operation_id: OperationId,
    project_id: ProjectId,
    scope: Scope,
    deadline_ms: u32,
    call: Call,
});
impl Request {
    /// Pure semantic validation. This is not authorization or filesystem proof.
    pub fn validate(
        &self,
        snapshot: Option<&RuntimeCapabilitySnapshot>,
        now: &UtcMillis,
    ) -> Result<(), ErrorCode> {
        if self.protocol != PROTOCOL {
            return Err(ErrorCode::VersionUnsupported);
        }
        if self.deadline_ms == 0 {
            return Err(ErrorCode::InvalidInput);
        }
        let scope = match (&self.scope, &self.call) {
            (Scope::Project, Call::Discover { context }) => return context.validate(),
            (Scope::Attempt(scope), call) if !matches!(call, Call::Discover { .. }) => scope,
            _ => return Err(ErrorCode::InvalidInput),
        };
        match &self.call {
            Call::Create {
                context,
                selection,
                display_name,
            } => {
                nonempty(display_name)?;
                self.validate_selection(context, selection, snapshot, now)
            }
            Call::Resume {
                context,
                binding,
                selection,
            } => {
                binding.validate(scope)?;
                if binding.provider != context.provider {
                    return Err(ErrorCode::InvalidInput);
                }
                self.validate_selection(context, selection, snapshot, now)
            }
            Call::Rename {
                binding,
                display_name,
            } => {
                binding.validate(scope)?;
                nonempty(display_name)
            }
            Call::Submit {
                binding,
                mode,
                expected_native_turn_id,
                content,
                ..
            } => {
                binding.validate(scope)?;
                if *mode == SubmitMode::Start && expected_native_turn_id.is_some() {
                    return Err(ErrorCode::InvalidInput);
                }
                if let Some(turn) = expected_native_turn_id {
                    nonempty(turn)?;
                }
                if content.is_empty() {
                    return Err(ErrorCode::InvalidInput);
                }
                for item in content {
                    match item {
                        Content::Text { text } => nonempty(text)?,
                    }
                }
                Ok(())
            }
            Call::Interrupt {
                binding,
                expected_native_turn_id,
                reason,
                discard_pending,
            } => {
                binding.validate(scope)?;
                if !discard_pending {
                    return Err(ErrorCode::InvalidInput);
                }
                if let Some(turn) = expected_native_turn_id {
                    nonempty(turn)?;
                }
                nonempty(reason)
            }
            Call::Inspect {
                binding,
                create_operation_id,
            } => match (binding, create_operation_id) {
                (Some(binding), None) => binding.validate(scope),
                (None, Some(_)) => Ok(()),
                _ => Err(ErrorCode::InvalidInput),
            },
            Call::Shutdown { binding, reason } => {
                binding.validate(scope)?;
                nonempty(reason)
            }
            Call::Discover { .. } => Err(ErrorCode::InvalidInput),
        }
    }

    fn validate_selection(
        &self,
        context: &RuntimeContext,
        selection: &Selection,
        snapshot: Option<&RuntimeCapabilitySnapshot>,
        now: &UtcMillis,
    ) -> Result<(), ErrorCode> {
        context.validate()?;
        let snapshot = snapshot.ok_or(ErrorCode::CapabilityStale)?;
        snapshot.validate(now)?;
        if self.project_id != selection.project_id
            || context.provider != selection.provider
            || context.config_fingerprint != snapshot.discovery.config_fingerprint
            || context.credential_profile_ref != snapshot.credential_profile_ref
        {
            return Err(ErrorCode::CapabilityStale);
        }
        if !snapshot.methods.iter().any(|capability| {
            capability.method == self.call.method()
                && capability.state == CapabilityState::Supported
        }) {
            return Err(ErrorCode::VersionUnsupported);
        }
        mine_v1_contracts::validate_selection(selection, &snapshot.discovery)
    }
}

wire_enum!(EffectOutcome {
    NotSent,
    Rejected,
    Accepted,
    Unknown
});
wire_enum!(FailureStage {
    Preflight,
    Spawn,
    Initialize,
    Create,
    Resume,
    Select,
    Submit,
    Stream,
    Interrupt,
    Inspect,
    Shutdown
});
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeFailureCode {
    AuthRequired,
    AccessDenied,
    QuotaOrRateLimit,
    TransientProvider,
    RuntimeUnavailable,
    VersionUnsupported,
    ConfigContextMismatch,
    SelectionUnsupported,
    CapabilityStale,
    SessionNotFound,
    SessionCorrupt,
    ProtocolInvalid,
    PayloadTooLarge,
    Backpressure,
    SpawnFailed,
    ProcessExited,
    Cancelled,
    QuiescenceUnconfirmed,
    PersistenceFailed,
    UnknownProviderFailure,
}
closed_struct!(RuntimeFailure {
    code: RuntimeFailureCode,
    stage: FailureStage,
    outcome: EffectOutcome,
    message: String,
    provider_code: Option<String>,
    retry_after_ms: Option<u32>,
    native_binding: Option<SessionBinding>,
    evidence_refs: Vec<String>,
});

wire_enum!(TurnState {
    Idle,
    Running,
    Terminal,
    Unknown
});
wire_enum!(PendingInputs {
    Empty,
    Present,
    Unknown
});
wire_enum!(AutomaticContinuation {
    Stopped,
    Possible,
    Unknown
});
wire_enum!(ProcessState {
    Running,
    Exited,
    Unknown
});
wire_enum!(ExternalEffects {
    Settled,
    Pending,
    Unknown
});
wire_enum!(Confidence {
    Confirmed,
    Unconfirmed
});
closed_struct!(OwnedProcess {
    pid: u32,
    launch_id: OperationId,
    state: ProcessState,
});
closed_struct!(QuiescenceObservation {
    observed_at: UtcMillis,
    turn_state: TurnState,
    pending_inputs: PendingInputs,
    automatic_continuation: AutomaticContinuation,
    owned_processes: Vec<OwnedProcess>,
    external_effects: ExternalEffects,
    evidence_refs: Vec<String>,
    confidence: Confidence,
});
impl QuiescenceObservation {
    /// Structural proof consistency, not independent proof of process ownership.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        for (index, process) in self.owned_processes.iter().enumerate() {
            if process.pid == 0
                || self.owned_processes[..index]
                    .iter()
                    .any(|p| p.pid == process.pid)
            {
                return Err(ErrorCode::InvalidInput);
            }
        }
        for evidence in &self.evidence_refs {
            nonempty(evidence)?;
        }
        if self.confidence == Confidence::Confirmed && !self.is_confirmed_quiescent() {
            return Err(ErrorCode::InvalidInput);
        }
        Ok(())
    }

    pub fn is_confirmed_quiescent(&self) -> bool {
        self.confidence == Confidence::Confirmed
            && matches!(self.turn_state, TurnState::Idle | TurnState::Terminal)
            && self.pending_inputs == PendingInputs::Empty
            && self.automatic_continuation == AutomaticContinuation::Stopped
            && self
                .owned_processes
                .iter()
                .all(|p| p.state == ProcessState::Exited)
            && self.external_effects == ExternalEffects::Settled
            && !self.evidence_refs.is_empty()
            && self.evidence_refs.iter().all(|v| !v.trim().is_empty())
    }
}

wire_enum!(InspectionMatch {
    Exact,
    Absent,
    Ambiguous,
    Unknown
});
closed_struct!(Inspection {
    matched: InspectionMatch,
    binding: Option<SessionBinding>,
    selection: Option<RuntimeEffectiveSelection>,
    observation: Option<QuiescenceObservation>,
    evidence_refs: Vec<String>,
});
impl Inspection {
    pub fn validate(&self, scope: &AttemptScope) -> Result<(), ErrorCode> {
        if let Some(binding) = &self.binding {
            binding.validate(scope)?;
        }
        if let Some(selection) = &self.selection {
            selection.validate()?;
        }
        for evidence in &self.evidence_refs {
            nonempty(evidence)?;
        }
        if let Some(observation) = &self.observation {
            observation.validate()?;
        }
        if matches!(
            self.matched,
            InspectionMatch::Exact | InspectionMatch::Absent
        ) && self.evidence_refs.is_empty()
        {
            return Err(ErrorCode::InvalidInput);
        }
        match (self.matched, &self.binding) {
            (InspectionMatch::Exact, None) | (InspectionMatch::Absent, Some(_)) => {
                Err(ErrorCode::InvalidInput)
            }
            _ => Ok(()),
        }
    }
}

wire_enum!(Admission { Ready, Blocked });
wire_enum!(InputDisposition {
    Accepted,
    Queued,
    Handled
});
wire_enum!(TurnFinishedStatus {
    Completed,
    Interrupted,
    Failed
});
wire_enum!(ToolFinishedStatus {
    Completed,
    Failed,
    Cancelled,
    Unknown
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "method",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MethodResult {
    Discover {
        snapshot: RuntimeCapabilitySnapshot,
    },
    Create {
        binding: SessionBinding,
        effective_selection: RuntimeEffectiveSelection,
        admission: Admission,
        diagnostics: Vec<String>,
    },
    Resume {
        binding: SessionBinding,
        effective_selection: RuntimeEffectiveSelection,
        admission: Admission,
        diagnostics: Vec<String>,
    },
    Rename {
        observed_name: String,
    },
    Submit {
        disposition: InputDisposition,
        native_turn_id: Option<String>,
    },
    Interrupt {
        observation: QuiescenceObservation,
    },
    Inspect {
        inspection: Inspection,
    },
    Shutdown {
        observation: QuiescenceObservation,
    },
}
impl MethodResult {
    pub const fn method(&self) -> Method {
        match self {
            Self::Discover { .. } => Method::Discover,
            Self::Create { .. } => Method::Create,
            Self::Resume { .. } => Method::Resume,
            Self::Rename { .. } => Method::Rename,
            Self::Submit { .. } => Method::Submit,
            Self::Interrupt { .. } => Method::Interrupt,
            Self::Inspect { .. } => Method::Inspect,
            Self::Shutdown { .. } => Method::Shutdown,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResponseOutcome {
    Ok { result: Box<MethodResult> },
    Error { error: RuntimeFailure },
}
closed_struct!(Response {
    protocol: String,
    request_id: RequestId,
    operation_id: OperationId,
    project_id: ProjectId,
    scope: Scope,
    outcome: ResponseOutcome,
});
impl Response {
    pub fn validate_for(&self, request: &Request, now: &UtcMillis) -> Result<(), ErrorCode> {
        if self.protocol != PROTOCOL {
            return Err(ErrorCode::VersionUnsupported);
        }
        if self.request_id != request.request_id
            || self.operation_id != request.operation_id
            || self.project_id != request.project_id
            || self.scope != request.scope
        {
            return Err(ErrorCode::InvalidInput);
        }
        match &self.outcome {
            ResponseOutcome::Error { error } => {
                nonempty(&error.message)?;
                if let Some(binding) = &error.native_binding {
                    let Scope::Attempt(scope) = &self.scope else {
                        return Err(ErrorCode::InvalidInput);
                    };
                    binding.validate(scope)?;
                }
                Ok(())
            }
            ResponseOutcome::Ok { result } => {
                if result.method() != request.call.method() {
                    return Err(ErrorCode::InvalidInput);
                }
                match result.as_ref() {
                    MethodResult::Discover { snapshot } => {
                        snapshot.validate(now)?;
                        if snapshot.discovery.project_id != self.project_id {
                            return Err(ErrorCode::CapabilityStale);
                        }
                        let Call::Discover { context } = &request.call else {
                            return Err(ErrorCode::InvalidInput);
                        };
                        if snapshot.discovery.provider != context.provider
                            || snapshot.discovery.config_fingerprint != context.config_fingerprint
                            || snapshot.credential_profile_ref != context.credential_profile_ref
                        {
                            return Err(ErrorCode::CapabilityStale);
                        }
                        Ok(())
                    }
                    MethodResult::Create {
                        binding,
                        effective_selection,
                        admission,
                        diagnostics,
                    }
                    | MethodResult::Resume {
                        binding,
                        effective_selection,
                        admission,
                        diagnostics,
                    } => {
                        let Scope::Attempt(scope) = &self.scope else {
                            return Err(ErrorCode::InvalidInput);
                        };
                        binding.validate(scope)?;
                        effective_selection.validate()?;
                        let (requested, context) = match &request.call {
                            Call::Create {
                                selection, context, ..
                            }
                            | Call::Resume {
                                selection, context, ..
                            } => (selection, context),
                            _ => return Err(ErrorCode::InvalidInput),
                        };
                        if binding.provider != context.provider
                            || effective_selection.provider != context.provider
                            || effective_selection.project_id != self.project_id
                            || effective_selection.snapshot_id != requested.snapshot_id
                        {
                            return Err(ErrorCode::CapabilityStale);
                        }
                        if let Call::Resume {
                            binding: original, ..
                        } = &request.call
                        {
                            if binding != original {
                                return Err(ErrorCode::InvalidInput);
                            }
                        }
                        if *admission == Admission::Blocked && diagnostics.is_empty() {
                            return Err(ErrorCode::InvalidInput);
                        }
                        if *admission == Admission::Ready {
                            if !matches!(effective_selection.model, ObservedValue::Observed(_)) {
                                return Err(ErrorCode::SelectionUnsupported);
                            }
                            for (wanted, actual) in [
                                (&requested.model, &effective_selection.model),
                                (&requested.effort, &effective_selection.effort),
                                (&requested.speed, &effective_selection.speed),
                            ] {
                                if let mine_v1_contracts::RequestedValue::Exact(value) = wanted {
                                    // Model may be a provider alias. Matching the requested alias
                                    // is retained here; resolved identity lives in details.
                                    if !matches!(actual, ObservedValue::Observed(observed) if observed == value)
                                    {
                                        return Err(ErrorCode::SelectionUnsupported);
                                    }
                                }
                            }
                        }
                        Ok(())
                    }
                    MethodResult::Rename { observed_name } => nonempty(observed_name),
                    MethodResult::Submit { native_turn_id, .. } => {
                        if let Some(turn) = native_turn_id {
                            nonempty(turn)?;
                        }
                        Ok(())
                    }
                    MethodResult::Interrupt { observation }
                    | MethodResult::Shutdown { observation } => observation.validate(),
                    MethodResult::Inspect { inspection } => {
                        let Scope::Attempt(scope) = &self.scope else {
                            return Err(ErrorCode::InvalidInput);
                        };
                        inspection.validate(scope)?;
                        let Call::Inspect {
                            binding: requested_binding,
                            ..
                        } = &request.call
                        else {
                            return Err(ErrorCode::InvalidInput);
                        };
                        if let (Some(requested), Some(returned)) =
                            (requested_binding, &inspection.binding)
                        {
                            if requested != returned {
                                return Err(ErrorCode::InvalidInput);
                            }
                        }
                        if let Some(selection) = &inspection.selection {
                            let returned =
                                inspection.binding.as_ref().ok_or(ErrorCode::InvalidInput)?;
                            if selection.project_id != self.project_id
                                || selection.provider != returned.provider
                            {
                                return Err(ErrorCode::InvalidInput);
                            }
                        }
                        // A create_operation_id lookup has no original native binding.
                        // This verifies structural consistency only; controller must
                        // still prove the returned native ID belongs to its durable
                        // create intent before using observations as authority.
                        Ok(())
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TurnReference {
    Native(String),
    Local(OperationId),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SelectionAppliesFrom {
    NextTurn,
    NativeTurn(String),
}
closed_struct!(MeasuredUsage {
    input_tokens: Option<EntityRevision>,
    output_tokens: Option<EntityRevision>,
    currency: Option<String>,
    amount_decimal: Option<String>,
});
identifier!(ApprovalId, OperationId);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EventPayload {
    SessionBound {
        binding: SessionBinding,
        effective_selection: RuntimeEffectiveSelection,
    },
    SelectionObserved {
        effective_selection: RuntimeEffectiveSelection,
        applies_from: SelectionAppliesFrom,
        reason: Option<String>,
    },
    InputDisposition {
        delivery_id: DeliveryId,
        disposition: InputDisposition,
        native_turn_id: Option<String>,
    },
    TurnStarted {
        turn: TurnReference,
    },
    TextDelta {
        stream_id: OperationId,
        chunk_index: EntityRevision,
        text: String,
    },
    ToolStarted {
        tool_call_id: String,
        tool_category: String,
        owned_process: Option<OwnedProcess>,
    },
    ToolFinished {
        tool_call_id: String,
        tool_category: String,
        owned_process: Option<OwnedProcess>,
        status: ToolFinishedStatus,
    },
    ApprovalRequested {
        approval_id: ApprovalId,
        tool_category: String,
        summary: String,
        resource_scope_ref: String,
    },
    TurnFinished {
        status: TurnFinishedStatus,
        delivery_ids: Vec<DeliveryId>,
        failure: Option<RuntimeFailure>,
        usage: Option<MeasuredUsage>,
    },
    QuiescenceObserved {
        observation: QuiescenceObservation,
    },
    Diagnostic {
        code: RuntimeFailureCode,
        message: String,
        evidence_refs: Vec<String>,
    },
    RuntimeExited {
        process: OwnedProcess,
        exit_code: Option<i32>,
        signal: Option<String>,
        observation: QuiescenceObservation,
    },
}
closed_struct!(Event {
    protocol: String,
    project_id: ProjectId,
    scope: AttemptScope,
    connection_id: ConnectionId,
    event_seq: EntityRevision,
    observed_at: UtcMillis,
    request_id: Option<RequestId>,
    operation_id: Option<OperationId>,
    native_session_id: Option<String>,
    native_turn_id: Option<String>,
    event: EventPayload,
});
impl Event {
    /// Reject stale events for state advancement. Store may still retain them
    /// as audit observations using its own old-generation ingestion path.
    pub fn validate_for(
        &self,
        project: &ProjectId,
        active: &AttemptScope,
    ) -> Result<(), ErrorCode> {
        if self.protocol != PROTOCOL {
            return Err(ErrorCode::VersionUnsupported);
        }
        if &self.project_id != project {
            return Err(ErrorCode::InvalidInput);
        }
        if &self.scope != active {
            return Err(ErrorCode::StaleEpoch);
        }
        for value in [&self.native_session_id, &self.native_turn_id]
            .into_iter()
            .flatten()
        {
            nonempty(value)?;
        }
        match &self.event {
            EventPayload::SessionBound {
                binding,
                effective_selection,
            } => {
                binding.validate(active)?;
                effective_selection.validate()?;
                if &effective_selection.project_id != project
                    || binding.provider != effective_selection.provider
                {
                    return Err(ErrorCode::InvalidInput);
                }
            }
            EventPayload::SelectionObserved {
                effective_selection,
                ..
            } => {
                effective_selection.validate()?;
                if &effective_selection.project_id != project {
                    return Err(ErrorCode::InvalidInput);
                }
            }
            EventPayload::QuiescenceObserved { observation }
            | EventPayload::RuntimeExited { observation, .. } => observation.validate()?,
            EventPayload::Diagnostic { message, .. } => nonempty(message)?,
            EventPayload::TurnStarted {
                turn: TurnReference::Native(value),
            } => nonempty(value)?,
            EventPayload::ToolStarted {
                tool_call_id,
                tool_category,
                ..
            }
            | EventPayload::ToolFinished {
                tool_call_id,
                tool_category,
                ..
            } => {
                nonempty(tool_call_id)?;
                nonempty(tool_category)?;
            }
            EventPayload::ApprovalRequested {
                tool_category,
                summary,
                resource_scope_ref,
                ..
            } => {
                nonempty(tool_category)?;
                nonempty(summary)?;
                nonempty(resource_scope_ref)?;
            }
            _ => {}
        }
        Ok(())
    }
}

// Schema root includes structural payloads, not an implemented RPC service.
closed_struct!(RuntimeContractSchema {
    handshake: Handshake,
    request: Request,
    response: Response,
    event: Event,
    snapshot: RuntimeCapabilitySnapshot,
    effective_selection: RuntimeEffectiveSelection,
    failure: RuntimeFailure,
    quiescence: QuiescenceObservation,
    inspection: Inspection,
});

fn nonempty(value: &str) -> Result<(), ErrorCode> {
    if value.trim().is_empty() || value.contains('\0') {
        Err(ErrorCode::InvalidInput)
    } else {
        Ok(())
    }
}

fn absolute_path_syntax(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'))
}

/// A single LF-terminated frame only. JSON parser and semantic validator remain
/// separate so caller authority and trusted context cannot be fabricated here.
pub fn decode_request_frame(frame: &[u8]) -> Result<Request, ErrorCode> {
    if frame.len() > MAX_FRAME_BYTES || !frame.ends_with(b"\n") {
        return Err(ErrorCode::InvalidInput);
    }
    let body = &frame[..frame.len() - 1];
    if body.contains(&b'\n') || body.contains(&b'\r') {
        return Err(ErrorCode::InvalidInput);
    }
    serde_json::from_slice(body).map_err(|_| ErrorCode::InvalidInput)
}
