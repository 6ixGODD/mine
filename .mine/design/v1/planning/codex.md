# Execution Blueprint — Codex Runtime Adapter

## Status, goal and start gate

**UNREGISTERED; NOT EXECUTABLE.** No ID is allocated. The coordinator must bind
canonical parent runtime Plan/worktree/report identity and exact predecessor
commits before registration under `v1.0.0`. This is its parallel Codex lane
attachment, not a separate serial node. Start only after 0.2 release closure,
independent foundation review and runtime WP0 protocol review. Do not use the
future 1.0 product to bootstrap this work.

Implement one owned stdio app-server adapter so MINE can discover the actual
Project runtime, create/name/select, submit/steer, resume an exact native thread,
observe cancellation, and reconcile uncertain outcomes without a second writer.
Governed by [runtime](../runtime.md), [identity](../identity.md) and the exact
[runtime contract](../runtime-contracts.md). This is not generic OpenAI API use
or a promise of visibility in every Codex GUI.

## Current evidence and exclusive scope

Current core remains the root Rust package and has no claimed 1.0 managed-session
adapter. Write only `v1/runtime/codex/**`: standalone `Cargo.toml`/`Cargo.lock`,
`src/{lib,transport,handshake,models,configuration,session,events,reconcile}.rs`,
`tests/{wire,models,configuration,session,recovery,native}.rs`,
`fixtures/`, `schemas/`, `scripts/` and `compatibility.md`.
Common `v1/contracts/` and runtime WP0's `v1/runtime/supervisor/contracts/`
DTO/schema/bindings are read-only. Do not edit root manifests,
current `src/`, setup support lists, graph, root workflows, Skills or another
runtime lane. Root configuration files are read-only even during conformance.
Private Cargo locks prevent dependency-write contention; build output is still
Plan/lane-isolated under Root. Supervisor, Claude and Pi proceed in parallel
only after WP0 review.

## Work packages

### WP1 — Pin and implement the transport

Record exact Codex executable version/hash and generate its schema using the
official `app-server generate-json-schema` route into Plan-owned outputs; copy
the reviewed schema fixture into this lane's `schemas/`, not shared contracts.
Fail unsupported methods/fields explicitly. Implement owned process/stdio and
initialize -> initialized exactly once per connection. Correlate responses by
ID while concurrently draining notifications and bounded stderr; validate JSONL
framing, maximum frames, unknown enum/field behavior and EOF partial records.
No direct `thread/shellCommand`, `process/spawn`, config-writing or generic RPC
endpoint is exported. Experimental API opt-in is capability-specific and recorded;
do not blanket-enable unknown fields to make tests green.

Tests: early request before handshake, duplicate initialize, reordered response,
unsolicited notification, malformed/truncated frame, pipe closure, slow consumer,
unknown schema and executable absence. Suggested commit:
`feat(runtime-codex): add pinned app-server transport`.

### WP2 — Project configuration and selection

Implement `discover` via actual runtime model enumeration, consuming every cursor
page. Preserve native model values/defaults and supported reasoning efforts.
No static list and no sample-model assumptions. Speed follows the pinned config/
protocol and actual capability/account evidence; `service_tier=fast` maps to
priority, but model/list effort alone does not establish priority entitlement.
Selection is requested/effective separately, and unsupported/unknown are distinct.

Project config layers are trusted-root-to-cwd; a separate Root configuration
parameter has not been verified. Implement and document a supported mapping
using only the pinned runtime's actual config read/override facilities. Do not
manually parse TOML as a substitute for actual runtime resolution. Native parity
fixture supplies contradictory Root/worktree sentinels and different Project
profiles. Verify effective provider/model/effort/speed and tool execution cwd.
If mapping cannot be proven, return `CONFIG_CONTEXT_MISMATCH` and block managed
admission for this adapter instead of ignoring project settings or writing global
defaults. Shared environment/toolchain reuse is not permission to share mutable
selection state between Projects.

Tests: pagination, duplicate/native identity, stale snapshot, profile/config
change, unsupported effort/tier, explicit runtime-default, default resolved to
different model, untrusted project and Root/worktree mismatch. Suggested commit:
`feat(runtime-codex): bind project-scoped selection and configuration`.

### WP3 — Session, messaging and recovery

Map create/resume/rename/inspect to actual `thread/start`, `thread/resume`,
`thread/name/set`, `thread/read` under the pinned schema. Save returned thread
and session IDs separately. `excludeTurns`/pagination is preferred for inspection;
do not load entire transcripts just to obtain status. Exact resume must never
use latest-thread heuristics. Return native binding as soon as known.

Map start to `turn/start`; steer requires `expectedTurnId` and cannot smuggle
model/cwd overrides. Follow-up remains durable controller work until an idle
turn is admitted. Map provider events to the common closed event schema without
claiming a prompt ack means completion. Turn interruption's `{}` ack is not the
terminal event; drain `turn/completed` and account for background tools through
supported inspect/terminate facilities and host process ownership. Unsupported
background inspection yields unconfirmed quiescence, not empty inventory.

Inspect/reconcile accepts only exact native IDs or real provider correlation.
Lost create response with no binding is UNKNOWN; same title/cwd/time is not an
exact match. Test lost response after native creation, failed required MCP init,
resume missing thread, active turn mismatch, interruption result race, late old-
generation events and surviving child commands. No adapter-owned replacement.
Suggested commit: `feat(runtime-codex): map session lifecycle and recovery observations`.

### WP4 — Native and cross-platform evidence

Implement a `native` test target whose `native_contract_smoke` test requires an
explicit profile and budget from the supervisor harness. Without them the
explicit native command fails with a clear prerequisite diagnostic; ordinary
offline tests never silently initiate inference. Native smoke creates a temporary
fixture Project/worktree, lists actual models, chooses a supported tuple, names
the thread, sends one bounded task, verifies output only in worktree, resumes
exact ID, steers/interrupts a bounded tool and confirms no surviving writer.
Record requested/effective values and fast unsupported/unknown accurately.
Run on every claimed Windows/Linux/macOS target. A Docker Linux fixture is not
macOS evidence. Failure to show a native GUI link is an explicit unavailable
navigation state, not an adapter failure if exact native inspection works.

## Verification commands and output discipline

Commands below are future package gates, not claims they ran during planning.
Set `CARGO_TARGET_DIR=<Root>/.mine/build/<exact-plan-id>/codex/cargo-target` first;
reports/logs/schema-generation output use that Plan namespace. Reuse Root tools
and safe downloads, never worktree-local target/environment trees.

```text
cargo fmt --manifest-path v1/runtime/codex/Cargo.toml -- --check
cargo clippy --manifest-path v1/runtime/codex/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code
cargo test --manifest-path v1/runtime/codex/Cargo.toml --test wire
cargo test --manifest-path v1/runtime/codex/Cargo.toml --test models
cargo test --manifest-path v1/runtime/codex/Cargo.toml --test recovery
cargo test --manifest-path v1/runtime/codex/Cargo.toml --all-targets --all-features
cargo build --manifest-path v1/runtime/codex/Cargo.toml --all-targets --all-features
cargo test --manifest-path v1/runtime/codex/Cargo.toml --test native native_contract_smoke -- --ignored --exact --nocapture
git diff --check
```

The ignored native marker prevents accidental paid execution; the separate
native command is mandatory and its ignored/default-suite status is never a
PASS. Private lockfile changes are generated by Cargo. Full root integration
gates belong to the integrator once all lanes join, not this leaf package.

## Research source register

Opened 2026-10-16. No minimum Codex build is guessed: lock the tested runtime
and generated schema, report unsupported older versions explicitly.

| Source | Verified implication |
|---|---|
| [OpenAI app-server](https://learn.chatgpt.com/docs/app-server) | Handshake, version-generated schema, thread/turn methods, paged models, expectedTurnId, background tools and experimental limitations |
| [OpenAI configuration basics](https://learn.chatgpt.com/docs/config-file/config-basic) | Trusted project layers resolve down to cwd; mandatory parity test |
| [OpenAI configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference) | service_tier and effort are separate; no global defaults mutation |
| [OpenAI SDK](https://learn.chatgpt.com/docs/codex-sdk) | SDK is a valid automation route, but app-server is the selected richer client integration route |

## Handoff and acceptance

Deliver private package, exact runtime/schema digests, offline fixtures,
per-platform native receipts, redacted failure classifications and integration
instructions. Coordinator assigns the implementation report location before
registration. No SDK installation or billable smoke is authorized by this
blueprint itself. IMPLEMENTED requires all claimed native capabilities proven;
independent reviewer decides ACCEPTED. Unknown config relocation, missing
quiescence proof or skipped native platforms remain named blockers.
