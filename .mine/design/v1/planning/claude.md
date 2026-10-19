# Execution Blueprint — Claude Code Runtime Adapter

## Status and goal

**UNREGISTERED; NOT EXECUTABLE.** No Plan ID is allocated. This blueprint is
the Claude lane attachment of the single runtime delivery Plan; the coordinator
assigns canonical identity and report location. Execution waits for 0.2 release
closure, independently accepted foundation, and the runtime protocol WP0 review.
No current root code, Skills or graph is migrated by this blueprint.

Deliver a bounded TypeScript worker using the official Claude Agent SDK that
preserves MINE identity, requested/effective selection, exact session resume,
messages and truthful interruption/reconciliation. Governed by
[runtime](../runtime.md), [identity](../identity.md) and
[runtime contracts](../runtime-contracts.md). SDK permission callbacks are not
an OS sandbox and are not authority to accept Plans or self-approve ADRs.

## Baseline, owned namespace and dependencies

Write only `v1/runtime/claude/**`: private `package.json`/`package-lock.json`,
strict TypeScript/formatter/linter configuration, `src/{worker,adapter,context,
models,session,events,permissions,reconcile}.ts`, `test/`, `fixtures/`,
`scripts/{check,native-smoke}.mjs` and `compatibility.md`.
Current root package.json has distribution scripts, not a Claude runtime worker.
No working runtime implementation or native evidence is assumed.

Read-only dependencies are `v1/contracts/` common IDs/selection DTOs and the
accepted `v1/runtime/supervisor/contracts/` WP0 wire/schema/bindings, with exact accepted hashes inserted
before registration. The supervisor owns the protocol; changes require its
single owner, not a private adapter fork. Do not edit root Cargo/npm locks,
current source/workflows/setup/provider support lists, another adapter, or global
Claude configuration. Runtime supervisor, Codex and Pi may proceed in parallel
after WP0; a lane's failure does not authorize another lane to edit its files.

## Work packages

### WP1 — Private worker and version-locked SDK bridge

Create the private npm package with exact SDK and tool versions recorded by its
own lockfile. Verify the bundled Claude executable version rather than inferring
it from package version. Required feature floor: Code >=2.1.275 for
`projectConfigRoot`; fast state/disabled reason requires >=2.1.219. These minima
do not replace a tested SDK/binary matrix. Use `query`, not removed experimental
V2 createSession. Enforce common wire framing/scope/limits before SDK effects.
The worker accepts only the closed MINE methods, with stdout protocol-only and
bounded redacted stderr. Startup failures return structured errors without a
fabricated session ID. Capture native ID immediately when init/result exposes it.

Supply a fake SDK façade for deterministic tests: delayed init, ID before failure,
lost create response, split Unicode frames, orphan response, dependency load
failure and callback cancellation. Tests cannot accidentally use real credentials
or network. Suggested commit: `feat(runtime-claude): add bounded official SDK worker`.

### WP2 — Root settings and model/effort/speed

Resolve trusted Root configuration separately from registered worktree cwd:
`projectConfigRoot=Root`, `cwd=worktree`, deliberate `settingSources`. Preserve
managed settings. CLAUDE.md/rules continue to follow cwd as the SDK documents;
do not claim they all move to Root. Hooks/helper scripts/stdio MCP processes can
start in Root: pre-admission controls and native parity probes must prove they
do not become uncontrolled product writers. Pass only approved profile/credential
references from the host; do not ingest a browser-supplied env map or secret path.

Model discovery uses live `Query.supportedModels()` after safe initialization,
without a user/LLM prompt. Preserve `value`, optional `resolvedModel`, supported
effort levels and fast support. Creation/resume validates exact snapshot/config
and requested tuple. `Options.effort` is separate from `settings.fastMode`;
do not invoke blind `/fast`, which may switch models. Observe initialization/
result fast state/reason and record cooldown/unknown honestly. If effective model
or effort cannot be verified, do not claim a confirmed field or silently execute
under a different tuple. Provider fallback requires controller policy and budget.

Tests cover two Projects with distinct Root settings, worktree conflicting
sentinels, removed credential/profile, alias changing resolved model, unsupported
effort, fast disabled by organization, cooldown, SDK ignoring a field and a
runtime too old for configRoot. Suggested commit:
`feat(runtime-claude): verify project context and requested selection`.

### WP3 — Streaming session and controlled tool boundary

Use streaming input with a controller-owned queue; no work prompt is yielded
before binding, selection and admission are durable. `create` initializes;
`submit` separately begins work. If safe initialization without a product effect
cannot be established for the pinned SDK, report blocked capability. Resume
uses the exact stored `session_id` and never `continue:true`; rename uses the
official helper only when available. Native transcript storage remains provider-
owned and is not copied into the MINE DB as a second authority.

Map message/tool/result events to shared wire. Delivery ack means accepted/queued,
not business completion. A callback approval request goes to the controller's
scope-bound approval port; transport loss/expiry denies. `canUseTool` alone is
insufficient because earlier allow rules can bypass it; use the pinned SDK's
documented complete hook/tool restrictions and test them. Do not claim this
guards arbitrary same-user filesystem access. Never enable bypass permissions
merely to get a native smoke green.

Interrupt then drain the interrupted result before processing a new query.
Prevent queued input from automatically starting after pause. Account for SDK
internal retries, tool/background processes and late old-attempt callbacks.
Return unconfirmed quiescence if a writer cannot be proven stopped. Lost create
or submit reply stays unknown unless exact provider evidence reconciles it;
native session list/name coincidence is not sufficient. No replacement decision
lives in this worker. Suggested commit:
`feat(runtime-claude): map streaming control and quiescence evidence`.

### WP4 — Real provider conformance

Run bounded native tests on each supported Windows/Linux/macOS environment using
an explicit authorized API/cloud credential profile and supervisor budget.
Do not assume third-party MINE sessions can consume Claude subscription login/
rate limits: official SDK docs require API/approved methods absent prior approval.
Native test must prove create/name, actual discovered selection, bounded worktree
write, exact resume, message queue/cancel, interrupted result drain, configRoot
parity and no live duplicate writer. Native client inspection is a tested resume
route; do not promise Desktop/sidebar availability. Do not export/share full
transcripts to a third party as part of testing.

## Verification and environment contract

The lane implements `scripts/check.mjs` with tasks `format`, `lint`, `typecheck`,
`test`, `build`, `all`. It reads `MINE_ROOT` and `MINE_PLAN_ID`, validates the
registered Plan output namespace, and runs pinned tools against an isolated build
projection under `<Root>/.mine/build/<id>/claude/`. Root-managed tool provisioning
is serialized by its owner and keyed by this package's lock digest. The check
runner verifies that digest, uses the immutable dependency installation, and
never performs installation itself. ESM resolution must use an explicit resolved
tool root/build projection; do not rely on `NODE_PATH` working for ESM. Build
projection bytes and output mappings are recorded for evidence attribution.

Use strict TypeScript with no blanket any/ignore suppressions; formatter,
ESLint, compiler and deterministic test runner are actual private package scripts
invoked by the check runner. Test logs distinguish SDK fakes from real provider
events. Commands below are implementation deliverables, not installed commands
or planning-time passes:

```text
node v1/runtime/claude/scripts/check.mjs --task format
node v1/runtime/claude/scripts/check.mjs --task lint
node v1/runtime/claude/scripts/check.mjs --task typecheck
node v1/runtime/claude/scripts/check.mjs --task test
node v1/runtime/claude/scripts/check.mjs --task build
node v1/runtime/claude/scripts/native-smoke.mjs --case lifecycle
node v1/runtime/claude/scripts/native-smoke.mjs --case config-parity
node v1/runtime/claude/scripts/native-smoke.mjs --case interruption
git diff --check
```

Native runner requires controller-issued `MINE_NATIVE_PROFILE_REF` and
`MINE_NATIVE_BUDGET_REF`; no raw secret or caller-imposed authorization comes from
the CLI arguments. Missing/bad prerequisites exit nonzero with a redacted reason.
Ordinary `check --task test` uses no inference. Explicit native commands are
mandatory for advertised capabilities; fixture passes cannot substitute them.
All mutable output is Plan-specific; no worktree-local node_modules/venv/target.

## Research source register

Sources opened 2026-10-16; implementation pins exact SDK/CLI and reopens docs.

| Source / applicability | Verified claim and implementation consequence |
|---|---|
| [Claude sessions](https://code.claude.com/docs/en/agent-sdk/sessions), current | session_id/resume/rename; V2 removed in TS0.3.142; no recent-session heuristic |
| [Claude TS reference](https://code.claude.com/docs/en/agent-sdk/typescript.md), Code2.1.275+ root flag | Current model capabilities, effort, fast state and separate configRoot; tests use exact locked API |
| [Streaming input](https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode), current | Persistent input stream and queue handling; acceptance differs from completion |
| [Permissions](https://code.claude.com/docs/en/agent-sdk/permissions), current | Early auto-allow can bypass canUseTool; do not claim callback-only fencing |
| [Configuration](https://code.claude.com/docs/en/agent-sdk/claude-code-features), current | Root/cwd and managed sources; native parity probes |
| [Fast mode](https://code.claude.com/docs/en/fast-mode), current | Fast can switch model if blindly toggled and costs more; capability/cost validation |
| [Quickstart](https://code.claude.com/docs/en/agent-sdk/quickstart), current | Authentication terms; no assumption of subscription reuse |

## Acceptance and handoff

Provide lock/binary/schema/config digests, offline cases, per-platform native
receipts, denied/unsupported/unknown cases, no-secret log audit and exact integration
imports. Record first failures, exact commands and exit codes. Missing credentials
or platform is BLOCKED/NOT_RUN, never passed. No shared-file change, installation,
LLM turn or external publication is performed by this blueprint creation.
The executor concludes IMPLEMENTED; independent review and integration own
acceptance, with no self-granted Plan lifecycle transitions from the worker.
