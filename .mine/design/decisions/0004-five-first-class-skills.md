# ADR-0004: Maintain Five First-Class Skills

## Status

Accepted.

## Decision

MINE v1 has five Skills:

1. `mine-arch`;
2. `mine-sync`;
3. `mine-plan-create`;
4. `mine-plan-exec`;
5. `mine-plan-review`.

## Rationale

These responsibilities require distinct model judgment:

- target architecture;
- repository-to-design synchronization;
- planning;
- implementation;
- independent acceptance.

Installation, graph state, backup mechanics, diagnostics, and distribution remain deterministic CLI work.

## Consequences

- `mine-sync` is a high-cost onboarding and release gate;
- `mine-arch` is requirement-first rather than repository-mirroring;
- no separate install, graph, debug, or integration Skill exists in v1.
