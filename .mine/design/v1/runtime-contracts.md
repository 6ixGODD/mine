# Runtime Adapter Contract — 1.0 Detailed Target

## Status and authority

This leaf refines approved ADR-0028 and [runtime](runtime.md), under the identity
and handoff rules in [identity](identity.md). It defines a new internal contract,
not an installed MINE command or a claim about existing provider APIs. The
foundation supplies common value types and an isolated runtime wire crate;
runtime WP0 independently reviews and binds these artifacts before delivery.
Provider lanes consume both read-only.
This design does not activate 1.0 behavior
in the current 0.2 release. No provider process or billable inference was started
to produce this document. Official sources were refreshed on 2026-10-16.

An isolated **NOT ACCEPTED** foundation now exists at
`v1/runtime/supervisor/contracts/`: request/response/event DTOs, selection reuse,
pure semantic checks, JSONL framing check and Rust-derived schema. It is not
linked into the current product. No process host, provider mapping, TS binding
generator, credential access or live recovery implementation is claimed. Its
README records focused evidence and what validation cannot prove.

Rust owns authority, durable operations, budgets, assignments and recovery.
Adapters translate explicit operations into supported provider calls and report
observations. They cannot accept/reject Plans, change owners, approve ADRs,
silently change models, allocate replacement attempts or retry unknown effects.
This is a three-provider boundary, not an arbitrary plugin or remote shell API.

## Code ownership and dependency direction

Four independently buildable implementation namespaces are reserved:

- `v1/runtime/supervisor/`: Rust attempt state machine, process supervision and
  the composition-facing runtime service. Own `Cargo.toml` and `Cargo.lock`.
- `v1/runtime/codex/`: Rust Codex app-server adapter and protocol fixtures. Own
  `Cargo.toml` and `Cargo.lock`.
- `v1/runtime/claude/`: TypeScript Claude SDK worker. Own `package.json` and
  `package-lock.json`.
- `v1/runtime/pi/`: TypeScript Pi SDK worker. Own `package.json` and
  `package-lock.json`.

The existing `v1/contracts/Cargo.toml`, `src/lib.rs`, `src/bin/export-schema.rs`
and Rust-derived `schema.json` supply common IDs, counters, Provider, Selection,
Discovery, WorkState, Frontier and ErrorCode; they do not supply this full wire.
Runtime WP0 exclusively owns `v1/runtime/supervisor/contracts/`: its private Rust
crate/lock, wire types, schema exporter and generated `schema.json`. A future
single protocol owner adds generated TS bindings and cross-language fixtures;
those are not present in this foundation. It composes common types,
never creates competing Provider/Selection/Choices definitions. WP0 independent
review freezes schema and fixture digests before four parallel lanes start.
Thereafter these contracts are read-only to all lanes; a required contract change
returns to the single protocol owner and repeats the dependent gate. No lane
patches a shared contract to make its own adapter pass. Private manifests do not authorize
worktree-local dependency trees: tool installations remain Root-managed,
immutable after provisioning and keyed by the private lock digest. Root package
and Cargo manifests, their lockfiles, current `src/`, current workflows, current
graph/config and current Skills are outside these lanes. A later integration
owner wires accepted packages into the 1.0 composition root.

## Transport and envelope

The internal protocol name is `mine-runtime/1`. TS workers use owned stdio pipes
with UTF-8, one JSON object followed by LF; stdout contains protocol only and
stderr contains bounded redacted diagnostics. A Rust in-process adapter exposes
the identical DTO semantics through the frozen runtime port. No Web client can send
these frames directly. The controller authenticates its caller and resolves
assignments before constructing a frame; quoting a principal ID is not authority.

All requests contain these exact fields:

| Field | Type and meaning |
|---|---|
| `protocol` | Literal `mine-runtime/1` |
| `request_id` | UUID, unique per dispatch; response correlation only |
| `operation_id` | UUID of the durable controller intent; stable during reconciliation |
| `project_id` | UUID, binds every request and event to one Project |
| `scope` | `project` or `attempt`, as defined below |
| `deadline_ms` | Positive integer, maximum remaining duration at dispatch |
| `call` | Closed `{method,payload}` discriminated object; method is from the table below, payload is method-specific; no arbitrary provider passthrough |

`scope` is a tagged object. `{kind:"project"}` is valid only for discovery.
Attempt scope contains `kind:"attempt"`, `principal_id`, `assignment_id`,
`assignment_revision`, `attempt_id`, `generation`, and `fencing_epoch`.
IDs are canonical UUID strings. Revisions/generations/epochs are canonical
unsigned decimal strings to avoid JavaScript integer truncation. Native provider
IDs are opaque strings: never parse a Codex session ID out of a thread ID.
Attempt scope must match the worker's bound scope before any provider effect.

A response repeats `protocol`, `request_id`, `operation_id`, `project_id` and
`scope`, then `outcome` is either `{status:"ok",result:{method,payload}}` or
`{status:"error",error:...}`. MethodResult repeats the correlated method to
disambiguate its closed payload; response validation checks it against the request.
Requests/responses use exact closed schemas;
unknown methods/fields or protocol versions are rejected before side effects.
Repeated request IDs within a connection are rejected; a correlation ID is not
a provider idempotency guarantee. A lost response is reconciled, not re-sent as
a supposedly idempotent create or prompt.

The implementation fixes and reports bounded frame, pending-request, diagnostic
and queue limits in its handshake; the controller refuses incompatible limits.
The adapter's first frame is a closed `hello` object: `protocol`, `kind:"hello"`,
`connection_id` (UUID), `adapter_version`, `schema_digest`, and `limits`
(`frame_bytes:1048576`, `pending_requests:64`, `control_events:256`,
`diagnostic_tail_bytes:65536`). Before any request the host replies with the
same protocol/connection/adapter-version/schema/limits and `kind:"hello_ack"`. Neither frame has
provider effects. Mismatch or any other pre-handshake frame closes the connection
as `PROTOCOL_INVALID`. These are MINE protocol constants, not provider limits.
Limits cannot change during a connection. Chunk text before serialization;
oversized non-chunkable payloads fail `PAYLOAD_TOO_LARGE`, never silently truncate
decision, control or authority data. Read stdout continuously, respect backpressure,
and reject new work when capacity is exhausted. Do not drop terminal, binding,
selection, approval or failure events. A saturated/disk-failed event sink stops
new admissions and enters reconciliation rather than pretending work completed.

## Runtime context and capability snapshot

`RuntimeContext` contains `provider` (`codex|claude|pi`), `runtime_profile_ref`,
`credential_profile_ref`, `trusted_config_root`, `execution_cwd`,
`config_fingerprint`, and `policy_revision`. Paths are absolute, validated by
the controller and rechecked at the adapter boundary; the execution path must
be the registered worktree. Profile references are opaque local keys, not raw
credentials, environment dumps or caller-chosen secret file paths.

`RuntimeCapabilitySnapshot` contains `discovery` (the common `Discovery`),
`adapter_version`, `protocol_schema_digest`, `credential_profile_ref`,
`observed_at`, `expires_at`, `methods`, `model_details`, and `diagnostics`.
All absolute timestamps are UTC epoch milliseconds encoded as canonical unsigned
decimal strings, matching service contracts; duration fields remain bounded integers.
The snapshot hash covers normalized nonsecret facts; credentials themselves
are never hashed into logs or returned in the snapshot. A profile/config/version
change invalidates the snapshot regardless of TTL. Project scopes cannot share
mutable SDK settings or model-selection objects.

Method capability has `{state:"supported"|"unsupported"|"unknown", reason?}`.
`unknown` requires a diagnostic and is not an empty supported list. Common
`Discovery.models` supplies model/effort/speed using common `Choices`:
`{kind:"supported",values:[...]}`, `{kind:"unsupported"}` or `{kind:"unknown"}`.
`model_details` adds display name, resolved identity when observed, default
observations and diagnostic references keyed by the same native model value.
Supported values are exact provider-native strings; unsupported/unknown cannot
carry invented values. No universal effort enum or static model catalog
is permitted. Enumeration is deterministic runtime work, not an LLM query.
Listing availability does not prove quota, network health or billing entitlement.

## Selection and session binding

Requested selection is the common `Selection` including `project_id`,
`snapshot_id`, `provider`, `model`, `effort`, `speed`. Each value is common
`RequestedValue`: `{kind:"runtime_default"}` or `{kind:"exact",value:...}`.
An omitted choice is invalid; the UI must make defaults explicit. Each request
is checked against the current runtime/config/profile before opening product
write capability. Common validation permits model-specific exact effort/speed
only after resolving an exact model: unresolved runtime-default model cannot
authorize an exact dependent option. No silent fallback, weaker model or ignored
field is allowed.

`RuntimeEffectiveSelection` repeats `project_id`, `snapshot_id`, `provider` and
contains common `ObservedValue` fields `model`, `effort`, `speed`:
`{kind:"observed",value:...}`, `{kind:"unsupported"}` or `{kind:"unknown"}`.
Separate `details` keyed by those three fields carries reasons/evidence references,
optional resolved model identity, and speed availability (for example Claude
cooldown). Do not inject extra fields into the closed common value enum. A requested
alias and the resolved model are separate. Report usage
with optional measured token/currency fields; missing cost is unknown, not zero.

`SessionBinding` contains `provider`, `native_session_id`, optional
`native_thread_id`, `native_storage_ref`, and `attempt_id`/`generation`.
`native_storage_ref` is a controller-approved local reference, not an arbitrary
path supplied by a browser. Native IDs are captured as soon as exposed and
emitted before further work. A session label carries the MINE display ID but
is not authentication or unique reconciliation evidence. No promise is made
that a provider desktop sidebar displays an SDK-created session.

## Methods and payloads

| Method | Payload | Successful result and effect boundary |
|---|---|---|
| `discover` | `context:RuntimeContext` | `snapshot:RuntimeCapabilitySnapshot`; no LLM prompt, Plan mutation or global config write |
| `create` | `context`, `selection`, `display_name` | `binding`, `effective_selection`, `admission`; creates/initializes a session but does not submit project work |
| `resume` | `context`, exact `binding`, `selection` | Validated same binding and effective selection; never most-recent, fork or implicit new session |
| `rename` | exact `binding`, `display_name` | `observed_name`; native unsupported is explicit, MINE display metadata remains independent |
| `submit` | `binding`, `delivery_id`, `mode`, `expected_native_turn_id?`, `content` | `disposition:accepted|queued|handled`, `native_turn_id?`; not task completion |
| `interrupt` | `binding`, `expected_native_turn_id?`, `reason`, `discard_pending:true` | `observation:QuiescenceObservation`; no assignment transfer |
| `inspect` | Exactly one of `binding`, `create_operation_id` | `inspection:{matched:exact|absent|ambiguous|unknown,binding?,selection?,observation?,evidence_refs}` |
| `shutdown` | `binding`, `reason` | `observation`; no deletion of provider transcript, worktree or unfinished edits |

`create`/`resume` return `admission:ready|blocked`; blocked returns diagnostics
and observations without exposing product work. An adapter may start a provider
process to initialize/discover but must withhold the first product prompt and
deny product-mutating startup actions until admission. If the provider cannot
be initialized safely without such actions, managed admission is unsupported.

`submit.mode` is `start|steer|follow_up`. `start` requires an idle admitted
attempt; `steer` requires the matching active turn where the provider exposes
one; `follow_up` stays in the controller's durable queue unless the adapter can
report a provider queue with defined cancellation. `content` is a bounded list
of `{type:"text", text:...}` in this version. Additional media is a future
explicit contract, not arbitrary provider JSON. Deliveries have durable UUIDs;
do not automatically resubmit after `ACCEPTED` or `UNKNOWN` outcomes. Provider
rejection before submission can be retried only by the controller policy.

Native transport details never leak into method names: Codex `turn/steer` and
Pi `steer` are mappings, not parallel public MINE operations. `inspect` returns
`exact` only with native-ID or provider-supported correlation evidence. Name,
timestamp proximity, cwd similarity, or an empty lookup list cannot resolve a
lost create response as safe absence.

## Events and ordering

Events contain `protocol`, `project_id`, attempt `scope` (the AttemptScope fields,
without a redundant `kind` because project events are forbidden), `connection_id`,
`event_seq` (unsigned decimal string), `observed_at`, `event:{kind,payload}`, and
optional `request_id`, `operation_id`, `native_session_id`, `native_turn_id`.
Sequence is per connection; reconnection starts a new connection ID. Rust adds
its durable event cursor after persistence. SDK event order is not wall-clock
order across connections. Stale-generation events remain audit evidence but
cannot advance current work or authorize tools.

Closed event kinds and minimum payloads:

- `session_bound`: `binding`, `effective_selection`.
- `selection_observed`: `effective_selection`, `applies_from` (`next_turn` or
  exact native turn ID), optional provider reason.
- `input_disposition`: `delivery_id`, `disposition`, optional native turn ID.
- `turn_started`: native turn ID if available; otherwise adapter-local turn
  correlation: `turn:{kind:native|local,value:...}`, never fabricated as provider-native.
- `text_delta`: `stream_id`, `chunk_index`, text; not durable task completion.
- `tool_started` / `tool_finished`: `tool_call_id`, redacted tool category,
  optional owned process identity, result status; no credentials/raw command dump.
- `approval_requested`: `approval_id`, tool category, bounded redacted summary,
  `resource_scope_ref` for the controller-owned requested resource scope;
  controller authority decides through its approval
  port, not by a free-form message promising permission.
- `turn_finished`: `status:completed|interrupted|failed`, `delivery_ids`, optional
  structured failure and usage (`input_tokens`, `output_tokens` as nullable
  counters, `currency` and `amount_decimal` as nullable strings); it does not
  imply quiescence or Plan completion. Absence is not zero.
- `quiescence_observed`: `QuiescenceObservation`.
- `diagnostic`: error code, redacted message and evidence references.
- `runtime_exited`: observed process identity, exit code/signal when known and
  final quiescence observation; not proof of remote-tool completion.

Provider SDK callbacks may await a controller approval result on the dedicated
runtime approval port (WP0-owned, not an already implemented foundation API).
That result binds approval ID, attempt/generation,
tool and resource scope, policy revision, expiry and allow/deny. It is not a
general `execute` wire method. Expiry, transport loss or mismatch denies; the
provider lane must not invent a locally approved default.

## Failure and retry semantics

`RuntimeFailure` contains `code`, `stage`, `outcome`, `message`,
`provider_code?`, `retry_after_ms?`, `native_binding?`, `evidence_refs`.
`stage` is `preflight|spawn|initialize|create|resume|select|submit|stream|interrupt|inspect|shutdown`.
`outcome` is `not_sent|rejected|accepted|unknown`; it describes the associated
effect, independently of error classification. A timeout after a write reaches
the pipe is not automatically `rejected`. A known terminal result may coexist
with unknown surviving-tool state.

Codes are `AUTH_REQUIRED`, `ACCESS_DENIED`, `QUOTA_OR_RATE_LIMIT`,
`TRANSIENT_PROVIDER`, `RUNTIME_UNAVAILABLE`, `VERSION_UNSUPPORTED`,
`CONFIG_CONTEXT_MISMATCH`, `SELECTION_UNSUPPORTED`, `CAPABILITY_STALE`,
`SESSION_NOT_FOUND`, `SESSION_CORRUPT`, `PROTOCOL_INVALID`, `PAYLOAD_TOO_LARGE`,
`BACKPRESSURE`, `SPAWN_FAILED`, `PROCESS_EXITED`, `CANCELLED`,
`QUIESCENCE_UNCONFIRMED`, `PERSISTENCE_FAILED`, `UNKNOWN_PROVIDER_FAILURE`.
These are the new `RuntimeFailureCode` enum, not a replacement for common
`ErrorCode`. Supervisor mapping preserves structured runtime details: stale
selection maps to common `CapabilityStale`/`SelectionUnsupported`, unknown effect
to `OutcomeUnknown`, invalid protocol input to `InvalidInput`, and storage
failure to `IoFailure` or `CorruptStore` only when corruption is actually proven.
Provider authentication failure must not be confused with HTTP caller identity.
Unknown provider errors retain a bounded redacted original code and diagnostic;
they do not become transient successes. Schema-invalid frames fail closed.

The controller owns bounded retry/backoff and budgets. Adapters disclose any
SDK-internal automatic retries and expose a way to stop them; they do not add
another unbounded retry loop. Authentication/quota failures never trigger identity
rotation to evade limits. Replacement is a separate durable attempt operation.

## Reconciliation and quiescence

`QuiescenceObservation` contains `observed_at`, `turn_state`
(`idle|running|terminal|unknown`), `pending_inputs`
(`empty|present|unknown`), `automatic_continuation`
(`stopped|possible|unknown`), `owned_processes` (identity and
`running|exited|unknown`), `external_effects` (`settled|pending|unknown`),
`evidence_refs`, and `confidence` (`confirmed|unconfirmed`). Process identity
contains host-local PID plus a launch identity recorded by the supervisor;
an adapter cannot promote a bare PID lookup to verified identity. Empty process
lists are not proof that discovery was complete.

The supervisor records create intent before spawn, persists native binding when
first observed, then selection and admission. Crash between any pair leaves an
explicit recoverable operation. On recovery, reconcile the operation, process
tree, provider binding, Git/worktree and pending durable deliveries. Do not
replay a submitted prompt merely because its result was not saved.

Pause first blocks new admissions/deliveries. Clear pending provider messages,
cancel retries, interrupt the active turn, drain its terminal events, and stop
or account for owned child commands. Only confirmed quiescence plus controller
resource checks can reach PAUSED or admit a replacement. A dead parent PID,
Codex `turn/completed`, Claude result, or Pi `agent_end` alone is insufficient.
Pi requires its settled/idle boundary after queue handling. Unknown external
tools or an unfenceable surviving writer keep the node uncertain and blocked.
This contract is not an OS sandbox against hostile same-user processes.

## Provider mappings and version proof

| Provider | Mapping and mandatory evidence |
|---|---|
| Codex | Own stdio app-server, initialize/initialized handshake; `thread/start/resume/name/set/read`, `turn/start/steer/interrupt`; paginate `model/list`; steer binds `expectedTurnId`. Generate protocol schema from the pinned binary and record its digest. Root-vs-worktree config mapping is a required parity probe, not an invented flag. Background tools require separate accounting. |
| Claude | Official TS `query`, exact `session_id/resume`, `renameSession`, `supportedModels`. No removed V2 createSession. Code >=2.1.275 is required for `projectConfigRoot`; project settings/MCP/Skills follow Root while CLAUDE.md/rules follow cwd. Effort and `settings.fastMode` are separate; fast state/reason reporting requires Code >=2.1.219. Drain interrupted results. Hooks are not assumed to sandbox all tool effects. |
| Pi | Official SDK `createAgentSession` and `ModelRuntime`; use injected settings/resources with explicit tool cwd. RPC reference supplies expected model/thinking/queue semantics. Observe `agent_settled`, not only prompt response/agent_end. No verified universal fast field or configRoot flag: unsupported capabilities stay explicit. |

These are feature minima/current-source facts, not a tested compatibility matrix.
Lock exact SDK/runtime versions and native OS evidence before advertising support.
Pi upstream main reported package 1.3.0 during research; this is not a dependency
pin or proof of a published install. Unsupported config mapping blocks that
adapter's managed admission, not unrelated Projects or other adapters.

## Source register and conformance

All sources below were opened 2026-10-16. Moving documentation must be rechecked
against the implementation's locked versions; record the resulting package,
binary and schema digests with each native receipt.

| Source | Verified claim and consequence |
|---|---|
| [OpenAI app-server](https://learn.chatgpt.com/docs/app-server) | Version-generated schema, typed thread/turn lifecycle, model pagination and interruption; do not expose arbitrary commands to Web |
| [OpenAI config basics](https://learn.chatgpt.com/docs/config-file/config-basic) | Trusted project configuration layers follow Root-to-cwd precedence; parity must be tested |
| [OpenAI config reference](https://learn.chatgpt.com/docs/config-file/config-reference) | `service_tier=fast` maps to priority independently of reasoning |
| [Claude sessions](https://code.claude.com/docs/en/agent-sdk/sessions) | Exact resume/rename and removed TS V2; history is not filesystem isolation |
| [Claude TS reference](https://code.claude.com/docs/en/agent-sdk/typescript.md) | Selection capabilities, projectConfigRoot and fast state/version minima; fetched as official Markdown |
| [Claude configuration](https://code.claude.com/docs/en/agent-sdk/claude-code-features) | settingSources, cwd, managed settings and hook discovery boundaries |
| [Claude permissions](https://code.claude.com/docs/en/agent-sdk/permissions) | canUseTool is not consulted for every already-approved call |
| [Pi SDK](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/sdk.md) | SDK ownership, explicit injection, queues and settled boundary |
| [Pi RPC](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/rpc.md) | LF framing, backpressure and acceptance-versus-completion |
| [Pi commands](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/rpc-commands.md) | Exact model/thinking, abort and queue semantics |
| [Pi configuration](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/configuration.md) | cwd project inputs versus agentDir credentials/models |

Runtime WP0 conformance fixtures cover scope mismatch, stale epochs, malformed
frames, Unicode separators, bounded queues, reordered responses, replayed events,
create-result loss and unsupported selection. Provider fixtures additionally
prove native mapping against recorded versioned schemas. Native acceptance must
prove real create/name/select/send/resume/cancel/reconcile and config parity on
each supported OS. Missing credentials/platform/resource is BLOCKED/NOT_RUN,
never fixture-derived PASS. No observed native testing is claimed by this leaf.
