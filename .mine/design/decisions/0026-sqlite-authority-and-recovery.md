# ADR-0026: SQLite Authority, Migration and Durable Effects

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

Replace MINE-owned TOML machine authority with SQLite: user-local control store for global facts, Project store for config/ADR/Plan/agents/operations/messages/evidence metadata. Markdown remains human-authored bodies, bound by exact digest; large artifacts and provider transcripts remain referenced files. Rust services are the shared CLI/MCP/Web write path.

Use short versioned transactions, durable audit/outbox, idempotent requests and recoverable external effects. Pin patched SQLite, local WAL/FULL durability, explicit foreign keys and bounded queues. Cross-store/Git/SDK work is not atomically committed by SQLite. Preserve exact lifecycle safety while removing graph-only Git checkpoint commits in1.0.

## Consequences and rejected alternatives

Accepting means Git clone alone no longer restores full approvals/runtime history. Provide explicit consistent state backup/import with identity/digest checks. Migrate only quiescent projects with verified backup, staged validation and old-writer refusal; no long-term TOML dual write or active0.2 migration. Retain immutable evidence outside ephemeral Git Plan cleanup.

## Acceptance boundary

Prove restart at each effect boundary, idempotency, stale decision CAS, full disk, corrupt/old stores, backup/import integrity, project isolation and missing-machine-history diagnostics.

## Design and evidence

[Detailed proposal](../v1/storage.md); [opened primary sources and known limits](../v1/sources.md).
These explain this decision; exact schema/API names remain post-approval design,
not installed command claims. Existing approved0.2 decisions continue to govern
until explicit1.0 replacement; no registry supersession is performed by proposing.
Owner approval must bind this body's exact native SHA-256. Future autoapproval
cannot be used to approve its own introduction or any of these proposals.

