# Execution Graph Domain Model

## Aggregate

The 0.2.0 writer uses `mine-graph-v7`; the installed 0.1.13 bootstrap source
still uses v4 until its old-binary lifecycle is finished. Ownership and acceptance-evidence
additions are defined in [semantic ownership](../governance/semantic-ownership.md)
and [lifecycle receipts](../governance/lifecycle-receipts.md). Their implementation
uses a new explicit graph-version boundary; they are not fields the installed
0.1.13 writer already preserves. Original Plan text, anchor and lineage remain
immutable. Scheduling exposes original/effective authority through the
[active ownership snapshot](active-ownership-scheduling.md).

The implemented ADR-0022 v7 boundary adds append-only node preparation generations
and a new-start generation reference, specified only in
[durable READY preparation](../governance/ready-preparation.md#preparation-generations-and-schema).
Historical absent evidence remains absent; reading an older graph does not
fabricate preparation, body hashes or acceptance receipts.

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
├── ephemeral_workspace
├── purge_before_stable_release
└── plans[]
```

`workspace_id` is generated internally. It is not a product version and the user does not provide it.

## Plan node

Canonical Plan identity is `planNN(?:-NN)*(?:-CNN)?`, where every `NN` is
`01..99`, `00` is forbidden, and `C` appears at most once as the final segment.
Removing a final compensation segment yields the lineage ID. Examples:
`plan01-02`, `plan01-02-C01`, `plan01-02-C02`.

The Markdown path is exactly `<configured-plan-root>/<id>-<lower-kebab-slug>.md`
(`.mine/plan/` for fresh 0.2.0). Reports use
the same stem plus `-implementation.md` or `-review.md`. The branch is
`plan/<id>-<slug>`. The lineage worktree is derived as
`<worktrees_root>/<lineage-id>` and is shared serially by compensation nodes.

Each `PlanNode` contains:

- `id`: stable within the active workspace;
- `path`: plan Markdown path under the configured Plan root;
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
- `planning_scope`: non-empty scope recorded at registration (ADR-0013), defaulting to the configured design root; immutable afterwards. With `readiness_checked_at` and `adr_registry_revision` — recorded by `mine plan release` when it re-runs planning readiness for that scope — it binds the node to the approved ADR decision set it was planned against;
- `withdraw_reason` and `withdrawn_at`: recorded by `mine plan withdraw` (ADR-0012); empty unless the node is `WITHDRAWN`;
- owner, run ID, timestamps, rejection, and compensation metadata. The
  `compensating_plan` field is set by `mine plan reject` and names the
  registered replacement plan that downstream successors are rewired onto via
  `mine plan rewire-compensation` (see
  `docs/design/execution-graph/state-machine-and-algorithms.md#compensation-rewiring`).
  It is the single source of truth for compensation rewiring; no sibling-id
  matching is ever performed.

The current node additionally contains optional `body_sha256`, `ownership`,
`ownership_policy`, append-only `clarifications`, portable terminal `acceptance`,
append-only `preparations` and `started_preparation_generation`. Their exact
authority and transition rules belong to the linked ownership, lifecycle and
preparation leaves; missing historical values are not synthetic defaults.

Absolute worktree locations and observed Git state are runtime DTO evidence,
not persisted graph facts. Commands launched from any linked checkout resolve
and mutate the one authoritative graph in the main integration worktree.

For schema-v4 canonical nodes, `branch_anchor` is empty only in `DRAFT`,
`BLOCKED`, `READY`, and `WITHDRAWN` (a withdrawn node never started).
`IN_PROGRESS`, `IMPLEMENTED`, `ACCEPTED`, and `REJECTED` nodes require a
non-empty value that Git resolves to a commit. The
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

The bootstrap wire schema is `mine-graph-v4`. Native 0.2.0 reads supported
older graph versions and upgrades normal supported mutations to v7 without
inventing historical evidence. The earlier v3-to-v4 evolution added only
optional node fields (planning scope, release readiness audit, withdrawal
evidence), so v3 graphs upgrade implicitly on the next lifecycle mutation
with no field derivation and no extra revision increment. The v2-to-v3
provenance migration derives `branch_anchor` only from unambiguous Git and
recorded graph evidence:
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

Before workspace closure, the graph records terminal Plan states and their
implementation/review evidence. Final sync reports live separately under
`.mine/runtime/sync/`; release operation/evidence and configured product version
also have their own authorities. They are not extra `PlanWorkspace` fields.
The configured ephemeral Plan tree is purged under the release contract, while
local ignored recovery evidence is retained outside the stable tree.
