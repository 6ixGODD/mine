# MINE 1.0 — Project Control Plane (Approved Target)

## Status and boundary

Requirement-first **approved target, not implemented**. The owner explicitly
approved ADR-0024 through ADR-0031 on2026-10-16; registry revision70 records the
unchanged proposed body digests and owner approval. Scoped readiness was true
for that baseline; the later ADR32 supplement was separately approved through
the owner's explicit coordination instruction at registry revision 73. Neither gate
result proves implementation or delivery.
Owner requests on2026-10-16 are the requirements; REQUIREMENTS.md is absent.
Eight ADRs24-31 record the digest-bound approved material decisions.
The later timing/logging supplement is separately approved as ADR-0032, not
retrospectively included in the earlier eight-ADR approval. Detailed shared
schemas still need foundation implementation/review before Plan activation.
Logical planning scope is `v1.0.0`; current0.2 registrations use the logical
planning token `docs/design`, not a promise to retain that physical directory.
0.2.0 retains TOML authority and OpenCode support.
These documents remain at the current installed0.1.13 bootstrap paths until the
approved0.2 terminal layout cutover carries them byte-for-byte into `.mine/design/`.
No future command/field named here is claimed as an installed interface.

## Goals and invariants

Project is the only business aggregate. Plans, work nodes, members, sessions,
decisions, evidence and conversations belong to it; Git repositories/worktrees
are execution resources, not another competing top-level product entity.
One user-local application lists projects and serves a unified dashboard.
The deterministic Rust core owns authority and recovery; LLM agents decide/work
only within recorded assignments and permissions. Durable protocol obligations,
not conversational reminders, drive waiting and wakeup.

- One authority per fact; UI, CLI and agent tools use identical transactions.
- One live writer per assigned worktree; identity is not a provider session.
- Waiting survives restart and costs no repeated inference. Events wake only
  affected authorized work; no broadcast-to-all inference by default.
- Every Plan assignment and session replacement is historically attributable.
- Independent review remains real, including in experimental autonomy.
- Pause, unknown outcomes and missing resources are visible; never fake done.
- Mutable outputs stay isolated by Project/Plan; shared resources are explicit.
- OpenCode removal happens only in1.0; Codex/Claude Code/Pi have capability-based
  official integrations, not one assumed universal SDK contract.

## Component boundaries

```text
User-local MINE control plane (Rust)
  Global directory + shared-resource coordinator + unified Web/API
    Project A application services ---- Project A SQLite + human Markdown
      assignments / work nodes / protocol / decisions / evidence
        session supervisor ---- Codex | Claude SDK worker | Pi runtime
    Project B application services ---- Project B SQLite + human Markdown
  Cross-project channels: explicit minimal disclosure, no inherited authority
```

No separate LLM manager for routine routing/heartbeats/retries. Global service
is a coordinator, not an extra business aggregate or project owner. Domain
policies do not depend on SQLite, HTTP, React or provider SDKs. Application
services expose narrow identity, persistence, runtime and resource ports at
these demonstrated variation boundaries; no generic plugin marketplace.

| Read next | Contract / ADR |
|---|---|
| [Projects and deinit](projects.md) | Global registration, location, cleanup /24 |
| [Identity and handoff](identity.md) | Roles, assignment, traceable transfers /25 |
| [Storage](storage.md) | SQLite authority, outbox, migration and recovery /26 |
| [Coordination protocol](coordination.md) | Commitments, escalation, group/DM /27 |
| [Runtime and recovery](runtime.md) | SDKs, models/effort/fast, attempts, pause /28 |
| [Dashboard](dashboard.md) | Unified project navigation, work-node visualization /29 |
| [Autonomy](autonomy.md) | Explicit opt-in, technical approvals, budgets /30 |
| [Cross-project resources](resources.md) | Admin channels and host arbitration /31 |
| [Sources and reality](sources.md) | Opened primary sources, implementation gaps |
| [Approval handoff](approval.md) | Exact eight approved ADR digests and tradeoffs |
| [Foundation](foundation.md) | Isolated executable contracts, current authority and quality |
| [Transaction contracts](transaction-contracts.md) | Typed transaction seams, schema ownership and atomic joins |
| [Application seams](service-contracts.md) | Commands, errors, local HTTP/SSE and shared consumers |
| [Runtime contract](runtime-contracts.md) | Closed adapter wire, capabilities, quiescence and failure outcomes |
| [Parallel delivery blueprint](planning/index.md) | Eleven bounded Plans, early integration and deferred native activation |
| [Timing and bounded logs](observability.md) | Approved ADR32 target; implementation and validation pending |

## Approval and delivery

Read ADR24-31 through the native decisions registry. Approval came from the
owner's explicit instruction, not an agent or future autonomy policy approving
itself. Scope `v1.0.0` deliberately separates release obligations;
future dependent plans must declare it, not use `docs/design` to bypass readiness.
The later planning pass refines schema/API/error and runtime contracts and builds
isolated executable foundations. Its eleven detailed blueprints remain unregistered
until0.2 closure; no1.0 execution Plan is created merely by approval or prototype tests.
Existing approved TOML/manual-approval/agent-support contracts remain operative
until their explicit1.0 replacement becomes effective; do not globally supersede
them while0.2 is still in flight.

Delivery must prove multi-project isolation, real three-provider native sessions,
runtime-scoped model selection, durable waits, crash/pause recovery, handoff
fencing, decision consistency, autonomy limits, safe deinit and accessible UI.
No prototype screenshot, SDK method list or unit-only suite proves that loop.
