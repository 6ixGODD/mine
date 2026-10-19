# Storage and Verified Import — Unregistered Execution Blueprint

## Activation status

Provisional allocation: `plan25`; canonical materialized filename
`.mine/plan/plan25-storage.md`, own report
`.mine/plan/reports/plan25-storage-implementation.md`. See [activation](index.md)
and [common protocol](parallel-protocol.md); allocation is not graph registration.

This is a planning blueprint, not a registered Plan, READY node or authority to
execute. Root will allocate its canonical Plan ID, worktree, owner, immutable
scope, predecessor graph and exact shared-contract commit before activation.
Planning scope is `v1.0.0`; approved ADR-0026 governs. Actual0.2 migration is
forbidden while0.2 is active. No `mine` command described as1.0 is installed yet.

## Outcome and non-goals

Deliver the SQLite transaction engine, common normalized schema, modular migration
runner, durable audit/outbox/idempotency storage, verified backup/export and explicit
0.2 import. Domain rules and domain-owned SQL mappings are not this lane's backlog.
Memory/test fixtures are not a production storage fallback. Do not change active
0.2 persistence, install/update MINE, or register a real user Project for testing.

## Governing inputs and frozen start gate

- [Transaction contracts](../transaction-contracts.md), [storage](../storage.md),
  [projects](../projects.md), ADR-0024/0026 and approved exact-document provenance.
- Root's accepted `v1/contracts` commit: IDs, envelopes, error names, revisions,
  typed events and DTO fixtures. Validate SQLite signed-integer range at admission.
- A frozen migration catalog identifies all modules, table names, dependency order,
  version/digest rules and each module's sole owner. Catalog changes belong to
  integration owner; domain SQL files do not become storage's write scope.
- Capture0.2 final config/graph/ADR/document schemas from its accepted candidate.
  Storage engine and fixture import implementation can start against declared
  fixtures earlier; real compatibility acceptance waits for that final baseline.

## Ownership and parallel interfaces

Own `v1/crates/storage/` including its manifest, `src/`, `tests/`, migration ledger
implementation and `migrations/common/{control,project}/`. Package name:
`mine-v1-storage`. Its standalone manifest may declare its own empty workspace
until integration adopts it; it must not edit root Cargo manifests or0.2 lockfile.
Own `v1/crates/storage/tests/fixtures/` and only its generated fixture artifacts.
Integration owner alone wires module exports and static migration
catalog. Existing `v1/contracts` is read-only; request contract corrections rather
than adding a private conflicting definition.
This lane owns its private Cargo.lock; it does not update another leaf's lockfile
or create a shared writable workspace lock during parallel execution.

Expose shared transaction/read capabilities and a borrowed SQLite transaction
adapter for each domain's own SQL mapping. Do not require identity/coordination
services to exist before the database engine or migration tests can compile.
Domain lanes can use the common test kit and baseline dependency migrations in
isolation. Complete domain implementations are integration join gates, not start
dependencies. Domain authentication remains authoritative; store only accepts
an internally verified actor context, never caller-supplied trusted actor text.

## Work packages

### S1 — Connections and module schema

Pin rusqlite and bundled/backup features with the verified patched SQLite runtime.
Validate actual version, journal_mode=WAL, synchronous=FULL, foreign_keys=ON,
bounded busy timeout and schema identity on every newly opened connection.
Writer queue and read concurrency are bounded; dedicated writer lifetime supports
quiescence and idle close. Failed validation opens no writable service.

Implement STRICT common tables from the transaction leaf, module-version digest
ledger, transactional baseline install, deterministic compiled dependency order,
old/new schema refusal and checksum mismatch refusal. Do not scan arbitrary SQL
files at runtime. Store kind/UUID/Project identity are immutable and checked before
reading business rows. Network/shared-file paths cannot silently become WAL stores.

### S2 — Atomic command execution and event delivery persistence

Implement actor/request-scope-bound idempotency, canonical digest comparison,
entity CAS, explicit rollback, committed result cursor and typed unknown outcomes.
Business mutation, audit, outbox and command result are one transaction. Provide
bounded page reads and event cursor reset handling. Outbox claims have generation,
due time and bounded retry; inbox deduplicates source-store/effect/digest.

Test cancellation before and after admission; a dropped response cannot claim
rollback after commit. Quarantine an unresolved connection, re-open and find the
original operation. The engine does not run provider/Git/file effects inside a
transaction. Full-disk/IO/corrupt database are observable failures, never fake green.

### S3 — Backup, staged import and ownership safety

Create consistent backups with SQLite Backup API into a distinct owned staging
target, manifest/schema/Project identity and document/artifact digests. Verify the
destination independently. Export references needed Git/document evidence and
distinguishes unavailable evidence from complete restore. Do not export secrets or
vendor transcripts by default. Interrupted staging never overwrites the last valid
backup or declares a new live authority.

Importer consumes a quiescent0.2 snapshot supplied by the lifecycle/integration
owner. Parse exact final TOML/ADR/document schemas; preserve approval/body hashes,
IDs, historical states, branch anchors, rejected lineage and receipts. Refuse active
unknown operations/writers, unsupported schema, dangling bindings, ambiguous roots,
digest drift and overflow. Import into a new staged store, run integrity_check and
foreign_key_check plus semantic validation, then request the Projects lifecycle
owner's authority switch. Storage does not independently rewrite live config or
disable old writers. Same-identity restore and new-Project fork are different modes;
neither silently copies active assignment authority into a second live Project.

### S4 — Real-store conformance and handoff

Run the shared contract suite against memory and SQLite and retain the distinction.
Real files prove crash/rollback/WAL/backup behavior. Deliver stable ports and a
minimal real-store request/result replay fixture for integration to consume before
all domain modules finish. Lane acceptance requires real common-store mappings
and final0.2 import fixtures. Cross-domain production mappings join in the final
integration Plan, not a circular acceptance dependency on every other lane. No
UI/provider completion is required to validate storage engine correctness itself.

## Mandatory tests

- Duplicate same request returns byte-equivalent typed outcome without a second
  entity revision/event; changed payload/actor/scope refuses; replay respects current
  read authorization. Intentionally new command needs new operation ID.
- Two writers contend on same entity; one CAS wins; unrelated entities do not
  fail merely because database event cursor advanced.
- Fault injection after each target/audit/outbox/result write rolls back all; lost
  post-commit response recovers original outcome; stale outbox claim cannot ack.
- Project ID, database kind, migration digest, unsupported version, bad FK, input
  bound and signed counter overflow fail before mutation.
- Startup/restart with WAL, long finite readers, busy timeout, full disk and owned
  backup interruption. No claim that closing an HTTP Future cancels writer work.
- Frozen0.2 fixture import, repeated import, corrupt file, missing documents,
  nonmatching raw digest, unknown operation, and full backup restore; no real user
  DB/config modified. Schema-only fixtures do not claim production upgrade tested.

## Commands and evidence

The lane creates the package/test targets below. These are **post-implementation
verification commands**, not commands already run or existing tests claimed green.
Before activation replace `<plan-id>` with the exact registered ID and resolve
`<Root>`/`<PlanWorktree>` from MINE, not shell cwd guesses. Set
`CARGO_TARGET_DIR=<Root>/.mine/build/<plan-id>/cargo-target`; all temporary DBs,
crash fixtures and backups use `<Root>/.mine/build/<plan-id>/storage-tests/`.

From the Plan worktree:

```text
cargo fmt --manifest-path v1/crates/storage/Cargo.toml -- --check
cargo clippy --manifest-path v1/crates/storage/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v1/crates/storage/Cargo.toml --test transaction_contract
cargo test --manifest-path v1/crates/storage/Cargo.toml --test recovery_import
cargo test --manifest-path v1/crates/storage/Cargo.toml --all-targets
cargo build --manifest-path v1/crates/storage/Cargo.toml --all-targets
```

Acceptance evidence includes exact dependency/runtime SQLite versions and pragmas,
commands/exits/counts, first failure, crash boundary/result, artifact manifest,
preserved unrelated files and explicit absent-platform coverage. Independent review
must inspect actual atomicity/import evidence, not merely fmt/clippy. Final shared
workspace checks are owned by integration; do not rerun unchanged full0.2 tests
as a substitute for this lane's evidence.

## Official sources to open before implementation

[rusqlite Connection](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Connection.html),
[Transaction](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Transaction.html),
[feature manifest](https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/Cargo.toml),
[SQLite WAL](https://www.sqlite.org/wal.html),
[transactions](https://www.sqlite.org/lang_transaction.html),
[backup](https://www.sqlite.org/backup.html),
[foreign keys](https://www.sqlite.org/foreignkeys.html),
[Tokio blocking work](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

## Stop/escalation and completion

Do not choose a new source-of-truth, compatibility shim, encryption/key-management
scheme or destructive authority switch to resolve a local test. Route uncovered
durable contracts to Architect with precise evidence. Implementation report may
conclude IMPLEMENTED only after the activated Plan's required joins; it never
self-accepts, migrates current Root, publishes or creates a release.
