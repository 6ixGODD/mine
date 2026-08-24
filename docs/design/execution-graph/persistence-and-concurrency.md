# Execution Graph Persistence and Concurrency

## Machine fact source

During an active plan workspace:

```text
docs/plan/execution-graph.toml
```

Generated view:

```text
docs/plan/execution-graph.md
```

Markdown is never a mutation target.

## Why TOML

TOML is readable, diffable, branch-aware, and simple to parse. SQLite and binary formats are rejected because the graph is small, workspace-scoped, and Git-reviewable.

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

The current graph wire schema is `mine-graph-v3`. In addition to canonical
branch/lineage/worktree provenance, every started node persists its immutable
`branch_anchor`. Older writers must refuse v3 before mutation; they must never
round-trip unknown fields away.

The sole supported v2-to-v3 path is the explicit, revision-checked
`mine graph repair-provenance` operation. It derives missing anchors only from
the ordered implementation-commit chain or an active compensation's declared,
preserved rejected-predecessor tip, validates actual Git commit objects and
ancestry, and refuses ambiguity. It increments the revision exactly once when
it migrates and is byte-preserving/idempotent on valid v3. Operators never edit
the graph files by hand or treat reflogs/runtime files as provenance authority.

## Revision and locking

Every successful mutation increments revision exactly once. Mutations accept `expected_revision`; mismatch returns `MINE_REVISION_CONFLICT` without writing.

Writers acquire an exclusive lock, reload state, recheck revision, write
atomically, then render Markdown from committed TOML. `mine plan start` also
re-reads Root and registered linked-worktree Git evidence under this lifecycle
lock immediately before recording `branch_anchor`; the lock serializes MINE
graph writers but is not misrepresented as a lock on arbitrary Git processes.

The exclusive lock is a cross-platform (POSIX and Windows) advisory file lock. Per `AGENTS.md` ("Business code must not use `unsafe`"), `mine` core Rust code (`src/`) must implement this lock through a maintained, vetted external locking crate (for example `fs4`, the maintained successor to `fs2`, which wraps the platform lock APIs — POSIX `flock`/`fcntl` and Windows `LockFileEx` — behind a safe `std::fs::File` extension trait) rather than hand-written `unsafe extern` FFI blocks in this repository's own source. Any `unsafe` needed to call the platform lock API must live inside the vetted dependency, not inside `mine`'s own crate. Adding this dependency is a routine `Cargo.toml`/`Cargo.lock` change and does not require a design exception.

## Plan-workspace purge safety

`mine workspace close --purge-plan-workspace` may delete only canonical repository-relative `docs/plan/` after all gates pass.

- expected workspace ID and ownership marker are mandatory;
- dry-run is available and used by Skills before mutation;
- repository root, filesystem root, empty paths, external links, and non-MINE directories are rejected;
- no `rm -rf`, shell expansion, `git clean`, or broad deletion is used;
- failure leaves the workspace intact.

## Design backup is separate

Timestamped `docs/design-backup-*` directories are local recovery artifacts created by `mine design backup`. They are not execution-graph state, are never included in stable release, and are not automatically purged until design validation succeeds and the managing agent confirms they are no longer needed.
