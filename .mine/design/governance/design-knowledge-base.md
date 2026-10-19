# Design Knowledge-Base Structure

## Purpose

The examples describe fresh 0.2.0 governance under
[ADR-0023's namespace contract](repository-layout.md). Installed 0.1.13 source
checkouts retain their configured `docs/design/` and old backup convention
until reviewed terminal cutover. Marker ownership, progressive disclosure and
verified backup guarantees remain; unrelated user `docs/` is not claimed.

`.mine/design/` is a navigable, MINE-owned knowledge base, not a single architecture manuscript or a shared namespace for arbitrary legacy documentation.

## Ownership marker

A managed design root contains:

```text
.mine/design/.mine-design.toml
```

Minimum content:

```toml
schema_version = 1
managed_by = "MINE"
repository_id = "<stable repository UUID>"
created_at = "<UTC timestamp>"
```

`mine init` behavior:

- absent configured Design root: create scaffold and marker;
- valid matching marker: preserve and validate;
- present directory without marker: fail with `MINE_DESIGN_NAMESPACE_CONFLICT`;
- marker with another repository ID: fail with `MINE_DESIGN_OWNERSHIP_MISMATCH`.

Native 0.2.0 `mine init` refuses unmarked/foreign configured governance and an
existing legacy configuration before creating the new namespace. It does not
auto-move user documents or perform terminal cutover. The older 0.1.13 auto-backup
behavior is a bootstrap-only contract, not the fresh-layout behavior.
`mine-sync` requires matching managed ownership before backup or rewrite.

## Hierarchical indexing

Every design area owns an `index.md`.

```text
.mine/design/
├── .mine-design.toml
├── index.md
└── database/
    ├── index.md
    ├── primary/
    │   ├── index.md
    │   ├── schema.md
    │   ├── tables/
    │   │   ├── users.md
    │   │   └── orders.md
    │   └── migrations.md
    └── analytics/
        ├── index.md
        └── datasets.md
```

The root links to domain indexes; domain indexes link to component indexes; leaves contain exact contracts.

## Local backup convention

Before rewriting existing managed design, `mine-sync` creates:

```text
.mine/runtime/design-backups/YYYYMMDDTHHMMSSZ/
```

The backup contains a byte-for-byte logical copy of the managed design tree plus:

```text
.mine/runtime/design-backups/<timestamp>/.gitignore
```

with:

```gitignore
*
```

The backup is local recovery material:

- it is never part of durable design;
- it must not be committed;
- release validation rejects tracked managed runtime/backups and legacy
  `design-backup-*` paths, including nested occurrences;
- it may be removed only after the user or release agent confirms the new design is valid;
- copying must not dereference links that leave the repository.

## Leaf document contract

Each leaf should include:

- purpose and scope;
- governing decisions and invariants;
- interfaces or data contracts;
- lifecycle and ownership;
- failure behavior;
- security and operational considerations;
- tests or verification expectations;
- links to related documents;
- status and last verified version when useful.

## Avoiding duplication

An index may summarize a child in one or two sentences but does not duplicate its full contract. A contract has one authoritative leaf.

## Change rules

- `mine-sync` updates descriptive design from current code using the configured authority order.
- `mine-arch` updates target design before plan creation.
- Every plan points to exact design paths and anchors.
- Accepted implementation details are incorporated before release.
- Obsolete design is deleted or rewritten directly unless the user explicitly protects it.
- A stale design document is not preserved merely because it existed.

## Context-budget rules

Skills do not recursively read the entire tree by default. They begin at indexes, identify affected areas, and load only relevant leaves and direct dependencies.

An unscoped `mine-sync` is the deliberate exception: it may progressively explore the entire repository and design tree. The sync report must state what was fully inspected, sampled, inferred, or left uncertain.
