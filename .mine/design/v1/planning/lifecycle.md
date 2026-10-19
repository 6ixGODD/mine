# plan31: SQLite Plan/work-node authority and governed command integration

## Status and activation

UNREGISTERED BLUEPRINT / NOT EXECUTABLE. Target1.0.0, provisional canonical ID
`plan31`. Materialize as `.mine/plan/plan31-lifecycle.md` only through the
[activation protocol](index.md). Current0.2 graph is unchanged. All runtime
verification below is NOT_RUN by planning; foundation checks are separate evidence.

## Goal and user outcome

Existing Plan/ADR/Git safety remains intact while SQLite owns machine state, work-node continuation and historical evidence; CLI/MCP/Skills use the same services.

## Governing Design and decisions

Read [storage](../storage.md), [service seams](../service-contracts.md),
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
Owner exclusively writes `v1/crates/lifecycle/**`, its private manifest/lock, tests,
and its module-qualified `work` SQL migration;
plus its exact implementation report. Read other domain packages/contracts only.
Forbidden: Root manifests/lockfiles, src, workflows, Skills/generated copies,
other lanes' migrations/reports, graph/ADR edits, current0.2 worktrees, stable,
global Agent config or installed MINE. Internal module splitting within this
responsibility is allowed without filename-by-filename approvals.

## Work packages

### WP1 — Port lifecycle invariants without weakening gates

Create src/plans.rs, adr.rs, graph.rs, scope.rs and sql/ work migration. Translate released 0.2 invariants into transaction views: canonical IDs, immutable body digests, predecessor satisfaction, namespace claims, original creation anchors, compensation lineage, review independence, exact-tip integration and clean owned cleanup. Reuse existing pure algorithms where possible; no copy of the old TOML service as second authority.

Deterministic evidence: tests/lifecycle.rs: released 0.2 valid/invalid graph fixtures, stopped compensation, held write scopes, reprepare advanced tip, dirty/foreign worktree refusal, stale acceptance evidence, rejected source preserved.

Inputs: governing typed contracts plus WP0 foundation artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(lifecycle): port lifecycle invariants without weakening gates`.

### WP2 — Work nodes, pause and receipt preservation

Create src/nodes.rs, checkpoint.rs and evidence.rs. Register Plan->package->step hierarchy and acyclic edges; admit only current assignment/epoch with checked predecessor/resource/budget facts. Checkpoint stores actual Git observations, dirty manifest, pending operation/wait IDs and input-bound receipt refs. Human docs/evidence snapshots survive ephemeral Git Plan cleanup. Pause/resume updates only eligible frontier; no test rerun inferred solely from session replacement.

Deterministic evidence: tests/nodes.rs: cycles, claim races, result during pause, process restart, changed inputs invalidate receipt, unchanged receipt reused, document removal backed by snapshot, not-found vs revision0.

Inputs: governing typed contracts plus WP1 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(lifecycle): work nodes, pause and receipt preservation`.

### WP3 — Transport-neutral command adapters and Skill handoff

Create src/commands.rs plus proposed_cli.md and skill_contract_tests fixtures inside this package. Export typed methods consumed by CLI/MCP/HTTP; specify migration-era refusal of old writers and no graph-only Git commits. Supply reviewed replacement snippets for all five Skills, role/ownership/blocker routing and OpenCode removal, but do not independently edit shared installed/source payloads; final integration owns that transaction.

Deterministic evidence: tests/commands.rs: same CLI/MCP/UI DTO path, actor spoof refusal, lost response operation lookup, read-only queries no recovery effects, no self-accept through ID/session replacement.

Inputs: governing typed contracts plus WP2 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(lifecycle): transport-neutral command adapters and skill handoff`.

## Integration artifact and closed loop

Deliver pure lifecycle/SQL mapping, work-node and evidence service plus exact command/Skill delta manifest. Integration owns root CLI/MCP wiring, removes old authority and updates generated payloads together.

Supply exported symbols/DTOs, migration digest, entry adapter and required dependency
changes in the report. Integrator merges accepted packages incrementally and runs
the first coherent vertical slice early. Neither shared mutable output nor changing
another lane's files is a shortcut. Immutable-plan changes follow clarification or
compensation; transport interruptions resume the same operation.

## Verification matrix

Create an independent Rust manifest `v1/crates/lifecycle/Cargo.toml` and declared test
targets above. Commands run with `CARGO_TARGET_DIR=<Root>/.mine/build/plan31/cargo-target`:

| Gate | Command | Expected evidence |
|---|---|---|
| Focused | `cargo test --manifest-path v1/crates/lifecycle/Cargo.toml` | Named unit/integration cases, exact counts |
| Format | `cargo fmt --manifest-path v1/crates/lifecycle/Cargo.toml -- --check` | exit0 |
| Lint | `cargo clippy --manifest-path v1/crates/lifecycle/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code` | no ignored diagnostic |
| Build | `cargo build --manifest-path v1/crates/lifecycle/Cargo.toml --all-targets --all-features` | exact isolated package artifacts |
| Docs | `cargo doc --manifest-path v1/crates/lifecycle/Cargo.toml --no-deps` with RUSTDOCFLAGS=-D warnings | no broken contracts |

All checks also include `git diff --check`, owned changed-file audit and dependency
vulnerability/license review appropriate to actual new dependencies. All fixtures,
logs, HOME, generated/build/browser output use Root `.mine/build/plan31/<class>`;
Root toolchains/downloads/immutable environments are reused. No local target,
node_modules or venv in the linked worktree; exceptional disk cost needs agreement.

## Research source register

Primary pages opened2026-10-16; executor/reviewer reopen against pinned versions.

| Source | Verified claim / implementation consequence |
|---|---|
| [Primary source](https://www.sqlite.org/backup.html) | Machine history must be portable beyond Git-only clone. |
| [Primary source](https://www.sqlite.org/lang_transaction.html) | Shared authority/CAS/audit atomicity. |

## Acceptance and report

- Every work package and failure case has real evidence, not merely API presence.
- Domain memory/transport fixtures are labeled separately from SQLite/native/E2E.
- Cross-Project authorization, CAS, idempotency, pause and unknown-outcome semantics
  preserve the common contracts; no generic retry of uncertain external effects.
- No unrelated scope, secret, current0.2 mutation or external publication.
- Executor records IMPLEMENTED only after required gates. Independent reviewer
  verifies exact tip, integrates into Root dev, removes only clean accepted owned
  worktree/branch, then ACCEPTED through released MINE. Rejection preserves lineage.

Report after activation: `.mine/plan/reports/plan31-lifecycle-implementation.md`.
Native/platform/credential gaps are explicit blockers, not silently waived tests.
