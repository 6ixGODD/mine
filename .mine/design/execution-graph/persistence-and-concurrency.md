# Execution Graph Persistence and Concurrency

## Machine fact source

Accepted performance/transaction targets are specified in
[Git evidence acquisition](git-evidence-and-performance.md) and
[recoverable lifecycle operations](../governance/lifecycle-receipts.md).
Candidate source implements request snapshots and native checkpoint recovery;
installed 0.1.13 remains the live bootstrap authority. The configured Plan root
owns source/view (`.mine/plan/` for fresh 0.2.0); literal `docs/plan/` examples
below describe the preserved bootstrap layout, not a second writable root.

During an active plan workspace:

```text
docs/plan/execution-graph.toml
```

Generated view:

```text
docs/plan/execution-graph.md
```

Markdown is never a mutation target.

## Generated view contract

`docs/plan/execution-graph.md` is a deterministic render of the machine TOML.
The renderer is the single writer; the view is never edited.

- Dependency edges render as real edges. Hard and soft dependencies stay
  distinguishable, and every rendered edge corresponds to an actual
  predecessor entry on a node.
- Topological order, when shown, renders as an ordered enumeration explicitly
  labeled as one legal execution order (numbered or comma-separated). Arrows
  (`->`) denote dependency edges only; the view must never arrow-join plans
  that merely happen to be adjacent in a topological sort, because readers
  would mistake sort adjacency for a hard dependency that contradicts the
  predecessor table and the real DAG.
- `mine graph validate` compares the on-disk view against a fresh
  deterministic render of the TOML. Findings distinguish an invalid machine
  source from a stale generated view. Validation never rewrites either file.
  Under ADR-0022, diagnostics distinguish exact-operation pending recovery,
  checkpoint-proven missing-view restoration via
  `mine graph render --expected-revision <observed-revision>`, and foreign or
  uncheckpointed view drift that must be preserved and classified. Never promise
  render can overwrite arbitrary stale/hand-edited Markdown. The exact target
  contract is [standalone restoration](../governance/root-graph-checkpoints.md#standalone-generated-view-restoration).

## Why TOML

TOML is readable, diffable, branch-aware, workspace-scoped and Git-reviewable.
Hundreds of nodes require bounded query/traversal work, not replacement with
SQLite or a daemon merely to compensate for repeated Git process creation.

## Local MINE directory

```text
.mine/
├── config.toml
├── runtime/
│   ├── events.jsonl
│   ├── mine.log
│   └── sync/
└── locks/
    └── execution-graph.lock
```

The graph never moves into `.mine/`.

## Wire schema and provenance migration

The current graph wire schema is `mine-graph-v4`. In addition to canonical
branch/lineage/worktree provenance, every started node persists its immutable
`branch_anchor`, and every node carries the optional v4 fields: the
registration-time `planning_scope` with release-time readiness audit
(`readiness_checked_at`, `adr_registry_revision`; ADR-0013) and the
`WITHDRAWN` withdrawal evidence (`withdraw_reason`, `withdrawn_at`;
ADR-0012). Older writers refuse newer schemas before mutation; they must
never round-trip unknown fields away.

The current reader accepts `mine-graph-v3` read-only. No v4 field requires
derivation (all are optional with defaults), so a v3 graph upgrades
implicitly on the next lifecycle mutation: the writer always emits `mine-graph-v4`
and the schema bump rides the mutation's own single revision increment.
`mine graph repair-provenance` upgrades a v3 graph as a version-only
migration (one revision increment, no plan repairs).

The v2-to-v3 anchor migration is unchanged and remains the only provenance
derivation path: the explicit, revision-checked
`mine graph repair-provenance` operation derives missing anchors only from
the ordered implementation-commit chain or an active compensation's declared,
preserved rejected-predecessor tip, validates actual Git commit objects and
ancestry, and refuses ambiguity. It increments the revision exactly once when
it migrates and is byte-preserving/idempotent on the current schema.
Operators never edit the graph files by hand or treat reflogs/runtime files
as provenance authority.

Approved ADR-0022's pending v7 preparation extension is authoritative in
[durable READY preparation](../governance/ready-preparation.md#preparation-generations-and-schema).
It follows the planned v5/v6 additions, preserves historical anchors and absent
legacy READY evidence, and does not migrate the real installed-0.1.13 Root.

ADR-0014's `mine graph repair-compensation-predecessors` is the only
current-schema semantic repair path. It may bypass initial validation solely
to replace one unstarted compensation's hard predecessors with the exact list
derived from its declared rejected predecessor. It requires an explicit graph
revision and reason, refuses any execution or compensation-branch evidence,
fully validates the candidate before persistence, and uses the same atomic
TOML plus deterministic-render transaction. It is separate from versioned
provenance migration and accepts no arbitrary field value.

## Revision and locking

Every successful graph-state mutation increments revision exactly once. Mutations
accept `expected_revision`; mismatch returns `MINE_REVISION_CONFLICT` without writing.
Checkpoint-proven standalone view restoration is not a graph-state mutation:
it uses the same lock and revision guard but leaves source/revision/Git unchanged.

Writers acquire an exclusive lock, reload state, recheck revision, write
TOML atomically, then render Markdown from that same validated snapshot.
The two files are not a fictitious atomic transaction: TOML remains authoritative
if rendering fails, and the operation reports committed-state/repair-needed.
`mine plan start` also
re-reads Root and registered linked-worktree Git evidence under this lifecycle
lock immediately before recording `branch_anchor`; the lock serializes MINE
graph writers but is not misrepresented as a lock on arbitrary Git processes.

The accepted target carries the observer's expected revision through every
lifecycle adapter and checks it before expensive Git validation. Current ordinary
CLI/MCP writes still obtain revisions internally; this enforcement is pending.

ADR-0022 additionally makes each native graph mutation's
[exact local Git checkpoint](../governance/root-graph-checkpoints.md) part of
operation completion. A persisted TOML/render/checkpoint partial result exposes
actual revisions and blocks subsequent writes, scheduling grants and closure
until same-operation recovery. The graph lock is not a Git/index/worktree lock;
the checkpoint protocol uses separate exact index/ref checks, without nested
graph-lock acquisition or a cross-resource atomicity claim.

The exclusive lock is a cross-platform (POSIX and Windows) advisory file lock. Per `AGENTS.md` ("Business code must not use `unsafe`"), `mine` core Rust code (`src/`) must implement this lock through a maintained, vetted external locking crate (for example `fs4`, the maintained successor to `fs2`, which wraps the platform lock APIs — POSIX `flock`/`fcntl` and Windows `LockFileEx` — behind a safe `std::fs::File` extension trait) rather than hand-written `unsafe extern` FFI blocks in this repository's own source. Any `unsafe` needed to call the platform lock API must live inside the vetted dependency, not inside `mine`'s own crate. Adding this dependency is a routine `Cargo.toml`/`Cargo.lock` change and does not require a design exception.

## Plan-workspace purge safety

`mine workspace close --purge-plan-workspace` may delete only the configured,
contained, ownership-marked Plan root after all gates pass (`.mine/plan/` for
fresh 0.2.0; the separate bootstrap writer retains `docs/plan/`).

- expected workspace ID and ownership marker are mandatory;
- dry-run is available and used by Skills before mutation;
- repository root, filesystem root, empty paths, external links, and non-MINE directories are rejected;
- no `rm -rf`, shell expansion, `git clean`, or broad deletion is used;
- failure leaves the workspace intact.

## Design backup is separate

Native 0.2.0 backups live under `.mine/runtime/design-backups/`; installed
0.1.13 uses timestamped `docs/design-backup-*` directories. Both are local
recovery artifacts, not execution-graph state, and never enter stable release.
Neither is automatically purged merely because Design validation succeeds;
the managing owner must confirm it is no longer needed.
