# MCP Contract

## Transport

v1 implements stdio only using the official Rust MCP SDK. The client launches:

```text
mine mcp serve --repo <repository>
```

stdin/stdout carry protocol messages only. Diagnostics go to stderr and local files.

## Tool design

The server exposes 17 focused tools today; the accepted ADR-0012 addition
raises this to 18 in the next release:

- `mine_workspace_status`;
- `mine_graph_validate`;
- `mine_graph_status`;
- `mine_graph_ready`;
- `mine_graph_wave`;
- `mine_graph_repair_provenance`;
- `mine_plan_add`;
- `mine_plan_show`;
- `mine_plan_start`;
- `mine_plan_mark_implemented`;
- `mine_plan_accept`;
- `mine_plan_reject`;
- `mine_plan_withdraw`;
- `mine_design_validate`;
- `mine_adr_show`;
- `mine_adr_list`;
- `mine_adr_validate`;
- `mine_design_readiness`.

Repository initialization, design backup, workspace opening/closure, installation, and release mutations remain CLI-only because they change local environment or delete owned temporary state.

`mine_plan_show(id)` includes branch, registered/resolved worktree location,
observed state, caller location, authoritative Root graph revision, and the
persisted `branch_anchor` after start. Plan worktree preparation is CLI-only
through `mine plan prepare --id <id>` because it mutates local Git checkout
state.

`mine_plan_start` revalidates the exact prepared Root/worktree/branch state and
atomically records `branch_anchor` with `IN_PROGRESS`. `mine_plan_reject`
accepts the canonical `review_report` path in addition to reason and next
compensation ID; it supports the evidence-bound `IN_PROGRESS -> REJECTED` edge
without adding another MCP tool or arbitrary status mutation.
`mine_plan_withdraw(id, reason)` is the ADR-0012 parity tool for
`mine plan withdraw`: it retires an unstarted plan (`DRAFT`/`BLOCKED`/`READY`)
with a recorded reason, performs the same provably-lossless Git cleanup checks,
and adds no arbitrary status mutation. `mine_plan_add` gains the optional
`planning_scope` argument recorded on the node (ADR-0013).

`mine_graph_repair_provenance(expected_revision)` exposes the same bounded,
revision-checked schema-v1/v2 recovery as
`mine graph repair-provenance --expected-revision <revision>`. It never accepts
arbitrary field values: canonical branch, lineage, worktree, and branch-anchor
metadata are derived by MINE from unambiguous graph/Git evidence, conflicting
or ambiguous values fail closed, and an already-valid v3 graph is an idempotent
byte-preserving no-op.

ADR approval/rejection/supersession remain CLI-only user-authority transitions.
MCP exposes only the read/query contracts above.

## Mutation contract

The server is bound to one configured repository. Mutating Plan tools accept a
typed Plan identity and operation-specific arguments, reload the authoritative
graph under its lock, and return `revision_before` / `revision_after`.
`mine_graph_repair_provenance` additionally requires the caller's explicit
`expected_revision` because it is a schema-recovery operation. No tool accepts
shell fragments.

The server returns the same DTOs as JSON CLI.

## Security

The MCP server exposes no arbitrary shell execution, arbitrary deletion, unrestricted Git mutation, plugin installation, unrestricted file writes, or network fetch primitive.

## Compatibility

Tool names and schemas are validated against Skill references before release. Skills never mention tools the current binary does not expose.
