# ADR-0032: Correlated Timing and Bounded Local Observability

## Status and scope

Owner-review proposal, 2026-10-16; scope `v1.0.0` only. Not approved by the
earlier approval of ADR24–31. No 0.2 implementation or active Plan is changed.

## Requirement and existing boundary

The owner requires time records for every request, Plan execution, review and
other workflow, clear MINE tool-call logs, bounded retention, and analysis of
slow development nodes. ADR26 already separates durable SQLite business history
from bounded raw logs; ADR29 provides the local dashboard. They do not yet
define timing attribution, observability coverage or concrete retention defaults.

## Proposed decision

1. Instrument MINE-owned CLI/MCP/HTTP request ingress, domain dispatch, workflow
   phases, work/session attempts, Git/SDK/tool calls, retries and waits with one
   correlated structured envelope. Distinguish logical request/operation from
   each invocation and retry; associate Project/Plan/node/agent/attempt where
   available. Calls made outside MINE or not exposed by a provider are explicitly
   unobserved, never inferred from silence. No LLM invocation is needed for logging.
2. Record UTC event and observation times; calculate same-process intervals with
   a monotonic clock. Preserve durable phase boundaries across restarts, identify
   interrupted/unknown intervals, and never fabricate an end time or success.
   Separate total wall latency, active attempt duration, queue/resource/external/
   peer/user waits, pauses and retries. Parallel child sums are work, not elapsed
   time; missing instrumentation is not automatically attributable CPU/LLM time.
3. Keep authoritative lifecycle/decision/handoff evidence in its existing SQLite
   transactions. Optional diagnostics cannot authorize state or substitute for
   audit. Aggregate timing projections are rebuildable and visibly coverage-bound.
   Persist bounded redacted structured call details in owned rotating files,
   indexed by SQLite metadata; no remote collector or automatic upload is required.
4. Initial configurable defaults: detailed call logs expire after 14 days or a
   256 MiB per-Project budget, whichever comes first; global service logs have
   their own 256 MiB budget and all managed raw logs share a 2 GiB host budget.
   Segments are at most 16 MiB including the active segment. Aggregate timing
   projections retain up to 180 days and 64 MiB per store. Pinned recovery/audit
   evidence and business history are never deleted by diagnostic retention.
   These budgets do not claim the whole business database is bounded forever.
5. Redact before persistence using an allowlist: identifiers, controlled tool
   name/category, result/error class, times, sizes and counts. Do not log raw
   prompts, arbitrary arguments, headers, environment, file contents, tokens or
   complete stdout/stderr by default. Diagnostic access follows Project authority;
   exports are explicit, sanitized and separately budgeted. Native transcripts
   remain provider-owned and are not silently copied or deleted.
6. Use bounded queues/batches and serialized rotation; no per-call Git spawn,
   recursive scans, unbounded cardinality or long writer transaction. Required
   audit persistence failure prevents a new authoritative mutation. Optional log
   failure is reported as degraded coverage, not changed business success; an
   already-performed uncertain effect is reconciled, not blindly repeated.
7. Dashboard provides phase timelines, waits/responsible party, slow tools/nodes,
   retry cost and duration distributions with sample count/window/coverage.
   The feature remains deterministic and local. Fit it into existing 11 delivery
   lanes with one shared contract owner, not a new serial prerequisite Plan.

## Alternatives and consequences

- Raw text timestamps alone cannot reliably distinguish queueing, retries or
  concurrent work; complete permanent transcripts violate size/privacy goals.
- Sampling away ordinary top-level calls hides missing work. Normally record every
  MINE request/call boundary; only optional debug detail is sampled. Capacity or
  crash losses are explicit, so the product never promises impossible zero loss.
- Full external OpenTelemetry deployment is unnecessary for a local product.
  Reuse compatible correlation semantics, without adding a daemon or cloud service.
- Retention can remove detail before an investigation. The UI exposes the retained
  window, budget, gaps and aggregate precision; exports are deliberate user actions.

## Required evidence

Overlapping spans without double-counted elapsed time; duplicate logical requests
with distinct attempts; UTC jumps and suspend/reboot; kill during a request;
queue/lock/tool/provider waits; long review/pause and owner handoff; malformed and
unauthenticated requests without secret disclosure; concurrent rotation; age/size/
host quota pressure; full disk; missing logs; no mutation replay caused by logging
failure; audit/history untouched by cleanup; no cross-Project disclosure; measured
logging-on/off resource overhead and indexed bounded dashboard queries.

## Primary research, opened 2026-10-16

| Source | Verified implication |
|---|---|
| [OpenTelemetry Logs Data Model](https://opentelemetry.io/docs/specs/otel/logs/data-model/) | Distinguish event/observation timestamps and correlate trace/span identity; not itself a lifecycle or authorization model. |
| [Rust Instant](https://doc.rust-lang.org/stable/std/time/struct.Instant.html) | Monotonic interval measurement is process-local; suspend behavior/platform anomalies require explicit quality handling. |
| [OWASP Logging Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html) | Exclude sensitive data, test logging failures, protect access and enforce retention. |
| [SQLite pragmas](https://sqlite.org/pragma.html#pragma_auto_vacuum) | Deleting rows and reclaiming file pages are different; bound compaction/checkpoint work, not merely row counts. |

## Approval question

Approve local correlated timing, visible coverage gaps, privacy-safe bounded
diagnostic retention with the defaults above, and dashboard bottleneck analysis
as part of 1.0, without deleting authoritative history or adding a serial Plan?
