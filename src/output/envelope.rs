//! Stable JSON envelope for machine-consumable CLI output.
//!
//! Implements the JSON output contract in
//! `docs/design/interfaces/cli-contract.md`:
//!
//! ```json
//! {
//!   "ok": true,
//!   "command": "plan.start",
//!   "repository": "D:/work/project",
//!   "workspace_id": "8dcd1df5-...",
//!   "revision_before": 7,
//!   "revision_after": 8,
//!   "data": {},
//!   "warnings": []
//! }
//! ```
//!
//! Errors use the same envelope with `ok: false`, a stable `error.code`, a
//! human message, and structured details. The envelope is serialized with
//! sorted, stable keys so JSON output is deterministic (important for golden
//! tests and diff-based consumption by Skills/MCP).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The stable command identifier emitted in the envelope (e.g. `plan.start`).
/// Names follow `<group>.<verb>` so a single command surface is exposed to
/// Skills and MCP independently of the human CLI spelling.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandRef(pub &'static str);

impl CommandRef {
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

/// A structured warning carried in a successful envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

/// A successful envelope payload, serialized with sorted keys for determinism.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub ok: bool,
    pub command: &'static str,
    pub repository: Option<String>,
    pub workspace_id: Option<String>,
    pub revision_before: Option<u64>,
    pub revision_after: Option<u64>,
    pub data: Value,
    pub warnings: Vec<Warning>,
}

impl Envelope {
    /// Builds a success envelope for `command` with optional repository and
    /// workspace context and a `data` object.
    #[must_use]
    pub fn success(command: &'static str) -> Self {
        Self {
            ok: true,
            command,
            repository: None,
            workspace_id: None,
            revision_before: None,
            revision_after: None,
            data: Value::Object(Map::new()),
            warnings: Vec::new(),
        }
    }

    /// Sets the repository root path.
    #[must_use]
    pub fn with_repository(mut self, repo: impl Into<String>) -> Self {
        self.repository = Some(repo.into());
        self
    }

    /// Sets the active workspace identifier.
    #[must_use]
    pub fn with_workspace_id(mut self, id: impl Into<String>) -> Self {
        self.workspace_id = Some(id.into());
        self
    }

    /// Sets the revision before/after a mutation (both when an optimistic
    /// write occurred).
    #[must_use]
    pub fn with_revision(mut self, before: u64, after: u64) -> Self {
        self.revision_before = Some(before);
        self.revision_after = Some(after);
        self
    }

    /// Sets the `data` object.
    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    /// Expose the generated ordinary-write identity and actual progress. A
    /// logical no-op has no checkpoint; it never fabricates one for reporting.
    #[must_use]
    pub fn with_checkpoint(
        mut self,
        operation: Option<crate::domain::lifecycle_operation::LifecycleOperation>,
    ) -> Self {
        if let Value::Object(data) = &mut self.data {
            data.insert(
                "operation".into(),
                serde_json::to_value(operation).expect("typed operation serializes"),
            );
        }
        self
    }

    /// Adds a warning.
    #[must_use]
    pub fn with_warning(mut self, code: impl Into<String>, message: impl Into<String>) -> Self {
        self.warnings.push(Warning {
            code: code.into(),
            message: message.into(),
        });
        self
    }

    /// Serializes the envelope to a deterministic JSON string (sorted keys,
    /// no trailing newline). Deterministic ordering makes golden tests stable
    /// across `serde_json` versions.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root: BTreeMap<&'static str, Value> = BTreeMap::new();
        root.insert("ok", Value::Bool(self.ok));
        root.insert("command", Value::String(self.command.to_string()));
        root.insert(
            "repository",
            match &self.repository {
                Some(r) => Value::String(r.clone()),
                None => Value::Null,
            },
        );
        root.insert(
            "workspace_id",
            match &self.workspace_id {
                Some(r) => Value::String(r.clone()),
                None => Value::Null,
            },
        );
        root.insert(
            "revision_before",
            match self.revision_before {
                Some(r) => Value::Number(r.into()),
                None => Value::Null,
            },
        );
        root.insert(
            "revision_after",
            match self.revision_after {
                Some(r) => Value::Number(r.into()),
                None => Value::Null,
            },
        );
        root.insert("data", self.data.clone());
        root.insert(
            "warnings",
            serde_json::to_value(&self.warnings).unwrap_or(Value::Null),
        );
        // BTreeMap iteration is sorted by key, so serialization is deterministic.
        serde_json::to_string(&root).expect("envelope is JSON-serializable")
    }
}

/// A structured error payload, serialized into the same envelope shape with
/// `ok: false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorEnvelope {
    pub command: &'static str,
    pub repository: Option<String>,
    pub workspace_id: Option<String>,
    pub error: EnvelopeError,
}

/// The `error` object of an error envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeError {
    pub code: &'static str,
    pub message: String,
    pub details: Value,
}

impl EnvelopeError {
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Value::Object(Map::new()),
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
}

impl ErrorEnvelope {
    #[must_use]
    pub fn new(command: &'static str, error: EnvelopeError) -> Self {
        Self {
            command,
            repository: None,
            workspace_id: None,
            error,
        }
    }

    #[must_use]
    pub fn with_repository(mut self, repo: impl Into<String>) -> Self {
        self.repository = Some(repo.into());
        self
    }

    #[must_use]
    pub fn with_workspace_id(mut self, id: impl Into<String>) -> Self {
        self.workspace_id = Some(id.into());
        self
    }

    /// Serializes to deterministic JSON (sorted keys).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root: BTreeMap<&'static str, Value> = BTreeMap::new();
        root.insert("ok", Value::Bool(false));
        root.insert("command", Value::String(self.command.to_string()));
        root.insert(
            "repository",
            match &self.repository {
                Some(r) => Value::String(r.clone()),
                None => Value::Null,
            },
        );
        root.insert(
            "workspace_id",
            match &self.workspace_id {
                Some(r) => Value::String(r.clone()),
                None => Value::Null,
            },
        );
        let err: Vec<(&'static str, Value)> = vec![
            ("code", Value::String(self.error.code.to_string())),
            ("message", Value::String(self.error.message.clone())),
            ("details", self.error.details.clone()),
        ];
        let err_map: Map<String, Value> =
            err.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        root.insert("error", Value::Object(err_map));
        root.insert("data", Value::Object(Map::new()));
        root.insert("warnings", Value::Array(Vec::new()));
        if self.error.code == "MINE_GRAPH_COMMITTED_REPAIR_NEEDED" {
            for key in ["revision_before", "revision_after"] {
                root.insert(key, self.error.details[key].clone());
            }
        }
        serde_json::to_string(&root).expect("error envelope is JSON-serializable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn success_envelope_is_deterministic_and_stable() {
        let env = Envelope::success("graph.validate")
            .with_repository("/repo")
            .with_workspace_id("ws-1")
            .with_revision(7, 8)
            .with_data(json!({"plans": 9}))
            .with_warning("MINE_DESIGN_SIZE_SOFT_LIMIT", "index.md exceeds soft limit");
        let s = env.to_json();
        // Sorted keys: command, data, ok, repository, revision_after,
        // revision_before, warnings, workspace_id.
        assert_eq!(
            s,
            r#"{"command":"graph.validate","data":{"plans":9},"ok":true,"repository":"/repo","revision_after":8,"revision_before":7,"warnings":[{"code":"MINE_DESIGN_SIZE_SOFT_LIMIT","message":"index.md exceeds soft limit"}],"workspace_id":"ws-1"}"#
        );
        // Deterministic: re-serialize is byte-identical.
        assert_eq!(s, env.to_json());
    }

    #[test]
    fn error_envelope_is_deterministic_and_has_code() {
        let err = ErrorEnvelope::new(
            "plan.start",
            EnvelopeError::new("MINE_PREDECESSOR_NOT_ACCEPTED", "predecessor not accepted")
                .with_details(json!({"plan_id":"02","predecessor_id":"01"})),
        )
        .with_repository("/repo");
        let s = err.to_json();
        assert!(s.contains(r#""ok":false"#));
        assert!(s.contains(r#""code":"MINE_PREDECESSOR_NOT_ACCEPTED""#));
        assert!(s.contains(r#""command":"plan.start""#));
        assert_eq!(s, err.to_json());
    }

    #[test]
    fn null_fields_serialize_as_null_not_omitted() {
        let env = Envelope::success("status");
        let s = env.to_json();
        assert!(s.contains(r#""repository":null"#));
        assert!(s.contains(r#""workspace_id":null"#));
        assert!(s.contains(r#""revision_before":null"#));
        assert!(s.contains(r#""revision_after":null"#));
    }
}
