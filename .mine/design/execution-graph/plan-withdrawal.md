# Unstarted Plan Withdrawal

## Purpose

`mine plan withdraw --id <plan-id> --reason <non-empty>` retires an unstarted
plan (ADR-0012). It is the truthful exit for registrations that will never
execute — for example after an approved architecture replacement — so stale
nodes leave the active graph through a normal operation instead of graph
hand-edits. This leaf preserves the existing withdrawal contract extracted
from [the state-machine document](state-machine-and-algorithms.md#plan-withdrawal).

## Preconditions (all checked under the graph lock, atomically)

1. The plan exists and its current status is exactly `DRAFT`, `BLOCKED`, or
   `READY`. Any other status (including `WITHDRAWN` itself) fails with
   `MINE_INVALID_TRANSITION` and mutates nothing.
2. The reason is non-empty.
3. The graph's current revision matches the transaction's expected revision,
   per the shared `save_with_revision` transaction. Explicit observer-bound
   public writes are the pending [ADR-0019 target](../governance/lifecycle-receipts.md).
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

## Mutation

- Remove a provably lossless linked worktree and delete the never-started
  `plan/*` branch under MINE's standing branch authorization; perform no Git
  operation when no artifacts exist.
- Record `WITHDRAWN`, the withdraw reason, and UTC timestamp on the node in
  one revision through the shared transaction path. No review report, owner
  reassignment, or compensation plan is written.

The approved pending ADR-0019/0022 implementation freezes the exact original
Git observations and candidate transition **before** cleanup, using the existing
[typed checkpoint Git-effect intent](../governance/root-graph-checkpoints.md#plan-git-effects-before-graph-publication).
Partial cleanup is pending recovery, not rollback or permission to treat a
previously existing branch as never prepared. Guard deletion by the observed
old OID, preserve rejected refs, and checkpoint only the exact graph pair after
all owned Git effects complete. This changes no withdrawal eligibility or status.

## Successor and closure semantics

- Successors listing the withdrawn plan as a hard predecessor remain
  unsatisfied forever; they must be withdrawn or re-planned explicitly. No
  automatic rewiring or replacement substitution exists; soft references are
  informational only.
- Workspace closure treats `WITHDRAWN` as retired; only `REJECTED` requires a
  compensation chain before closure.

## Idempotency and result

Withdrawal is not idempotent-success: re-running on a `WITHDRAWN` node returns
`MINE_INVALID_TRANSITION` and writes nothing, matching `mine plan release`.
Same-ID operation query/resume is distinct from a new withdrawal: it recovers
the original recorded result without a second status/revision/checkpoint change.
The JSON envelope reports `command: "plan.withdraw"`, `revision_before` /
`revision_after` (`revision_after == revision_before + 1`), and
`data.plan`, `data.status_before`, `data.status_after`, `data.withdrawn_at`,
and `data.cleaned_git_artifacts` (boolean).

## Stability errors

- `MINE_PLAN_NOT_FOUND`: plan id absent.
- `MINE_INVALID_TRANSITION`: current status is not `DRAFT`/`BLOCKED`/`READY`.
- `MINE_GRAPH_INVALID`: inconsistent registered Git artifacts on an unstarted
  node.
- `MINE_GIT_EVIDENCE` class failure: diverged tip, dirty worktree, or missing
  Git object refusing lossless cleanup.
- `MINE_REVISION_CONFLICT` / `MINE_LOCK_TIMEOUT`: shared transactional checks.
