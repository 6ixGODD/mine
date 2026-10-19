# ADR-0011: Reject Infeasible In-Progress Plans Without Faking Implementation

## Status

Proposed for repository-owner approval.

## Context

The current Plan state machine permits only `IN_PROGRESS -> IMPLEMENTED`.
`REJECTED` is reachable only from `IMPLEMENTED`, because rejection was
originally modeled solely as an independent review verdict on completed code.

`plan02-C02` proves that this is incomplete. Its immutable acceptance contract
requires an anchor rule that is impossible in the repository's real Git
topology while explicitly forbidding the persisted-contract change needed to
repair the rule. The executor correctly stopped with a failing real-Git test,
made no production change, and refused to claim `IMPLEMENTED`. The graph is now
truthfully `IN_PROGRESS`, but the current state machine provides no truthful
exit. Marking it `IMPLEMENTED`, editing its immutable Plan, or hand-editing the
graph would each corrupt a different evidence boundary.

## Decision

Add a narrow, evidence-bound `IN_PROGRESS -> REJECTED` transition for an
immutable Plan that is proven infeasible or materially invalid during
execution.

### Authority and gate

The existing `mine plan reject` command remains the only rejection mutation;
there is no generic cancel or set-status command. It accepts `IN_PROGRESS` only
when all of the following hold:

1. the executor has stopped and no agent still owns a live write lane for the
   Plan;
2. an independent reviewer or `mine-arch` has recorded a Root-visible report
   at the canonical Plan review-report path;
3. the report identifies the immutable requirement, exact contradicting
   evidence, commands and exit codes, preserved branch/worktree state, and why
   bounded implementation cannot resolve it;
4. `--reason`, `--review-report`, and the next canonical sibling
   `--compensating-plan` are supplied;
5. the current user has explicitly authorized the architecture/recovery
   direction when the failure requires a material Design decision; and
6. graph revision, Plan status, compensation identity, path safety, and report
   existence are revalidated under the Root graph lock.

The transition records the review report, reason, compensation ID, timestamp,
and `REJECTED` in one revision. It does not invent an implementation report or
implementation commits. The Plan branch and lineage worktree remain preserved
and unmerged for its compensation, exactly like an implementation-review
rejection.

Skills enforce the human/independent authority boundary; the binary validates
evidence and topology but does not claim to authenticate the human actor.

### Scope of the transition

This edge is for a material contradiction, unsafe immutable contract, or
missing architecture decision that makes continued execution unauthorized. It
is not a convenience cancellation for slow tests, a failed implementation
attempt that can still be corrected within the Plan, owner reassignment, or an
excuse to bypass review.

`IMPLEMENTED -> REJECTED` remains the ordinary independent implementation
review path. `REJECTED` remains terminal, compensation IDs remain contiguous
siblings, and downstream rewiring remains the existing explicit operation.

### Recovery of the current repository

Because the installed v2 binary cannot perform this new transition and cannot
review a normally advanced new Plan branch, recovery is deliberately two-step:

1. Freeze `plan02-C02` at its clean preserved worktree and create one serialized
   governance-recovery base Plan from current `dev`. That Plan implements
   ADR-0010 plus this transition, records its commits through the v2
   `IMPLEMENTED` lifecycle, migrates the graph to v3 through the explicit
   provenance repair, and is independently reviewed with the repaired prepare
   contract. Its temporary ownership of overlapping governance paths is
   explicit; C02 cannot resume concurrently.
2. After that recovery Plan is accepted, write the canonical C02 rejection
   report, transition `plan02-C02 -> REJECTED` with
   `compensating_plan = "plan02-C03"`, and create C03 in the existing
   `.mine/worktrees/plan02` lineage. C03 carries forward the valid C02 evidence,
   completes the inherited regression/release closure, and is independently
   reviewed normally.

This bootstrap sequence never marks C02 implemented, never edits C02, never
merges its rejected branch, and never hand-edits graph state.

## Consequences

- The graph can represent an honest execution-discovered Plan failure instead
  of forcing a false success claim.
- Rejection remains compensation-producing and terminal; no parallel
  cancellation state or abandoned lineage is introduced.
- Review-report evidence becomes mandatory for new rejection mutations,
  including the exceptional active rejection path.
- Recovery can temporarily serialize ownership away from a frozen active Plan,
  but only with explicit evidence and without running both agents concurrently.
- `mine-plan-exec`, `mine-plan-review`, `mine-plan-create`, and `mine-arch` must
  distinguish implementation failure within scope from Plan infeasibility that
  requires this transition.

## Alternatives rejected

- Mark C02 `IMPLEMENTED` and then reject it: falsely asserts production work
  and implementation evidence that do not exist.
- Edit or replace C02 in place: violates immutable Plan identity and destroys
  the exact failed contract under review.
- Hand-edit the TOML/Markdown graph: bypasses revision, validation, rendering,
  and audit rules.
- Add a permanent `ABORTED`/`CANCELLED` state: duplicates rejection closure and
  would require a second compensation/release policy without a distinct
  product need.
- Leave C02 `IN_PROGRESS` forever and start release work around it: makes graph
  closure and stable release impossible and hides the governance failure.

## Verification obligations

- domain, CLI, and MCP parity for legal `IN_PROGRESS -> REJECTED` and all
  illegal source states;
- required canonical review-report, reason, next sibling compensation, and
  unchanged graph on every failure;
- no implementation report/commit fabrication;
- rejected branch/worktree preservation, same-lineage C03 reuse, and no merge
  into Root before acceptance;
- concurrent revision conflict and executor/reviewer Skill contract tests;
- release closure continues to require an accepted compensation chain.

## Approval questions

1. Approve the evidence-bound `IN_PROGRESS -> REJECTED` edge for an immutable
   Plan proven infeasible during execution?
2. Approve reusing `REJECTED` and the existing compensation lifecycle rather
   than adding `ABORTED`/`CANCELLED`?
3. Approve the two-step bootstrap recovery for current C02: governance-recovery
   Plan first, then `plan02-C03` in the preserved lineage?
