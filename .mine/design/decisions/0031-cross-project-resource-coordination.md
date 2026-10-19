# ADR-0031: Cross-Project Resource Arbitration and Temporary Channels

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

User-local control plane arbitrates explicitly registered shared resources across Projects with capacity/queue/fairness policy and durable grants. Each Project's designated Architect may establish a purpose-, membership- and expiry-bound temporary channel with another Project Lead for exceptional negotiation using the same durable protocol.

Channel membership does not grant cross-project file/lifecycle authority. Accepted negotiation terms still require an authoritative coordinator grant. Lease expiry is not proof old work stopped; unfenceable resources become UNCERTAIN until reconciled. Use minimal disclosure and preserve outstanding grants through pause, deinit and restart.

## Consequences and rejected alternatives

Default routine resource admission is deterministic, not an LLM conversation. Preserve per-Plan mutable-output isolation; exceptional shared outputs require explicit measured serialized policy. No silent preemption or unbounded source/transcript sharing. Global service coordinates resources, not duplicate Project business state.

## Acceptance boundary

Prove two-project races, fairness, wait cycles, stale epochs, lost release reply, offline lead, revoked channel, deinit with held resource and no resource double-grant or private-data leakage.

## Design and evidence

[Detailed proposal](../v1/resources.md); [opened primary sources and known limits](../v1/sources.md).
These explain this decision; exact schema/API names remain post-approval design,
not installed command claims. Existing approved0.2 decisions continue to govern
until explicit1.0 replacement; no registry supersession is performed by proposing.
Owner approval must bind this body's exact native SHA-256. Future autoapproval
cannot be used to approve its own introduction or any of these proposals.

