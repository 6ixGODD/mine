# Timing and Tool Logs — Approved-Scope Addendum to the Eleven Blueprints

Requirement received 2026-10-16. ADR-0032 is now APPROVED at registry revision 73.
This remains **unregistered and not executable**: common contract implementation,
independent review and the existing 0.2 closure/activation gate still apply.
No current immutable Plan or active graph node changes. This is an attached
scope delta, not a twelfth serial Plan. The existing activation gate still applies.

## Shared contract gate

After approval, the Architect foundation owner extends `v1/contracts` with closed
timing/correlation/outcome/coverage types, injected clock/sink ports and Rust/TS
fixtures. Review these before freezing/materializing the eleven Plans. Do not
give every executor a separate interpretation or instrument with free-form JSON.
Contract work is a small pre-activation correction, not a dependency on a finished
storage backend. Leaf lanes test against the same deterministic sink/clock.

## Sole owners and parallel packages

| Existing blueprint | Owned delta and real handoff |
|---|---|
| [Storage](storage.md) | `v1/crates/storage/src/telemetry/`, common telemetry migrations and `tests/telemetry.rs`: bounded file segments, SQLite indexes/aggregates/settings/coverage cursors, queue/batch limits, rotation/recovery and disk accounting; exports sink/query port. No lifecycle audit deletion. |
| [Identity](identity.md) | Own assignment/handoff phase events with original principal/attempt attribution; no rewriting old spans on transfer. |
| [Coordination](coordination.md) | Own durable peer/user wait boundaries, causal links and resolution reasons; no inferred active work while asleep. |
| [Projects](projects.md) | Own init/deinit scopes and declared diagnostic artifact cleanup; unavailable stores and Project access stay explicit. |
| [Resources](resources.md) | Own requested/queued/granted/released intervals and shared host logging-quota grants; grants are capacity, not access to foreign logs. |
| [Autonomy](autonomy.md) | Own scheduling/budget/decision wait timing; telemetry has zero model calls, does not auto-expand budget or approval authority. |
| [Lifecycle](lifecycle.md) | Own durable execution/review/arch/planning/sync/release phase facts and attempts; distinguish request completion from workflow completion. |
| [Runtime](runtime.md) | Supervisor owns adapter dispatch/response and retry spans; each provider owns native event mapping and unavailable-coverage declaration inside its existing namespace. Preserve effective model/effort/speed provenance without prompt logging. |
| [HTTP](http.md) | Instrument ingress/auth/rejection/dispatch and add typed authorized timing query/filter mapping; reuse common service, no arbitrary file endpoint. |
| [Dashboard](dashboard.md) | Own Timing view, timelines, latency/work distinctions, distributions and coverage/retention UI; fixtures first, real store join later. |
| [Integration](integration.md) | Sole owner of Root CLI/MCP/Skills and shipping hooks: every request/tool path, all Skill workflow boundaries, actual begin/end/recovery help, shared schema generation, log policy packaging and end-to-end benchmark. |

Every lane instruments only its namespace. Storage owns the sink, integration
owns the shared schema after activation; callers cannot write diagnostic files
directly. Domain exports attach narrow telemetry ports without importing SQLite
or creating a plugin system. Integration harness starts in parallel and detects
missing instrumentation before final acceptance. No storage -> HTTP -> UI waterfall.

## Decisive fixture and verification matrix

| Scenario | Required evidence / responsible lane |
|---|---|
| Every CLI/MCP/HTTP success, deny, invalid input, query and replay | One invocation, correct correlation/coverage, no extra effect / integration+HTTP |
| Overlapping tools; request retry; review across A-to-B handoff | Parent latency != child sum, distinct attempts and retained owners / lifecycle+identity+runtime |
| Wall-clock reversal, suspend, kill and reboot | No negative/fake durations or fabricated successful end; explicit clock/gap quality / common+storage+runtime |
| Peer/resource/user wait, pause and resumed execution | Disjoint phase attribution and correct causal continuation / coordination+resources+lifecycle |
| Age/Project/host quota; concurrent append/rotation; crash mid-delete | Exact bounded bytes, recoverable cursors, no foreign/pinned deletion / storage+projects+resources |
| Queue full, disk full, unavailable store, logger failure after mutation | Visible coverage degradation, no success reversal/retry or extra grant / storage+integration |
| Malicious arguments, errors, credentials, foreign Project query | No secret in files/DB/export/UI, access denial / all owners+HTTP |
| Many spans; indexed paged timing view; diagnostic reads instrumented | Bounded memory, no recursive logging loop; sample/window/coverage labels / storage+dashboard |

Future focused commands (NOT RUN):

```text
cargo test --manifest-path v1/contracts/Cargo.toml --test timing
cargo test --manifest-path v1/crates/storage/Cargo.toml --test telemetry
cargo test --test v1_observability
```

These targets must be created by their named owners; they do not exist now.
Each command uses that node's Root-managed mutable output namespace. UI owner
adds deterministic timing fixtures to its actual test runner; integration adds
real three-transport/three-provider coverage probes, marking unexposed native
events unavailable rather than fabricating them. Run existing relevant quality
gates as well, without repeating unrelated unchanged suites.

Logging performance comparison uses identical input/clock/fixture/build and host:
logging on versus off, at least three runs each, including concurrent tools and
quota pressure. Record p50/p95, peak memory, bytes written and Git/LLM process
counts. Initial target: <=5% p95 overhead on representative >=20ms requests,
<=1ms p95 additive on shorter requests, queue limits respected, zero extra Git
children/LLM calls for logging. Failure requires optimization or an explicit
reviewed tradeoff, not a claim of low overhead from unit tests. Final user flow:
find a slow Plan/review, drill into its wait/tool cause, rotate details, restart,
then still see truthful aggregates/history and any missing-detail boundary.

Research source register is incorporated from
[ADR32](../../decisions/0032-request-timing-and-bounded-observability.md#primary-research-opened-2026-10-16).
Exact shared schema/SQL/API names are frozen after approval, not invented as
already-installed tools here. No implementation or test PASS is claimed.

## Proposal verification, 2026-10-16

Native `mine adr add` and `mine adr propose` completed with exit 0. ADR-0032 is
PROPOSED at registry revision 72, body SHA-256
`AF7C150AF5774EF90EFACD91698A0A9AB2D89127C7A959CA15E37E232FBA2AB1`.
Scoped readiness is false for `v1.0.0` solely pending this decision, and true for
the current `docs/design` 0.2 scope. The execution graph remains revision 90,
24 nodes, with no registration or transition. Existing ADR bodies are unchanged.
No Rust/UI/production code or tests were changed or run for this documentation
proposal. Do not describe this supplement as already instrumented or accepted.

Subsequent owner instruction explicitly approved all current ADRs while delegating
0.2 coordination. Installed CLI approved this exact ADR32 digest at registry 73;
the original body remains unchanged. The proposal-stage readiness failure above
is historical: both scoped readiness checks now succeed. This approval does not
put observability implementation into the active 0.2 Plans.
