# ADR-0015: Bounded Git Evidence and Graph Operations

## Status and scope

Repository-owner review candidate. The ADR registry is the lifecycle authority.
This proposal governs Git evidence acquisition and graph-operation performance;
it does not authorize implementation or weaken Git safety checks.

## Context and evidence

`src/infrastructure/toml_store.rs:144-152,532-558` validates every nonempty
Plan anchor with a separate `git rev-parse` on every load, including terminal
history and duplicate anchors. `src/cli/commands.rs:1002-1007` loads twice for
`plan show`. Ordinary lifecycle operations add caller/service validation and
three validations inside `save_with_revision` (`toml_store.rs:175-225`).
For N anchored nodes, ordinary unchanged-anchor mutations therefore launch
approximately 5N anchor queries, excluding fixed context queries. These are
blocking sequential launches within one operation, not proof of hundreds of
simultaneously live children. Parallel agents multiply the aggregate load.

The graph lock covers repeated whole-graph Git validation. Stale revision
rejection occurs after the first expensive pass. `git.rs:44-75,288-292` uses
blocking process output without a deadline or per-operation query telemetry.
`domain/validation.rs:305-351` repeatedly scans all nodes and vector membership
inside its topological traversal; long-chain work can grow cubically.

LODE reviewer turn records observed about 26.1 seconds for `plan show` and
33.8 seconds for `plan prepare`. These are historical individual command
durations, not a controlled benchmark or attribution of all latency to Git.

## Proposed decision

1. Acquire one validated, command-scoped graph snapshot and reuse it across
   the service, response envelope, and generated rendering. Do not reload a
   graph merely to obtain the response workspace ID or render already-written
   bytes. Pure graph validation remains separate from Git evidence acquisition.
2. Deduplicate exact full object IDs and validate commit identity/type in a
   bounded `git cat-file --batch-check` session. Missing objects, wrong types,
   short IDs, unexpected output and process failures remain errors. Do not
   introduce network fetches or weaken exact-anchor semantics. Preserve Git
   replacement/peeling behavior through explicit regression fixtures.
3. Acquire refs/worktree inventory in bulk where supported. Reuse only within
   the request; branch tips, ancestry and cleanliness needed for a mutation
   must be freshly checked at its safety boundary. No persistent stale cache.
4. Under the graph lock, parse and reject a stale expected revision before
   expensive Git checks. Validate the authoritative old state and resulting
   state with deduplicated evidence; render the validated resulting snapshot.
5. Bound Git child concurrency, total request time and captured output. On
   timeout/cancellation, terminate and reap owned children. A failed read must
   not advance graph state; a write with uncertain completion follows ADR-0019.
6. Index nodes and adjacency once; replace repeated membership scans with
   bounded indexed traversal while preserving deterministic declaration-order
   output. No speculative daemon, object-database library or permanent cache.
7. Report operation ID, phase, Git invocation count, peak owned children,
   Git/validation/lock-wait durations and graph size without secrets. Distinguish
   aggregate process launches from simultaneously live processes.

## Alternatives and consequences

- Parallelizing the existing N queries retains amplification and creates a
  process storm; increasing lock timeouts only hides the bottleneck.
- Skipping terminal-anchor validation silently changes evidence guarantees.
- A permanent server/cache adds invalidation and deployment obligations before
  request-local batching has been evaluated.

The selected design preserves safety while reducing process creation. It
requires injectable process evidence and performance fixtures, not elapsed-time
claims from one developer machine. Graph traversal optimization is a separate
work package if it would delay the primary batching fix.

## Verification obligations

- Count child invocations on 0/10/100/1000-node fixtures with unique and repeated
  anchors. Anchor-object validation uses one batch per validation snapshot,
  not one child per node. Record warm/cold p50/p95 latency and peak children.
- Preserve missing/wrong-type/rewritten-tip/dirty-worktree/revision-conflict
  refusals on Windows, Linux and macOS; errors never become false success.
- Test cancellation, blocked child/output, lock contention, malformed batch
  responses and rendering interruption. Stale writers launch no anchor batch.
- Compare topology/wave results and deterministic order against existing
  fixtures; show chain/wide-DAG scaling without fragile wall-clock-only gates.

## Primary sources

Opened 2026-10-09; local Git is 2.45.1.windows.1. Use supported batch features,
not newer-only flags inferred from the latest manual.

- Git project, [git-cat-file](https://git-scm.com/docs/git-cat-file): stdin batch
  object information permits one process to validate many requested objects.
- Git project, [git-for-each-ref](https://git-scm.com/docs/git-for-each-ref):
  structured ref enumeration supports a request-local inventory.

## Approval question

Approve request-local batching, bounded processes and indexed graph operations
with unchanged evidence guarantees, rather than raising timeouts or skipping
validation?
