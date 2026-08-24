# ADR-0010: Persist the Plan Branch Anchor at Start

## Status

Proposed for repository-owner approval.

## Context

`mine plan prepare` currently derives a base Plan's creation anchor from the
current Root integration HEAD. That value is correct before execution starts,
but it is not stable after `mine plan start`: the lifecycle graph mutation is
committed on Root `dev`, while the isolated Plan branch continues from the
pre-start commit. Ordinary Plan commits therefore produce two descendants of
the original anchor. The new Root HEAD is not, and must not be required to be,
an ancestor of the Plan tip.

The failure is reproduced by `plan02-C02`:

```text
A  original accepted dev / Plan creation anchor
|\
| G  Root graph commit for READY -> IN_PROGRESS
P    ordinary committed Plan evidence
```

Git defines `merge-base --is-ancestor G P` as false for this topology. Reusing
the current Root HEAD as the anchor therefore rejects valid active Plan work.
Deriving only a merge base later is also insufficient: it cannot distinguish a
correctly prepared branch from an old unattached branch rooted at an arbitrary
stale ancestor of `dev`.

## Decision

Persist an immutable `branch_anchor` commit object ID on every started Plan
node. The execution graph remains the durable authority for this provenance.

### Start-time capture

`mine plan prepare` remains graph-read-only. For a `READY` Plan it continues to
require exact initial Git state:

- a base Plan branch/worktree HEAD equals the current authoritative Root
  integration HEAD;
- a compensation branch/worktree HEAD equals the rejected predecessor branch
  tip from which it must continue.

`mine plan start` becomes the only normal writer of `branch_anchor`. Under the
authoritative Root graph lock, immediately before `READY -> IN_PROGRESS`, it:

1. reloads and validates the expected graph revision and hard predecessors;
2. resolves the registered worktree through `git worktree list --porcelain -z`;
3. requires the exact registered path, branch, branch tip, and observed HEAD;
4. recomputes the expected initial anchor using the base/compensation rule
   above and requires exact equality;
5. resolves that value as a commit object without assuming SHA-1 length; and
6. writes `branch_anchor`, owner, run ID, timestamps, and `IN_PROGRESS` in the
   same graph transaction and revision increment.

The graph lock serializes lifecycle writers. Git evidence is re-read as late as
possible before the atomic graph write; no claim is made that Git refs and the
filesystem participate in a fictitious cross-resource transaction.

### Active and reviewable reuse

For `IN_PROGRESS` and `IMPLEMENTED`, `mine plan prepare` reads the stored
`branch_anchor`; it never substitutes the current Root HEAD. Reuse succeeds
only when all existing path/branch/containment/cleanliness gates pass, observed
worktree HEAD equals the registered branch tip, and:

```text
git merge-base --is-ancestor <branch_anchor> <registered-branch-tip>
```

returns ancestor=true. Ancestor=false is a validation refusal; a Git execution
error is reported separately. `READY` remains exact-anchor only. Terminal
nodes retain the anchor as audit evidence. Rewritten, detached, unrelated, or
owner-mismatched history remains fail-closed.

### Schema and deterministic repair

Adding `branch_anchor` changes the persisted graph contract and therefore bumps
the current graph wire schema from v2 to v3. Older writers refuse v3 rather than
silently dropping the field.

The existing explicit provenance-repair boundary is extended to support the
v2-to-v3 migration. `mine graph repair-provenance --expected-revision <n>` may
derive a missing anchor only from unambiguous repository evidence:

- for a node with ordered implementation commits, the first parent of the
  first recorded implementation commit, after validating the recorded commit
  chain;
- for an active compensation without recorded commits, the exact preserved
  rejected-predecessor branch tip, after validating the declared compensation
  link and proving it is an ancestor of the observed compensation tip;
- empty for nodes that have never started (`DRAFT`, `BLOCKED`, or `READY`).

Ambiguous active base nodes, missing objects, multiple parents where a unique
first-parent origin cannot be established, inconsistent recorded commits, or
non-descendant history fail without mutation. The repair is locked,
revision-checked, atomic, deterministic, and idempotent on an already valid v3
graph.

The current repository can migrate without guessing: historical implemented
nodes have recorded implementation commits; `plan02-C02` is a compensation
whose preserved predecessor tip is `29d0ab0...`; and the governance-recovery
Plan can record its commits through the v2 lifecycle before running the v3
repair.

## Consequences

- Root graph commits and Plan implementation commits may diverge normally from
  one recorded anchor without contaminating either branch.
- Initial stale-branch protection and later append-only advancement are both
  provable; branch name or merge-base coincidence alone is never trusted.
- `plan start` gains a Git evidence dependency and may fail before graph
  mutation when preparation is absent or drifted.
- The graph gains one durable provenance field and one explicit schema
  migration; no hidden ref, reflog, runtime file, or worktree-local metadata is
  authoritative.
- Skills must call `plan prepare` before `plan start`, preserve Root `dev`, and
  report anchor validation failures without trying to merge Root graph commits
  into a Plan branch.

## Alternatives rejected

- Recompute the anchor from current Root HEAD on every prepare: Root lifecycle
  commits make it the wrong commit after start.
- Use `git merge-base` as the only source of truth: an unattached stale branch
  can have a valid merge base and would regain the C01 vulnerability.
- Merge every Root graph commit into every active Plan branch: couples
  independent Plan work to central graph churn and creates avoidable conflicts.
- Store the anchor only in a Git reflog, hidden ref, or `.mine/runtime`: those
  records are pruneable, locally mutable, or outside the graph's revisioned
  audit boundary.
- Persist at `plan prepare`: a Git/worktree partial failure would require a
  second preparation-state transaction. Capturing at `plan start` uses an
  already verified worktree and is atomic with the lifecycle fact that needs
  the anchor.

## Verification obligations

- real-Git base and compensation sequences covering prepare -> start -> Root
  graph commit -> Plan commits -> active/reviewable prepare;
- stale unattached branch, rewritten history, detached HEAD, wrong path/branch,
  missing anchor, and Git-error refusals with byte-identical graph evidence;
- v2-to-v3 repair success for every current node and refusal for every
  ambiguous derivation;
- old-writer refusal of v3 and idempotent v3 repair;
- Windows junction and Unix symlink containment regressions remain green.

## Primary sources

- [Git worktree documentation](https://git-scm.com/docs/git-worktree), accessed
  2026-08-24: linked worktrees have independent `HEAD`/index state, share normal
  branch refs, and expose a stable porcelain inventory.
- [Git merge-base documentation](https://git-scm.com/docs/git-merge-base),
  accessed 2026-08-24: `--is-ancestor A B` reports whether A is an ancestor of
  B; it does not turn sibling descendants into an ancestor relationship.
- [Git rev-parse documentation](https://git-scm.com/docs/git-rev-parse),
  accessed 2026-08-24: exact refs and commit objects must be resolved through
  Git rather than filesystem guesses or fixed object-ID lengths.

## Approval questions

1. Approve `branch_anchor` as a v3 persisted graph fact captured atomically by
   `plan start`?
2. Approve stored-anchor ancestry for active/reviewable prepare while `READY`
   remains exact-anchor only?
3. Approve deterministic v2-to-v3 provenance repair and fail-closed refusal of
   ambiguous anchors?
