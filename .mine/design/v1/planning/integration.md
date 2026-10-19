# plan35: Continuous Integration Harness and Complete 1.0 Candidate

## Status and start/finish gates

UNREGISTERED BLUEPRINT, NOT EXECUTABLE until [activation](index.md). Provisional
ID plan35, target1.0.0. Native hard predecessors are empty so WP1 can start beside
the ten independent leaf Plans. **WP3/4 must wait for all ten leaf Plans ACCEPTED**;
the executor cannot declare IMPLEMENTED before the complete join. This explicit
internal gate replaces a wasteful plan chain, not independent review or truth.

Governing Design: every leaf in [v1 index](../index.md), especially
[service](../service-contracts.md), [transaction](../transaction-contracts.md),
[runtime](../runtime-contracts.md), [foundation](../foundation.md), and
[parallel protocol](parallel-protocol.md). ADR24–31 owner approval covers target;
no current0.2 migration/publication is authorized by this blueprint.

## Ownership

Own `v1/integration/**`, Root manifests/locks, composition/CLI/MCP wiring in src,
root tests for cross-feature delivery, CI/package/install assets, root Skills and
generated plugin payload transaction, bilingual user docs/metadata and own report.
Own static migration catalogue/shared contract generation after foundation freeze.
Do not edit leaf namespaces to win a merge; request owned fixes or review-scoped
correction. No independent parallel writer touches these central locations.
Approved ADR bodies/graph are never direct edits. Current0.2 remains out of scope.

## WP1 — Harness early, not at the end

Create `v1/integration/Cargo.toml`, `src/composition.rs`, `tests/vertical_slice.rs`,
`tests/recovery.rs`, `tests/security.rs`, and typed fixture builders. Private lock,
isolated output. Assemble only actually available accepted packages through their
exported ports; absent domains remain explicit NOT_RUN, not production stubs.
Prepare test assertions while leaves develop. Root default product wiring waits
until the relevant accepted slice exists; no fabricated API to appease a test.

First useful slice, as soon as its participants are accepted: two Projects,
authenticated assigned identities, REQUEST_RESULT+wait, real SQLite restart,
RESULT, one effective continuation wake, HTTP event cursor and UI projection.
Inject result-before-subscription, duplicate delivery and paused waiter. This is
the first interoperability checkpoint, not a claim all1.0 features are complete.
Record exact constituent commits and request shared-contract corrections once.

## WP2 — Incremental accepted integration and single-owner wiring

Wire accepted packages into Rust service composition, current CLI/MCP consumers
and owned provider workers. Keep one business command dispatcher, real authority
and finite routes. Compile module migration catalogue with exact SQL digests;
validate all referenced FKs before admissions. Generate schemas/clients once per
shared contract revision, update consumers through owners and check drift.

Implement Project/service startup, build/static UI and worker packaging, version
checks and capability probes. No arbitrary Node or SDK download at first agent
prompt without a verified/pinned installation contract. Include required official
licenses and reproducible dependency manifests; verify runtime paths containing
spaces/non-ASCII. Bundle policy stays local-only; non-loopback hosting is excluded.

Update all five Skills after actual command/MCP help is inspectable: role grants,
assignment/handoff/blocker route, exact continuation recovery, SQLite authority,
Project-scoped discovery, default-off autonomy, evidence reuse and publish boundary.
Remove OpenCode from active support/setup/distribution in1.0 only; preserve
historical reports and native identities. No unused old aliases/dual TOML writer,
duplicate registry or mock fallback survives production wiring.

## WP3 — Complete real join (all leaf Plans ACCEPTED)

Create/execute `tests/v1_closed_loop.rs` and `tests/v1_migration.rs` against real
services/stores. The mandatory scenario matrix is:

| Scenario | Decisive observation |
|---|---|
| Two Project registration/move/remove | One stable ID; interrupted stages reconcile; no foreign file deletion |
| Auth and assignment race | Forged/foreign/stale token fails; one CAS owner; short ID is not credential |
| A-to-B dirty handoff + pending result | Old writer quiescent, exact dirty hashes survive, history retains A, result reaches B once |
| Same-owner session replacement | New attempt not fabricated new ownership; unknown create never double-spawns |
| Wait/offline/early-result/cycle | No polling LLM turns; explicit obligation outcome; one effective wake; bounded Architect escalation |
| User pause + OS/service restart | PAUSING until proven stop; explicit pause persists; resume only eligible frontier |
| Browser and Architect simultaneous decision | Exact same question digest; one committed winner; stale loser sees conflict |
| Models/effort/fast in two distinct Projects | Actual runtime configuration/profile discovery, requested/effective provenance, unsupported rejected |
| Resource contention + cross-Project channel | Whole-set grant, fairness, uncertain old holder blocks double grant, private membership enforced |
| Autonomy | Explicit opt-in, covered technical approval actor=policy, business question to user, bounded budget/retries, independent review |
| Quiescent0.2 import/export/restore | Raw ADR/Plan hashes, history/anchors/receipts preserved, old writers refused, no TOML dual authority |
| Security and retention | Host/Origin/CSRF/auth/XSS tests, revoked streams, no leaked secrets, exact artifact/history retention |

Each provider needs real native create/name/select/send/exact-resume/interrupt/
quiescence/config-parity evidence on supported Windows/Linux/macOS combinations.
Missing credentials/hosts/explicit smoke budget is BLOCKED, not a fake-adapter PASS.
Use preauthorized minimal inference budget; no model substitution or quota evasion.
Service/worker cancellation evidence includes surviving local/remote tools.

## WP4 — Candidate, migration handoff and quality gates

Align actual managed versions to1.0.0 only after leaf acceptance. Build the real
release artifacts, not just Rust library packages. Test clean install/upgrade,
offline/incompatible runtime diagnostics, exact artifact checksums, native launch,
static assets/worker locations and project-scoped configuration. Preserve approved
release0.2 safety while extending it to the real1.0 artifact set.

Run narrow checks first, then applicable final Root cargo fmt/clippy/build/test/doc,
all new package/TS/schema/UI gates, `python scripts/sync-plugin-assets.py --check`,
`python scripts/verify.py`, Design/ADR/graph validation through the released binary,
candidate scanner/version/distribution checks and final native install workflow.
Canonical commands/scripts added by this lane must exist and be recorded with
actual help before report handoff. Never use a future candidate to mutate real
development governance; its state migration tests use isolated fixtures.

Example isolated harness commands after targets exist:

```text
cargo test --manifest-path v1/integration/Cargo.toml --test vertical_slice
cargo test --manifest-path v1/integration/Cargo.toml --test recovery
cargo test --manifest-path v1/integration/Cargo.toml --test security
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings -W unsafe-code
cargo fmt --all -- --check
cargo build --all-targets --all-features
cargo doc --no-deps --all-features
```

All output is Root `.mine/build/plan35/<class>`; set CARGO_TARGET_DIR explicitly,
RUSTDOCFLAGS=-D warnings, use Root-managed dependency environments. Separate native
OS/architecture receipts and generated products. Input-bound evidence may be reused;
unsupported/missing native results remain visible. No repeated full tests for a
new review agent when the exact evidence is still valid.

## Research and completion

Primary sources opened2026-10-16 are attached to the governing service/runtime/
storage leaves and every leaf blueprint; reopen exact pinned upstream versions.
[Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
governs package/lock integration, [SQLite backup](https://www.sqlite.org/backup.html)
governs consistent export, [SSE](https://html.spec.whatwg.org/multipage/server-sent-events.html)
governs reconnect, and [shadcn CLI](https://ui.shadcn.com/docs/cli) governs actual UI
generation. None proves MINE behavior without the tests above.

Deliver complete candidate, source/dependency/artifact/evidence ledger, explicit
native matrix, migration/restore/operator guide and
`.mine/plan/reports/plan35-integration-implementation.md`. Executor may only record
IMPLEMENTED. Independent reviewer owns integrated acceptance/cleanup. Separate
full sync and release closure follow; this Plan grants no push/tag/publish or
global self-upgrade permission. Do not defer an approved requirement silently.
