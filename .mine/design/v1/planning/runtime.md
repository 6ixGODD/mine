# Execution Blueprint — Runtime Protocol, Supervisor and Three Providers

## Status and identity

Provisional allocation is `plan34`, materialized as
`.mine/plan/plan34-runtime.md` with report
`.mine/plan/reports/plan34-runtime-implementation.md` after [activation](index.md).
Codex/Claude/Pi documents are attached work packages, not separate graph nodes.

**UNREGISTERED; NOT EXECUTABLE.** This is a planning blueprint, not an immutable
execution Plan. No Plan ID is allocated here. The coordinator must assign the
canonical ID, register it with scope `v1.0.0`, bind the report path and worktree,
and attach exact accepted predecessor commits. Execution requires 0.2 release
closure and independently reviewed foundation contracts. No new 1.0 runtime is
used to govern its own unaccepted implementation.

This is one runtime Plan: short serial WP0 freezes the protocol, then supervisor,
[Codex](codex.md), [Claude](claude.md) and [Pi](pi.md) run as four disjoint lanes,
followed by WP4 join. Those three documents are lane attachments, not additional
serial graph nodes. Each lane has one owner and a separate output subdirectory.

## Goal, traceability and baseline

Deliver the Rust supervisor that safely admits one attempt, persists/reconciles
unknown outcomes, pauses without waking queued work and replaces only a proven
quiescent writer. A user sees truthful running/pausing/paused/recoverable states
and can resume eligible work without restarting completed steps.

| Requirement | Governing design | Package / evidence |
|---|---|---|
| Principal survives provider replacement | [identity](../identity.md#principal-versus-session) | WP2/WP3 stale-generation and handoff history tests |
| Durable nodes, exact checkpoint continuation | [runtime](../runtime.md#attempts-and-exact-work-nodes) | WP2 crash-boundary tests |
| Pause/restart without duplicate writers | [runtime](../runtime.md#pause-crash-and-restart) | WP3 native process-tree tests |
| Typed, bounded adapter protocol | [runtime contracts](../runtime-contracts.md#transport-and-envelope) | WP1 Rust/TS fixture parity |

Current production core is the existing root Rust package; no supervisor or
SQLite control plane is claimed implemented. Approved 1.0 Design is the target.
This lane does not change root `src/`, root manifests/locks, 0.2 workflows/Skills,
graph/config or accepted ADR bodies. Provider scopes belong only to their named
lane owners; supervisor work cannot edit those scopes.

## Ownership and prerequisites

The supervisor owner writes only `v1/runtime/supervisor/**`. It is a standalone Rust package with
private `Cargo.toml`/`Cargo.lock`; do not enroll it into the current root workspace.
Common `v1/contracts/` and store/assignment/resource ports are read-only. Exact
import paths and accepted hashes must be inserted before Plan registration.
WP0 owns the nested `contracts/` crate and freezes it before parallel work.
The supervisor owns `migrations/project/` and `src/sql/` mappings for
session_attempts, model_selections, runtime_observations and node_checkpoints,
as assigned by the transaction catalogue. It borrows the common transaction;
it does not own connection/commit/backup machinery or another domain's tables.
Provider instances enter through the runtime port;
production dispatch registration belongs to the final integrator.

Independent provider lanes run in parallel only after WP0 independent review.
Supervisor first uses a deterministic fake adapter and injected clock/process
inspector; it does not wait for all three real providers to implement recovery.
Final join requires their accepted conformance evidence plus the accepted store
transaction implementation. There is one owner per report and per shared file.

## Work packages

### WP0 — Independently review and bind existing foundation (short serial gate)

The isolated NOT ACCEPTED `v1/runtime/supervisor/contracts/` already contains
private `Cargo.toml`/`Cargo.lock`, `src/lib.rs`, `src/bin/export-schema.rs`,
Rust-derived `schema.json` and focused `tests/contract.rs`. Do not rebuild a
competing wire package. Review this exact foundation commit/schema and bind it
as predecessor evidence; correct any demonstrated defects through its single
owner before parallel start. Common Provider,
Selection, RequestedValue, ObservedValue and Choices are imported, not copied.
Verify the existing envelopes, method payloads/results, closed events, failures,
reconciliation and quiescence types against [runtime contracts](../runtime-contracts.md).
Add consumable shared fixtures and a version-locked TS binding generator from
the existing Rust-derived schema, owned only by this package. Those two artifacts
are not part of the current delivered foundation. No handwritten second schema.
Exported port signatures return observations, not permission to mutate graph.

Preserve and verify existing local tested constants: 1 MiB maximum UTF-8 frame,
64 pending request correlations, 256 queued control events, 64 KiB retained
diagnostic tail per process. These are MINE limits, not claims about provider
APIs. Text deltas may be chunked/coalesced; control or terminal data may not be
silently dropped. Limit exhaustion stops admission and preserves uncertainty.
Handshake and host tests must compare the same version/digest/limits before use.

Fixtures cover every enum/result branch, common `kind:exact`, unsupported versus
unknown, unsigned counters beyond JavaScript safe integer, UTC-millisecond string
timestamps, duplicate keys/fields, scope mismatch, unknown versions, malformed
UTF-8/JSONL, limit boundaries and lost-result observations. Rust validates and
round-trips the full corpus; each TS lane validates the identical corpus using
the frozen schema. WP0 records generated-file equality and schema/fixture SHA-256;
independent reviewer checks exact wire consistency before opening the four lanes.
No SDK, credential or billable inference is needed for this gate.

Commands: `cargo fmt --manifest-path v1/runtime/supervisor/contracts/Cargo.toml -- --check`,
`cargo clippy --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --all-targets -- -D warnings -W unsafe-code`,
`cargo test --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --all-targets`.
The existing exporter supports `--check` and compares without rewriting source:
`cargo run --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --bin export-schema -- --check`.
The generated TS check is owned by WP0's lock-pinned generator and invoked by
its `scripts/check-bindings.mjs --check`; absence/nonzero is a failure.
Use `<Root>/.mine/build/<exact-plan-id>/runtime-protocol/` for mutable outputs.

Suggested commit: `feat(runtime): bind reviewed wire and cross-language corpus`.

### WP1 — Typed port, process host and admission

Create `src/lib.rs`, `src/admission.rs`, `src/process_host.rs`,
`src/adapter_host.rs`, `tests/wire.rs`, `tests/admission.rs` and fixture support.
Import common DTOs; do not duplicate runtime-wire types. Validate Project,
assignment revision, attempt generation, epoch, registered cwd and capability
snapshot before invoking a provider. Private process launches use explicit
executable/arguments/environment allowlist, hidden windows on Windows, owned
stdio, bounded output and launch identity. Never build a shell command string
from session names or model IDs. No raw arbitrary command endpoint is exported.

Persist intent/reservation through a short store transaction before spawn;
release the transaction before any SDK/network wait. Write native binding and
observed selection before admitting work. Startup hooks unable to obey the
pre-admission boundary return blocked, not ready. Test corrupt context, revoked
assignment, overflow, invalid selection, failed spawn and disk failure both
before and after native ID arrival. Deliver a fake adapter implementing every
wire disposition and failure outcome.

Suggested commit: `feat(runtime): add scoped adapter admission and process host`.

### WP2 — Durable operation and event reconciliation

Create `src/attempt.rs`, `src/reconcile.rs`, `src/events.rs`,
`src/delivery.rs`, `tests/recovery.rs`, `tests/events.rs`.
Implement create/resume/submit intent -> effect -> observed result as durable
operations. `request_id` is not idempotency. A lost create response enters
UNKNOWN; exact lookup/correlation may reconcile, otherwise quarantine until old
writer proof exists. A lost submit response does not resend the prompt. The
delivery state machine binds durable delivery ID to attempt/generation/native
turn where available. Persist events before acknowledging durable consumption;
deduplicate within `(attempt,generation,connection,event_seq)` and retain late
old-generation audit facts without advancing current work.

Snapshot invalidation uses Project, runtime version, config fingerprint,
credential-profile reference, policy revision and expiry. Requested/default and
effective selection stay distinct. Failure classifications and outcome axes are
exactly those in the frozen runtime contract, with explicit mapping to common
ErrorCode rather than a competing enum. Automatic bounded retries use injected
clock and policy; do not rotate principal/session on auth or quota failures.
Native provider automatic retries are observable and must be cancellable.

Test interruption after each persistence/effect boundary, receipt loss, duplicate
wake, reordered events, reconnect sequence reset, stale credential/config,
expired capability snapshot, accepted-but-unpersisted result and full disk.
Prove completed test/checkpoint references are reused, not rerun on restart.

Suggested commit: `feat(runtime): reconcile attempts and durable deliveries`.

### WP3 — Pause, replacement and recovery frontier

Create `src/pause.rs`, `src/replacement.rs`, `src/frontier.rs`,
`tests/quiescence.rs`, `tests/process_tree.rs`, `tests/restart.rs`.
Pause transaction blocks new admissions first. Drain/clear queued provider input,
stop internal retries, interrupt, drain terminal results and account for owned
tool processes. Persist observations and dirty-worktree handoff references; do
not auto-commit, clean, stash or reset unfinished changes. Only controller-checked
quiescence and assignment CAS permit new attempt generation. Parent PID death,
turn completion or timeout alone never grant replacement. Test PID reuse and
surviving child process. Unknown remote tool status remains uncertain.

Restart first reconciles pending operations/resources, then reconstructs eligible
nodes. Explicitly paused Projects stay paused. Default is recoverable-awaiting-
resume; optional preauthorized auto-resume consumes the existing budget. A
one-click resume admits only the eligible frontier, not every node or held peer
wait. Late events may update evidence but cannot wake paused work.

Suggested commit: `feat(runtime): enforce quiescent pause and attempt replacement`.

### WP4 — Integration proof

Create `tests/provider_join.rs` and `scripts/native-smoke.mjs` only as a harness
client, not as another scheduler. Bind accepted store and provider packages at
the final integration point approved by the coordinator. Run multi-Project
fixtures with two contenders, stale assignment epoch, configured fallback,
unknown create, pause during a tool, service restart and bounded retry budget.
Verify no provider directly transitions Plan lifecycle or changes assignments.
Capture process inventory before/after and actual branch/head/dirty manifests.
No placeholder provider is advertised as native-ready.

## Verification and outputs

The registered executor sets `CARGO_TARGET_DIR` to the absolute
`<Root>/.mine/build/<exact-plan-id>/supervisor/cargo-target`; all logs and test artifacts
use that same Plan namespace. Reuse the Root stable toolchain and downloads.
Never create worktree-local `target`, `node_modules` or virtual environments.

Provider siblings use `codex/`, `claude/` and `pi/` output subdirectories under
the same exact Plan namespace; one Plan is not permission to share mutable outputs.
Proposed supervisor package commands to be made real by WP1 (not run in planning):

```text
cargo fmt --manifest-path v1/runtime/supervisor/Cargo.toml -- --check
cargo clippy --manifest-path v1/runtime/supervisor/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code
cargo test --manifest-path v1/runtime/supervisor/Cargo.toml --test admission
cargo test --manifest-path v1/runtime/supervisor/Cargo.toml --test recovery
cargo test --manifest-path v1/runtime/supervisor/Cargo.toml --test quiescence
cargo test --manifest-path v1/runtime/supervisor/Cargo.toml --all-targets --all-features
cargo build --manifest-path v1/runtime/supervisor/Cargo.toml --all-targets --all-features
git diff --check
```

Native process tests run on Windows/Linux/macOS and do not require LLM calls;
real-provider join smoke does and requires an explicit budget/profile tuple.
Mocks prove deterministic state transitions, not native cancellation. Reports
must separate fixture/native/platform-NOT_RUN, exact commands and exit codes,
runtime/adapter/schema/config hashes, request/operation IDs and preserved first
failures. Missing OS or credentials cannot be marked pass. Root quality gates
are performed once by the final integration owner, not redundantly by every lane.

## Research source register

Opened 2026-10-16; moving source contracts must be rechecked at execution.

| Source / applicability | Verified claim and test consequence |
|---|---|
| [OpenAI app-server](https://learn.chatgpt.com/docs/app-server), pinned-binary schema required | Terminal turn event is distinct from background tools; test quiescence separately |
| [Claude Python reference](https://code.claude.com/docs/en/agent-sdk/python), current docs | Interrupt leaves buffered old results; never attribute them to a new turn |
| [Pi RPC](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/rpc.md), current upstream | `agent_settled` is stronger than prompt ack/agent_end; handle backpressure |
| [Pi commands](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/rpc-commands.md), current upstream | Abort does not by itself discard queued work; pause must account for queues |

## Handoff and completion

Deliver private package/lock, fixture/native evidence, unsupported capability
diagnostics, schema digest, and a bounded integration manifest listing imports
and required wiring. The coordinator assigns the implementation report under
the active registered report root before execution. Completion is IMPLEMENTED
only; independent review checks writer exclusivity and retained recovery
evidence. No merge, release or graph acceptance is performed by this lane.
