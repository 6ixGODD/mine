# ADR-0030: Experimental Bounded Autonomy

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

Provide explicitly opted-in experimental autonomy, off by default. User binds requirements/scope, runtime/model/effort/speed policies, concurrency/time/retry/token or cost ceilings and authorized side effects. Covered technical ADRs are automatically policy-approved with exact digest, classification and policy revision; business/product ambiguity and unavailable hard resources go to the human.

Architect is the technical decision fallback; deterministic services schedule work and route blockers. Preserve immutable Plans, assigned ownership, independent review, evidence gates and controlled release authority. User pause stops new admissions/wakes and persists across reboot. Unaffected work can continue while one node needs a decision.

## Consequences and rejected alternatives

This grants bounded decision delegation, not agent self-approval of this ADR or permission/budget expansion. Unknown billing is not free; disclose unenforceable monetary precision and enforce available limits. No automatic publish, budget raise, expensive fallback, infinite retries or identity cycling to evade quotas.

## Acceptance boundary

Prove automatic covered decisions, human business decision through UI/session, exact digest conflicts, stop during work, budget reservation/exhaustion, offline architect recovery and independent review under autonomy.

## Design and evidence

[Detailed proposal](../v1/autonomy.md); [opened primary sources and known limits](../v1/sources.md).
These explain this decision; exact schema/API names remain post-approval design,
not installed command claims. Existing approved0.2 decisions continue to govern
until explicit1.0 replacement; no registry supersession is performed by proposing.
Owner approval must bind this body's exact native SHA-256. Future autoapproval
cannot be used to approve its own introduction or any of these proposals.

