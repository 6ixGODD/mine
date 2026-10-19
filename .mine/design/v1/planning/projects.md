# plan28: Global Project directory, safe init/deinit and relocation

## Status and activation

UNREGISTERED BLUEPRINT / NOT EXECUTABLE. Target1.0.0, provisional canonical ID
`plan28`. Materialize as `.mine/plan/plan28-projects.md` only through the
[activation protocol](index.md). Current0.2 graph is unchanged. All runtime
verification below is NOT_RUN by planning; foundation checks are separate evidence.

## Goal and user outcome

One local directory lists every managed Project; init, move and removal are recoverable without deleting user work or leaving phantom registrations.

## Governing Design and decisions

Read [projects](../projects.md), [service seams](../service-contracts.md),
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
Owner exclusively writes `v1/crates/projects/**`, its private manifest/lock, tests,
and its module-qualified `projects` SQL migration;
plus its exact implementation report. Read other domain packages/contracts only.
Forbidden: Root manifests/lockfiles, src, workflows, Skills/generated copies,
other lanes' migrations/reports, graph/ADR edits, current0.2 worktrees, stable,
global Agent config or installed MINE. Internal module splitting within this
responsibility is allowed without filename-by-filename approvals.

## Work packages

### WP1 — Identity and registration saga

Create src/directory.rs, identity.rs, init.rs and sql/ with Project/control migrations. Canonicalize Root through Git and filesystem identity; linked worktrees resolve same Project. Persist REGISTERING intent globally before Project store creation; validate marker/store/Git/operation IDs, then ACTIVE. Same operation resumes stages, never allocates another ID. Duplicate root or simultaneously live UUID refuses.

Deterministic evidence: tests/registration.rs: repeated init, lost each stage response, linked worktree, spaces/non-ASCII, case/path alias, duplicate cloned identity, absent/unreadable disk.

Inputs: governing typed contracts plus WP0 foundation artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(projects): identity and registration saga`.

### WP2 — Relocation and safe cleanup preview

Create src/relocate.rs, cleanup_manifest.rs and ownership.rs. Relocate verifies same Project and repository identity before changing directory record. PreviewDeinit enumerates exact proven-owned paths, generated sections, worktrees/branches and unknown blockers; hash manifest and revisions. Preserve user text, provider accounts/history, foreign artifacts and rejected/diverged work. No broad recursive delete or Git clean.

Deterministic evidence: tests/preview.rs: unknown .mine file, symlink escape, foreign marker, dirty/rejected worktree, changed manifest, already missing directory, no mutation on refusal.

Inputs: governing typed contracts plus WP1 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(projects): relocation and safe cleanup preview`.

### WP3 — Recoverable deinit and user-visible directory

Create src/deinit.rs and recovery.rs. Durable barrier stops new admissions; reconcile child writers/resources. Backup/export outside removal tree through storage port, close handles, apply each manifest item once and persist global tombstone. Final removal requires every cleanup item confirmed. Unavailable path unregister explicitly reports local cleanup impossible. Global journal survives deletion; resource uncertainty stays globally visible.

Deterministic evidence: tests/deinit.rs: crash before/after each effect, open handles, failed backup/disk full, surviving writer, held resource, repeat operation, two Projects isolation and exact restored archive.

Inputs: governing typed contracts plus WP2 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(projects): recoverable deinit and user-visible directory`.

## Integration artifact and closed loop

Deliver directory service, ordered cleanup manifest/outcome DTOs, recoverable saga and typed filesystem/Git effect port. Composition integrates existing Root Git safety without duplicating its process-spawning implementation.

Supply exported symbols/DTOs, migration digest, entry adapter and required dependency
changes in the report. Integrator merges accepted packages incrementally and runs
the first coherent vertical slice early. Neither shared mutable output nor changing
another lane's files is a shortcut. Immutable-plan changes follow clarification or
compensation; transport interruptions resume the same operation.

## Verification matrix

Create an independent Rust manifest `v1/crates/projects/Cargo.toml` and declared test
targets above. Commands run with `CARGO_TARGET_DIR=<Root>/.mine/build/plan28/cargo-target`:

| Gate | Command | Expected evidence |
|---|---|---|
| Focused | `cargo test --manifest-path v1/crates/projects/Cargo.toml` | Named unit/integration cases, exact counts |
| Format | `cargo fmt --manifest-path v1/crates/projects/Cargo.toml -- --check` | exit0 |
| Lint | `cargo clippy --manifest-path v1/crates/projects/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code` | no ignored diagnostic |
| Build | `cargo build --manifest-path v1/crates/projects/Cargo.toml --all-targets --all-features` | exact isolated package artifacts |
| Docs | `cargo doc --manifest-path v1/crates/projects/Cargo.toml --no-deps` with RUSTDOCFLAGS=-D warnings | no broken contracts |

All checks also include `git diff --check`, owned changed-file audit and dependency
vulnerability/license review appropriate to actual new dependencies. All fixtures,
logs, HOME, generated/build/browser output use Root `.mine/build/plan28/<class>`;
Root toolchains/downloads/immutable environments are reused. No local target,
node_modules or venv in the linked worktree; exceptional disk cost needs agreement.

## Research source register

Primary pages opened2026-10-16; executor/reviewer reopen against pinned versions.

| Source | Verified claim / implementation consequence |
|---|---|
| [Primary source](https://www.sqlite.org/lang_transaction.html) | Single-store transaction boundary; separate stores require saga. |
| [Primary source](https://www.sqlite.org/backup.html) | Consistent online backup before destructive cleanup. |

## Acceptance and report

- Every work package and failure case has real evidence, not merely API presence.
- Domain memory/transport fixtures are labeled separately from SQLite/native/E2E.
- Cross-Project authorization, CAS, idempotency, pause and unknown-outcome semantics
  preserve the common contracts; no generic retry of uncertain external effects.
- No unrelated scope, secret, current0.2 mutation or external publication.
- Executor records IMPLEMENTED only after required gates. Independent reviewer
  verifies exact tip, integrates into Root dev, removes only clean accepted owned
  worktree/branch, then ACCEPTED through released MINE. Rejection preserves lineage.

Report after activation: `.mine/plan/reports/plan28-projects-implementation.md`.
Native/platform/credential gaps are explicit blockers, not silently waived tests.
