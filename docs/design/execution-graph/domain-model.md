# Execution Graph Domain Model

## Aggregate

`PlanWorkspace` is the aggregate root.

```text
PlanWorkspace
├── schema_version
├── revision
├── project_id
├── workspace_id
├── stable_branch
├── integration_branch
├── stable_baseline_commit
├── design_root
├── created_at
├── updated_at
└── plans[]
```

`workspace_id` is generated internally. It is not a product version and the user does not provide it.

## Plan node

Canonical Plan identity is `planNN(?:-NN)*(?:-CNN)?`, where every `NN` is
`01..99`, `00` is forbidden, and `C` appears at most once as the final segment.
Removing a final compensation segment yields the lineage ID. Examples:
`plan01-02`, `plan01-02-C01`, `plan01-02-C02`.

The Markdown path is exactly `docs/plan/<id>-<lower-kebab-slug>.md`. Reports use
the same stem plus `-implementation.md` or `-review.md`. The branch is
`plan/<id>-<slug>`. The lineage worktree is derived as
`<worktrees_root>/<lineage-id>` and is shared serially by compensation nodes.

Each `PlanNode` contains:

- `id`: stable within the active workspace;
- `path`: plan Markdown path under `docs/plan/`;
- `branch`: validated managed branch `plan/<id>-<slug>`;
- `lineage_id`: derived canonical base identity (serialized for query/audit but
  validation must equal the ID-derived value);
- `worktree_path`: portable repository-relative lineage worktree path shared by
  the base and all its compensation nodes;
- `branch_anchor`: immutable Git commit object ID captured by `mine plan start`
  after exact worktree preparation has been revalidated. It is empty before a
  node starts and retained thereafter as the append-only branch-history anchor;
- `title` and status;
- hard and soft predecessors;
- exact design references with anchors;
- exclusive, read-only, and reserved shared paths;
- implementation and review report paths;
- implementation commits;
- owner, run ID, timestamps, rejection, and compensation metadata. The
  `compensating_plan` field is set by `mine plan reject` and names the
  registered replacement plan that downstream successors are rewired onto via
  `mine plan rewire-compensation` (see
  `docs/design/execution-graph/state-machine-and-algorithms.md#compensation-rewiring`).
  It is the single source of truth for compensation rewiring; no sibling-id
  matching is ever performed.

Absolute worktree locations and observed Git state are runtime DTO evidence,
not persisted graph facts. Commands launched from any linked checkout resolve
and mutate the one authoritative graph in the main integration worktree.

For schema-v3 canonical nodes, `branch_anchor` is empty only in `DRAFT`,
`BLOCKED`, and `READY`. `IN_PROGRESS`, `IMPLEMENTED`, `ACCEPTED`, and
`REJECTED` nodes require a non-empty value that Git resolves to a commit. The
stored object ID is repository-native and is never validated by assuming a
fixed SHA-1 or SHA-256 string length. `READY -> IN_PROGRESS` records it in the
same graph transaction as owner/run/timestamp evidence. Later lifecycle writes
preserve it byte-for-byte.

For canonical Plan nodes, `lineage_id` and `worktree_path` are mandatory
persisted provenance. Graph validation fails when either field is absent,
empty, or inconsistent with the ID and configured worktree root. A lifecycle
writer must never deserialize fields it does not understand and then silently
rewrite the graph without them. Schema evolution therefore uses an explicit
version boundary: an older writer refuses a newer graph, while the current
writer performs any supported deterministic migration before a lifecycle
mutation. Recovery of legacy or damaged metadata is an explicit, validated
operation whose before/after revision is auditable; operators never repair the
TOML by hand.

The current wire schema is `mine-graph-v3`. The v2-to-v3 provenance migration
derives `branch_anchor` only from unambiguous Git and recorded graph evidence:
the first parent of the first ordered implementation commit, or for an active
compensation without implementation commits, its declared rejected
predecessor's preserved branch tip. Every derived anchor must exist as a commit
and be an ancestor of the observed Plan tip. Ambiguous active base nodes,
missing objects, inconsistent commit chains, or non-descendant history are
refused without mutation.

## Safe paths

All graph paths are normalized repository-relative UTF-8 strings using `/` separators.

Rejected forms include absolute paths, empty paths, `..` traversal, repository-escaping symlinks/junctions, broad wildcard ownership patterns, and platform drive roots.

## Design references

Every plan references at least one design leaf. Referencing only `docs/design/index.md` is insufficient unless the plan exclusively changes top-level scope.

```toml
path = "docs/design/interfaces/cli-contract.md"
anchors = ["#json-output", "#error-contract"]
reason = "Defines the public contract implemented by this plan"
```

Plan creation fails when paths or anchors do not exist.

## Release closure evidence

Before workspace closure, the graph points to final accepted states, latest full `mine-sync` report, approved exceptions, release verification, and suggested repository version. These records are temporary and are purged with `docs/plan/`.
