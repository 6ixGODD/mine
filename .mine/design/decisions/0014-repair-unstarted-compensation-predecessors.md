# ADR-0014: Repair Invalid Unstarted Compensation Predecessors Without Arbitrary Graph Editing

## Status

Proposed for repository-owner approval.

## Context

The accepted compensation contract is exact: a compensation inherits the
rejected Plan's hard predecessors, uses the next canonical sibling ID, and
reuses the lineage worktree. `mine plan prepare` enforces that invariant before
creating or switching Git artifacts.

The registration and whole-graph validation paths do not enforce the same
invariant. `mine plan add` currently checks only that every supplied predecessor
exists, and `mine graph validate` checks references and cycles but not the
declared rejected-Plan/compensation relation. An invalid compensation can
therefore be registered, released to `READY`, and reported by validation as
healthy; the first precise failure appears later in `prepare` as
`compensation must inherit hard predecessors`.

This is now a reproduced production incident in the LODE graph: an unstarted
compensation was registered with the rejected Plan's predecessor plus one
additional already-accepted predecessor. Validation passed, release produced
`READY`, and prepare refused. Manual TOML editing is forbidden, withdrawal
cannot repair a rejected Plan's recorded compensation chain, and the Plan ID
cannot be reused. The state is safe but has no accepted recovery operation.

The rejected predecessor's exact dependency list remains the authority. The
incident does not justify weakening exact inheritance or accepting a superset:
that would change compensation lineage semantics and allow planners to hide
new scheduling decisions inside a replacement. The missing capability is a
bounded repair of invalid **unstarted registration state**, plus earlier
enforcement so new incidents cannot be created.

Git's official worktree documentation confirms that linked worktrees have
independent checked-out branches while refs are shared, and that automation
should use porcelain worktree inventory rather than assuming filesystem
layout. The recovery therefore does not move, delete, switch, or rewrite Git;
it is permitted only before the compensation branch/worktree exists. Source:
<https://git-scm.com/docs/git-worktree> (accessed 2026-09-04).

## Decision

### Enforce one invariant at add, validate, release, and prepare

For every canonical compensation node `C` declared by exactly one rejected
node `R` (`R.compensating_plan == C.id`):

- `C.lineage_id` is the same derived lineage as `R`;
- `C.id` is the next contiguous sibling compensation of `R.id`;
- `C.hard_predecessors == R.hard_predecessors`, including order;
- the terminal rejected node itself is never inserted as a predecessor.

`mine plan add` checks this relation under the graph lock before insertion.
`mine graph validate` checks it structurally for every declared compensation,
so an invalid graph fails at validation rather than later at Git preparation.
`release` and `prepare` retain their existing checks; no gate is weakened.

### Add one bounded recovery operation

Add:

```text
mine graph repair-compensation-predecessors \
  --id <compensation-id> \
  --expected-revision <revision> \
  --reason <non-empty>
```

and MCP parity tool `mine_graph_repair_compensation_predecessors` with the same
inputs. The command accepts no predecessor list. It derives both the rejected
predecessor and the replacement list from graph facts.

This is the only operation besides schema/provenance migration allowed to read
a current-schema graph before ordinary validation. It acquires the Root graph
lock, parses the current machine source, verifies the expected revision, applies
the bounded candidate repair in memory, requires full current validation of the
result, then atomically writes TOML and deterministic Markdown.

### Preconditions

All conditions are mandatory; failure writes nothing:

1. The target exists, is a canonical compensation, and is `DRAFT`, `BLOCKED`,
   or `READY` only.
2. Exactly one `REJECTED` node declares it as `compensating_plan`; IDs and
   lineage form the next contiguous sibling relation.
3. The target has no `branch_anchor`, owner/run/start evidence, implementation
   or review report, implementation commits, compensation link, or withdrawal
   evidence.
4. The compensation branch does not exist and the lineage worktree is not
   checked out on that compensation branch. The command never cleans or
   rewrites Git and refuses ambiguous unique work.
5. The reason is non-empty and the expected revision matches.
6. Replacing only `target.hard_predecessors` with the rejected node's exact
   list produces a fully valid graph and deterministic rendered view.

### Mutation and status

- Replace the target's hard-predecessor list with the rejected predecessor's
  exact list. No caller-supplied edge, soft predecessor, successor, Plan body,
  rejected history, or Git object is changed.
- Keep `DRAFT` as `DRAFT`. A repaired `BLOCKED` or `READY` target becomes
  `READY`, because the rejected predecessor could only have started after all
  inherited hard predecessors were accepted and accepted states are terminal.
- Refresh `updated_at`, increment revision exactly once, and render the
  generated Markdown from the repaired graph.
- Return old/new predecessor lists, old/new status, rejected predecessor ID,
  reason, and revision before/after in the deterministic envelope. The caller
  records that envelope in its repository repair/review evidence.
- Re-running against an already-exact target is idempotent success with no
  write and no revision bump, after all identity/evidence checks still pass.

## Consequences

- New invalid compensation registrations fail at `plan add`; imported or
  older current-schema incidents fail at `graph validate` with a diagnostic
  naming the target and expected predecessors.
- Existing incidents have one non-arbitrary recovery path. The command cannot
  add a dependency, choose a replacement, alter a started Plan, bypass review,
  or repair unrelated graph corruption.
- `prepare` remains fail closed and continues to derive the compensation Git
  anchor from the preserved rejected branch tip.
- CLI, MCP, JSON schema, help, Skills, distribution copies, and regression
  suites gain the operation. No graph wire-schema field is added.

## Alternatives rejected

- Allow additional accepted predecessors: changes the exact lineage contract
  and lets compensation registration introduce scheduling decisions that the
  rejected Plan never had.
- Remove the extra predecessor manually: violates the machine-source and audit
  boundary and is not reproducible.
- Withdraw and recreate the same ID: withdrawn IDs are terminal and never
  reused; it also leaves the rejected Plan's compensation chain unresolved.
- Reuse `repair-provenance`: that operation is a versioned wire migration and
  must not become an arbitrary current-schema repair bag.
- Make `prepare` silently normalize the graph: combines Git mutation with an
  unexpected graph rewrite and hides the earlier invalid registration.

## Verification obligations

- pure validation tests for missing, additional, reordered, wrong-lineage, and
  non-contiguous compensation predecessors;
- `plan add` CLI/MCP refusal before graph mutation for each invalid relation;
- repair success from DRAFT/BLOCKED/READY, deterministic revision/render and
  idempotent no-op;
- refusal matrix for started/implemented/accepted/rejected/withdrawn targets,
  owner/run/report/commit evidence, existing compensation branch/worktree,
  empty reason, revision conflict, missing/ambiguous rejected declaration,
  and unrelated graph invalidity;
- post-repair `graph validate`, `plan prepare`, and `plan start` integration
  probe using a preserved rejected lineage;
- CLI/MCP schema, tool-count, help, Skill synchronization, and cross-platform
  distribution gates.

## Approval questions

1. Approve preserving exact compensation predecessor inheritance and enforcing
   it at registration plus whole-graph validation?
2. Approve the bounded pre-start repair command that derives the only permitted
   predecessor list from the rejected graph fact and never accepts arbitrary
   edges?
3. Approve refusing repair once the compensation branch/worktree or any start,
   implementation, review, or withdrawal evidence exists?
