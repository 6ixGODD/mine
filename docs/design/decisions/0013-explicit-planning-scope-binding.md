# ADR-0013: Persist an Explicit Planning Scope on Every Plan Node

## Status

Proposed for repository-owner approval.

## Context

ADR registry entries carry `planning_scopes`: design-path prefixes or explicit
scope tokens; an empty list is repository-wide. Planning readiness
(`mine design readiness --scope <scope>`) resolves applicability by prefix
matching.

The automatic gate is broken in two ways:

1. **Wrong scope key.** `mine plan add` (CLI and MCP) derives the readiness
   scope from the plan's canonical lineage id (for example `plan02`). Lineage
   ids never match design-path scopes such as `docs/design/execution-graph`,
   so scoped planning-blocking ADRs — including this repository's ADR-0010 and
   ADR-0011 — never gate plan registration; only repository-wide ADRs do. The
   `milo` issue record (2026-08) independently reports the auditability
   symptom: nothing proves which approved decision set a plan was planned
   against.
2. **No re-check at release.** Readiness is enforced only at registration.
   ADR state can change between `plan add` and `plan release` with no gate.

## Decision

1. `PlanNode` persists a non-empty `planning_scope` string (a design-path
   prefix or explicit scope token), recorded at registration and immutable
   afterwards.
2. `mine plan add` (CLI and MCP) gains an optional `--planning-scope`. When
   omitted, the node records the configured design root directory (for this
   repository, `docs/design`) — the fail-safe superset under which every
   design-path-scoped ADR applies. The lineage-id derivation is removed as
   defective baggage.
3. Registration and release both gate on the node's persisted scope:
   - `plan add` requires planning readiness for the recorded scope;
   - `plan release` re-runs planning readiness for the persisted scope before
     the `DRAFT -> READY/BLOCKED` transition and records
     `readiness_checked_at` and the satisfied `adr_registry_revision` on the
     node as audit evidence.
4. `mine plan show` (CLI and MCP) exposes `planning_scope` and the recorded
   readiness evidence. Legacy nodes without the field are read with the
   design-root scope semantics; no field value is silently invented, and the
   next node mutation persists the resolved value.

## Consequences

- Plan registration becomes provably bound to the approved ADR decision set
  for a named scope; scoped ADRs gate registration again — a strict, intended
  tightening relative to the silent bypass.
- Repositories without scoped ADRs see no practical change: under the
  design-root default, only repository-wide ADRs apply, as today.
- The graph wire schema gains one optional node field; the fail-closed schema
  rule already refuses older writers that round-trip unknown fields away.
- Skills that register plans (`mine-plan-create`) pass the scope explicitly or
  accept the documented default.

## Alternatives rejected

- Keep lineage-id scopes: demonstrated never to match design-path scopes; a
  gate that cannot fire is worse than no gate.
- Require `--planning-scope` with no default: forces every registration in
  single-scope repositories to repeat a constant; the design-root default is
  fail-safe (strictly stronger than today's effective behavior).
- Persist a full approved-ADR-id snapshot per node: bloats the graph with a
  derived value; registry revision plus check timestamp reconstruct the exact
  decision set.

## Verification obligations

- readiness gating at add and release uses the persisted scope: a scoped ADR
  blocks registration/release within its scope and not outside it; a global
  ADR blocks everywhere;
- default resolution to the configured design root; explicit scope recorded
  verbatim;
- release records `readiness_checked_at` and `adr_registry_revision`; `mine
  plan show` exposes both; CLI/MCP parity;
- legacy nodes without the field validate under design-root semantics and
  persist the resolved value on next mutation.

## Approval questions

1. Approve persisting `planning_scope` per node with the design-root default
   at registration, and removing lineage-id scope derivation?
2. Approve the release-time readiness re-check with recorded audit evidence?
3. Confirm the design-root default (rather than a mandatory explicit flag) is
   the ergonomics/safety trade-off you want?
