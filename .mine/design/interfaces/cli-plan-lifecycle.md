# CLI Plan Lifecycle Details

These existing command contracts are extracted from the
[CLI overview](cli-contract.md). Candidate observer/operation arguments implement
[ADR-0019](../governance/lifecycle-receipts.md); ordinary write examples also carry
the explicit actor/revision described there. Installed 0.1.13 keeps its older
contract; source documentation does not add flags to that installation.

## Preparation and checkpoint contract

ADR-0022's candidate mutating prepare request, explicit refresh/legacy recovery,
generation/operation DTOs and exact start semantics are defined in
[durable READY preparation](../governance/ready-preparation.md#start-scheduling-and-public-contract).
Prepare remains CLI-only; this does not invent an existing MCP preparation tool
or put write-only revision arguments on read-only Plan show. Existing MCP
start/show/wave adapters share the updated application facts.
The candidate prepare request requires explicit `--actor` attribution alongside
ID, observer revision and operation ID; see the
[actor contract](../governance/ready-preparation.md#preparation-actor-attribution).
It does not derive a READY actor from future start ownership or Git identity.
All graph mutations complete only after their narrow
[Root checkpoint](../governance/root-graph-checkpoints.md); pending stage and
actual committed revision remain queryable and block subsequent writes/closure.
Installed 0.1.13 has none of these new flags or automatic checkpoint semantics.

## Plan release

`mine plan release --id <plan-id>` moves a newly registered plan from `DRAFT`
into the startable frontier, deterministically. Registration (`mine plan add`)
always creates a `DRAFT` node; release is the explicit gate between
registration and execution. See the
[full algorithm](../execution-graph/state-machine-and-algorithms.md#plan-release).

- Accepts only a `DRAFT` plan; returns `MINE_INVALID_TRANSITION` for any other
  status and mutates nothing.
- Re-runs planning readiness for the node's persisted `planning_scope`
  (ADR-0013) before transitioning; a blocked readiness fails with the blocking
  ADR DTOs and writes nothing. On success the node records
  `readiness_checked_at` and the satisfied `adr_registry_revision` as audit
  evidence.
- Transitions `DRAFT -> READY` when every hard predecessor is `ACCEPTED`
  (including a plan with no hard predecessors); transitions `DRAFT -> BLOCKED`
  when one or more hard predecessors are not yet `ACCEPTED`.
- Never alters `IN_PROGRESS`/`IMPLEMENTED`/`ACCEPTED`/`REJECTED`/`WITHDRAWN`
  plans.
- Goes through the shared transaction (`lock -> reload -> revision check ->
  semantic validation -> mutation -> atomic write -> deterministic render`);
  every successful release increments the graph revision exactly once.
- Not idempotent-success: re-running on an already-released node returns
  `MINE_INVALID_TRANSITION` and writes nothing.

```json
{
  "ok": true,
  "command": "plan.release",
  "revision_before": 18,
  "revision_after": 19,
  "data": {
    "plan": "plan09",
    "status_before": "DRAFT",
    "status_after": "READY",
    "hard_predecessors": ["plan03"],
    "unsatisfied_predecessors": []
  },
  "warnings": []
}
```

Errors reuse stable `MINE_*` codes (`MINE_PLAN_NOT_FOUND`,
`MINE_INVALID_TRANSITION`, `MINE_REVISION_CONFLICT`, `MINE_LOCK_TIMEOUT`).

## Plan withdrawal

`mine plan withdraw --id <plan-id> --reason <reason>` retires an unstarted
plan (`DRAFT`/`BLOCKED`/`READY`) into terminal `WITHDRAWN` (ADR-0012). See the
[full withdrawal contract](../execution-graph/plan-withdrawal.md)
for the algorithm, preconditions, and Git-cleanup proofs.

- Requires a non-empty reason; never accepts a review report or compensating
  plan (withdrawal closes registration, not execution).
- Performs provably-lossless cleanup when a prepared-but-unstarted
  branch/worktree exists (base: tip equals or is an ancestor of the
  integration branch tip; compensation: tip equals the rejected predecessor's
  preserved tip; worktree clean) and refuses with typed errors otherwise.
- Withdrawn hard predecessors keep successors unsatisfied; withdrawn nodes
  never block workspace closure and are never revived.
- Not idempotent-success: re-running returns `MINE_INVALID_TRANSITION` and
  writes nothing.

Under ADR-0019/0022, this new-operation refusal is distinct from query/resume
of the original exact operation. Interrupted Git cleanup and checkpointing use
the [typed Git-effect intent](../governance/root-graph-checkpoints.md#plan-git-effects-before-graph-publication)
and return/reconcile that stored result without a second withdrawal.

## Compensation rewiring

`mine plan rewire-compensation --id <rejected-plan-id>` reroutes downstream
dependencies from an explicitly rejected plan onto its registered compensating
plan. It is the deterministic, CLI-managed closure of `mine plan reject` (which
set `compensating_plan`), and the only supported rerouting path after the
bootstrap exception ended. See the
[full algorithm](../execution-graph/state-machine-and-algorithms.md#compensation-rewiring).

- Required flags are `--id` (the rejected Plan), `--actor` and caller-observed
  `--expected-revision`. The replacement is
  derived from the rejected plan's `compensating_plan`; the caller never supplies
  it, so substitution can never be triggered by a similar id.
- Rewiring goes through the shared application/persistence transaction
  (`lock -> reload -> revision check -> semantic validation -> mutation ->
  atomic write -> deterministic render`). TOML commits atomically; a later
  Markdown-render failure requires repair and does not roll back committed state.
- It verifies the original is `REJECTED`, `compensating_plan` names an existing
  non-rejected replacement, every affected successor is still mutable
  (`DRAFT`/`BLOCKED`/`READY`), no cycle is introduced, and unrelated predecessors
  and successors are unchanged.
- Repeating a completed rewiring is safe idempotent success: it writes
  nothing, bumps no revision, and returns `affected_successors: []`.
- It never weakens the immutability of accepted or active plans: successors in
  `IN_PROGRESS`/`IMPLEMENTED`/`ACCEPTED`/`REJECTED` are never touched.

### Result envelope

```json
{
  "ok": true,
  "command": "plan.rewire-compensation",
  "revision_before": 17,
  "revision_after": 18,
  "data": {
    "rejected_plan": "plan05",
    "compensating_plan": "plan05-C01",
    "affected_successors": ["plan06"]
  },
  "warnings": []
}
```

Errors reuse the stable `MINE_*` codes; rewiring-specific failures use
`MINE_REWIRE_SUCCESSOR_LOCKED` and the existing `MINE_GRAPH_CYCLE`. No
arbitrary graph-editing CLI is introduced.
