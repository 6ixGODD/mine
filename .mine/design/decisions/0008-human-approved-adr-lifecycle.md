# ADR-0008: Human-Approved ADR Lifecycle Before Planning

## Status

Approved by the repository owner on 2026-08-21.

Bootstrap approval evidence: proposed-content SHA-256
`3A32178468650BFF2265F40D676B1AA2F87CB0AD4314D07D2C97A0F54ECBB2A0`.
This approval predates the deterministic ADR registry introduced by this ADR
and must be imported by the one-time registry bootstrap.

## Context

`mine-arch` currently writes target Design directly and can hand it to
`mine-plan-create` without a machine-verifiable record that the repository owner
reviewed the material architecture decisions. Large systems need decisions
presented as bounded review units before detailed Design is treated as mature.
Markdown status text alone is easy to drift and does not bind approval to the
content the user actually reviewed.

## Decision

Add a deterministic ADR registry and lifecycle to the `mine` binary.

### Identity and files

- ADR IDs retain this repository's existing monotonic four-digit convention:
  `ADR-0001`, `ADR-0002`, ...; numbers are never reused.
- Decision bodies remain small Markdown files under
  `docs/design/decisions/NNNN-<slug>.md`.
- `docs/design/decisions/adr-registry.toml` is the machine fact source for ADR
  identity, status, content digest, approval evidence, blocking scope, and
  supersession links.
- `docs/design/decisions/index.md` is deterministically rendered from the
  registry. Neither registry nor generated index is hand-edited after the
  binary supports them.

### State machine

```text
DRAFT -> PROPOSED -> APPROVED
                  -> REJECTED
APPROVED -> SUPERSEDED (requires a replacement ADR that is APPROVED)
```

- `DRAFT` may be edited by `mine-arch` and is not ready for user review.
- `PROPOSED` freezes the digest presented to the user.
- `APPROVED` requires explicit current repository-owner authorization and
  records `approved_by`, `approved_at`, approval note, and SHA-256 content
  digest. A digest mismatch makes Design readiness invalid; it never silently
  keeps approval after content changes.
- `REJECTED` is terminal; a revised decision receives a new ADR number.
- An approved decision is not rewritten to reverse it. A new approved ADR
  atomically supersedes it and both registry links are recorded.

### Commands and adapters

Target JSON CLI commands are:

```text
mine adr add|show|list|propose|approve|reject|supersede|validate
mine design readiness
```

Read-only `mine_adr_show`, `mine_adr_list`, `mine_adr_validate`, and
`mine_design_readiness` MCP tools may expose the same DTOs. Approval, rejection,
and supersession remain CLI-only authority transitions. The Skill may run them
only after an explicit current user instruction; the binary cannot authenticate
human intent by itself and Skills must never fabricate approval.

Every registry mutation uses the same lock/revision/atomic-render discipline as
the execution graph, with a separate ADR revision. `approve` recomputes the
digest under lock immediately before writing evidence.

### Architecture and planning gate

`mine-arch` first discovers and researches, then creates or updates `DRAFT`
ADRs for each material decision. It renders their context, decision, options,
consequences, rejected alternatives, evidence, and explicit approval questions;
then proposes them and stops for user review.

After the owner explicitly approves/rejects each proposal, `mine-arch` completes
the detailed Design consistent with the approved decisions and runs
`mine design readiness`. `mine-plan-create` refuses to create or release a Plan
when any ADR marked `planning_blocking = true` is `DRAFT`, `PROPOSED`, rejected
without an approved replacement, superseded without an approved replacement,
or has a digest mismatch. Deferred/project-later decisions are explicitly
non-blocking for named scopes rather than pretending the whole architecture is
complete.

## Consequences

- Users review architecture as individual decisions before implementation plans
  are considered mature.
- ADR approval and Design validation are distinct: ADRs record why/authority;
  Design leaves still specify the complete executable contract.
- The binary gains a focused ADR domain/service/store plus CLI/MCP read models;
  it does not become a generic document workflow engine.
- Existing ADR-0001..0006 require a one-time registry bootstrap that records
  their already released `Accepted` state and current content digests. This
  migration is explicit and has no alternate permanent status source.

## Rejected alternatives

- Markdown status lines as the only fact source: no revision or digest binding;
- letting an architecture agent self-approve: defeats the user review boundary;
- editing an approved ADR in place: loses the reviewed decision record;
- requiring every future/project-later decision before any scoped planning:
  prevents incremental architecture and contradicts staged delivery;
- treating ADR approval as proof that detailed Design is complete: ADRs do not
  replace contracts, failure semantics, tests, or readiness evidence.

## Approval questions

1. Approve the state machine and explicit user-only approval boundary?
2. Approve digest-bound approval and new-ADR supersession instead of editing
   approved decisions in place?
3. Approve Plan creation being blocked only by ADRs marked planning-blocking for
   the requested scope, followed by `mine design readiness`?
