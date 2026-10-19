# ADR-0024: Project Registry and Safe Deinitialization

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

Project is the only business aggregate. Install/first use establishes a user-local .mine control directory and unified dashboard; mine init registers a stable Project identity and canonical Root. Global control.sqlite3 owns directory facts, not a duplicate Plan graph. Each Project has one local authority store. Linked worktrees are resources of that Project.

Initialization and deinit are recoverable cross-store operations with stable IDs. Deinit previews exact ownership, quiesces writers, preserves or explicitly exports valuable evidence, cleans only proven-owned artifacts and removes the active global entry. Recovery journals remain outside the deleted tree. Dirty/foreign/unknown files refuse automatic deletion; no source-history rewrite or global agent-uninstall.

## Consequences and rejected alternatives

Accepting this changes installation/setup and introduces global local-state ownership. Reject one unbounded global business DB, path-as-identity, silent duplicate clone registration and recursive deletion of arbitrary .mine content.

## Acceptance boundary

Prove repeated/interrupted init/deinit, moved/missing roots, duplicate UUID, linked-worktree resolution, foreign data preservation and global/project directory agreement.

## Design and evidence

[Detailed proposal](../v1/projects.md); [opened primary sources and known limits](../v1/sources.md).
These explain this decision; exact schema/API names remain post-approval design,
not installed command claims. Existing approved0.2 decisions continue to govern
until explicit1.0 replacement; no registry supersession is performed by proposing.
Owner approval must bind this body's exact native SHA-256. Future autoapproval
cannot be used to approve its own introduction or any of these proposals.

