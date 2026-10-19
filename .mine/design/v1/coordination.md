# Durable Coordination Protocol — Approved 1.0 Target (ADR-0027)

## A protocol, not a chat wrapper

Uniform semantics across Codex, Claude Code and Pi. DM and project/Plan/incident
rooms are presentation/addressing; the same typed protocol owns obligations.
Messages identify authenticated Project/principal/attempt sender, explicit recipient
or membership snapshot, work/Plan, message/request/correlation/reply IDs, kind,
priority, concise body plus evidence pointers and any acceptance/deadline policy.
The caller cannot forge the sender. Group membership is not permission to access
all project files. Message content never grants roles, approval or shell authority.

| Kind | Meaning | Scheduler consequence |
|---|---|---|
| INFORM | Relevant fact; no response requested | Store/digest; neither party blocked or automatically awakened |
| REQUEST_RESULT | Named work/question with completion criterion and recipient | Durable reply obligation; optional dependent-node wait |
| COMMIT / DECLINE | Recipient accepts responsibility or refuses with reason/route | Acceptance is not completion; refusal reroutes/escalates |
| PROGRESS | Material changed estimate/checkpoint/risk | Update existing obligation, no reply-to-reply loop |
| RESULT / FAILED | Correlated outcome with evidence | Validate criterion; resolve wait or route failure once |
| DECISION_REQUIRED / DECISION | Versioned options/tradeoffs and authorized resolution | Wake only affected dependents after committed decision |
| RESOURCE_REQUEST / PROPOSAL | Explicit resource terms/negotiation | Separate coordinator grant required before use |
| CANCEL / CANCELLED | Request cancellation and confirm quiescence | No false completion or release merely on sent cancel |
| HANDOFF | Audited reassignment of responsibility | Rebind outstanding obligations; retain history |

Kinds are proposed domain concepts, not a frozen wire enum before ADR approval.
Each request has one accountable responder; multiple responders use explicit
child obligations and an `all`/`any` join criterion, not an ambiguous room-wide ask.

## Durable commitment and lost-wakeup prevention

```text
A requests R + registers wait W (same transaction) -> QUEUED
  B receives -> COMMITTED or DECLINED
  B resolves R with evidence -> RESULT_AVAILABLE
  scheduler records W runnable once -> A resumes continuation C
```

The system guarantees tracking, delivery/recovery and escalation, not that B can
always succeed. A may sleep immediately after durable request+wait registration;
before B's COMMIT it waits for acceptance, afterward for the result. Neither
requires repeated “are you done?” messages. Wait persists a continuation/work-node
ID and correlation, not a suspended OS thread or LLM context. Check already
available result and insert the subscription atomically to prevent fast-response
lost wakeup. Results received while paused are retained until authorized resume.

Message transport states (persisted, adapter accepted, agent acknowledged) are
separate from work states (committed, completed, failed, cancelled). Acceptance by
SDK or a delivered notification is not proof the task was done. Each group
recipient has an independent cursor/receipt. Retries deduplicate by stable ID;
same request cannot execute twice merely from duplicate transport. Impossible
exactly-once external side effects are not advertised.

Durable timers handle no acceptance, missed deadline, offline receiver and failed
delivery with bounded backoff. Escalate once per meaningful state/version, not
repeated LLM pings. Offline work queues; explicit policy may recreate a recipient
session. Expiration/decline returns a visible failed obligation; never silently
drop a request on which another node waits. Do not reassign silently.

## Blocker triage and technical ownership

The same issue object handles pre-start refusal and mid-execution blockage. Record
category, impact, affected node/dependency closure, evidence, responsible party,
requested decision and resolution revision. It is not automatically a Plan
REJECTED state. Paused/waiting/operational issue states do not rewrite Plan history.

1. Executor resolves bounded local implementation choices within accepted contracts.
2. A peer Executor resolves an owned dependency/artifact via REQUEST_RESULT. Scope
   conflicts become a coordinator ownership check, not permission to edit a peer.
3. Project Lead Architect receives missing contracts, irreducible technical choices,
   scope arbitration and any issue others cannot decide. It owns a concrete
   resolution/handoff; cannot bounce endlessly between peers or invent acceptance.
4. User receives only business/product ambiguity, authority/budget decisions or
   unavailable hard resources that the Architect cannot resolve within delegation.

Under the approved experimental policy, covered technical ADRs are policy-approved;
outside it the existing explicit approval boundary remains. Planning cannot silently
change immutable active Plans: use accepted clarification boundaries or explicit
rejection/compensation when materially necessary. The user-facing loop should not
request ceremonial approval for an already delegated technical implementation choice.

Only affected nodes/downstream joins wait; independent work continues. Project-wide
pause requires genuinely project-wide hard dependency/resource/authority failure,
explicit user pause, or a safety barrier. Track wait-for edges including obligations
and held resources; detect cycles and route a bounded explanation to Architect.
Do not label a cycle “waiting normally” forever. Do not hold scarce resources while
waiting when safe release/checkpoint is possible.

## Efficient delivery and costs

Presence ONLINE/BUSY/OFFLINE/UNKNOWN derives from owned runtime liveness and dated
heartbeats, not merely stored session ID. Routing, heartbeat, wait evaluation,
deduplication and timer dispatch are deterministic code, no inference.
Only actionable directed requests wake recipients by default. Informational room
traffic is cursor-based batched digest; raw context/evidence is fetched on demand.
Expose configurable message/context budgets, bounded summaries and original links;
never summarize away commitments/authority or repeatedly inject full transcripts.
Budget exhaustion is observable, not silent loss. Measure inference wakeups/request,
tokens per completed collaboration, duplicate wakes, queue age and unresolved cycles.
Acceptance: long wait with zero polling LLM turns; exactly one effective wake per
resolved wait; offline/restart/out-of-order/duplicate/cancellation/handoff tests.

Design comparison: [Temporal durable executions](https://docs.temporal.io/workflow-execution)
motivates persisted continuations; [RabbitMQ confirms](https://www.rabbitmq.com/docs/confirms)
distinguishes delivery acknowledgment from processing. MINE implements a bounded
local protocol over SQLite; neither service is a new runtime dependency.
