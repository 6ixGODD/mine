# Unified Dashboard and Decisions — Approved 1.0 Target (ADR-0029)

## Stack and layout

One user-local Rust service serves static React/TypeScript/Vite UI and authenticated
local API; no separate Next.js production server. Use shadcn preset **b2jI**, exact
[requested preset](https://ui.shadcn.com/create?preset=b2jI). Official CLI supports
preset creation/decoding. The later2026-10-16 foundation actually generated b2jI
using pinned shadcn4.21.5: Base UI/nova, neutral, Geist, large radius, Lucide.
See `v1/dashboard/README.md` for provenance and bounded demo evidence. This is not
production UI acceptance; actual light/dark/responsive service behavior remains
an implementation gate. Do not replace the requested preset with an approximation.

```text
Global: Projects | Resources | Service health        [New/open project]
  Project card: state / needs-user count / active nodes / budget / last seen

Project > Overview | Work | Decisions | Agents | Conversations | Evidence
  Needs you: product decision / hard resource / explicit pause (not all warnings)
  Current frontier: RUNNING / READY / WAITING / REVIEW / DONE
  Plan card -> package -> step; compact dependency graph toggle
  Right details: owner, model/effort/speed, last checkpoint, waits, next action
  Timeline: assignments A -> B, session attempts, decisions, retained receipts
```

Default is an actionable work frontier, not a wall of graph nodes or chat tokens.
Show why each waiting node waits, **who owes the result**, commitment state and
affected dependents. Reveal DAG edges on demand; visually distinguish hard Plan
dependencies, transient waits and resource holds. Whole-project pause and one
waiting package look different. Reboot landing offers “resume N eligible nodes”,
pending reconciliation and last checkpoint, not an empty dashboard.

Agent cards show stable ID, role, provider/native session, online/busy/offline/
unknown timestamp, current node and model tuple. Session creation selects role,
assignment and runtime-discovered model/effort/speed. Capability/credential errors
are inline; no global hardcoded dropdown. Native client inspection link/copy ID
is capability-based, not a fake guaranteed deep link.

Conversations show human-readable exchanges plus protocol badges: informational,
awaiting acceptance, committed, waiting result, completed, declined/escalated.
Resource panel shows current holder, queued projects, uncertainty and proposed
terms. Handoff timeline exposes all owners/attempts with exact evidence links.
Do not imply progress percentages from token count or hide incomplete native gates.

## Decisions share one transaction

Decision cards display issue/ADR revision and digest, author, evidence, options,
tradeoffs, recommendation, impact and permitted human action. User may decide
here or instruct the Architect session. Both submit the same authorized command
to Project service, binding exact question/body/revision and actor provenance.
Architect records “on explicit owner instruction” only with an attributable user
request; its own technical policy decisions are labeled separately.

Commit decision+audit+outbox once; UI updates from committed revision. A stale
browser or agent receives conflict plus latest state, not last-writer-wins.
Repeated click/retried tool response is idempotent. If the question changes while
user is deciding, re-present changed scope; don't apply old approval to new text.
SSE carries cursor/version; reconnect replays or resets snapshot after retention.
Lost UI connection does not undo a decision or silently submit it again.

## Safety, usability and quality

Bind loopback only by default; strong local session authentication, strict Host/
Origin checks, CSRF protection for browser mutations, narrow CORS, CSP and no
arbitrary path/shell routes. Treat Markdown/messages as untrusted rendered content,
sanitize HTML and prevent secret/token leakage into URLs/logs. Non-loopback
exposure is out of scope without explicit TLS/auth/deployment design approval.
Global UI visibility never grants Project administration or agent-role authority.

Keyboard-operable controls, semantic headings, readable focus/contrast, text/icons
in addition to color; calm information density, progressive disclosure, mobile
readability. Use virtualized/paged large lists and event-driven updates, not rapid
polling/long SQLite read transactions. Cost view distinguishes measured usage,
estimated price and unavailable values. Dangerous cleanup has exact preview.
Frontend adds pinned formatter/lint/strict typecheck, component/state-machine and
browser accessibility/e2e checks; exact package scripts determined after approval.
Rust quality baseline remains mandatory; no JavaScript core business authority.

Required end-to-end: two projects, offline agent, durable wait, fast result before
subscribe, crash recovery, user pause, simultaneous UI/session decisions, stale
decision, A-to-B handoff, failed model discovery and resource arbitration. Verify
screenshots at desktop/narrow widths and actual shadcn preset generation.
Primary: [shadcn CLI](https://ui.shadcn.com/docs/cli),
[SSE](https://html.spec.whatwg.org/multipage/server-sent-events.html),
[OWASP CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html).
