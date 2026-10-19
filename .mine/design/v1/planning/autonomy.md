# plan30: Experimental policy, technical decisions and budget admission

## Status and activation

UNREGISTERED BLUEPRINT / NOT EXECUTABLE. Target1.0.0, provisional canonical ID
`plan30`. Materialize as `.mine/plan/plan30-autonomy.md` only through the
[activation protocol](index.md). Current0.2 graph is unchanged. All runtime
verification below is NOT_RUN by planning; foundation checks are separate evidence.

## Goal and user outcome

Explicitly enabled autonomy handles delegated technical decisions and dispatch within recorded bounds; business decisions and true hard shortages reach the user, while Stop wins.

## Governing Design and decisions

Read [autonomy](../autonomy.md), [service seams](../service-contracts.md),
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
Owner exclusively writes `v1/crates/autonomy/**`, its private manifest/lock, tests,
and its module-qualified `autonomy` SQL migration;
plus its exact implementation report. Read other domain packages/contracts only.
Forbidden: Root manifests/lockfiles, src, workflows, Skills/generated copies,
other lanes' migrations/reports, graph/ADR edits, current0.2 worktrees, stable,
global Agent config or installed MINE. Internal module splitting within this
responsibility is allowed without filename-by-filename approvals.

## Work packages

### WP1 — Policy and decision classification

Create src/policy.rs, decisions.rs and sql/ migration. Default disabled. Enable binds Project, requirements digest, exact policy version, allowed scope/effects/runtime tuples, owner instruction evidence and limits. EvaluateTechnicalApproval requires authorized Architect classification/rationale, exact ADR digest and covered technical scope; record actor=policy not human. Business/authority/privacy/budget expansion routes a versioned user issue.

Deterministic evidence: tests/policy.rs: default-off, manual mode unaffected, changed ADR digest, stale policy, technical automatic approval, business ambiguity, mislabeled authority expansion, explicit manual hold and no forged human approval.

Inputs: governing typed contracts plus WP0 foundation artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(autonomy): policy and decision classification`.

### WP2 — Reserved concurrent budgets

Create src/budget.rs and usage.rs. Reserve token/turn/time/concurrency allowance atomically with node admission; settle deduplicated provider observations, preserve outstanding reservations. Unknown cost is unknown, not zero; enforce available hard units and disclose monetary limit uncertainty. No provider quota evasion or unauthorized expensive fallback.

Deterministic evidence: tests/budget.rs: concurrent reservations, duplicate/out-of-order usage, unknown price, exhausted limits, resume with remaining allowance, missing final usage, crash/lost reservation response and no negative totals.

Inputs: governing typed contracts plus WP1 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(autonomy): reserved concurrent budgets`.

### WP3 — Deterministic dispatch and stop barriers

Create src/admission.rs and breakers.rs. Scheduler consumes durable runnable nodes and grants; no LLM for heartbeats/waits/routing. Persistent user pause revokes queued starts/wakes before stop effects. Retry circuit-breakers use class+outcome; UNKNOWN reconciles. Auto-resume only separately preauthorized and never overrides explicit pause. Project Lead replacement is a bounded identity/session handoff, not repeated user approval.

Deterministic evidence: tests/dispatch.rs: pause vs queued start, delayed result while paused, offline Architect, circuit trips, native startup failure, independent nodes continue, publication denied without separate permission.

Inputs: governing typed contracts plus WP2 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(autonomy): deterministic dispatch and stop barriers`.

## Integration artifact and closed loop

Deliver policy and admission components with deterministic fake clock/runtime fixtures. Final join proves real assigned Executors and independent Reviewers; this package never auto-accepts Plans.

Supply exported symbols/DTOs, migration digest, entry adapter and required dependency
changes in the report. Integrator merges accepted packages incrementally and runs
the first coherent vertical slice early. Neither shared mutable output nor changing
another lane's files is a shortcut. Immutable-plan changes follow clarification or
compensation; transport interruptions resume the same operation.

## Verification matrix

Create an independent Rust manifest `v1/crates/autonomy/Cargo.toml` and declared test
targets above. Commands run with `CARGO_TARGET_DIR=<Root>/.mine/build/plan30/cargo-target`:

| Gate | Command | Expected evidence |
|---|---|---|
| Focused | `cargo test --manifest-path v1/crates/autonomy/Cargo.toml` | Named unit/integration cases, exact counts |
| Format | `cargo fmt --manifest-path v1/crates/autonomy/Cargo.toml -- --check` | exit0 |
| Lint | `cargo clippy --manifest-path v1/crates/autonomy/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code` | no ignored diagnostic |
| Build | `cargo build --manifest-path v1/crates/autonomy/Cargo.toml --all-targets --all-features` | exact isolated package artifacts |
| Docs | `cargo doc --manifest-path v1/crates/autonomy/Cargo.toml --no-deps` with RUSTDOCFLAGS=-D warnings | no broken contracts |

All checks also include `git diff --check`, owned changed-file audit and dependency
vulnerability/license review appropriate to actual new dependencies. All fixtures,
logs, HOME, generated/build/browser output use Root `.mine/build/plan30/<class>`;
Root toolchains/downloads/immutable environments are reused. No local target,
node_modules or venv in the linked worktree; exceptional disk cost needs agreement.

## Research source register

Primary pages opened2026-10-16; executor/reviewer reopen against pinned versions.

| Source | Verified claim / implementation consequence |
|---|---|
| [Primary source](https://docs.temporal.io/workflow-execution) | Persisted workflows and bounded recovery, not in-memory chat loops. |
| [Primary source](https://www.sqlite.org/lang_transaction.html) | Reserve allowance with admission in one local transaction. |

## Acceptance and report

- Every work package and failure case has real evidence, not merely API presence.
- Domain memory/transport fixtures are labeled separately from SQLite/native/E2E.
- Cross-Project authorization, CAS, idempotency, pause and unknown-outcome semantics
  preserve the common contracts; no generic retry of uncertain external effects.
- No unrelated scope, secret, current0.2 mutation or external publication.
- Executor records IMPLEMENTED only after required gates. Independent reviewer
  verifies exact tip, integrates into Root dev, removes only clean accepted owned
  worktree/branch, then ACCEPTED through released MINE. Rejection preserves lineage.

Report after activation: `.mine/plan/reports/plan30-autonomy-implementation.md`.
Native/platform/credential gaps are explicit blockers, not silently waived tests.
