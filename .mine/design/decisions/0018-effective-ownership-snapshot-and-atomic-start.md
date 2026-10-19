# ADR-0018: Effective Ownership Snapshot and Atomic Start

## Status and scope

Repository-owner review candidate. Depends on ADR-0016 for semantic ownership;
the existing path model's active-occupancy safety defect can be fixed independently.

## Context and evidence

`src/domain/validation.rs:409-437` selects READY nodes and compares only the
selected READY candidates. IN_PROGRESS/IMPLEMENTED owners are omitted, and
scope/relationship errors use `unwrap_or(false)`. `PlanService::start`
(`src/application/plan_service.rs:619-683`) checks predecessor/Git/anchor
evidence but not active ownership conflicts.

LODE's `docs/plan/tools/worktree-wave.ps1:221-246` explicitly treats both
IN_PROGRESS and IMPLEMENTED as held. It supplements graph show with per-Plan
show calls for effective scope, then rechecks revision and authorization;
that workaround adds round trips and leaves native enforcement incomplete.

## Proposed decision

1. Provide one revision-bound scheduling snapshot containing relevant node
   states, predecessors, original/effective ownership, policy/body identities,
   lineage/worktree locations and blocking reasons. Querying many active Plans
   must not require one CLI process per node (ADR-0015).
2. READY is eligibility, not a resource reservation. Wave computes candidates
   against existing IN_PROGRESS and IMPLEMENTED owners, selected candidates,
   shared integration resources and worktree/lineage exclusivity. Rejected
   code remains preserved; compensation reuses the lineage worktree only after
   the rejection handoff completes. Rejected status alone is not an active run.
3. Repeat compatibility in `start` under the existing graph mutation lock,
   bound to the caller-observed revision and current ownership policy. Start
   atomically grants ownership with IN_PROGRESS; competing incompatible starts
   cannot both succeed from the same observation.
4. Hold ownership through IMPLEMENTED and any pending review/integration phase.
   Release it only after accepted integration/cleanup, or an evidenced rejected
   handoff. Process death or a guessed timeout never silently frees an owner.
5. Use one deterministic compatibility function for wave and start. Invalid
   paths, unknown namespace mappings, stale supplements or policy evidence
   return errors, not presumed compatibility. A stale wave remains advisory
   and must never be advertised as an enduring grant.
6. Explicit semantic partitions may permit shared-directory work; they never
   bypass physical worktree exclusivity, shared mutable outputs, generator
   transactions, or an unpartitioned exclusive algorithm.

## Alternatives and consequences

- A prompt-only warning cannot close the time-of-check/time-of-use gap.
- A separate lease daemon/database duplicates graph authority and recovery.
- Serializing all work would avoid some races but discard valid parallelism.

The selected graph-bound ownership lifecycle needs no expiring worker lease.
Resource states are derived from truthful node/integration state. Exact DTOs,
schema versioning and caller revision arguments belong in detailed Design after
approval; the current CLI is not claimed to expose these proposed interfaces.

## Verification obligations

Race two overlapping starts at one revision: exactly one succeeds; the other
reports stale/conflict evidence without mutation. Test active/implemented holds,
non-overlapping starts, semantic partitions, same lineage/worktree refusal,
invalid scope fail-closed behavior, pending integration, compensation reuse and
consistent snapshot reasons through CLI/MCP. No scheduling test may rely only
on a READY-only wave.

## Primary sources

Opened 2026-10-09.

- Git project, [git-worktree](https://git-scm.com/docs/git-worktree): linked
  checkout identity and shared repository metadata remain physical constraints.
- Martin Fowler, [Bounded Context](https://martinfowler.com/bliki/BoundedContext.html):
  explicit boundary relationships inform ownership, not automatic source analysis.

## Approval question

Approve revision-bound effective scheduling snapshots and locked active-owner
checks at start, retaining ownership until an evidenced lifecycle handoff?
