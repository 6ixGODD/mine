# Request, Workflow and Tool Timing — Approved Target Supplement

Governed by [ADR-0032](../decisions/0032-request-timing-and-bounded-observability.md),
approved on explicit owner instruction at registry revision 73. This is target
Design, not implemented functionality. ADR24–31 remain approved; foundation code
is unchanged and shared DTO/SQL/query details still require pre-activation review.

## Coverage and meaning

Record every MINE-owned request attempt, including query, denial, invalid input,
replay, timeout and cancellation; every Plan execution and review cycle; arch,
planning, sync, release, handoff, checkpoint, restart and recovery stages; every
MINE-dispatched tool/SDK/Git call. Review queue time and actual review are separate.
Use dispatch instrumentation, not a prompt asking an agent to remember timestamps.
Provider-native calls are included only when the adapter receives reliable events;
mark unsupported boundaries explicitly. An external unmanaged session's elapsed
time is not measurable agent work. A Skill/manual invocation can explicitly open
and close a workflow attempt, but claimed boundaries retain their source and
cannot override native observed events. Close the loop through real future CLI
contracts and all Skills, not invented flags on installed 0.1.13.

One finite envelope carries schema version, event ID, invocation ID, logical
request/operation ID when present, trace/span/parent or causal-link IDs, attempt
ordinal, Project/Plan/node/principal/session-attempt identities when resolved,
controlled component/action/phase, source UTC, observed UTC, process-instance and
clock-domain identity, measured duration when known, outcome/error class and
coverage quality. Pre-auth/unresolved calls belong to the control scope with no
invented actor/Project. Validate untrusted correlation IDs; they confer no rights.
No arbitrary attributes map containing request arguments is accepted. Exact DTO
schema and shared fixtures are frozen by the foundation owner after ADR approval.

Retry retains logical operation identity but has a new invocation/span. A replay
is not a second effect or second completed Plan. Work and review cycles retain
their own attempt IDs across assignment changes. Causal links join asynchronous
messages/continuations; parent nesting is reserved for actual containment.

## Time accounting and recovery

- UTC milliseconds align with existing service DTOs and human display. Use
  checked monotonic duration within one process/clock domain; keep raw wall
  observations plus clock-quality flags if clocks move backwards or diverge.
- Record queued/admitted/dispatched/observed-start/completed separately. Provider
  round-trip duration is not model compute duration; unknown start remains unknown.
- Persist authoritative work-node phase changes in existing domain transactions.
  A restarted observer does not subtract its clock from a previous process's
  monotonic tick. Keep last observation and interrupted/unknown quality; never
  close a crashed span at restart time and call the gap measured active work.
- Pauses, peer/user/resource waits and offline/unknown gaps are visible. Within
  a node, exclusive phase intervals do not overlap; parallel child intervals may.
  Show parent elapsed versus sum of child work separately; compute overlap unions
  or causal critical paths only when the required coverage/ordering is provable.
- Exact monotonic intervals do not guarantee cross-platform suspend inclusion.
  Clock source/quality explains this; cross-reboot wall elapsed is labeled wall
  estimate, not CPU time or precise active duration. Open intervals remain open.
- p50/p95/max results include window, sample count, exclusions and coverage;
  approximate aggregate histograms are labeled. No false completion percentages
  or guessed attribution of an uninstrumented remainder.

## Persistence, retention and backpressure

Use existing Project/control stores for lifecycle facts and audit, span indexes,
daily aggregate projections, retention settings and coverage/cleanup cursors.
Storage owns telemetry migrations and raw log writer. Only redacted optional
details are in `.mine/runtime/logs/` (Project) and user `.mine/runtime/logs/`
(control), registered as diagnostic-owned artifacts. Never mix test/build output
or provider transcripts into this cleanup namespace. SQLite is the machine
index/config authority, not a second file-based workflow database.

ADR32's age/size/host budgets apply to **all active and closed raw segments**;
reserve byte budget before append, including index overhead and bounded buffering.
Do not silently compress an unlimited archive. Rotation is per-store serialized
and journaled: readers either find the exact segment or receive `expired`/gap,
never partial renamed data masquerading as complete. Enforce host allocations
through the existing control coordinator without opening/scanning every project
on each tool call. An unavailable project retains its allocation; do not delete
arbitrary files to reclaim it. Free-space reserve can trigger earlier pruning.

Project business/audit/handoff history and required unfinished-operation evidence
are not subject to these log quotas. Keep the minimal long-lived phase summaries
with their domain history; raw spans/aggregates can expire. References to expired
optional logs return retention metadata, not a false corruption of core evidence.
Pinned authoritative evidence never shares a disposable diagnostic segment.

Bound diagnostic events to 4 KiB and each store's in-memory queue to 1 MiB;
cap total resident logging queues at 8 MiB for the service. Bounded writer batches
yield to control mutations; no transaction spans SDK/Git calls. Skip recursive
instrumentation of the sink itself and use counters for its own health. Enforce
storage byte/page accounting plus bounded checkpoint/incremental reclamation;
deleting SQLite rows alone does not prove reduced disk use. Never run blocking
full VACUUM on each call or copy whole databases solely for logging.

Required audit cannot commit -> no new authority grant/mutation. Optional sink
cannot persist -> bounded explicit warning/coverage-degraded response and UI,
never reverse an already-successful mutation or create a new retry identity.
Persist a loss marker/counter once possible; after a crash without a durable
count show an unknown gap, not an invented exact loss count. Emergency stderr
is bounded/redacted; exhaustion is not permission for an unbounded fallback file.
Already-dispatched effects with unknown outcomes follow normal reconciliation.

## Access and user-facing analysis

Reuse Project authorization for detail and export. Global overview does not give
an agent access to other projects' timing, native IDs, message content or paths.
Allowlist safe arguments only; error output and stdout/stderr are secret-bearing
inputs, not automatically safe logs. No prompt/body/environment/header capture by
default; explicit diagnostics export still redacts and previews its exact scope.

Add a Project Timing view: workflow/Plan/review timeline, queue versus execution
versus waits, responsible party, retry/attempt drilldown, slow-tool ranking and
retention/coverage status. Project summary shows observed bottlenecks, not inferred
blame. Filters use IDs/tool class/time/window; paginated indexed queries and
revisioned events replace polling or loading all spans into browser memory.
No automatic upload or remote tracing service. Unavailable data remains visible.

## Research and implementation mapping

The [ADR source register](../decisions/0032-request-timing-and-bounded-observability.md#primary-research-opened-2026-10-16)
records opened primary sources and decisions. Common OTel correlation semantics
are reused; dependency on its SDK/collector is not required. Existing approved
bounded-file storage fits diagnostic details better than putting all raw output
in authoritative SQLite audit, or copying complete native transcripts.
See [parallel addendum](planning/observability-addendum.md) for owners,
tests and activation gate. No instrumentation or timing tests have run yet.
