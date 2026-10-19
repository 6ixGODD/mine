# ADR-0005: Code Is the Default Authority During `mine-sync`

## Status

Accepted.

## Decision

During `mine-sync`, explicit current user instructions and named protected design decisions have highest authority. Otherwise, current observable repository behavior overrides conflicting design documentation.

## Rationale

The purpose of synchronization is to make durable design describe the repository that actually exists. Treating stale design as automatically superior would turn sync into an implementation audit rather than a repository-to-design reconciliation.

`mine-arch` remains the mechanism for deliberately changing target architecture away from current code.

## Consequences

- existing managed design is backed up before rewrite;
- code/design discrepancies update design by default;
- suspicious implementation is documented and flagged, not silently hidden;
- protected design drift blocks release until planned or explicitly resolved;
- sync scope and incomplete coverage are recorded.
