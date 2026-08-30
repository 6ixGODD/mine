# ADR-0012: Withdraw Unstarted Plans Through a `WITHDRAWN` Terminal State

## Status

Proposed for repository-owner approval.

## Context

The Plan state machine has seven statuses. `REJECTED` is reachable only from
`IN_PROGRESS`/`IMPLEMENTED` and requires an independent review report plus the
next canonical sibling compensation. Unstarted plans — `DRAFT`, `BLOCKED`,
`READY` — have no truthful exit at all.

Real usage shows the gap. In the `milo` repository (MINE v0.1.10, 2026-08), an
approved architecture replacement made two registered, never-started plans
(Plan17/Plan18) permanently stale. The available lifecycle exits were:

- fake a start/implementation/rejection sequence for work that never ran,
  fabricating evidence and a compensation lineage;
- leave the nodes active forever, where they also block workspace closure
  ("no unresolved draft, blocked, ready ... node"); or
- hand-edit the active TOML graph, which the repository owner had to
  explicitly authorize as a one-off local workaround.

ADR-0011 rejected a permanent `ABORTED`/`CANCELLED` state for the
**in-flight** rejection case as duplicating rejection closure "without a
distinct product need". The unstarted case is materially different and now has
a demonstrated need: nothing was executed, so there is no implementation
evidence, no owner or run, no persisted `branch_anchor` obligation, and no
compensation semantics to produce.

## Decision

Add a terminal `WITHDRAWN` status with edges from `DRAFT`, `BLOCKED`, and
`READY` only, exposed through one new mutation.

### Command and evidence

`mine plan withdraw --id <plan-id> --reason <non-empty>` performs the
transition under the Root graph lock through the shared revision-checked
transaction (`lock -> reload -> revision check -> status check -> mutation ->
atomic write -> deterministic render`). It records the withdraw reason and UTC
timestamp on the node and bumps the revision exactly once. No review report
and no compensation plan are required or accepted: withdrawal closes
registration, not execution. `REJECTED` remains the only evidence-bound
failure exit. Re-running on an already-withdrawn node returns
`MINE_INVALID_TRANSITION` and writes nothing, matching `mine plan release`'s
documented non-idempotency.

MCP gains the parity tool `mine_plan_withdraw`.

### Git cleanup is provably lossless or refused

Unstarted nodes normally have no branch or worktree artifacts.
`mine plan prepare` is available from `READY`, so a prepared-but-unstarted
lane can exist; withdrawal handles it explicitly:

- `DRAFT`/`BLOCKED` nodes must have no registered branch/worktree evidence;
  any such evidence is refused as inconsistent state.
- A `READY` node with a registered branch/worktree is withdrawable only when
  deletion is provably lossless: for a base plan the branch tip must equal or
  be an ancestor of the current integration branch tip; for a compensation it
  must equal its rejected predecessor's preserved tip exactly; the linked
  worktree must be clean. MINE then removes that linked worktree and deletes
  the never-started `plan/*` branch under its standing branch authorization.
- A diverged tip, dirty worktree, or missing Git evidence is a typed refusal
  writing nothing: unique work may exist and belongs to the evidence-bound
  rejection path or owner investigation, not routine withdrawal.

### Downstream and closure semantics

- A withdrawn hard predecessor is never satisfied: successors referencing it
  remain blocked/unreleasable until they are themselves withdrawn or
  re-planned. There is no automatic rewiring and no replacement substitution;
  soft references are informational only.
- `WITHDRAWN` is terminal. Nodes are never revived or re-statused, and plan
  ids are not reused; replanning the same work registers a new plan.
- Workspace closure treats `WITHDRAWN` as retired: closure requires no node
  in `DRAFT`/`BLOCKED`/`READY`/`IN_PROGRESS`/`IMPLEMENTED`/`REJECTED`.
  Unlike `REJECTED`, a withdrawn node does not require a compensation chain.

## Consequences

- The status alphabet grows to eight; transition validation, terminal-status
  logic, waves, derived readiness, render, and the closure gate are updated
  together.
- Stale unstarted registrations leave the active graph through a normal,
  auditable, evidence-light operation instead of graph hand-edits.
- `mine-plan-create` (which registers plans), `mine-arch` (which replaces
  architectures), and `mine-plan-review` (closure) document the transition and
  its authority boundary: withdrawal is an owner-authorized retirement of
  unstarted work, never a way to erase started history.
- CLI and MCP surface, JSON envelopes, and Skill contracts gain the operation
  with stable error codes for illegal source states and unsafe Git evidence.

## Alternatives rejected

- Reuse `REJECTED` for unstarted plans: imports review-report, compensation,
  and lineage requirements that fabricate evidence for work that never ran.
- Extend `mine plan reject` source states instead of adding a status: couples
  two different closures and leaves the graph unable to express "retired
  before start".
- Continue authorizing one-off graph hand-edits: violates the core MINE rule
  that agents never mutate graph files; the milo workaround is incident
  evidence, not a pattern.
- Auto-purge stale `DRAFT` nodes: silently destroys registration and audit
  history.
- Non-terminal `archived` flag: adds a second activeness dimension to every
  wave/ready/closure query and leaves the status machine unable to
  distinguish retired from live registrations.

## Verification obligations

- domain transition table: three new legal edges; every other source state
  (including `WITHDRAWN` itself) refused and tested;
- CLI and MCP parity including the JSON envelope (`command: "plan.withdraw"`,
  `status_before`/`status_after`, revision bumped exactly once);
- lossless-cleanup matrix: ancestor/equality tip proofs and clean worktree
  succeed; divergence, dirt, and missing evidence refuse with nothing written;
- withdrawn predecessors keep successors unsatisfied; closure accepts a graph
  whose only non-`ACCEPTED` nodes are `WITHDRAWN`; waves exclude withdrawn
  nodes;
- revision conflict, lock timeout, and deterministic render including
  `WITHDRAWN`.

## Approval questions

1. Approve the terminal `WITHDRAWN` status reachable from
   `DRAFT`/`BLOCKED`/`READY` via `mine plan withdraw`, with reason recorded
   and no review report or compensation?
2. Approve the provably-lossless Git cleanup rule (and typed refusal when
   unique work may exist) for prepared-but-unstarted lanes?
3. Approve that withdrawn nodes do not block workspace closure and are never
   revived, with successors explicitly withdrawn or re-planned?
