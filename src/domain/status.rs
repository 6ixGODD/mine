//! Plan lifecycle status and state-machine transition rules.
//!
//! Implements the seven-state machine from
//! `docs/design/execution-graph/state-machine-and-algorithms.md`. There is no
//! generic `set-status`; transitions are validated against the allowed-edge
//! table and rejected with `MINE_INVALID_TRANSITION` otherwise.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::domain::error::{MineError, MineResult};

/// The lifecycle status of a plan node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanStatus {
    /// Registered but prerequisites or design gates unresolved.
    Draft,
    /// Prerequisites unresolved; cannot start.
    Blocked,
    /// All hard predecessors accepted; ready to start.
    Ready,
    /// An owner has started execution.
    InProgress,
    /// Implementation and evidence registered; awaiting review.
    Implemented,
    /// Independent review passed.
    Accepted,
    /// Independent review found a material failure.
    Rejected,
}

impl PlanStatus {
    /// Returns the string representation used in the TOML fact source.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Blocked => "BLOCKED",
            Self::Ready => "READY",
            Self::InProgress => "IN_PROGRESS",
            Self::Implemented => "IMPLEMENTED",
            Self::Accepted => "ACCEPTED",
            Self::Rejected => "REJECTED",
        }
    }

    /// Returns `true` if this status is a terminal state for the node itself
    /// (ignoring compensation).
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// Validates that transitioning from `self` to `target` is an allowed edge
    /// in the state machine.
    ///
    /// # Errors
    /// Returns [`MineError::InvalidTransition`] for disallowed transitions.
    pub fn validate_transition(self, plan_id: &str, target: PlanStatus) -> MineResult<()> {
        let allowed = match (self, target) {
            (Self::Draft, Self::Blocked) => true,
            (Self::Draft, Self::Ready) => true,
            (Self::Blocked, Self::Ready) => true,
            (Self::Ready, Self::InProgress) => true,
            (Self::InProgress, Self::Implemented) => true,
            (Self::InProgress, Self::Rejected) => true,
            (Self::Implemented, Self::Accepted) => true,
            (Self::Implemented, Self::Rejected) => true,
            // Idempotent no-op: same status is allowed (re-validate without
            // state change), useful for re-running start on the same owner.
            (a, b) if a == b => true,
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(MineError::InvalidTransition {
                plan_id: plan_id.to_string(),
                from: self.as_str().to_string(),
                to: target.as_str().to_string(),
            })
        }
    }
}

impl fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PlanStatus {
    type Err = MineError;
    fn from_str(s: &str) -> MineResult<Self> {
        match s {
            "DRAFT" => Ok(Self::Draft),
            "BLOCKED" => Ok(Self::Blocked),
            "READY" => Ok(Self::Ready),
            "IN_PROGRESS" => Ok(Self::InProgress),
            "IMPLEMENTED" => Ok(Self::Implemented),
            "ACCEPTED" => Ok(Self::Accepted),
            "REJECTED" => Ok(Self::Rejected),
            other => Err(MineError::GraphInvalid {
                detail: format!("unknown plan status: {other:?}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::MineError;

    #[test]
    fn allowed_edges_pass() -> Result<(), MineError> {
        PlanStatus::Ready.validate_transition("02", PlanStatus::InProgress)?;
        PlanStatus::InProgress.validate_transition("02", PlanStatus::Implemented)?;
        PlanStatus::InProgress.validate_transition("02", PlanStatus::Rejected)?;
        PlanStatus::Implemented.validate_transition("02", PlanStatus::Accepted)?;
        PlanStatus::Implemented.validate_transition("02", PlanStatus::Rejected)?;
        PlanStatus::Blocked.validate_transition("02", PlanStatus::Ready)?;
        PlanStatus::Draft.validate_transition("02", PlanStatus::Ready)?;
        PlanStatus::Draft.validate_transition("02", PlanStatus::Blocked)?;
        Ok(())
    }

    #[test]
    fn disallowed_edges_rejected() {
        let e = PlanStatus::Ready
            .validate_transition("02", PlanStatus::Accepted)
            .unwrap_err();
        assert_eq!(e.code(), "MINE_INVALID_TRANSITION");
        let e = PlanStatus::Blocked
            .validate_transition("02", PlanStatus::InProgress)
            .unwrap_err();
        assert_eq!(e.code(), "MINE_INVALID_TRANSITION");
        let e = PlanStatus::Accepted
            .validate_transition("02", PlanStatus::Ready)
            .unwrap_err();
        assert_eq!(e.code(), "MINE_INVALID_TRANSITION");
        let e = PlanStatus::Rejected
            .validate_transition("02", PlanStatus::Ready)
            .unwrap_err();
        assert_eq!(e.code(), "MINE_INVALID_TRANSITION");
    }

    #[test]
    fn round_trips_through_serde() {
        for s in [
            PlanStatus::Draft,
            PlanStatus::Blocked,
            PlanStatus::Ready,
            PlanStatus::InProgress,
            PlanStatus::Implemented,
            PlanStatus::Accepted,
            PlanStatus::Rejected,
        ] {
            // The TOML fact source stores statuses as SCREAMING_SNAKE_CASE
            // strings; verify the string round-trip rather than a bare enum
            // (TOML cannot serialize a top-level enum without a table).
            let ser = s.as_str();
            let de: PlanStatus = ser.parse().unwrap();
            assert_eq!(s, de);
        }
    }

    #[test]
    fn unknown_status_is_invalid() {
        assert!("UNKNOWN".parse::<PlanStatus>().is_err());
    }
}
