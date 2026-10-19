# MCP Contract

## Transport

The inventory below describes the 0.2.0 candidate. DTO/argument extensions follow
[active scheduling](../execution-graph/active-ownership-scheduling.md) and
[lifecycle receipts](../governance/lifecycle-receipts.md). Candidate adapters
implement them; installed 0.1.13 retains its older schema. Actual
schema/discovery and Skill references ship together;
no speculative tool name or permanently frozen tool-count gate is introduced.

ADR-0022 also extends existing start/show/wave/workspace DTOs with
[preparation generations](../governance/ready-preparation.md) and
[checkpoint progress/barriers](../governance/root-graph-checkpoints.md).
Prepare remains CLI-only; pending checkpoint is not completed execution
admission even when the authoritative graph already records its write.

v1 implements stdio only using the official Rust MCP SDK. The client launches:

```text
mine mcp serve --repo <repository>
```

stdin/stdout carry protocol messages only. Diagnostics go to stderr and local files.

## Tool design

The server exposes 19 focused tools:

- `mine_workspace_status`;
- `mine_graph_validate`;
- `mine_graph_status`;
- `mine_graph_ready`;
- `mine_graph_wave`;
- `mine_graph_repair_provenance`;
- `mine_graph_repair_compensation_predecessors`;
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
through `mine plan prepare --id <id> --actor <actor> --expected-revision <r>
--operation-id <uuid-v4>` because it mutates local Git checkout state. Typed
operation and verification commands, preparation and ownership clarification
remain CLI-only; the 19-tool inventory does not imply adapters for those groups.

`mine_plan_start` revalidates the exact prepared Root/worktree/branch state and
atomically records `branch_anchor` with `IN_PROGRESS`. `mine_plan_reject`
accepts the canonical `review_report` path in addition to reason and next
compensation ID; it supports the evidence-bound `IN_PROGRESS -> REJECTED` edge
without adding another MCP tool or arbitrary status mutation.
`mine_plan_withdraw(id, reason, actor, expected_revision, operation_id)` is the parity tool for
`mine plan withdraw`: it retires an unstarted plan (`DRAFT`/`BLOCKED`/`READY`)
with a recorded reason, performs the same provably-lossless Git cleanup checks,
and adds no arbitrary status mutation. `mine_plan_add` gains the optional
`planning_scope` argument recorded on the node (ADR-0013).

`mine_graph_repair_provenance(actor, expected_revision)` exposes the same bounded,
revision-checked schema-v1/v2 recovery as
`mine graph repair-provenance --actor <actor> --expected-revision <revision>`. It never accepts
arbitrary field values: canonical branch, lineage, worktree, and branch-anchor
metadata are derived by MINE from unambiguous graph/Git evidence, conflicting
or ambiguous values fail closed. Valid older supported graphs receive a
version-only upgrade to v7; an already-current valid graph is a byte-preserving
no-op. Reads do not fabricate missing preparation/acceptance evidence.

`mine_graph_repair_compensation_predecessors(id, actor, expected_revision, reason)`
is the ADR-0014 parity surface for the CLI recovery. It accepts no predecessor
values, derives the exact list from the rejected predecessor, repairs only an
unstarted compensation with no own Git branch/worktree or execution evidence,
and returns old/new dependencies and status. The operation is an idempotent
no-op when the list is already exact and refuses every ambiguous or broader
mutation.

ADR approval/rejection/supersession remain CLI-only user-authority transitions.
MCP exposes only the read/query contracts above.

## Mutation contract

The server is bound to one configured repository. Mutating Plan tools accept a
typed Plan identity, the caller's observed `expected_revision`, explicit actor
(start uses `owner`), and operation-specific arguments, reload the authoritative
graph under its lock, and return `revision_before` / `revision_after`. Acceptance
uses `operation_id` and typed `input`, not the older report-only invocation.
Both graph repair tools require explicit `expected_revision`; compensation
repair also requires a non-empty audit reason. No tool accepts shell fragments.

The server returns the same DTOs as JSON CLI.

## Security

The MCP server exposes no arbitrary shell execution, arbitrary deletion, unrestricted Git mutation, plugin installation, unrestricted file writes, or network fetch primitive.

## Compatibility

Tool names and schemas are validated against Skill references before release. Skills never mention tools the current binary does not expose.
