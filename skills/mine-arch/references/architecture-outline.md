# Architecture and Detailed Design Outline

Use this outline as a completeness checklist, not as a rigid template. Remove sections that are genuinely inapplicable and add domain-specific sections when needed.

## 1. Background and goals
- stakeholders, users, business context, goals, non-goals, constraints
- current reality versus target state

## 2. Quality attributes and invariants
- correctness, availability, latency, throughput, privacy, safety, cost, operability
- measurable constraints and immutable business rules

## 3. System context and trust boundaries
- actors, external systems, process boundaries, deployment boundaries, trust zones
- context and container diagrams

## 4. Architecture style and component boundaries
- component ownership and responsibilities
- allowed dependency direction and forbidden coupling
- composition roots and concrete adapter placement
- explicit SOLID analysis without speculative abstractions
- responsibility/namespaces, exclusions, physical mappings and named shared integration owners; internal file autonomy is not permission to change contracts
- concrete policy/ADR byte binding for semantic partitions; conservative authority remains valid without fabricated approval

## 5. Technology decisions and source register
- selected versions and alternatives
- verified official or primary sources with URLs, claims, and implications
- rejected alternatives and trade-offs

## 6. Code organization
- workspace/package/module layout
- generated code and source ownership
- public versus internal boundaries
- real consumer/invariant for an explicitly assigned architect foundation; ordinary Plan/prepare/start/review, no unused scaffold or Root implementation exception

## 7. Detailed component design
For every component, define responsibility, inputs, outputs, dependencies, state, lifecycle, errors, concurrency, retries, timeouts, degradation, and tests.

## 8. Domain and data design
- terminology, aggregates, identifiers, state machines, invariants
- persistence model, provenance, retention, deletion, migrations, rebuilds
- consistency, transactions, locking, indexing, pagination, empty/error behavior

## 9. Contract design
- APIs, events, files, CLIs, tools, prompts, model inputs/outputs
- versioning, authorization, idempotency, error envelopes, limits, citations/provenance

## 10. Core workflows and failure paths
- sequence/state diagrams for major closed loops
- cancellation, retries, partial failure, recovery, cleanup, degraded operation
- complete held-owner snapshots and explicit observer revisions; same-operation checkpoint recovery, rejection handoff and objective cleanup, not time-based release

## 11. Configuration and secrets
- configuration sources and precedence
- environment separation, validation, rotation, redaction, ownership

## 12. Security, privacy, and safety
- authentication, authorization, PII, audit, threat boundaries, unsafe outputs

## 13. Observability and operations
- logs, metrics, traces, audit events, health/readiness, alerts, runbooks

## 14. Testing and repository quality gates
- language-specific format/lint/static/type/test/build commands
- unit, integration, contract, migration, end-to-end, security, and smoke strategy
- exact configuration sources and CI ownership
- input-bound RUN/REUSED/INVALIDATED/NON_REUSABLE/NOT_RUN evidence, fresh independent/native probes and language-independent Plan-isolated mutable outputs

## 15. Build, deployment, migration, and rollback
- artifacts, containers, environments, release flow, backups, rollout, rollback/rebuild

## 16. Risks, trade-offs, and open decisions
- impact, evidence, owner, trigger, mitigation, decision deadline
- material decisions as separate digest-bound ADRs for explicit owner approval before dependent planning; project-specific detailed coverage is not a new universal lifecycle

## 17. Historical-baggage removal
- obsolete fields, APIs, adapters, aliases, shims, migrations, or packages to remove
- explicit follow-up plans when cleanup cannot fit the current increment

## 18. Final closed-loop verification
- prove that user flows, operator flows, failure recovery, deployment, and observability form a coherent system
