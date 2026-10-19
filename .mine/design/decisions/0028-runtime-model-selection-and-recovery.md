# ADR-0028: Provider Sessions, Model Selection and Recoverable Work Nodes

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

Use official Codex, Claude Code and Pi runtime adapters under a Rust session supervisor. Remove OpenCode only in1.0. Persist principal/attempt/native-session bindings and durable work-node checkpoints. Create/resume/rename/send/interrupt are capability-tested against pinned runtime versions, not inferred universal SDK features.

Discover models from the current Project's effective runtime configuration and credentials. Store provider/model, effort and fast/speed tier separately with requested/effective values and discovery fingerprint. Respect supported options, project-config Root versus execution-worktree cwd, and explicit fallback/cost policy. Never silently switch model, ignore effort/fast or use a global hardcoded list.

## Consequences and rejected alternatives

Pause stops admissions and then confirms writer quiescence; reboot reconstructs nodes/waits/effects and offers exact continuation. Explicit pause survives restart; optional auto-resume requires prior policy. Failed resume can replace a session only after old writer reconciliation. SDK transport success is not work completion. Native client inspectability is provider capability, not an unconditional deep-link promise.

## Acceptance boundary

Prove each provider's actual create/resume/model-effort-speed/config mapping, lost creation response, unsupported option, changed model availability, pending tool cancellation, restart and safe replacement without duplicate writers.

## Design and evidence

[Detailed proposal](../v1/runtime.md); [opened primary sources and known limits](../v1/sources.md).
These explain this decision; exact schema/API names remain post-approval design,
not installed command claims. Existing approved0.2 decisions continue to govern
until explicit1.0 replacement; no registry supersession is performed by proposing.
Owner approval must bind this body's exact native SHA-256. Future autoapproval
cannot be used to approve its own introduction or any of these proposals.

