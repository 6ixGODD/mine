# Provider Runtime, Selection and Recovery — Approved 1.0 Target (ADR-0028)

## Managed execution boundary

Rust owns scheduling, permissions, operations and event normalization. A bounded
TypeScript worker uses official SDKs where necessary; provider transport adapters
do not decide Plan lifecycle. Adapter capabilities include create/resume/rename,
model discovery, selection, stream, send, interrupt, quiescence and session lookup.
Unsupported capabilities are explicit, not simulated success. Every call has
correlation/timeout/backpressure and nonsecret diagnostics. No arbitrary SDK
filesystem/command endpoint is exposed directly to the Web.

| Provider | Verified current route | Required limitation |
|---|---|---|
| Codex | SDK startThread/resumeThread; richer app-server stdio for thread/name/set, model/list, turn events | Pin installed protocol; capability probe; experimental features are not assumed stable |
| Claude Code | Agent SDK query, exact session_id/resume, renameSession; Query.supportedModels | Do not use removed V2 createSession; permission callbacks alone are not a complete sandbox |
| Pi | SDK createAgentSession or owned JSONL RPC; get_state, new_session/switch_session, set_session_name | Successful prompt acknowledgment is not completion; observe agent_settled |

Official sources and package/version caveats are in [sources](sources.md).
Owned child sessions are controllable. An arbitrary already-open external client
session is not automatically attachable; it must enroll through supported tools/
bridge and prove identity. Do not claim every provider's desktop sidebar will
display an SDK-created session. Show tested native ID/resume route or honest
unavailable deep link. Inspecting via another client must not create a second
writer for the same assignment. OpenCode adapters/setup/docs are removed in1.0,
not0.2; preserve historical evidence instead of rewriting provider names.

## Project-local model, effort and speed selection

Discovery uses the **actual provider runtime in this Project's effective config
and credential context**, not a global catalog or a manually parsed settings file.
For example project `.claude/settings.json` can differ from another Project.
Credential/profile references are keys, never raw secrets. Cache key includes
Project ID, runtime version, trusted config-root fingerprint, profile reference
and expiry; invalidate when any changes. Queries are deterministic runtime calls,
not LLM turns. Different projects cannot accidentally share mutable model state.

Separate selection fields: provider, requested model value, resolved model identity,
effort, speed/service mode, runtime/config discovery snapshot and effective values.
`gpt-6-astra-high fast` is human shorthand, not a literal universal model ID or a
promise that every runtime supports it. Use model capabilities for effort choices;
do not invent a common enum for Claude effort, Codex reasoning effort and Pi
thinking level. Fast may be a paid service tier, different runtime option or
unsupported. Do not implement it by silently picking a weaker model. Defaults
are explicit “runtime default” selections with the observed effective result.

Codex model/list advertises per-model supportedReasoningEfforts; enumerate pages.
Claude Query.supportedModels returns runtime values/aliases and capabilities;
preserve requested alias and resolved model. Pi get_available_models plus
set_model/get_state supplies available provider/model identity; SDK ModelRuntime
can enumerate current authenticated availability. Listing does not guarantee
remaining quota, network or successful execution. Missing list/unknown capability
is a visible diagnostic, not an empty list or static fallback catalog.

The picker has separate Provider, Model, Effort and Speed controls with provenance,
refresh, supported options and cost warnings. Create/resume/replacement revalidates
the exact tuple and passes supported options, then records effective acknowledgments.
If runtime cannot report a field, mark unverified and prove adapter behavior before
advertising support. No silent downgrade/ignored field. Fallback model/effort/speed
requires an explicit prior policy and cost ceiling; retain requested vs actual.
Changing selection is audited per attempt/turn, never retroactively relabels work.

Project configuration root and execution cwd are distinct: trusted Root determines
selected project settings; Plan worktree determines product/tool writes. Claude
documents projectConfigRoot (Code>=2.1.275) for settings while CLAUDE.md/rules still
follow cwd; set settingSources deliberately. Pi has SDK injection boundaries but
no verified equivalent root flag; Codex layering must be verified against pinned
runtime. Implement explicit supported mapping and parity probes; do not invent
flags, copy another environment tree or silently discover a different model list
after changing cwd. Unsupported mapping blocks that adapter's managed admission,
not the whole Project or a request for users to debug it repeatedly.

Claude model capabilities include supportedEffortLevels/supportsFastMode; pass
effort separately and use settings.fastMode, then observe fast_mode_state and
disable/cooldown reason. Do not invoke a blind /fast toggle that may switch model.
Pi exposes get_available_thinking_levels/set_thinking_level and observed state;
no universal fast/service-tier field was verified in its RPC contract. Show
unsupported/unknown until a concrete provider extension has conformance evidence.
Codex speed mapping must follow pinned runtime config/protocol, separate from
model/list effort. Never assume the model list alone proves priority entitlement.
Current official configuration documents service_tier=fast mapping to request
priority, independently of reasoning effort. Feature enablement is not account/
model availability; only advertise validated supported tiers, and do not write
global user defaults merely to configure one Project session.

## Attempts and exact work nodes

Work node means an execution package/step and its durable continuation, not only
a Plan status or a chat thread. It records Plan/body/Design binding, assignment
epoch, last committed checkpoint, Git/worktree observations, pending effects,
waits, evidence cursors, selected runtime tuple and next admissible action.
Node states distinguish runnable, running, waiting-peer/decision/resource, pausing,
paused, recovering, failed and done; Plan lifecycle remains its own authority.

Before provider creation persist intent and reserve the assigned attempt. Capture
native ID immediately. A lost creation reply is UNKNOWN: reconcile by supported
lookup/correlation or quarantine possible duplicate; no blind replacement with
potential old writer. Resume exact ID, never most-recent. On failure use bounded
retry/backoff, then replace attempt under [handoff](identity.md) rules. Auth/quota
failures do not justify rotating identities to evade provider limits. Session
creation is cheap relative to work, but tokens and overlapping writers are not.

## Pause, crash and restart

Pause is durable policy: immediately stop new admissions, then quiesce current
runtime/tools at a supported safe boundary. Until confirmed show PAUSING, not
PAUSED. Persist dirty-file manifests, unfinished effects and waits; no forced
checkpoint commit of arbitrary work. Pi cancellation must clear pending queues
and observe idle/settled; other adapters prove their own cancellation semantics.
Messages/results can arrive during pause but cannot wake execution.

On service/OS restart recover global directory operations first, then open selected
Project stores, reconcile journals/actual processes/Git/resources, and reconstruct
the runnable frontier from durable nodes. A dead PID is not sufficient proof
against PID reuse or surviving remote tools. Do not rerun completed effect/test
steps just because in-memory session context vanished. Unknown facts stay explicit.
Default restart shows “recoverable, awaiting resume”; auto-resume is an optional
preauthorized policy within remaining budget. Explicit user-paused projects never
auto-resume on reboot. One click resumes the eligible frontier, not every node.

## Acceptance

Each provider on supported native platforms: create/name/select model+effort+speed,
send/receive/wait, exact resume, cancellation and replacement, project/worktree
config parity. Inject full disk, failed spawn, lost create reply, interrupted tool,
changed credentials/model, stale capability list, unsupported tuple, duplicate
wake and OS restart. Preserve handoff evidence and never admit two writers.
Metrics: active child/process count, queue age, retries, checkpoint age, resumable
nodes and measured token/cost fields; unavailable cost is UNKNOWN, not zero.
