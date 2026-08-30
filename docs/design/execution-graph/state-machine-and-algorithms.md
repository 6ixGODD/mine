# Execution Graph State Machine and Algorithms

## States

```text
DRAFT
BLOCKED
READY
IN_PROGRESS
IMPLEMENTED
ACCEPTED
REJECTED
WITHDRAWN
```

## Allowed transitions

| From | To | Gate |
|---|---|---|
| DRAFT | BLOCKED | Plan registered but one or more hard predecessors not yet accepted; `mine plan release` or automatic successor release inside `mine plan accept` |
| DRAFT | READY | Plan complete and every hard predecessor accepted; `mine plan release` or automatic successor release inside `mine plan accept` |
| DRAFT | WITHDRAWN | Owner-authorized retirement before start; `mine plan withdraw` (ADR-0012) |
| BLOCKED | READY | All blocking conditions resolved (automatic successor release inside `mine plan accept`, or the blocker's own acceptance) |
| BLOCKED | WITHDRAWN | Owner-authorized retirement before start; `mine plan withdraw` (ADR-0012) |
| READY | IN_PROGRESS | Successful start, owner assigned, revision matches |
| READY | WITHDRAWN | Owner-authorized retirement before start; `mine plan withdraw` (ADR-0012) |
| IN_PROGRESS | IMPLEMENTED | Report and commit evidence registered |
| IN_PROGRESS | REJECTED | Independent evidence proves the immutable Plan infeasible or materially invalid; canonical review report and compensation recorded |
| IMPLEMENTED | ACCEPTED | Independent review passes every hard contract |
| IMPLEMENTED | REJECTED | Independent review finds material failure |

`REJECTED` is **terminal**. A rejected plan is never revived, restarted, or
re-statused; repository evidence confirms this (the bootstrap-era rejected
Plan `02` stayed `REJECTED` after its compensating Plan `02-1` was accepted).
The earlier `REJECTED -> BLOCKED` row is removed as misleading historical
baggage: it implied a status transition that no operation performs.

`WITHDRAWN` is **terminal** (ADR-0012). A withdrawn plan is never revived,
re-statused, or re-registered under the same id. Unlike `REJECTED`, it carries
no compensation obligation and does not block workspace closure; its hard
successors remain unsatisfied until explicitly withdrawn or re-planned.

Closing a rejection has two independent parts, both performed through the
accepted CLI; neither changes the rejected plan's status:

1. **Register the compensating plan** with `mine plan add` (hard predecessor
   set to the rejected plan's accepted predecessor, not the rejected plan).
2. **Rewire downstream successors** off the rejected plan onto the
   compensating plan with `mine plan rewire-compensation` (see
   [Compensation rewiring](#compensation-rewiring) below).

No generic `set-status` exists.

## Start-time branch anchor

`mine plan start` is both the execution-owner transition and the provenance
boundary for Plan branch history. Under the authoritative Root graph lock it
revalidates the exact registered linked worktree before changing status:

1. Root is clean and checked out on the configured integration branch;
2. the registered worktree path, branch, observed HEAD, and branch tip agree;
3. for a base Plan, that tip equals the current Root integration HEAD;
4. for a compensation, that tip equals the preserved branch tip of the
   rejected predecessor which declared this compensation; and
5. the resolved tip is a Git commit object.

The successful `READY -> IN_PROGRESS` transaction writes that commit as the
node's immutable `branch_anchor` together with owner, run ID, and timestamps.
No Plan mutation may begin before this succeeds. A later Root graph commit is
expected to diverge from the Plan branch at this anchor and is never merged
into the Plan merely to manufacture ancestry.

For `IN_PROGRESS` and `IMPLEMENTED`, linked-worktree reuse reads the stored
anchor and requires `git merge-base --is-ancestor <anchor> <tip>` plus exact
observed-HEAD/branch-tip parity. It does not recompute the anchor from current
Root HEAD. `READY` remains exact-initial-anchor only. Rewritten or unrelated
history is invalid.

## Execution-discovered infeasibility rejection

`IN_PROGRESS -> REJECTED` is a narrow recovery edge for an immutable Plan that
cannot be completed without contradicting its own accepted scope or making an
unauthorized material Design change. The executor stops and preserves evidence;
it does not invoke the transition on itself.

An independent reviewer or `mine-arch`, after explicit repository-owner
approval of any material replacement decision, supplies:

- a non-empty reason;
- a Root-visible canonical review report documenting the immutable conflict,
  exact commands and exit codes, branch/worktree preservation, and why the
  issue cannot be corrected within scope; and
- the next canonical sibling compensation ID.

Under the graph lock, `mine plan reject` rechecks revision and status, validates
the report path/file and compensation identity, then records `REJECTED`, report,
reason, compensation, and timestamp in one revision. It records no fabricated
implementation report or commits. The rejected branch is not merged; its
lineage worktree remains for compensation. This edge is not available for
timeouts, correctable implementation defects, owner reassignment, or routine
cancellation.

## Plan release

`mine plan release --id <plan-id>` is the explicit, deterministic operation
that moves a newly registered plan from `DRAFT` into the startable frontier.
Registration (`mine plan add`) always creates a `DRAFT` node by design: it
records the plan's identity, design references, write paths, and dependencies
but makes no claim about whether the plan may execute. Release is the
separate, explicit gate between registration and execution. Preserving that
distinction prevents a freshly added plan from becoming silently executable.

### Anchors and inputs

- The single input is the plan's id. No predecessor or status is supplied by
  the caller; the operation reads the current node and its hard predecessors
  under the graph lock.

### Preconditions (all checked under the graph lock, atomically)

1. The plan exists and its current status is exactly `DRAFT`. Any other
   status (`BLOCKED`/`READY`/`IN_PROGRESS`/`IMPLEMENTED`/`ACCEPTED`/`REJECTED`)
   fails with `MINE_INVALID_TRANSITION` and mutates nothing.
2. The graph's current revision must match the caller-supplied expectation
   (optimistic-concurrency), per the shared `save_with_revision` transaction.

### Mutation

- Compute `unsatisfied_predecessors` = the hard predecessors whose status is
  not `ACCEPTED`, in stable predecessor-list order.
- If `unsatisfied_predecessors` is empty (this includes a plan with no hard
  predecessors), transition the node `DRAFT -> READY`.
- Otherwise transition the node `DRAFT -> BLOCKED`.
- Refresh the node's `updated_at`; no other node is touched. Persistence and
  rendering are atomic via the shared `TomlStore::save_with_revision` path:
  lock -> reload -> revision check -> semantic checks -> status transition ->
  atomic TOML write -> deterministic Markdown render -> release lock. Every
  successful release increments the graph revision exactly once.

### Idempotency and re-run behavior

Release is **not** idempotent-success: it accepts only `DRAFT`. Re-running on
an already-released `READY` or `BLOCKED` node returns the precise stable error
`MINE_INVALID_TRANSITION` (`status_before` is not `DRAFT`) and writes nothing,
so callers cannot accidentally double-release. This is the documented
idempotency choice; automation that may race should read `mine plan show`
first or treat `MINE_INVALID_TRANSITION` as "already released".

### Result

The JSON envelope reports deterministically:

- `command`: `"plan.release"`;
- `revision_before` / `revision_after` (`revision_after == revision_before + 1`);
- `data.plan`, `data.status_before` (`"DRAFT"`), `data.status_after`
  (`"READY"` or `"BLOCKED"`), `data.hard_predecessors`, and
  `data.unsatisfied_predecessors` (empty for `READY`).

### Stability errors

- `MINE_PLAN_NOT_FOUND`: plan id absent.
- `MINE_INVALID_TRANSITION`: current status is not `DRAFT`.
- `MINE_REVISION_CONFLICT` / `MINE_LOCK_TIMEOUT`: shared transactional checks.

No arbitrary state-editing capability is introduced: release only moves a
`DRAFT` node to `READY`/`BLOCKED` based on hard-predecessor acceptance, behind
all validation above.

### Relationship to automatic successor release

`mine plan accept` keeps its existing automatic release pass: when a plan is
accepted, any `BLOCKED` successor whose hard predecessors are all now
accepted is moved to `READY` in the same transaction. That automatic pass and
`mine plan release` are two gates over the same allowed `DRAFT/BLOCKED ->
READY` edges; together they close the lifecycle gap where a standalone newly
registered `DRAFT` plan (no accepted upstream) could not previously become
executable. `mine plan release` never weakens `mine plan accept`'s release
pass and never alters non-`DRAFT` nodes.

## Plan withdrawal

`mine plan withdraw --id <plan-id> --reason <non-empty>` retires an unstarted
plan (ADR-0012). It is the truthful exit for registrations that will never
execute — for example after an approved architecture replacement — so stale
nodes leave the active graph through a normal operation instead of graph
hand-edits.

### Preconditions (all checked under the graph lock, atomically)

1. The plan exists and its current status is exactly `DRAFT`, `BLOCKED`, or
   `READY`. Any other status (including `WITHDRAWN` itself) fails with
   `MINE_INVALID_TRANSITION` and mutates nothing.
2. The reason is non-empty.
3. The graph's current revision matches the caller-supplied expectation
   (optimistic concurrency), per the shared `save_with_revision` transaction.
4. Git evidence is either absent or provably lossless:
   - `DRAFT`/`BLOCKED` nodes must carry no registered branch/worktree
     artifacts; any such evidence fails as inconsistent state;
   - a `READY` node with a registered branch/worktree is withdrawable only
     when, for a base plan, the branch tip equals or is an ancestor of the
     current integration branch tip, or for a compensation, equals its
     rejected predecessor's preserved tip exactly, and the linked worktree is
     clean. Diverged tips, a dirty worktree, or missing Git evidence fail
     with a typed refusal: unique work may exist and belongs to the
     evidence-bound rejection path or owner investigation.

### Mutation

- Remove a provably lossless linked worktree and delete the never-started
  `plan/*` branch under MINE's standing branch authorization; perform no Git
  operation when no artifacts exist.
- Record `WITHDRAWN`, the withdraw reason, and UTC timestamp on the node in
  one revision through the shared transaction path. No review report, owner
  reassignment, or compensation plan is written.

### Successor and closure semantics

- Successors listing the withdrawn plan as a hard predecessor remain
  unsatisfied forever; they must be withdrawn or re-planned explicitly. No
  automatic rewiring or replacement substitution exists; soft references are
  informational only.
- Workspace closure treats `WITHDRAWN` as retired; only `REJECTED` requires a
  compensation chain before closure.

### Idempotency and result

Withdrawal is not idempotent-success: re-running on a `WITHDRAWN` node returns
`MINE_INVALID_TRANSITION` and writes nothing, matching `mine plan release`.
The JSON envelope reports `command: "plan.withdraw"`, `revision_before` /
`revision_after` (`revision_after == revision_before + 1`), and
`data.plan`, `data.status_before`, `data.status_after`, `data.withdrawn_at`,
and `data.cleaned_git_artifacts` (boolean).

### Stability errors

- `MINE_PLAN_NOT_FOUND`: plan id absent.
- `MINE_INVALID_TRANSITION`: current status is not `DRAFT`/`BLOCKED`/`READY`.
- `MINE_GRAPH_INVALID`: inconsistent registered Git artifacts on an unstarted
  node.
- `MINE_GIT_EVIDENCE` class failure: diverged tip, dirty worktree, or missing
  Git object refusing lossless cleanup.
- `MINE_REVISION_CONFLICT` / `MINE_LOCK_TIMEOUT`: shared transactional checks.

## Validation

Validation includes unique IDs/paths, valid predecessors, acyclic hard dependencies, legal states, required files/evidence, valid design anchors, safe paths, generated-view byte parity against a fresh deterministic render (distinguishing machine-source failure from stale-view drift), and branch/workspace consistency.

## Derived readiness

A node is ready only when every hard predecessor is accepted, design references are valid, no material decision remains, path ownership does not conflict, and stored state allows readiness.

## Compensation rewiring

### Compensation identity and worktree lineage

A rejected plan may name only the next canonical compensation ID in its own
lineage. The base rejection names `C01`; rejection of `C01` names sibling
`C02`, and so on without gaps. Registration of that compensation requires the
exact declared ID, the same derived lineage/worktree path, and the rejected
node's hard predecessors. It never makes the terminal rejected node a hard
predecessor.

Rewiring remains exact and stepwise: base successors move to C01; if C01 is
later rejected, its mutable successors move to C02. Similar-looking IDs never
participate. Git preparation preserves each rejected branch ref and switches
the single clean lineage worktree to the new compensation branch.

`mine plan rewire-compensation --id <rejected-id>` is a deterministic,
first-class graph operation that replaces an explicitly rejected predecessor
with its registered compensating plan in every still-mutable downstream
successor. It is the only supported way to reroute downstream dependencies
after the bootstrap exception ended; manual editing of the graph files is
forbidden (`AGENTS.md`).

### Anchors and inputs

- The single input is the rejected plan's id. The replacement plan id is
  **derived** from the rejected plan's `compensating_plan` field — the single
  source of truth registered at reject time. The caller never supplies a
  replacement id, so dependency substitution can never be driven by a
  similar-looking id (no fuzzy/substring matching).

### Preconditions (all checked under the graph lock, atomically)

1. The original plan exists and its status is exactly `REJECTED`.
2. Its `compensating_plan` is non-empty and names an existing plan (the
   replacement). The replacement must not itself be `REJECTED`.
3. Every successor that lists the rejected id in either hard or soft
   predecessors must be in a still-mutable status: `DRAFT`, `BLOCKED`, or
   `READY`. Any successor in `IN_PROGRESS`, `IMPLEMENTED`, `ACCEPTED`, or
   `REJECTED` fails the operation with a stable error and rewires nothing.
4. The proposed rewired graph must remain acyclic (reuses `topological_sort`).
5. The graph's current revision must match the caller-supplied expectation
   (optimistic-concurrency), per the shared `save_with_revision` transaction.

### Mutation

- For each still-mutable successor, every exact occurrence of the rejected
  id in `hard_predecessors` and `soft_predecessors` is replaced by the
  compensating id in place. Predecessor order is preserved; no edges are
  added, removed, or reordered; unrelated predecessors and successors are
  untouched.
- The successor's `updated_at` is refreshed; the rejected plan's fields are
  not modified.
- Persistence and rendering are atomic via the shared
  `TomlStore::save_with_revision` path: lock -> reload -> revision check ->
  semantic checks above -> in-place replacement -> atomic TOML write ->
  deterministic Markdown render -> release lock. From the caller's
  perspective the graph mutation and its generated view change atomically.

### Idempotency

Re-running a completed rewiring is **safe idempotent success**: if no
successor still references the rejected id, the operation performs a
read-only load, writes nothing, bumps no revision, and returns
`revision_before == revision_after` with `affected_successors: []`. A
re-run that finds newly-added successors still referencing the rejected id
rewires them normally (a real mutation, revision bumps). A precise stable
error is reserved for genuine invalid state (not-rejected original,
missing/compensating-plan mismatch, locked successor, cycle), never for the
no-work case.

### Result

The JSON envelope reports deterministically:

- `command`: `"plan.rewire-compensation"`;
- `revision_before` / `revision_after` (equal on the idempotent no-op);
- `data.rejected_plan`, `data.compensating_plan`, and
  `data.affected_successors` (ids of successors whose predecessor lists
  changed, in stable plan-insertion order).

### Stability errors

- `MINE_PLAN_NOT_FOUND`: original or compensating plan missing.
- `MINE_INVALID_TRANSITION`: original is not `REJECTED`.
- `MINE_GRAPH_INVALID`: `compensating_plan` empty, replacement is `REJECTED`,
  or successors already rewired is not raised (see idempotency).
  `MINE_REWIRE_SUCCESSOR_LOCKED`: a referencing successor is in
  `IN_PROGRESS`/`IMPLEMENTED`/`ACCEPTED`/`REJECTED` (details name it).
- `MINE_GRAPH_CYCLE`: rewiring would create a hard-dependency cycle.
- `MINE_REVISION_CONFLICT` / `MINE_LOCK_TIMEOUT`: shared transactional checks.

No arbitrary graph-editing API is introduced: rewiring only substitutes a
rejected id with its declared compensating id in mutable successors' exact
predecessor entries, behind all validation above.

## Parallel wave

A wave is a stable maximal set of ready plans without ancestor relationships, exclusive-write overlap, exclusive/reserved overlap, or active conflicts.

## Workspace closure gate

Closure requires:

- every terminal implementation node accepted;
- no unresolved draft, blocked, ready, in-progress, implemented, or rejected node; withdrawn nodes are retired and do not block closure (ADR-0012);
- final full `mine-sync` attestation not blocked;
- release verification passes;
- current branch is managed `dev`;
- working tree has no unclassified changes;
- expected workspace ID matches the ownership marker.
