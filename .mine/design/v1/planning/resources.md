# plan29: Shared-resource arbitration and cross-Project channels

## Status and activation

UNREGISTERED BLUEPRINT / NOT EXECUTABLE. Target1.0.0, provisional canonical ID
`plan29`. Materialize as `.mine/plan/plan29-resources.md` only through the
[activation protocol](index.md). Current0.2 graph is unchanged. All runtime
verification below is NOT_RUN by planning; foundation checks are separate evidence.

## Goal and user outcome

Projects sharing scarce resources coordinate once, wait durably, and see the same authoritative holder without double grants or cross-Project data leaks.

## Governing Design and decisions

Read [resources](../resources.md), [service seams](../service-contracts.md),
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
Owner exclusively writes `v1/crates/resources/**`, its private manifest/lock, tests,
and its module-qualified `resources` SQL migration;
plus its exact implementation report. Read other domain packages/contracts only.
Forbidden: Root manifests/lockfiles, src, workflows, Skills/generated copies,
other lanes' migrations/reports, graph/ADR edits, current0.2 worktrees, stable,
global Agent config or installed MINE. Internal module splitting within this
responsibility is allowed without filename-by-filename approvals.

## Work packages

### WP1 — Whole-set capacity and fair queue

Create src/capacity.rs, queue.rs, grants.rs and sql/ control migration. Validate typed resource IDs, positive quantities, Project grant and attempt epoch. In one control transaction reserve the entire set or queue all; count provisional and UNCERTAIN holders. Use priority+aging and created-time/ID tie-break. Resource policy does not override Project budgets or permit shared mutable build outputs.

Deterministic evidence: tests/capacity.rs: two Projects race, whole-set all-or-none, starvation/aging clock, expired deadlines, untracked capacity shortage, duplicate reservation, rollback no leaked partial holds.

Inputs: governing typed contracts plus WP0 foundation artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(resources): whole-set capacity and fair queue`.

### WP2 — Lease recovery and cross-store admission

Create src/reconcile.rs and admission.rs. Global provisional reservation -> Project admission intent -> global confirmation -> effect admission; preserve one operation identity. Confirmed release requires quiescence evidence; timeout/PID absence alone yields UNCERTAIN. Reboot reconciles actual holder and refuses new grant until safe. Deinit revokes new membership but preserves held resources.

Deterministic evidence: tests/recovery.rs: failure every cross-store stage, lost grant/release reply, stale epoch, unfenceable live tool, controller unavailable, shutdown/reopen and no two physical writers.

Inputs: governing typed contracts plus WP1 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(resources): lease recovery and cross-store admission`.

### WP3 — Purpose-bound temporary channels

Create src/channels.rs and offers.rs. Two designated Project Leads explicitly accept membership; expiry/revocation checked per read/send. Same request/commit/result semantics, minimal resource terms only. Versioned offer acceptance never directly grants a lease. Closing resolves outstanding obligations visibly; unauthorized members cannot enumerate private history.

Deterministic evidence: tests/channels.rs: forged Lead, revoked/expired membership, concurrent counteroffers, unknown outcome, unauthorized history, no source/secrets disclosure, negotiation accepted but lease not yet available.

Inputs: governing typed contracts plus WP2 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(resources): purpose-bound temporary channels`.

## Integration artifact and closed loop

Deliver control-store mappings, resource coordinator, channel authorization and grant projections. Integration tests two Projects plus actual long-running test child to prove no premature replacement.

Supply exported symbols/DTOs, migration digest, entry adapter and required dependency
changes in the report. Integrator merges accepted packages incrementally and runs
the first coherent vertical slice early. Neither shared mutable output nor changing
another lane's files is a shortcut. Immutable-plan changes follow clarification or
compensation; transport interruptions resume the same operation.

## Verification matrix

Create an independent Rust manifest `v1/crates/resources/Cargo.toml` and declared test
targets above. Commands run with `CARGO_TARGET_DIR=<Root>/.mine/build/plan29/cargo-target`:

| Gate | Command | Expected evidence |
|---|---|---|
| Focused | `cargo test --manifest-path v1/crates/resources/Cargo.toml` | Named unit/integration cases, exact counts |
| Format | `cargo fmt --manifest-path v1/crates/resources/Cargo.toml -- --check` | exit0 |
| Lint | `cargo clippy --manifest-path v1/crates/resources/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code` | no ignored diagnostic |
| Build | `cargo build --manifest-path v1/crates/resources/Cargo.toml --all-targets --all-features` | exact isolated package artifacts |
| Docs | `cargo doc --manifest-path v1/crates/resources/Cargo.toml --no-deps` with RUSTDOCFLAGS=-D warnings | no broken contracts |

All checks also include `git diff --check`, owned changed-file audit and dependency
vulnerability/license review appropriate to actual new dependencies. All fixtures,
logs, HOME, generated/build/browser output use Root `.mine/build/plan29/<class>`;
Root toolchains/downloads/immutable environments are reused. No local target,
node_modules or venv in the linked worktree; exceptional disk cost needs agreement.

## Research source register

Primary pages opened2026-10-16; executor/reviewer reopen against pinned versions.

| Source | Verified claim / implementation consequence |
|---|---|
| [Primary source](https://www.sqlite.org/lang_transaction.html) | Serialize whole-set capacity validation and mutation. |
| [Primary source](https://www.rabbitmq.com/docs/confirms) | Accepted message/offer differs from fulfilled resource obligation. |

## Acceptance and report

- Every work package and failure case has real evidence, not merely API presence.
- Domain memory/transport fixtures are labeled separately from SQLite/native/E2E.
- Cross-Project authorization, CAS, idempotency, pause and unknown-outcome semantics
  preserve the common contracts; no generic retry of uncertain external effects.
- No unrelated scope, secret, current0.2 mutation or external publication.
- Executor records IMPLEMENTED only after required gates. Independent reviewer
  verifies exact tip, integrates into Root dev, removes only clean accepted owned
  worktree/branch, then ACCEPTED through released MINE. Rejection preserves lineage.

Report after activation: `.mine/plan/reports/plan29-resources-implementation.md`.
Native/platform/credential gaps are explicit blockers, not silently waived tests.
