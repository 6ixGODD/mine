# Application seams and transport contract

This is detailed target Design under ADR24–31, not an installed API. The names
below are finite internal commands, not arbitrary SDK/SQL/shell forwarding.
The common types are in `v1/contracts`; transaction invariants and table ownership
are in [transaction contracts](transaction-contracts.md). Provider observations
are in [runtime contracts](runtime-contracts.md).

## Common call boundary

Every Project command is `CommandMeta + closed typed payload`; global commands
use `ControlCommandMeta` without inventing a Project ID before registration.
Authentication supplies ActorContext separately; recompute/compare input digest
from the validated typed command instead of trusting the caller's assertion.
Entity revision is per primary target, not a global
Project lock. Payload lists additional expected revisions for cross-entity CAS.
Response is `Applied | Replayed { result, entity_revisions, event_cursor }` or
`Error { code, safe_details, operation_id?, current_revision? }`. Effect-bearing
success means durable intent accepted, not physical completion; its result names
the operation and stage. Queries never perform hidden recovery mutations.

Amounts/counts/revisions use checked integers, identifiers use distinct types,
times are UTC milliseconds, text is UTF-8. No caller-supplied actor, raw shell,
SQL, arbitrary URL callback or absolute file read is accepted by these commands.
Owned file effects use validated manifests and artifact IDs. A generated schema
and Rust/TS round-trip fixtures are required before a transport is advertised.

## Domain-owned seams

| Owner | Commands and required payload | Result / rejection boundary |
|---|---|---|
| Projects | `RegisterProject(root, repository_identity, operation_id)`; `RelocateProject(project, expected_identity, new_root)`; `PreviewDeinit(project)`; `BeginDeinit(project, manifest_digest, archive_choice)`; `ReconcileDirectory(operation)` | Stable ProjectId plus operation; cleanup manifest exact paths/kinds/ownership hashes, blockers and expiry revision. Changed manifest refuses; missing directory is unavailable, not cleaned. No implicit duplicate Project for a linked worktree. |
| Identity | `ConsumeEnrollment(grant, provider_binding)`; `Assign(plan, executor, reviewer, plan_revision)`; `BeginHandoff(plan, to_principal, reason, authority_evidence)`; `AdvanceHandoff(operation, observation_artifacts)` | Principal/short ID or staged handoff; assigned owner/epoch/history snapshot. Grants are secret one-use input, redacted from logs/results. Failed stop never grants new writer; historical contribution rejects self-review. |
| Work/lifecycle | `RegisterPlan(body_artifact, dependencies, namespace_scope)`; `TransitionPlan(plan, intended_transition, evidence)`; `ClaimNode(node, attempt, assignment_epoch, resource_grants, budget_reservation)`; `Checkpoint(node, artifacts, pending_operations)`; `PauseProject(reason)`; `ResumeFrontier(expected_pause_revision, eligible_nodes)` | Plan/WorkNode snapshots and effects; retained stored-anchor/receipt/cleanup gates. Claim checks every required authority, no synthesized ACCEPTED, no session state used as Plan completion. |
| Coordination | `Inform(room_or_recipients, body, evidence)`; `RequestAndWait(responder, criterion_digest, dependent_node?, continuation?, deadline)`; `Respond(obligation, commit_or_decline, reason?)`; `Resolve(obligation, result_or_failure, evidence)`; `Cancel(obligation)`; `RaiseIssue(node, category, impact, requested_decision)` | Message/obligation IDs, durable wait or outcome. Requests have one accountable responder; fanout creates explicit children and all/any join. Cancelled requires quiescence. Foreign sender/recipient/room refuses. |
| Resources | `RequestSet(items, project, attempt, purpose, deadline)`; `ConfirmGrant(operation)`; `ReleaseGrant(grant, epoch, quiescence_artifact)`; `ReconcileHolder(grant, observation)`; `InviteChannel(participants, resource_set, purpose, expiry)`; `AcceptOffer(channel, offer_version, terms_digest)` | Whole-set queued/reserved/granted or uncertain, never partial grant. Membership authorizes only channel disclosure. Accepted offer still requires coordinator grant and old-holder release. |
| Autonomy | `EnablePolicy(policy_artifact, owner_approval)`; `DisablePolicy(expected_policy_revision)`; `ClassifyDecision(question_digest, category, rationale)`; `EvaluateTechnicalApproval(adr_digest, policy_version)`; `ReserveBudget(node, attempt, allowance)`; `SettleUsage(reservation, provider_observation)` | Policy-authorized technical decision or needs-human issue, reservation or refusal. Unknown cost is explicit; no raised limit, role expansion, publication grant or provider quota evasion. |
| Runtime | `Discover`, `Create`, `Resume`, `Rename`, `Submit`, `Interrupt`, `Inspect`, `Shutdown` through versioned adapter port | Observations only; supervisor owns admission/recovery, persist intent before spawn and exact native binding before next action. |

The table freezes responsibility and required data, not every private implementation
function. Domain DTO exports live with their owning leaf packages; shared IDs,
error/envelope/schema changes have one foundation/integration owner. No duplicated
authority between CLI, MCP, HTTP or provider workers.

## Deterministic bounds and scheduling defaults

Initial local defaults are policy configuration, visible and overridable within
owner authorization: 1 MiB command/frame, 16 KiB inline message body, page limit
100 (maximum500), 256 pending writer commands per open store, 5-second busy budget,
30-second ordinary transport deadline. Binary/artifact contents are referenced,
not embedded in messages. A deadline is not proof an external effect did not run.
Queues apply backpressure with a retrievable operation ID, not unbounded RAM.

Timers use injected clocks. Retry delays are bounded exponential backoff with
jitter, maximum3 attempts for definitively retryable not-sent effects; UNKNOWN
requires reconciliation. Exact provider policy may be stricter. No LLM polling.
Resource ordering is priority with aging; equal priority uses created-time then ID.
An entire requested resource set is evaluated atomically; uncertainty holds capacity.

## Local HTTP and SSE seam

Rust Axum is a transport adapter, not business authority. Bind only `127.0.0.1`
or explicitly tested `::1`; do not copy framework examples binding0.0.0.0.
The endpoint set is intentionally small:

- `GET /api/v1/projects`: authorized directory projection, unavailable entries kept.
- `GET /api/v1/projects/{project}/snapshot`: authorized current summary/frontier.
- `GET /api/v1/projects/{project}/events?after={cursor}`: bounded SSE replay; event
  IDs are committed cursors. Compacted/foreign cursor emits reset-required.
- `GET /api/v1/projects/{project}/{collection}` for the closed collections
  `plans`, `nodes`, `decisions`, `agents`, `conversations`, `evidence`, `operations`;
  keyset pagination and typed filters only.
- `POST /api/v1/projects/{project}/commands`: closed domain command union.
- `POST /api/v1/control/commands`: authorized init/deinit/resource/channel commands.
- `POST /api/v1/session`: one-use local pairing token exchange; `DELETE` ends session.

CLI/MCP call the same dispatcher over authenticated local service transport. Native
agent grants are Project/role/attempt scoped, distinct from the owner's browser
session. Project path in a URL never authorizes access. Unknown routes/methods deny.

Owner pairing requires a locally generated256-bit secret, kept in user-restricted
control files and entered through the local launcher flow; never URL query/logs.
Exchange once for a rotated HttpOnly SameSite=Strict session cookie; synchronize
CSRF token with the session and require header on browser mutations. Verify exact
Host/Origin (including port), deny cross-origin CORS and `Origin:null`; reject DNS
rebinding. Session expiry/revocation invalidates live streams. Same-user arbitrary
filesystem readers remain the approved trust limit, not an OS isolation claim.
Use CSP default-src self, no inline script/eval, frame-ancestors none, nosniff,
no-referrer; render sanitized Markdown without executable HTML. No credentials
in localStorage, session URLs, messages or fixtures. SSE uses same-origin cookie
auth, never long-lived bearer tokens in a query string.

HTTP mapping: invalid400, unauthenticated401, forbidden403, absent404, revision/
epoch/idempotency/capability-stale409, busy429 with bounded retry, pending202 with
operation locator, corruption/IO500 redacted, unsupported-version426. UNKNOWN
is a typed pending operation with outcome_unknown, not a retryable200 completion.
Keep retry identity; failed network response prompts lookup, not a new command.

## Read models and test seams

Snapshot pages always carry source cursor and capture time. View models expose
wait owner/criterion/state, last checkpoint, next admissible action, assignment
history and selection provenance. Percent complete cannot be guessed from tokens.
Fixtures include paused, stale, offline, unknown effect, reset and rejected action.
The demo shell is fixture-only; production mode cannot silently fall back to it.

Each domain supplies a pure handler contract suite and a SQL mapping suite. HTTP
uses an injected typed dispatcher; UI uses transport fixtures. Real integrated
tests must replace both with actual stores/services. Package-local dependency
injection is a testing boundary, not a general plugin framework.

## Primary source register — opened2026-10-16

| Source | Decision supported |
|---|---|
| [Axum0.8.9](https://docs.rs/axum/0.8.9/axum/) | Typed routes/state/error conversion, Rust service sharing; pin in transport package |
| [WHATWG SSE](https://html.spec.whatwg.org/multipage/server-sent-events.html) | Event ID/reconnect behavior; app defines cursor reset and authorization |
| [OWASP CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html) | SameSite alone is insufficient; explicit request authentication/origin/token checks |
| [SQLite transaction](https://www.sqlite.org/lang_transaction.html) | Short serialized transactions; no external effect inside transaction |
| [RabbitMQ acknowledgments](https://www.rabbitmq.com/docs/confirms) | Delivery confirmation and work completion are distinct; no broker dependency |
| [Temporal workflow execution](https://docs.temporal.io/workflow-execution) | Durable continuation concept; no Temporal dependency or LLM wait loop |
