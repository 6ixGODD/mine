# plan27: Durable obligations, waits and blocker routing

## Status and activation

UNREGISTERED BLUEPRINT / NOT EXECUTABLE. Target1.0.0, provisional canonical ID
`plan27`. Materialize as `.mine/plan/plan27-coordination.md` only through the
[activation protocol](index.md). Current0.2 graph is unchanged. All runtime
verification below is NOT_RUN by planning; foundation checks are separate evidence.

## Goal and user outcome

A can ask B once, sleep without inference, and resume exactly one effective continuation after a durable result, including offline/restart/early-result races.

## Governing Design and decisions

Read [coordination](../coordination.md), [service seams](../service-contracts.md),
[transaction contracts](../transaction-contracts.md), [foundation](../foundation.md)
and [parallel protocol](parallel-protocol.md) fully. ADR24–31 are owner-approved;
this blueprint refines their implementation, not product/security authority.
Current product has TOML CLI/MCP, not this1.0 service. The prepared foundation is
unshipped and independently reviewable, not proof this lane exists.

Local planner decision: one cohesive domain namespace with package-private files,
manifest and lockfile; no root registry churn. Public/shared contract changes go
to the sole integration owner, not a competing local definition.

## Dependency and write-domain contract

After0.2 release and independently verified foundation activation, hard predecessors:
none within the new batch. Work can start beside other leaf lanes using contract
fixtures. Real integrated behavior is a final-join gate, never faked by a mock.
Owner exclusively writes `v1/crates/coordination/**`, its private manifest/lock, tests,
and its module-qualified `coordination` SQL migration;
plus its exact implementation report. Read other domain packages/contracts only.
Forbidden: Root manifests/lockfiles, src, workflows, Skills/generated copies,
other lanes' migrations/reports, graph/ADR edits, current0.2 worktrees, stable,
global Agent config or installed MINE. Internal module splitting within this
responsibility is allowed without filename-by-filename approvals.

## Work packages

### WP1 — Protocol and addressing

Create src/protocol.rs, rooms.rs and handlers.rs with the finite MessageKind contract. Derive sender from ActorContext; validate Project/room membership and exactly one accountable responder. INFORM/PROGRESS never imply a reply or wake. Fanout creates child obligations plus explicit all/any join; persist membership snapshot and independent recipient cursors. Reject body/frame limits and unknown recipient without leaking foreign identities.

Deterministic evidence: tests/protocol.rs: forged sender, revoked room, oversized body, duplicate message, all/any fanout, unknown enum, delivery acknowledgment != work completion.

Inputs: governing typed contracts plus WP0 foundation artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(coordination): protocol and addressing`.

### WP2 — Atomic waits and delivery

Create src/obligations.rs, waits.rs, timers.rs and sql/ with the coordination migration module. RequestAndWait inserts request, obligation, continuation wait and delivery outbox together. Resolve checks criterion/evidence and CAS, commits result+resolved waits+unique wake together. Already available result is checked in the same transaction; paused nodes retain results without runnable admission. Handoff readdresses outstanding obligations through the shared transaction seam.

Deterministic evidence: tests/waits.rs: fast result, duplicate/out-of-order result, concurrent resolution, rollback at each write, pause race, A-to-B reassignment, reopen same SQLite DB. Memory fixture is not restart proof.

Inputs: governing typed contracts plus WP1 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(coordination): atomic waits and delivery`.

### WP3 — Escalation and efficient presence

Create src/issues.rs and scheduling.rs. Persist acceptance/deadline timers; bounded retry/escalation keys include issue version. Route local -> responsible peer -> Project Lead -> user only for business/authority/budget/hard capacity. Detect wait cycles including node/obligation/resource edges and produce one bounded explanation. Presence is timestamped process evidence. Cursor digests and directed wakes have configurable byte budgets; deterministic timers incur zero LLM calls.

Deterministic evidence: tests/routing.rs: offline peer/architect, missed deadline, decline/reroute, cycle, cancellation pending vs cancelled, two unrelated runnable nodes continue; count zero inference during long simulated wait.

Inputs: governing typed contracts plus WP2 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(coordination): escalation and efficient presence`.

## Integration artifact and closed loop

Deliver typed coordination service, SQL mappings/migration, deterministic clock fixtures, delivery/inbox ports and measured wake/queue metrics. Integration must run two real Projects with persistent wait across service restart.

Supply exported symbols/DTOs, migration digest, entry adapter and required dependency
changes in the report. Integrator merges accepted packages incrementally and runs
the first coherent vertical slice early. Neither shared mutable output nor changing
another lane's files is a shortcut. Immutable-plan changes follow clarification or
compensation; transport interruptions resume the same operation.

## Verification matrix

Create an independent Rust manifest `v1/crates/coordination/Cargo.toml` and declared test
targets above. Commands run with `CARGO_TARGET_DIR=<Root>/.mine/build/plan27/cargo-target`:

| Gate | Command | Expected evidence |
|---|---|---|
| Focused | `cargo test --manifest-path v1/crates/coordination/Cargo.toml` | Named unit/integration cases, exact counts |
| Format | `cargo fmt --manifest-path v1/crates/coordination/Cargo.toml -- --check` | exit0 |
| Lint | `cargo clippy --manifest-path v1/crates/coordination/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code` | no ignored diagnostic |
| Build | `cargo build --manifest-path v1/crates/coordination/Cargo.toml --all-targets --all-features` | exact isolated package artifacts |
| Docs | `cargo doc --manifest-path v1/crates/coordination/Cargo.toml --no-deps` with RUSTDOCFLAGS=-D warnings | no broken contracts |

All checks also include `git diff --check`, owned changed-file audit and dependency
vulnerability/license review appropriate to actual new dependencies. All fixtures,
logs, HOME, generated/build/browser output use Root `.mine/build/plan27/<class>`;
Root toolchains/downloads/immutable environments are reused. No local target,
node_modules or venv in the linked worktree; exceptional disk cost needs agreement.

## Research source register

Primary pages opened2026-10-16; executor/reviewer reopen against pinned versions.

| Source | Verified claim / implementation consequence |
|---|---|
| [Primary source](https://www.rabbitmq.com/docs/confirms) | Delivery acknowledgments are not work completion. |
| [Primary source](https://docs.temporal.io/workflow-execution) | Persistent continuation pattern; no Temporal dependency. |

## Acceptance and report

- Every work package and failure case has real evidence, not merely API presence.
- Domain memory/transport fixtures are labeled separately from SQLite/native/E2E.
- Cross-Project authorization, CAS, idempotency, pause and unknown-outcome semantics
  preserve the common contracts; no generic retry of uncertain external effects.
- No unrelated scope, secret, current0.2 mutation or external publication.
- Executor records IMPLEMENTED only after required gates. Independent reviewer
  verifies exact tip, integrates into Root dev, removes only clean accepted owned
  worktree/branch, then ACCEPTED through released MINE. Rejection preserves lineage.

Report after activation: `.mine/plan/reports/plan27-coordination-implementation.md`.
Native/platform/credential gaps are explicit blockers, not silently waived tests.
