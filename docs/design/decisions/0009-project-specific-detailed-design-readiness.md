# ADR-0009: Keep Detailed-Design Coverage Project-Specific

## Status

Approved by the repository owner on 2026-08-21.

Bootstrap approval evidence: proposed-content SHA-256
`AB207439B386F5CD041230E94C7817AF3B2301532E2825BE4A8E19DA4644726F`.
This approval predates the deterministic ADR registry and must be imported by
the one-time registry bootstrap.

## Context

Complex projects such as LODE benefit from an explicit subsystem maturity
matrix (`A`, `C`, `D`, `V`, `N`), cross-boundary gap register, staged planning
admission, and detailed-design completion checklist. Small repositories may not
have those subsystems or maturity levels. ADRs themselves record significant
decisions; they do not prove full detailed-design coverage.

## Decision

Do not bake LODE's `detailed-design-coverage.md` schema or maturity alphabet into
MINE's core domain or binary.

MINE core provides only these universal gates:

1. material planning-blocking ADRs for the requested scope are approved and
   their digests match;
2. the managed Design namespace and references validate;
3. `mine-arch` records unresolved material decisions and does not claim readiness
   for their affected scope;
4. `mine-plan-create` checks the approved ADRs and exact Design leaves governing
   its explicit scope.

For a complex project, `mine-arch` may create a project-specific readiness leaf
such as `docs/design/architecture/detailed-design-coverage.md`, with its own
maturity vocabulary, subsystem matrix, admission rules, and verification
evidence. The path and schema are Design-owned, not hardcoded into the binary.
Projects that do not need the artifact are not forced to create ceremonial
coverage documents.

## Consequences

- MINE preserves progressive disclosure and scope-first planning without
  pretending one completeness matrix fits every architecture.
- Large projects can reproduce or improve the LODE pattern as an optional
  `mine-arch` output selected by complexity and user needs.
- The ADR registry/readiness gate remains small and deterministic; qualitative
  coverage judgment remains model-driven and project-specific.

## Rejected alternatives

- hardcode LODE's `A/C/D/V/N` states into every repository;
- treat approved ADR count as a detailed-design completeness metric;
- omit readiness analysis entirely for complex projects;
- allow a project-specific coverage leaf to override unresolved blocking ADRs.

## Approval questions

1. Approve keeping detailed-design coverage matrices optional and project-owned?
2. Approve the four universal MINE planning gates above as the core minimum?
