# Cross-Project Coordination and Shared Resources — Approved 1.0 Target (ADR-0031)

## Global arbitration, Project authority

Projects may contend for ports, GPU/memory/disk budgets, test appliances, external
rate budgets or exceptional expensive shared build environments. Global coordinator
owns registered resource IDs, capacity/conflict modes, requests/queue, grants and
uncertainty. Resource request names Project/principal/attempt, required quantity,
purpose, epoch and deadline/priority policy. Project Lead authorizes its project
within human-delegated limits; it cannot seize another project's resources.

Normal policy is deterministic queueing/capacity admission with aging/fairness;
no LLM negotiation for every CPU slot. Acquire required resource sets in one
global short transaction or queue the whole set; avoid partial hold-and-wait.
Lease grant is not a promise of physical capacity unless enforced by the host;
recheck measured disk/runtime capacity and report external untracked usage.
No silent preemption of an active tool because another project claims urgency.

Plan-isolated mutable artifacts remain default. Exceptional shared-output use
requires an approved serialized strategy, exact ownership, invalidation and cleanup
boundaries; a resource grant alone cannot make concurrent mutable build trees safe.
Global quota/queue policy does not override project budgets or model authorization.

## Temporary administrator channels

Each project's designated Architect/Project Lead may invite the other to a
temporary purpose-bound channel. Invitation accepted under each project's grant;
both endpoints carry Project ID and stable principal ID. Specify participants,
resource IDs, minimal disclosure, expiry and revocation. Same durable protocol as
project conversations: request, commit/decline, counterproposal, result/cancel.
One correlated negotiation thread and versioned offer prevents ping-pong updates.
Proposal acceptance does not itself create a resource lease; coordinator atomically
validates terms and grants or returns conflict to both sides.

Default shares resource requirements, constraints and a concise reason, not source,
full plans, transcripts or secrets. Channel membership grants neither cross-project
file access nor lifecycle approval. Nonparticipants cannot enumerate private room
history. Closed/expired channels reject new messages, retain bounded audit and
resolve outstanding waits with explicit outcome instead of silently dropping them.

Example: LODE holds exclusive integration appliance; MINE requests it. Routine
queueing returns a durable wait. If both need a schedule change, their Architects
agree to a checkpoint/release window; only confirmed old-holder release lets the
coordinator admit the new holder. Both projects' UI show the same global grant ID.

## Failure, fencing and escalation

Lease expiry is suspicion, not proof that an old tool stopped. Fenced services
must reject stale generations. For unfenceable disk/hardware writers, expired or
disconnected holders become UNCERTAIN until objective quiescence; no double grant.
Shutdown/reboot revalidates actual state. Deinit revokes new access but preserves
unresolved resource evidence globally rather than erasing an active holder.

If global service is unavailable, no new shared-resource admission; existing
effects obey their safe stopping rules. Project-independent read-only work may
continue. Cross-project decisions propagate through idempotent outbox/inbox pairs,
not an alleged atomic transaction across all project databases. Inspect one
authoritative global grant when projections lag.

Unresolvable resource shortage is one of the legitimate human decisions, presented
with measured capacity, affected nodes and options. Pause only affected work unless
all useful project paths depend on it. Detect wait cycles and route to Architects;
no repeated agent polls or “everyone is waiting” invisible deadlock.

Acceptance: two projects race, fairness/starvation, stale epoch, lost release reply,
offline administrator, revoked/expired channel, deinit with held resource, resource
failure mid-run, host restart and no cross-project information leak.
