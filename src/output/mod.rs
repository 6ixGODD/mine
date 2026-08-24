//! Output layer: stable JSON envelope and human-readable output, plus the
//! exit-code contract from `docs/design/interfaces/cli-contract.md`.
//!
//! The CLI reports results in two modes:
//! - default: concise human-readable text;
//! - `--format json`: a stable, deterministic JSON envelope (see [`envelope`]).
//!
//! Exit codes are a public contract:
//! - `0` success;
//! - `2` invalid invocation;
//! - `3` repository/branch/namespace/workspace gate failure;
//! - `4` validation failure;
//! - `5` revision or lock conflict;
//! - `6` external dependency or Git evidence failure;
//! - `7` partial success requiring repair;
//! - `1` unexpected internal failure.

pub mod envelope;
pub mod human;

/// Process exit codes (public contract, see module docs).
pub mod exit_code {
    /// Success.
    pub const SUCCESS: i32 = 0;
    /// Invalid invocation (bad arguments, unknown command).
    pub const USAGE: i32 = 2;
    /// Repository/branch/namespace/workspace gate failure.
    pub const GATE: i32 = 3;
    /// Validation failure (graph/design/plan structural validation).
    pub const VALIDATION: i32 = 4;
    /// Revision or lock conflict.
    pub const CONFLICT: i32 = 5;
    /// External dependency or Git evidence failure.
    pub const EXTERNAL: i32 = 6;
    /// Partial success requiring repair (e.g. TOML written but render failed).
    pub const PARTIAL: i32 = 7;
    /// Unexpected internal failure.
    pub const INTERNAL: i32 = 1;
}

use crate::domain::error::MineError;

/// Maps a [`MineError`] to the public exit-code contract.
///
/// This is the single source of truth for error -> exit-code mapping so the
/// CLI, tests, and any future MCP bridge agree on the public surface.
#[must_use]
pub fn exit_code_for(err: &MineError) -> i32 {
    match err {
        // Repository / branch / namespace / workspace gate failures.
        MineError::RepositoryNotFound { .. }
        | MineError::DesignNamespaceConflict { .. }
        | MineError::DesignOwnershipMismatch { .. }
        | MineError::DesignMarkerInvalid { .. }
        | MineError::RepositoryIdMismatch { .. }
        | MineError::EvidenceMissing { .. }
        | MineError::RewireSuccessorLocked { .. } => exit_code::GATE,
        // Configuration is treated as a validation gate (the source of truth
        // is present but invalid).
        MineError::ConfigInvalid { .. } => exit_code::VALIDATION,
        // Execution-graph structural/validation failures.
        MineError::GraphNotInitialized { .. }
        | MineError::GraphInvalid { .. }
        | MineError::GraphCycle { .. }
        | MineError::PlanNotFound { .. }
        | MineError::InvalidPlanIdentity { .. }
        | MineError::AdrNotFound { .. }
        | MineError::AdrRegistryInvalid { .. }
        | MineError::AdrDigestMismatch { .. }
        | MineError::InvalidTransition { .. }
        | MineError::PredecessorNotAccepted { .. }
        | MineError::WriteScopeConflict { .. } => exit_code::VALIDATION,
        // Revision / lock conflicts.
        MineError::RevisionConflict { .. } | MineError::LockTimeout { .. } => exit_code::CONFLICT,
        // Agent installer path-escape / backup-failed are gate
        // failures (write denied or mutation blocked by a failed backup).
        MineError::AgentPathEscape { .. }
        | MineError::AgentUnsupported { .. }
        | MineError::AgentBackupFailed { .. } => exit_code::GATE,
        // Agent installer collision / malformed managed state are validation
        // gate failures (ownership or state provenance cannot be established).
        MineError::AgentCollision { .. } | MineError::AgentManagedStateInvalid { .. } => {
            exit_code::VALIDATION
        }
        // An interrupted transaction is a partial-success state requiring
        // recovery (exit 7).
        MineError::AgentTransactionIncomplete { .. } => exit_code::PARTIAL,
        MineError::AgentRollbackFailed { .. } => exit_code::PARTIAL,
        // I/O and infrastructure failures are treated as external/gate
        // failures: filesystem is an external dependency. Partial-success
        // cases (TOML written, render failed) are surfaced as GraphInvalid with
        // a "render" hint and mapped to PARTIAL by the caller via the hint;
        // here we map generic GraphInvalid to VALIDATION and rely on the
        // command layer to elevate the partial-success case.
        MineError::Io(_) => exit_code::EXTERNAL,
        MineError::ExternalDependency { .. } => exit_code::EXTERNAL,
    }
}

/// Human-readable rendering of an error's primary message, suitable for the
/// default (non-JSON) mode. Kept here so the CLI and tests share one phrasing.
#[must_use]
pub fn human_error_message(err: &MineError) -> String {
    format!("{}", err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::MineError;
    use std::path::PathBuf;

    #[test]
    fn maps_gate_errors_to_3() {
        assert_eq!(
            exit_code_for(&MineError::RepositoryNotFound { detail: "x".into() }),
            exit_code::GATE
        );
        assert_eq!(
            exit_code_for(&MineError::DesignNamespaceConflict {
                path: PathBuf::from("docs/design")
            }),
            exit_code::GATE
        );
        assert_eq!(
            exit_code_for(&MineError::EvidenceMissing {
                plan_id: "03".into(),
                detail: "no report".into()
            }),
            exit_code::GATE
        );
        assert_eq!(
            exit_code_for(&MineError::RewireSuccessorLocked {
                plan_id: "05".into(),
                successor_id: "06".into(),
                successor_status: "IN_PROGRESS".into()
            }),
            exit_code::GATE
        );
    }

    #[test]
    fn maps_validation_errors_to_4() {
        assert_eq!(
            exit_code_for(&MineError::GraphInvalid {
                detail: "dup".into()
            }),
            exit_code::VALIDATION
        );
        assert_eq!(
            exit_code_for(&MineError::InvalidTransition {
                plan_id: "03".into(),
                from: "DRAFT".into(),
                to: "ACCEPTED".into()
            }),
            exit_code::VALIDATION
        );
        assert_eq!(
            exit_code_for(&MineError::ConfigInvalid {
                path: PathBuf::from(".mine/config.toml"),
                detail: "bad".into()
            }),
            exit_code::VALIDATION
        );
    }

    #[test]
    fn maps_conflict_errors_to_5() {
        assert_eq!(
            exit_code_for(&MineError::RevisionConflict {
                expected: 1,
                actual: 2
            }),
            exit_code::CONFLICT
        );
        assert_eq!(
            exit_code_for(&MineError::LockTimeout {
                path: PathBuf::from(".mine/locks/x.lock"),
                detail: "timed out".into()
            }),
            exit_code::CONFLICT
        );
    }

    #[test]
    fn maps_io_to_external_6() {
        assert_eq!(
            exit_code_for(&MineError::Io(std::io::Error::from(
                std::io::ErrorKind::NotFound
            ))),
            exit_code::EXTERNAL
        );
    }
}
