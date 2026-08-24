# Architecture Decision Approval Lifecycle

## Purpose

MINE separates architecture discovery from repository-owner approval. Material
architecture choices are reviewed as small ADRs before detailed Design is
declared mature or an implementation Plan is released. ADR approval records
authority and rationale; it does not replace detailed contracts or verification.

## ADR identity and storage

- IDs are monotonic `ADR-NNNN`, beginning at `ADR-0001`; numbers are never reused.
- Bodies are `docs/design/decisions/NNNN-<slug>.md`.
- `docs/design/decisions/adr-registry.toml` is the machine fact source.
- `docs/design/decisions/index.md` is a generated view once the registry is
  implemented.
- ADR bodies remain ordinary progressive-disclosure Design leaves and contain
  context, decision, alternatives, consequences, evidence, and approval questions.

Registry fields per ADR are:

```text
id
path
title
status
planning_blocking
planning_scopes[]
content_sha256
proposed_at
approved_by
approved_at
approval_note
supersedes[]
superseded_by
rejection_reason
updated_at
```

Paths are safe repository-relative paths. `planning_scopes` are Design path
prefixes or explicit scope tokens selected by `mine-arch`; an empty list on a
blocking ADR means repository-wide. Approval actors/notes are evidence strings,
not an authentication mechanism.

## State machine

```text
DRAFT -> PROPOSED -> APPROVED
                  -> REJECTED
APPROVED -> SUPERSEDED
```

- `DRAFT`: agent-owned editing state; no approval claim.
- `PROPOSED`: immutable review candidate whose digest is recorded.
- `APPROVED`: explicit repository-owner approval of that digest.
- `REJECTED`: terminal negative decision; revision uses a new ADR number.
- `SUPERSEDED`: terminal historical decision, with one approved replacement.

`propose` computes SHA-256 from exact ADR bytes under the ADR lock. `approve`
requires exact digest parity, explicit `approved_by` and approval note, and
records UTC time. A later byte change makes validation/readiness fail; it never
silently returns to APPROVED. Reversal creates a new ADR and `supersede`
atomically links both records only after the replacement is approved.

## Binary contracts

The CLI exposes JSON/human adapters over one ADR service:

```text
mine adr add|show|list|propose|approve|reject|supersede|validate
mine design readiness [--scope <scope>]
```

Read-only MCP tools are:

```text
mine_adr_show
mine_adr_list
mine_adr_validate
mine_design_readiness
```

Mutation tools are CLI-only. The accepted ADR store uses a dedicated lock,
monotonic revision, revision checks, atomic TOML write, and deterministic index
render. It does not share execution-graph revision but follows the same error
classes and JSON envelope conventions.

`mine adr approve` cannot establish human intent itself. `mine-arch` may invoke
it only after an explicit current user instruction naming the ADR(s). It records
the current user's supplied approval wording; an agent must not infer, batch,
or fabricate approval.

## `mine-arch` workflow

For a material architecture run:

1. inspect requirements, repository reality, current Design, and official sources;
2. identify discrete material decisions and allocate new ADR IDs;
3. write/update DRAFT bodies without rewriting an already approved decision;
4. validate and propose each review candidate;
5. present ADR paths, decisions, alternatives, consequences, open questions,
   and digests to the user, then stop;
6. on explicit approval/rejection, record transitions through the binary;
7. replace rejected proposals with new numbered ADRs when necessary;
8. complete detailed Design consistent with approved ADRs;
9. run Design validation and scoped readiness before handing off to planning.

The Skill never calls its own proposal approved. Existing approved ADR changes
use supersession, not in-place reversal. Bounded elaboration that preserves the
approved decision may update detailed leaves without a new ADR.

## Planning readiness

`mine design readiness --scope <scope>` joins registry state with Design
validation. It blocks when an applicable planning-blocking ADR is DRAFT,
PROPOSED, REJECTED without an approved replacement, SUPERSEDED without a valid
approved replacement, missing, unsafe, or digest-mismatched.

`mine-plan-create` must call readiness for its explicit scope and cite the
approved ADRs plus exact detailed leaves. Plan registration/release repeats the
gate from registered Design references so a Skill omission cannot silently
bypass known blocking decisions.

Non-blocking deferred decisions must name scopes they do not yet permit; they
do not prevent unrelated incremental Plans. “All ADRs approved” therefore means
all decisions material to the requested planning scope, not every imagined
future product decision.

## Detailed-design coverage

MINE does not standardize a universal subsystem maturity alphabet or coverage
matrix. For complex systems, `mine-arch` may add a project-specific readiness
leaf with maturity levels, cross-boundary gaps, staged admission, and verification
evidence. The project's Design owns that schema. Core readiness still requires
applicable ADR approval and valid detailed Design; an optional matrix cannot
override either.

## Bootstrap

The first implementation imports ADR-0001 through ADR-0009 into registry
revision 1. ADR-0001..0006 use their released Accepted text and current exact
digests. ADR-0007..0009 import the repository-owner approval evidence embedded
in those files. After import, registry/index mutations use only the binary.
Bootstrap code and alternate status parsing are removed before stable release.

## Failure and verification

- digest mismatch is a validation failure, not an automatic state mutation;
- concurrent proposals/approvals use revision conflict, never last-writer wins;
- missing/corrupt registry blocks readiness but does not rewrite ADR bodies;
- tests cover every transition, illegal back-transition, digest drift,
  supersession atomicity, scope filtering, concurrent writers, deterministic
  render, CLI/MCP parity, and explicit user-approval Skill wording.

## Primary sources

- [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions), Michael Nygard, accessed 2026-08-21: numbered significant decisions, proposed/accepted status, and supersession rather than erasure.
- [MADR 4.0](https://adr.github.io/madr/), accessed 2026-08-21: repository decision records with status, decision makers, alternatives, consequences, and confirmation.
- [adr-tools](https://github.com/npryce/adr-tools/blob/master/README.md), accessed 2026-08-21: monotonic numbered ADR log and linked supersession workflow.
