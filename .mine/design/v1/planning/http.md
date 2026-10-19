# plan32: Authenticated local service, projections and SSE

## Status and activation

UNREGISTERED BLUEPRINT / NOT EXECUTABLE. Target1.0.0, provisional canonical ID
`plan32`. Materialize as `.mine/plan/plan32-http.md` only through the
[activation protocol](index.md). Current0.2 graph is unchanged. All runtime
verification below is NOT_RUN by planning; foundation checks are separate evidence.

## Goal and user outcome

One loopback service provides the same authoritative operations to browser, CLI and agent tools, with stale-state conflicts and restart-safe event replay.

## Governing Design and decisions

Read [dashboard](../dashboard.md), [service seams](../service-contracts.md),
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
Owner exclusively writes `v1/crates/http/**`, its private manifest/lock, tests,
and package-local quality configuration;
plus its exact implementation report. Read other domain packages/contracts only.
Forbidden: Root manifests/lockfiles, src, workflows, Skills/generated copies,
other lanes' migrations/reports, graph/ADR edits, current0.2 worktrees, stable,
global Agent config or installed MINE. Internal module splitting within this
responsibility is allowed without filename-by-filename approvals.

## Work packages

### WP1 — Local service lifecycle and authentication

Create src/server.rs, auth.rs, origin.rs and routes.rs using pinned Axum0.8.9 and actual supported Tokio. Implement serialized user-local startup discovery, restrictive credential files and one-use pairing; owner sessions differ from Project/attempt grants. Strict Host/Origin/CSRF/CORS checks apply before command dispatch. Never bind wildcard or expose arbitrary shell/file/SDK endpoints.

Deterministic evidence: tests/security.rs: DNS rebinding Host, null/cross origin, missing CSRF, revoked/expired token, replayed pairing, unauthorized Project, unknown route/method, bad content type/oversized JSON, secrets absent logs/URLs.

Inputs: governing typed contracts plus WP0 foundation artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(http): local service lifecycle and authentication`.

### WP2 — Typed dispatcher and revisioned event projection

Create src/commands.rs, queries.rs and events.rs following service-contracts exact routes/envelopes. Call injected typed application services, no local business state. Finite snapshot query releases DB read before SSE wait; event IDs use committed cursor. Reconnect replay or reset after retention, bounded slow-reader disconnect and safe resnapshot. Lost command response queries same operation; duplicate key with changed payload conflicts.

Deterministic evidence: tests/protocol.rs: stale browser vs session decision one winner, duplicate click replay, paused snapshot, unavailable Project, SSE gap/reset/reconnect/slow reader and subscription revocation.

Inputs: governing typed contracts plus WP1 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(http): typed dispatcher and revisioned event projection`.

### WP3 — Static production surface and service operations

Create src/assets.rs and health.rs. Serve built immutable UI assets with CSP/nosniff/referrer headers and no cache for private API. Traverse-proof paths and sanitized evidence references; page/payload bounds. Health exposes availability/backlog/reconciliation without secrets. Graceful stop halts admission and persists pending operations; service death must not erase Project state.

Deterministic evidence: tests/assets.rs: traversal, malformed percent encoding, XSS Markdown fixture, no inline eval, deep route asset behavior, stop during command, no long-lived DB transaction.

Inputs: governing typed contracts plus WP2 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(http): static production surface and service operations`.

## Integration artifact and closed loop

Deliver Router/facade accepting actual service dependencies plus localhost test harness. Stub-dispatch tests enable parallel work, but final join must use real SQLite and owner/agent authentication.

Supply exported symbols/DTOs, migration digest, entry adapter and required dependency
changes in the report. Integrator merges accepted packages incrementally and runs
the first coherent vertical slice early. Neither shared mutable output nor changing
another lane's files is a shortcut. Immutable-plan changes follow clarification or
compensation; transport interruptions resume the same operation.

## Verification matrix

Create an independent Rust manifest `v1/crates/http/Cargo.toml` and declared test
targets above. Commands run with `CARGO_TARGET_DIR=<Root>/.mine/build/plan32/cargo-target`:

| Gate | Command | Expected evidence |
|---|---|---|
| Focused | `cargo test --manifest-path v1/crates/http/Cargo.toml` | Named unit/integration cases, exact counts |
| Format | `cargo fmt --manifest-path v1/crates/http/Cargo.toml -- --check` | exit0 |
| Lint | `cargo clippy --manifest-path v1/crates/http/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code` | no ignored diagnostic |
| Build | `cargo build --manifest-path v1/crates/http/Cargo.toml --all-targets --all-features` | exact isolated package artifacts |
| Docs | `cargo doc --manifest-path v1/crates/http/Cargo.toml --no-deps` with RUSTDOCFLAGS=-D warnings | no broken contracts |

All checks also include `git diff --check`, owned changed-file audit and dependency
vulnerability/license review appropriate to actual new dependencies. All fixtures,
logs, HOME, generated/build/browser output use Root `.mine/build/plan32/<class>`;
Root toolchains/downloads/immutable environments are reused. No local target,
node_modules or venv in the linked worktree; exceptional disk cost needs agreement.

## Research source register

Primary pages opened2026-10-16; executor/reviewer reopen against pinned versions.

| Source | Verified claim / implementation consequence |
|---|---|
| [Primary source](https://docs.rs/axum/0.8.9/axum/) | Typed routing/state and transport composition. |
| [Primary source](https://html.spec.whatwg.org/multipage/server-sent-events.html) | Event IDs and reconnect protocol. |
| [Primary source](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html) | Explicit same-origin/CSRF controls; loopback alone is insufficient. |

## Acceptance and report

- Every work package and failure case has real evidence, not merely API presence.
- Domain memory/transport fixtures are labeled separately from SQLite/native/E2E.
- Cross-Project authorization, CAS, idempotency, pause and unknown-outcome semantics
  preserve the common contracts; no generic retry of uncertain external effects.
- No unrelated scope, secret, current0.2 mutation or external publication.
- Executor records IMPLEMENTED only after required gates. Independent reviewer
  verifies exact tip, integrates into Root dev, removes only clean accepted owned
  worktree/branch, then ACCEPTED through released MINE. Rejection preserves lineage.

Report after activation: `.mine/plan/reports/plan32-http-implementation.md`.
Native/platform/credential gaps are explicit blockers, not silently waived tests.
