# ADR-0022: Durable READY Preparation and Root Graph Checkpoints

## Status and required release

Repository-owner review candidate; registry state and exact body digest are
authoritative. This correction is mandatory before **0.2.0**, not a later patch.
It replaces only the pre-start preparation and routine graph-checkpoint choices
of [ADR-0010](0010-persist-plan-branch-anchor-at-start.md). Its active execution
anchor, compensation lineage and historical recovery guarantees remain required.
The replacement carries the remaining ADR-0010 guarantees forward; no approved
body is edited. ADR-0018/0019 are reused, not treated as prior approval of this change.

## Confirmed repository evidence

The plan10 executor's 2026-10-09 feedback is reproduced by current repository
facts, not merely inferred from a test name:

- Root dev is adb2c48690544de16601246c2d9c0bc3613e1e4a, graph revision 37.
- plan04 is IN_PROGRESS; plan10 is READY without owner/run/branch_anchor.
- plan10's clean registered checkout remains at
  da92ef9c9236c151f1e70d99b35bb0b36d792620. The intervening single-parent
  Root commit changes only docs/plan/execution-graph.toml and its generated .md.
- master remains d871a48aab07bdde4e565625a8b49e7620168f59.
- src/application/plan_service.rs initial_branch_anchor derives current Root
  HEAD for every base preparation. prepare and start require exact equality.
  Both require clean Root; committing another Plan's lifecycle record therefore
  moves the required base away from an already prepared READY checkout.
- validate_registered_worktree and same-branch prepare reuse do not currently
  check Plan checkout cleanliness; only compensation switching does. READY
  staged, unstaged and untracked counterexamples must be covered explicitly.
- tests/plan_prepare.rs covers repeated prepare and post-start anchored reuse,
  but not prepare B -> start A -> Root graph checkpoint -> prepare/start B.

This is a real pre-start lifecycle gap missed by the architecture/planning
handoff. Existing CLI refusals enforce its old contract; the executor is not
responsible for inventing an unauthorized recovery.

## Decision

1. **Preparation is durable provenance, not a new Plan status.** Successful
   prepare records a versioned preparation generation in the authoritative
   graph: repository/workspace/Plan/body/policy identity, exact lineage/branch/
   worktree, prepared branch commit, observed Root commit and operation identity.
   The two commits are distinct for compensation. Repeated valid prepare reuses
   this identity without revision churn. New successful preparation increments
   once and returns the actual observer revision for the subsequent start.
   Advance the real graph schema only when implementing these facts; old writers
   refuse it. An ignored journal, reflog or guessed merge-base is not authority.
2. **READY stays exact to its recorded preparation, not moving Root HEAD.**
   Require the owned clean checkout and branch tip to equal its prepared commit.
   Do not auto-merge, rebase or fast-forward it merely for Root graph churn.
   At start, freshly validate preparation, current graph revision/predecessors,
   effective ownership and all Git/path gates under the existing mutation lock.
   Atomically capture the prepared branch commit as immutable branch_anchor with
   IN_PROGRESS. Active/reviewable reuse continues using that stored anchor.
3. **Graph-only advancement is proved, not guessed.** The recorded Root commit
   must be a real ancestor of current Root. Inspect every intervening commit;
   require a single-parent checkpoint chain changing only the two configured
   graph paths, coherent workspace/revisions and valid source/generated-view
   pairing. The target Plan's prepared contract/body/policy must still match.
   A product/Design/config/Plan/policy change, change then revert, merge commit,
   rewrite, diverged history or unverifiable checkpoint is not this allowance.
   Such advancement invalidates pre-start reuse and requires an explicit native
   re-preparation generation against the newly approved baseline. Preserve prior
   evidence and refuse dirty/advanced Plan work; never force-reset or infer a
   missing original preparation from an arbitrary stale branch.
4. **Routine lifecycle graph persistence is MINE-owned.** Add a narrow native
   Root checkpoint/recovery capability under integration ownership. Serialize
   graph mutations and their local Git checkpoints; ordinary prepare/start and
   other lifecycle operations complete their owned graph checkpoint without a
   separate human approval for each generated update. It may stage/commit only
   the exact configured graph source/view bytes attributable to that operation,
   on Root dev, with freshly checked HEAD/index/worktree and no foreign changes.
   This is an explicit limited replacement of the Skills-only graph-commit
   ownership rule, not permission for executors to stage Root product files.
   No push, tag, branch switch, general shell runner or global installation.
5. **Partial outcomes remain recoverable.** Reuse ADR-0019 operation identity,
   journal and query semantics for Git creation, preparation persistence and
   checkpoint interruption. No cross-resource atomicity fiction: graph committed
   but Git checkpoint failed is pending-checkpoint, not rolled back or startable
   completion. Reconcile the same operation before another mutating operation;
   never blindly retry, create another compensation or commit unrelated staging.
   Lost responses can be queried and resumed once without duplicate transitions.
   Missing legacy READY evidence remains explicitly absent; a typed recovery
   proves a fresh baseline rather than fabricating historical preparation.
6. **All affected Skills consume this native protocol.** Use returned observed
   revisions, re-observe stale snapshots and re-evaluate autonomously, and query
   uncertain operations. No per-lifecycle owner prompt, manual branch fast-forward
   recipe or permanently installed candidate binary is the normal solution.
   Root dev, real independent acceptance/cleanup, stopped rejection handoff,
   exact compensation predecessor and per-node mutable outputs remain required.

## Verification and mandatory release join

Use real isolated Git repositories and the actual CLI/MCP public paths:

- Exact reported two-Plan sequence, repeated graph-only checkpoints and repeated
  prepare/start: both disjoint Plans can execute; B's prepared HEAD never moves
  for graph-only Root churn; recorded preparation/active anchors remain truthful.
- Same-observation simultaneous contenders: incompatible starts grant once;
  disjoint stale callers re-observe before proceeding, without human intervention.
- Dirty Root/Plan staged, unstaged and untracked files; wrong/detached/locked/
  prunable checkout; path escape; premature READY commit; forged/missing identity;
  product or policy advancement including change-and-revert; rewritten history.
- Creation/persistence/checkpoint/lost-response interruptions, same-operation
  resume, unrelated staging preserved, no duplicate commit or revision increment.
- Base and compensation provenance; stopped predecessor; active anchored reuse;
  migration preserving old history without invented READY evidence; old-writer
  refusal; actual source/generated/embedded Skill payload agreement.

After explicit approval, detailed Design and a new normal mandatory Plan must
carry binary, Skill/template and regression scope together. Existing handed-off
Plans04-13 and their edges remain immutable. The new Plan can depend on plan13;
the final 0.2.0 candidate, full sync and release closure must wait for its
independent ACCEPTED result and fresh applicable evidence. plan13's earlier
candidate is then an intermediate milestone, not authority to publish. Register
the new nonterminal Plan before resuming release work; an ignored feedback note
alone is not a release gate. No self-hosting the fix with an unshipped binary.

## Alternatives and consequences

- Merely committing Root graph fixes dirtiness but invalidates dynamic READY HEAD.
- Serializing all implementation discards legitimate disjoint parallelism.
- Blindly accepting ancestors or net-diff equality admits stale/rewritten or
  change-and-revert evidence. Automatic READY fast-forward without recorded
  preparation cannot establish original preparation identity.
- Persisting preparation adds a real partial-outcome boundary, rejected by
  ADR-0010; the now-required operation recovery makes that trade-off explicit.
- Automatic native graph checkpoints broaden only this specific local Git
  responsibility. They must never consume foreign index/worktree state.

## Primary sources

Opened 2026-10-09, Git project documentation; use supported Git features:

- [git-worktree](https://git-scm.com/docs/git-worktree): linked HEAD/index are
  independent but repository refs are shared; graph locking alone is not a Git lock.
- [git-merge-base](https://git-scm.com/docs/git-merge-base): is-ancestor proves
  commit ancestry, not authorization or graph-only history.
- [git-update-ref](https://git-scm.com/docs/git-update-ref): an expected old OID
  can guard a ref update; it does not atomically transact graph files and Git.

## Approval boundary

Approve this single replacement contract and mandatory pre-0.2.0 delivery.
Approval does not itself run implementation or authorize a manual workaround.
Current plan10 remains READY and blocked under installed 0.1.13 until a separately
authorized safe bootstrap repair is available; do not claim this proposal has
already repaired its checkout. The architect must report that separate blockage
honestly rather than silently bypassing the installed prepare refusal.
