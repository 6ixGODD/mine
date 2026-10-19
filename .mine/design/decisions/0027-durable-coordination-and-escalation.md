# ADR-0027: Durable Commitments and Tiered Blocker Resolution

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

Implement one provider-independent communication protocol behind DM and rooms. Distinguish nonblocking information, result requests, commitment/decline, correlated outcome, versioned decision, resource negotiation, cancellation and handoff. Each request has accountable recipients and a durable completion criterion; transport receipt is not task completion.

Persist request and waiting continuation atomically; completion resolves and wakes the dependent node without LLM polling, including across offline/reboot/handoff. Timers, deduplication, routing and presence are deterministic. Escalate local executor choices to responsible peers, then Project Lead Architect; only undelegated business/authority decisions or unavailable hard conditions reach user. Pause affected nodes, not all unrelated work; detect wait cycles.

## Consequences and rejected alternatives

Model statements cannot guarantee task success; the system guarantees durable tracking, failure/escalation and no lost wakeup. Outside opted-in autonomy, existing approval gates still apply. Reject simple text ping-pong, broadcast inference, silent dropped offline requests and treating technical difficulty as global project shutdown.

## Acceptance boundary

Prove result-before-subscription, duplicate/out-of-order delivery, zero-inference waiting, recipient offline, deadline, cancel, owner transfer, cyclic waits, pre-start/mid-run blockers and one effective continuation wake.

## Design and evidence

[Detailed proposal](../v1/coordination.md); [opened primary sources and known limits](../v1/sources.md).
These explain this decision; exact schema/API names remain post-approval design,
not installed command claims. Existing approved0.2 decisions continue to govern
until explicit1.0 replacement; no registry supersession is performed by proposing.
Owner approval must bind this body's exact native SHA-256. Future autoapproval
cannot be used to approve its own introduction or any of these proposals.

