# ADR-0002: Treat Planning Artifacts as Ephemeral

## Status

Accepted.

## Decision

`docs/plan/` exists only on `dev` and temporary plan branches. It is purged before stable release integration. Stable history uses squash or curated commits to avoid retaining temporary plan ancestry.

## Rationale

Plans are execution scaffolding, not product knowledge. Keeping all historical plans indefinitely bloats context, obscures current truth, and encourages agents to reason from obsolete designs.

## Consequences

- durable decisions must be moved into `docs/design/`;
- implementation reports are temporary evidence;
- release closure requires design synchronization;
- debugging old workspaces relies on temporary branch history or external archives before branch deletion, not stable repository docs.
