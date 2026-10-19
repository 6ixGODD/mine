# ADR-0025: Principal Roles and Traceable Plan Handoffs

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

Use stable Project-qualified principals with short display IDs and separate Architect, Executor and Reviewer role grants. Assign one Architect as Project Lead for technical escalation. Native provider session IDs and attempts are bindings, not principal identity or credentials. Enforce Plan executor/reviewer assignments through the same application service.

Persist append-only A-to-B assignment history and same-principal session replacements separately. An explicit owner instruction to let B continue authorizes transfer without another ritual approval. Quiesce/fence A before admitting B, CAS assignment revisions, retain dirty checkpoint/evidence/waits, and audit reason, authorization and acknowledgment. Reviewer independence follows contribution history across ID/session replacement.

## Consequences and rejected alternatives

This replaces free-text owner attribution as authorization. Short IDs and same-user API fencing are not OS isolation. Reject implicit owner theft, erased handoff history, timeout-as-death, and self-review under fresh identities.

## Acceptance boundary

Prove simultaneous claims, failed/lost-response transfer, stale writer, session replacement, forced recovery without invented acknowledgment, contribution-aware independent review and ordered handoff UI.

## Design and evidence

[Detailed proposal](../v1/identity.md); [opened primary sources and known limits](../v1/sources.md).
These explain this decision; exact schema/API names remain post-approval design,
not installed command claims. Existing approved0.2 decisions continue to govern
until explicit1.0 replacement; no registry supersession is performed by proposing.
Owner approval must bind this body's exact native SHA-256. Future autoapproval
cannot be used to approve its own introduction or any of these proposals.

