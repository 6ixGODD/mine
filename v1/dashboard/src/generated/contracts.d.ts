/* Generated from ../contracts/schema.json by json-schema-to-typescript. Do not edit. */

/**
 * Canonical decimal 0..9223372036854775807; consumers additionally enforce this numeric bound.
 *
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "AssignmentEpoch".
 */
export type AssignmentEpoch = string;
/**
 * Canonical decimal 0..9223372036854775807; consumers additionally enforce this numeric bound.
 *
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "EntityRevision".
 */
export type EntityRevision = string;
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "Digest".
 */
export type Digest = string;
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "OperationId".
 */
export type OperationId = string;
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "ProjectId".
 */
export type ProjectId = string;
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "Choices".
 */
export type Choices =
  | {
      kind: "supported";
      values: string[];
    }
  | {
      kind: "unsupported";
    }
  | {
      kind: "unknown";
    };
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "Provider".
 */
export type Provider = "codex" | "claude" | "pi";
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "SnapshotId".
 */
export type SnapshotId = string;
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "EffectOutcome".
 */
export type EffectOutcome = "not_sent" | "rejected" | "accepted" | "unknown";
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "ErrorCode".
 */
export type ErrorCode =
  | "invalid_input"
  | "unauthenticated"
  | "forbidden"
  | "not_found"
  | "revision_conflict"
  | "stale_epoch"
  | "idempotency_conflict"
  | "busy"
  | "pending"
  | "outcome_unknown"
  | "corrupt_store"
  | "version_unsupported"
  | "io_failure"
  | "capability_stale"
  | "selection_unsupported";
/**
 * Canonical decimal 0..9223372036854775807; consumers additionally enforce this numeric bound.
 *
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "EventCursor".
 */
export type EventCursor = string;
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "WorkNodeId".
 */
export type WorkNodeId = string;
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "PrincipalId".
 */
export type PrincipalId = string;
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "WorkState".
 */
export type WorkState =
  | "runnable"
  | "running"
  | "waiting_peer"
  | "waiting_decision"
  | "waiting_resource"
  | "pausing"
  | "paused"
  | "recovering"
  | "failed"
  | "done";
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "MessageKind".
 */
export type MessageKind =
  | "inform"
  | "request_result"
  | "commit"
  | "decline"
  | "progress"
  | "result"
  | "failed"
  | "decision_required"
  | "decision"
  | "resource_request"
  | "proposal"
  | "cancel"
  | "cancelled"
  | "handoff";
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "ObservedValue".
 */
export type ObservedValue =
  | {
      kind: "observed";
      value: string;
    }
  | {
      kind: "unsupported";
    }
  | {
      kind: "unknown";
    };
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "CommitDisposition".
 */
export type CommitDisposition = "applied" | "replayed";
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "Role".
 */
export type Role = "architect" | "executor" | "reviewer";
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "RequestedValue".
 */
export type RequestedValue =
  | {
      kind: "runtime_default";
    }
  | {
      kind: "exact";
      value: string;
    };

/**
 * Root for schema generation; not a network request or a duplicated state store.
 */
export interface ContractSchema {
  command: CommandMeta;
  control_command: ControlCommandMeta;
  discovery: Discovery;
  effect_outcome: EffectOutcome;
  error: ErrorCode;
  frontier: FrontierSnapshot;
  message_kind: MessageKind;
  observed_value: ObservedValue;
  receipt: CommitReceipt;
  role: Role;
  selection: Selection;
  [k: string]: unknown;
}
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "CommandMeta".
 */
export interface CommandMeta {
  assignment_epoch?: AssignmentEpoch | null;
  expected_revision: EntityRevision;
  input_digest: Digest;
  operation_id: OperationId;
  project_id: ProjectId;
}
/**
 * Global commands (including first registration) must not invent a Project ID.
 *
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "ControlCommandMeta".
 */
export interface ControlCommandMeta {
  expected_revision: EntityRevision;
  input_digest: Digest;
  operation_id: OperationId;
}
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "Discovery".
 */
export interface Discovery {
  config_fingerprint: Digest;
  models: ModelCapability[];
  project_id: ProjectId;
  provider: Provider;
  runtime_version: string;
  snapshot_id: SnapshotId;
}
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "ModelCapability".
 */
export interface ModelCapability {
  effort: Choices;
  model: string;
  speed: Choices;
}
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "FrontierSnapshot".
 */
export interface FrontierSnapshot {
  cursor: EventCursor;
  nodes: FrontierNode[];
  paused: boolean;
  project_id: ProjectId;
}
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "FrontierNode".
 */
export interface FrontierNode {
  id: WorkNodeId;
  next_action: string;
  owner: PrincipalId;
  state: WorkState;
  title: string;
  waiting_on?: PrincipalId | null;
}
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "CommitReceipt".
 */
export interface CommitReceipt {
  cursor: EventCursor;
  disposition: CommitDisposition;
  operation_id: OperationId;
  revision: EntityRevision;
}
/**
 * This interface was referenced by `ContractSchema`'s JSON-Schema
 * via the `definition` "Selection".
 */
export interface Selection {
  effort: RequestedValue;
  model: RequestedValue;
  project_id: ProjectId;
  provider: Provider;
  snapshot_id: SnapshotId;
  speed: RequestedValue;
}
