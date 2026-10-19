# ADR-0006: MINE Owns `docs/design/`

## Status

Accepted.

## Decision

MINE exclusively owns `docs/design/` and identifies managed trees with `.mine-design.toml`.

Unmarked legacy `docs/design/` is rejected. MINE does not provide automatic adoption or compatibility migration for arbitrary historical layouts.

## Rationale

MINE depends on predictable indexes, markers, links, document contracts, backup behavior, and destructive synchronization authority. Silent adoption of unknown content would make those guarantees impossible.

## Consequences

- users onboarding old repositories get their legacy `docs/design/` auto-backed-up to a timestamped `docs/design-backup-<UTC timestamp>/` by `mine init`;
- `mine init` resolves namespace conflicts deterministically (backup + fresh root); `mine-sync` refuses an unmarked tree only before `mine init` has claimed it;
- foreign repository markers are rejected;
- user guide and README prominently warn about ownership;
- unsupported migration belongs in an external tool or fork.
