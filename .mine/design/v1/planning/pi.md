# Execution Blueprint — Pi Runtime Adapter

## Status and objective

**UNREGISTERED; NOT EXECUTABLE.** No ID is allocated. The coordinator binds this
as a Pi lane attachment of the single runtime delivery Plan, not a separate
serial node. Start requires 0.2 closure, independently accepted foundation
and the runtime protocol WP0 review. Read [runtime](../runtime.md),
[identity](../identity.md) and [runtime contracts](../runtime-contracts.md).

Deliver Pi-managed sessions with real Project configuration/model discovery,
exact session binding, safe queued communication and pause/recovery observations.
No global static model list, fabricated fast support, arbitrary external-session
attachment or implicit writer replacement is permitted.

## Current evidence, private ownership and parallelism

The existing root package distributes Pi Skills; it is not the proposed runtime
adapter. Current upstream redirects `badlogic/pi-mono` to `earendil-works/pi`.
The opened main manifest reported `@earendil-works/pi-coding-agent` 1.3.0; this
is source evidence, not a tested package pin or an instruction to install latest.

Only `v1/runtime/pi/**` is writable: private npm manifest/lock, strict TS quality
configuration, `src/{worker,adapter,context,models,session,events,permissions,
reconcile}.ts`, `test/`, `fixtures/`, `scripts/{check,native-smoke}.mjs`,
`compatibility.md`. Common `v1/contracts/` and accepted runtime WP0
`v1/runtime/supervisor/contracts/` wire/schema/bindings are read-only.
Root manifest/locks, current source, setup
support list, graph/config, Skills and other provider lanes are forbidden.
Supervisor/Codex/Claude can work in parallel after WP0; no shared npm lock is
introduced. Root tool provisioning stays serialized, while builds/tests have
separate Plan outputs.

## Work packages

### WP1 — Locked SDK and bounded worker

Pin the official package and its exact transitive lock. Record package/runtime
version/hash, supported OS and imported exported declarations. Use
`createAgentSession` and the current `ModelRuntime`, not obsolete inferred
ModelRegistry factory arguments. An SDK worker is selected because Pi has no
verified equivalent of Claude's separate projectConfigRoot flag; explicit SDK
injection is required to control the approved context boundary. The CLI RPC
documents supply behavioral tests, not permission to invent unsupported SDK calls.

Import shared wire bindings, validate envelopes/scopes, own stdio and bounded
diagnostics, and map only approved methods. Test fake SDK startup errors,
extension errors, malformed frames, slow stream consumers, session ID emitted
before reply loss, cancelled new-session/switch events and version mismatch.
No ordinary test loads user extensions or credentials. Suggested commit:
`feat(runtime-pi): add isolated official SDK worker`.

### WP2 — Root resource mapping and model selection

Build a per-Project context using a Root-owned trusted settings/resource snapshot,
approved agentDir/profile and explicit worktree tool cwd. `ModelRuntime` owns
credential/model discovery; package examples expose `getAvailable()` and
`getModel(provider,id)`. Project `.pi` settings/resources normally follow cwd,
while auth/models live in agentDir. Explicitly map settings, provider extensions,
MCP, Skills, context instructions and tool paths; a settingsManager override
alone is not proof all other resources followed Root. Preserve relative-path
origins rather than copying environment trees or flattening instructions.

Runtime discovery runs without an LLM prompt and includes configured/custom
models with usable authentication. Snapshot keys include Project/config/profile/
runtime. Validate and observe actual provider/model identity and thinking level.
Use the current supported-level API/contract rather than a global enum; RPC
`get_available_thinking_levels` is the verified behavior to match. Unsupported
thinking choices fail before work. No verified universal fast/service-tier field
exists in the inspected RPC/settings contract: basic adapter reports unsupported,
not silently normal speed. An extension requires separately versioned conformance
evidence before advertising such support.

Test two Project profiles, Root/worktree conflicting `.pi` sentinels, injected
custom provider, unavailable credential, stale model list, resumed model no
longer available and SDK fallback warning. A fallback warning blocks admission
unless an exact prior controller fallback policy permits the effective tuple.
Suggested commit: `feat(runtime-pi): bind root resources and live model selection`.

### WP3 — Sessions, queues and settled boundary

Create persistent sessions through the supported SessionManager/runtime APIs;
ephemeral sessions cannot satisfy exact durable resume. Capture ID/file reference
immediately. Resume exact approved stored binding, not current-directory latest.
Rename through supported session API and observe its value; map cancelled
switch/create distinctly from successful activation. Avoid direct writes to
provider JSONL when supported manager methods exist.

Subscribe before submitting to avoid missing fast completion. During active
streaming, explicitly select steer or follow-up; omission must not guess.
Steer enters after current assistant tool calls, follow-up after pending work.
Keep durable input identity in MINE and do not treat SDK `queued/handled` as
work complete. `agent_end` is not the final idle boundary: retries, compaction
and queued input can continue. Observe `agent_settled` and actual queues.

For pause, clear/account for queued inputs, cancel retry, abort and wait for idle/
settled, then account for known child effects. Keep queue-removal evidence so
durable delivery can later be resumed exactly once under controller policy.
Do not let cleared provider input become silently lost MINE work. A `tool_call`
extension routes permission checks to the controller; exceptions fail closed.
Inspect missing/corrupt sessions and lost-create responses as common contract
observations. Worker never allocates a replacement attempt or transfers owner.

Tests: queued steering/follow-up during abort, input handled by extension without
a run, cancelled session switch, retries after agent_end, output backpressure,
permission-hook error, surviving bash command, lost create/submit response,
late old-generation event and resume exact ID. Suggested commit:
`feat(runtime-pi): reconcile queues and quiescent session control`.

### WP4 — Native three-platform proof

Use the supervisor's explicit credential-profile and budget grant, temporary
Project/worktree, and actually discovered tuple. Prove create/name, no-inference
model list, bounded worktree-only write, exact resume, steering/follow-up, queue
clearing, cancellation, settled boundary and Root resource parity. Open a saved
session through the documented Pi inspection/resume route only when doing so
cannot create a second writer. Do not automatically `/share` transcripts or
upload bug reports. Run Windows/Linux/macOS independently for claimed support.
Fixture success is not native support and Linux containers are not macOS proof.

## Verification and mutable outputs

Implement lane-local `scripts/check.mjs --task format|lint|typecheck|test|build|all`.
The runner requires `MINE_ROOT` and registered `MINE_PLAN_ID`, verifies its private
lock digest against a Root-owned immutable dependency installation, and uses a
Plan-specific build projection under `<Root>/.mine/build/<id>/pi/`. It does not
install dependencies. The Root provisioner serializes dependency setup, while
mutable build/test artifacts are never shared between Plans. ESM dependency
resolution uses explicit resolved paths/build projection, not assumed NODE_PATH.
No worktree-local node_modules, venv or target is permitted.

Private configuration enables strict TypeScript, formatter, ESLint and a
deterministic test runner without blanket ignores. The following are new lane
gate entrypoints to implement, not commands already run:

```text
node v1/runtime/pi/scripts/check.mjs --task format
node v1/runtime/pi/scripts/check.mjs --task lint
node v1/runtime/pi/scripts/check.mjs --task typecheck
node v1/runtime/pi/scripts/check.mjs --task test
node v1/runtime/pi/scripts/check.mjs --task build
node v1/runtime/pi/scripts/native-smoke.mjs --case lifecycle
node v1/runtime/pi/scripts/native-smoke.mjs --case config-parity
node v1/runtime/pi/scripts/native-smoke.mjs --case queue-cancellation
git diff --check
```

Native runner requires controller-issued `MINE_NATIVE_PROFILE_REF` and
`MINE_NATIVE_BUDGET_REF`; missing prerequisites exit nonzero. Offline tests must
never silently invoke a provider. Record schema/package/config fingerprints,
selection requested/effective, original failure, platform, exact command/exit,
operation IDs, bounded redacted events and surviving-process checks. Do not
claim unknown usage cost as zero.

## Research source register

All opened 2026-10-16. Source main version is a research observation; choose and
record a reproducible package+lock only after checking its matching declarations.

| Source | Verified claim and test implication |
|---|---|
| [Pi package](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/package.json) | Current package identity/main1.3.0; do not assume old npm name |
| [SDK](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/sdk.md) | createAgentSession, injection boundaries, subscriptions/settled semantics |
| [Model example](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/examples/sdk/02-custom-model.ts) | ModelRuntime getAvailable/getModel; not static built-in selection |
| [Runtime services](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/src/core/agent-session-services.ts) | cwd-bound settings/resources and agentDir model/auth paths; mapping must be explicit |
| [RPC](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/rpc.md) | Stream backpressure and acceptance versus settled completion |
| [Commands](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/rpc-commands.md) | Queue/abort/model/thinking/session-name behavior |
| [Configuration](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/configuration.md) | Root input origins versus execution cwd |
| [Extensions](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/extensions.md) | tool_call can block; failures fail-safe; no OS sandbox claim |

## Acceptance and integration handoff

Deliver package/lock, exact compatibility matrix, common-wire conformance,
recorded fixtures, real native receipts and explicit unsupported/unknown fields.
No shared-file changes or global user-setting writes. Coordinator assigns the
report path before registration. IMPLEMENTED is not ACCEPTED; final integrator
consumes only independently reviewed artifacts and rechecks the combined
single-writer/restart loop. Missing parity or native quiescence remains a named
blocker, not an invitation to fabricate a configRoot flag or silently drop queues.
