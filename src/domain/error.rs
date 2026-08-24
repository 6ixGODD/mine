//! Typed error model with stable machine-readable codes.
//!
//! Error variants for repository discovery,
//! design-namespace validation, marker validation, configuration validation,
//! and identity reconciliation. Later plans extend this enum with the
//! execution-graph, plan-lifecycle, revision, lock, distribution, and
//! agent-configuration variants. Each variant exposes a stable [`MineError::code`]
//! string that becomes part of the public JSON error contract.

use std::path::PathBuf;
use thiserror::Error;

/// A MINE domain error carrying a stable machine-readable code.
#[derive(Debug, Error)]
pub enum MineError {
    /// The repository root could not be discovered.
    #[error("repository not found: {detail}")]
    RepositoryNotFound { detail: String },

    /// `docs/design/` exists but is not MINE-managed (no marker, or a foreign
    /// marker). `mine init` resolves this deterministically by moving the
    /// legacy directory aside to a timestamped backup and creating a fresh
    /// managed root; other operations (e.g. `mine-sync`) report this error to
    /// refuse operating on a tree `mine init` has not claimed.
    #[error("design namespace conflict at {path}: existing docs/design is not MINE-managed")]
    DesignNamespaceConflict { path: PathBuf },

    /// The design marker belongs to a different repository identifier than the
    /// one already recorded for this repository.
    #[error(
        "design ownership mismatch: marker repository_id {marker_id} does not match {expected_id}"
    )]
    DesignOwnershipMismatch {
        marker_id: String,
        expected_id: String,
    },

    /// The design marker exists but is structurally invalid.
    #[error("design marker is invalid at {path}: {detail}")]
    DesignMarkerInvalid { path: PathBuf, detail: String },

    /// The configuration repository identifier does not match the design marker.
    #[error(
        "repository identity mismatch: config repository_id {config_id} does not match marker {marker_id}"
    )]
    RepositoryIdMismatch {
        config_id: String,
        marker_id: String,
    },

    /// `.mine/config.toml` exists but cannot be parsed or is missing required
    /// fields. MINE does not silently rewrite an invalid source of truth.
    #[error("configuration is invalid at {path}: {detail}")]
    ConfigInvalid { path: PathBuf, detail: String },

    /// The execution graph is not initialized at the expected path.
    #[error("execution graph not initialized at {path}")]
    GraphNotInitialized { path: PathBuf },

    /// The execution graph failed structural validation.
    #[error("execution graph is invalid: {detail}")]
    GraphInvalid { detail: String },

    /// The execution graph contains a cycle in hard dependencies.
    #[error("execution graph contains a cycle: {cycle}")]
    GraphCycle { cycle: String },

    /// A referenced plan node was not found.
    #[error("plan not found: {plan_id}")]
    PlanNotFound { plan_id: String },

    /// A Plan identity, path, or lineage does not follow the canonical
    /// grammar. The legacy bootstrap node is handled only by the explicit
    /// workspace migration gate.
    #[error("invalid canonical Plan identity {value:?}: {detail}")]
    InvalidPlanIdentity { value: String, detail: String },

    /// An ADR registry entry was not found.
    #[error("ADR not found: {adr_id}")]
    AdrNotFound { adr_id: String },

    /// The ADR registry is absent or cannot be used as a fact source.
    #[error("ADR registry is not initialized at {path}: {detail}")]
    AdrRegistryInvalid { path: PathBuf, detail: String },

    /// An ADR body no longer matches the digest recorded for its proposal or
    /// approval evidence.
    #[error("ADR {adr_id} digest mismatch: expected {expected}, actual {actual}")]
    AdrDigestMismatch {
        adr_id: String,
        expected: String,
        actual: String,
    },

    /// A requested state transition is not allowed by the state machine.
    #[error("invalid transition for plan {plan_id}: {from} -> {to}")]
    InvalidTransition {
        plan_id: String,
        from: String,
        to: String,
    },

    /// A hard predecessor is not yet accepted.
    #[error(
        "plan {plan_id} predecessor {predecessor_id} is not accepted (status: {predecessor_status})"
    )]
    PredecessorNotAccepted {
        plan_id: String,
        predecessor_id: String,
        predecessor_status: String,
    },

    /// Two plans have conflicting write scopes.
    #[error("write scope conflict between plan {plan_a} and plan {plan_b}: {detail}")]
    WriteScopeConflict {
        plan_a: String,
        plan_b: String,
        detail: String,
    },

    /// Compensation rewiring refused because a downstream successor that
    /// references a rejected plan is in an active, accepted, or terminal status
    /// and must not be mutated.
    #[error(
        "rewire of plan {plan_id} refused: successor {successor_id} is not mutable (status: {successor_status})"
    )]
    RewireSuccessorLocked {
        plan_id: String,
        successor_id: String,
        successor_status: String,
    },

    /// The caller's `expected_revision` does not match the stored revision.
    #[error("revision conflict: expected revision {expected}, actual revision {actual}")]
    RevisionConflict { expected: u64, actual: u64 },

    /// The exclusive graph lock could not be acquired within the timeout.
    #[error("lock timeout acquiring {path}: {detail}")]
    LockTimeout { path: PathBuf, detail: String },

    /// Required implementation or review evidence is missing.
    #[error("evidence missing for plan {plan_id}: {detail}")]
    EvidenceMissing { plan_id: String, detail: String },

    /// A filesystem input/output error.
    #[error("input/output error: {0}")]
    Io(#[from] std::io::Error),

    // --- agent installer / managed state / doctor / transaction ---
    /// A write target for the agent installer resolves outside the injected
    /// configuration root (path traversal, symlink/junction escape).
    /// Exit: GATE (3). Stable code `MINE_AGENT_PATH_ESCAPE`.
    #[error(
        "agent installer path escape: {candidate:?} is outside the configuration root {root:?}: {detail}"
    )]
    AgentPathEscape {
        candidate: PathBuf,
        root: PathBuf,
        detail: String,
    },

    /// The agent installer refused to overwrite a pre-existing user-owned
    /// resource whose ownership cannot be proven.
    /// Exit: VALIDATION (4). Stable code `MINE_AGENT_COLLISION`.
    #[error("agent installer collision: {target:?} already exists and is not MINE-owned: {detail}")]
    AgentCollision { target: PathBuf, detail: String },

    /// Managed installation state is malformed, foreign, or failed validation.
    /// Exit: VALIDATION (4). Stable code `MINE_AGENT_MANAGED_STATE_INVALID`.
    #[error("managed installation state is invalid: {detail}")]
    AgentManagedStateInvalid { detail: String },

    /// An agent installer operation was requested for an unsupported or
    /// undetected harness.
    /// Exit: GATE (3). Stable code `MINE_AGENT_UNSUPPORTED`.
    #[error("unsupported or undetected agent: {detail}")]
    AgentUnsupported { detail: String },

    /// A mandatory configuration backup could not be created or verified before
    /// mutation. No external mutation is performed when this fires.
    /// Exit: GATE (3). Stable code `MINE_AGENT_BACKUP_FAILED`.
    #[error("agent installer backup failed: {target:?}: {detail}")]
    AgentBackupFailed { target: PathBuf, detail: String },

    /// An incomplete (interrupted) installation transaction was detected. The
    /// operation recovers or reports an actionable recovery state.
    /// Exit: PARTIAL (7). Stable code `MINE_AGENT_TRANSACTION_INCOMPLETE`.
    #[error("agent installer transaction incomplete: {detail}")]
    AgentTransactionIncomplete { detail: String },

    /// A rollback attempt failed after an installation operation failure. The
    /// pending-transaction record is preserved with evidence so doctor or a
    /// later invocation can recover. The error distinguishes the original
    /// operation failure from the rollback failure.
    /// Exit: PARTIAL (7). Stable code `MINE_AGENT_ROLLBACK_FAILED`.
    #[error(
        "agent installer rollback failed (original: {original_code}, rollback: {rollback_detail})"
    )]
    AgentRollbackFailed {
        original_code: String,
        original_message: String,
        rollback_detail: String,
    },
    /// An external dependency failed (network, GitHub Release lookup, binary
    /// download). Used by `mine setup` / `mine update` lifecycle operations.
    #[error("external dependency failure: {detail}")]
    ExternalDependency { detail: String },
}

impl MineError {
    /// Returns the stable machine-readable error code for this variant.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::RepositoryNotFound { .. } => "MINE_REPOSITORY_NOT_FOUND",
            Self::DesignNamespaceConflict { .. } => "MINE_DESIGN_NAMESPACE_CONFLICT",
            Self::DesignOwnershipMismatch { .. } => "MINE_DESIGN_OWNERSHIP_MISMATCH",
            Self::DesignMarkerInvalid { .. } => "MINE_DESIGN_MARKER_INVALID",
            Self::RepositoryIdMismatch { .. } => "MINE_REPOSITORY_ID_MISMATCH",
            Self::ConfigInvalid { .. } => "MINE_CONFIG_INVALID",
            Self::GraphNotInitialized { .. } => "MINE_GRAPH_NOT_INITIALIZED",
            Self::GraphInvalid { .. } => "MINE_GRAPH_INVALID",
            Self::GraphCycle { .. } => "MINE_GRAPH_CYCLE",
            Self::PlanNotFound { .. } => "MINE_PLAN_NOT_FOUND",
            Self::InvalidPlanIdentity { .. } => "MINE_PLAN_ID_INVALID",
            Self::AdrNotFound { .. } => "MINE_ADR_NOT_FOUND",
            Self::AdrRegistryInvalid { .. } => "MINE_ADR_REGISTRY_INVALID",
            Self::AdrDigestMismatch { .. } => "MINE_ADR_DIGEST_MISMATCH",
            Self::InvalidTransition { .. } => "MINE_INVALID_TRANSITION",
            Self::PredecessorNotAccepted { .. } => "MINE_PREDECESSOR_NOT_ACCEPTED",
            Self::WriteScopeConflict { .. } => "MINE_WRITE_SCOPE_CONFLICT",
            Self::RewireSuccessorLocked { .. } => "MINE_REWIRE_SUCCESSOR_LOCKED",
            Self::RevisionConflict { .. } => "MINE_REVISION_CONFLICT",
            Self::LockTimeout { .. } => "MINE_LOCK_TIMEOUT",
            Self::EvidenceMissing { .. } => "MINE_EVIDENCE_MISSING",
            Self::Io(_) => "MINE_IO",
            Self::AgentPathEscape { .. } => "MINE_AGENT_PATH_ESCAPE",
            Self::AgentCollision { .. } => "MINE_AGENT_COLLISION",
            Self::AgentManagedStateInvalid { .. } => "MINE_AGENT_MANAGED_STATE_INVALID",
            Self::AgentUnsupported { .. } => "MINE_AGENT_UNSUPPORTED",
            Self::AgentBackupFailed { .. } => "MINE_AGENT_BACKUP_FAILED",
            Self::AgentTransactionIncomplete { .. } => "MINE_AGENT_TRANSACTION_INCOMPLETE",
            Self::AgentRollbackFailed { .. } => "MINE_AGENT_ROLLBACK_FAILED",
            Self::ExternalDependency { .. } => "MINE_EXTERNAL_DEPENDENCY",
        }
    }
}

/// A specialized [`Result`] for MINE domain operations.
pub type MineResult<T> = std::result::Result<T, MineError>;
