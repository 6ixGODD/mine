# plan33: Project dashboard and actionable work frontier

## Status and activation

UNREGISTERED BLUEPRINT / NOT EXECUTABLE. Target1.0.0, provisional canonical ID
`plan33`. Materialize as `.mine/plan/plan33-dashboard.md` only through the
[activation protocol](index.md). Current0.2 graph is unchanged. All runtime
verification below is NOT_RUN by planning; foundation checks are separate evidence.

## Goal and user outcome

Users can follow work, decide genuine business questions, choose runtime models and recover stopped projects from one clear local dashboard.

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
Owner exclusively writes `v1/dashboard/**`, its private manifest/lock, tests,
and package-local quality configuration;
plus its exact implementation report. Read other domain packages/contracts only.
Forbidden: Root manifests/lockfiles, src, workflows, Skills/generated copies,
other lanes' migrations/reports, graph/ADR edits, current0.2 worktrees, stable,
global Agent config or installed MINE. Internal module splitting within this
responsibility is allowed without filename-by-filename approvals.

## Work packages

### WP1 — Promote shell to typed service client

Keep exact decoded/generated b2jI preset; extend src/ with api, queries and event state reducers. Production requires authenticated service and never silently loads demo fixtures. Consume generated shared DTOs, per-field requested/effective selection and bounded keyset pages. Add strict typecheck/lint/format/component/browser gates with lockfile-owned dependencies.

Deterministic evidence: tests/client: stale conflict, command pending/unknown, reconnect reset, HTTP unauthorized, unavailable Project, malformed DTO and no invented success. Fixture-only shell remains explicitly separate demo mode.

Inputs: governing typed contracts plus WP0 foundation artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(dashboard): promote shell to typed service client`.

### WP2 — Work, sessions and communication

Implement Projects/Overview/Work/Agents/Conversations/Evidence views with semantic components. Show node hierarchy/frontier, who owes a result, acceptance vs completion, checkpoint/next action, pending reconciliation and complete handoff timeline. Session form discovers models in selected Project context, separates model/effort/speed, disables unsupported/unknown capability and preserves requested/effective mismatch.

Deterministic evidence: Component tests: offline/busy/unknown, multi-Project switch, large paged lists, paused vs waiting, handoff A->B plus same-owner replacement, model discovery failure, no literal hardcoded model catalog.

Inputs: governing typed contracts plus WP1 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(dashboard): work, sessions and communication`.

### WP3 — Decisions, resources and recovery usability

Implement exact-digest decision cards with options/evidence, stale re-present, audit origin and shared backend transaction. Resource holders/queues/UNCERTAIN and temporary negotiation visible. Experimental autonomy opt-in shows scope/limits/cost warning; pause/stop reflect PAUSING until confirmed. Deinit always shows exact manifest preview and blockers. Keyboard/focus/contrast at desktop/narrow widths, dark/light, sanitized Markdown.

Deterministic evidence: Browser tests: two competing decisions one winner, repeated click same operation, reload after commit, result while paused, resume eligible frontier, inaccessible control refusal, screenshots1440/390px and automated accessibility.

Inputs: governing typed contracts plus WP2 artifacts.
Output: actual implementation, exact fixture inputs and failure evidence; no TODO
or successful fake response in production. Suggested commit: `feat(dashboard): decisions, resources and recovery usability`.

## Integration artifact and closed loop

Deliver real service-connected UI and declared browser/platform evidence; screenshots or fixture tests do not prove backend workflows. Final integration owns embedding/package assets and actual end-to-end services.

Supply exported symbols/DTOs, migration digest, entry adapter and required dependency
changes in the report. Integrator merges accepted packages incrementally and runs
the first coherent vertical slice early. Neither shared mutable output nor changing
another lane's files is a shortcut. Immutable-plan changes follow clarification or
compensation; transport interruptions resume the same operation.

## Verification matrix

From `v1/dashboard`, use the actual `npm run typecheck`, `npm test`,
`npm run build` and `npm run check-contract` commands. The executor adds and
verifies lint/format/component/browser modes before claiming them. The runner
is `scripts/run.mjs`; verify its command contract against the reviewed foundation.

| Gate | Required evidence |
|---|---|
| Typecheck/build | strict noEmit and production Vite bundle, exact private lock |
| Unit/component | typed state/client/form/render tests above |
| Browser/accessibility | authenticated service harness, narrow/desktop screenshots, keyboard/a11y |
| Contract drift | generated DTO parity and malformed input refusal |

All checks also include `git diff --check`, owned changed-file audit and dependency
vulnerability/license review appropriate to actual new dependencies. All fixtures,
logs, HOME, generated/build/browser output use Root `.mine/build/plan33/<class>`;
Root toolchains/downloads/immutable environments are reused. No local target,
node_modules or venv in the linked worktree; exceptional disk cost needs agreement.

## Research source register

Primary pages opened2026-10-16; executor/reviewer reopen against pinned versions.

| Source | Verified claim / implementation consequence |
|---|---|
| [Primary source](https://ui.shadcn.com/docs/changelog/2026-04-preset-commands) | Exact b2jI preset decode and provenance. |
| [Primary source](https://ui.shadcn.com/docs/installation/vite) | Selected React/Vite local app. |
| [Primary source](https://html.spec.whatwg.org/multipage/server-sent-events.html) | Cursor reconnect/replay. |
| [Primary source](https://www.typescriptlang.org/tsconfig/strict.html) | Strict TypeScript gate. |

## Acceptance and report

- Every work package and failure case has real evidence, not merely API presence.
- Domain memory/transport fixtures are labeled separately from SQLite/native/E2E.
- Cross-Project authorization, CAS, idempotency, pause and unknown-outcome semantics
  preserve the common contracts; no generic retry of uncertain external effects.
- No unrelated scope, secret, current0.2 mutation or external publication.
- Executor records IMPLEMENTED only after required gates. Independent reviewer
  verifies exact tip, integrates into Root dev, removes only clean accepted owned
  worktree/branch, then ACCEPTED through released MINE. Rejection preserves lineage.

Report after activation: `.mine/plan/reports/plan33-dashboard-implementation.md`.
Native/platform/credential gaps are explicit blockers, not silently waived tests.
