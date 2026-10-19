# ADR-0029: Unified Local Dashboard and Consistent Decisions

## Status and scope

Proposed for **1.0.0**; native registry is authoritative. Planning-blocking scope
`v1.0.0`. Not approved, not implemented, and not a new0.2 release gate.
Requirement source: repository owner's2026-10-16 requests and follow-ups.

## Context

Current MINE has five Skills and a Rust CLI/MCP over repository-local TOML. The
owner requests a Project-centered multi-agent control plane, durable efficient
collaboration, recoverable sessions and a unified dashboard. This bounded decision
belongs to that next release, not an early upgrade of today's bootstrap tooling.

## Decision proposed for owner approval

Build one local Rust-served React/TypeScript/Vite dashboard using shadcn preset b2jI. Global project list opens each Project's work frontier, decisions, agents, conversations, resources and evidence. Drill Plan to package/step, show exact waits/owners/checkpoints and ordered handoff history, not only raw chat or a dense dependency graph.

Dashboard and Architect-session decisions submit the same authenticated revision/digest-bound native transaction. Audit+state+outbox commit together, idempotent retries and stale conflicts prevent double approval; event-cursor replay/reset keeps UI consistent. Runtime model/effort/speed selectors are capability-driven.

## Consequences and rejected alternatives

Localhost is not authentication. Require session security, Host/Origin/CSRF controls, safe content rendering and no arbitrary shell endpoint. Keep Rust core authority, static browser assets and bounded SDK workers. Exact b2jI preview was inaccessible in research; generation/visual verification is an implementation gate, not permission to substitute a preset.

## Acceptance boundary

Prove usable desktop/narrow layouts, keyboard/accessibility, actual preset generation, two-project isolation, stale simultaneous decisions, offline/reconnected UI, recovery frontier, handoff history and authenticated mutation rejection.

## Design and evidence

[Detailed proposal](../v1/dashboard.md); [opened primary sources and known limits](../v1/sources.md).
These explain this decision; exact schema/API names remain post-approval design,
not installed command claims. Existing approved0.2 decisions continue to govern
until explicit1.0 replacement; no registry supersession is performed by proposing.
Owner approval must bind this body's exact native SHA-256. Future autoapproval
cannot be used to approve its own introduction or any of these proposals.

