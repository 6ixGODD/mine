# Experimental Autonomy and Decision Policy — Approved 1.0 Target (ADR-0030)

## Explicit opt-in

Off by default, clearly labeled experimental and potentially expensive. User binds
requirements digest, Project, scope/risk boundaries, model/effort/speed policies,
credentials, budget/concurrency/time/retry ceilings and permitted external effects
before enabling. Normal manual use and dashboard session creation do not enable
autonomy. “Automatically approve technical ADRs” is a specific delegated policy,
not self-issued permission to remove all governance or raise budgets.

All covered technical ADRs are automatically approved through the deterministic
policy path, with ADR digest, policy version, classification/rationale and audit
actor **policy**, never falsely recorded as a human approval. Architect owns
technical decisions and routes blockers; it does not claim that ambiguous business
requirements are technical simply to proceed. Product/business ambiguity, missing
hard resources, unapproved cost/authority/privacy expansion or explicit manual
hold requires user decision. Technical uncertainty within granted bounds belongs
to Architect, not a ceremonial user prompt for every implementation choice.

The proposed ability to approve technical ADRs is itself owner-approved first;
none of this run's new ADRs is self-approved. Existing manual ADR workflow remains
outside the enabled policy. Approvals bind exact version/body, including after
scope clarification; message text alone does not alter decision state.

## Execution loop and stopping

Requirements -> Architect design/technical decisions -> approved planning ->
assigned Executors -> independent Reviewers -> accepted work -> verified candidate.
Session supervisor, not another LLM, dispatches runnable nodes within policy and
resource budgets. Preserve scope/predecessor/worktree/output/recovery gates;
autonomy changes admission policy, not truth of implementation or tests.
Reviewer independence survives principal/session replacement. No auto-accept on
timeout, retry exhaustion or disappearing executor. Publication/push/deployment
requires separately explicit authorization within policy; completion is not
permission for arbitrary external effects.

Blocker escalation follows [coordination](coordination.md): local -> peer ->
Project Lead Architect -> user only where delegation/hard conditions cannot
resolve it. Waiting suspends affected nodes, not independent work. Project-wide
pause is reserved for genuinely global gates or explicit user pause. An offline
Architect is a recoverable role assignment/session problem before human escalation.

Budget admission reserves an allowance per run before spawning; account for
actual usage and outstanding concurrent reservations. Unknown price/usage is
UNKNOWN and handled conservatively, never zero. Hard monetary ceilings cannot be
promised if provider cannot expose/bound billing; show this limitation and enforce
available token/turn/concurrency/time caps. No silent increase or expensive-model
fallback. Durable waits/heartbeats/routing use zero inference; summaries are bounded.

User Stop/Pause wins over queued starts/wakes. Preserve current operations and
confirm quiescence; show stopping until actual writers settle. After reboot,
explicit pause persists. Auto-resume only under separately recorded policy and
remaining budget. Repeated provider failures trip a circuit breaker; do not spin
new sessions indefinitely or create new identities to bypass quota.

## Evidence

Prove automatic covered technical approval, unresolved product question via UI and
Architect CLI, unchanged question digest on retry, independent review, bounded
failure/retry, exhausted budget, unknown costs, user stop during tool execution,
offline architect replacement and recovery without duplicate work. Observe actual
tokens/wakeups per completed node, not a promotional “fully autonomous” claim.
