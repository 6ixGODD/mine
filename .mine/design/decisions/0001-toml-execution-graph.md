# ADR-0001: Use TOML as the Execution-Graph Fact Source

## Status

Accepted.

## Decision

Store the active temporary plan-workspace graph in `docs/plan/execution-graph.toml` and generate `execution-graph.md` for humans.

## Rationale

The graph is small, Git-scoped, diffable, and temporary. TOML is transparent and strongly typed enough for deterministic Rust parsing.

## Rejected alternatives

- SQLite: binary diffs and poor branch merge behavior;
- custom binary: opaque and unnecessary;
- Markdown as fact source: ambiguous mutation;
- remote service: conflicts with local-first personal workflow.
