# Code-to-Design Synchronization

## Purpose

Paths resolve from validated Root configuration. Under the implemented candidate
[0.2.0 namespace and cutover contract](repository-layout.md), fresh sync
owns `.mine/design/` and verified backups live in
`.mine/runtime/design-backups/`; arbitrary user `docs/` remains untouched.
Installed 0.1.13 keeps its existing `docs/design/` and timestamped `docs/`
backups until terminal release cutover. Never move live bootstrap governance
early. Backup-before-rewrite and owner-authority rules remain intact.

`mine-sync` is MINE's high-cost, code-first synchronization Skill. It creates or reconciles the durable design knowledge base so it represents current repository reality.

It is used:

- when onboarding an existing repository;
- after substantial manual or out-of-band changes;
- when design drift is suspected;
- before stable release;
- when the user explicitly requests a repository/design audit.

## Supported starting states

### Managed design exists

A valid marker in the configured Design root matches the current repository.
`mine-sync` backs up that exact tree, verifies bytes, explores code, compares,
and rewrites Design. A different checkout's backup is insufficient unless the
pre-rewrite trees are proven byte-identical.

### No meaningful design exists

The MINE scaffold exists but contains no real architecture, or the design root is absent before initialization. `mine-sync` scans repository reality and creates a descriptive modular baseline.

### Unmarked or foreign configured governance

Sync refuses a tree init has not claimed. Candidate init refuses foreign or
unmarked configured Design and active legacy layout without moving/overwriting
it. Finish existing lifecycle writes with the governing old binary and use the
reviewed terminal cutover; no automatic migration or second writable authority.
Unrelated user `docs/` is not a namespace conflict for fresh `.mine/design/`.
MINE never guesses that unknown documents are disposable.

## Mandatory backup before rewrite

When managed design contains real content, synchronization begins by creating:

```text
.mine/runtime/design-backups/YYYYMMDDTHHMMSSZ/
```

The timestamp is UTC and sortable. The backup operation:

1. verifies the source marker and repository ownership;
2. verifies the destination does not exist;
3. copies regular files and repository-internal links without following links outside the repository;
4. writes `.gitignore` containing exactly `*` in the backup root;
5. verifies the copied file manifest and hashes where practical;
6. records the backup path in the local sync report;
7. performs no Design mutation until backup verification succeeds.

A failed backup blocks synchronization.

## Discovery scope

### User-scoped sync

When the user names packages, directories, services, APIs, tables, symbols, or subsystems, start there and follow:

- direct imports and dependencies;
- inbound consumers;
- public contracts;
- persistence and schema ownership;
- lifecycle and operational boundaries;
- relevant tests and deployment configuration.

The agent may widen scope when required to avoid a false local model, but it records why.

### Unscoped sync

When no scope is supplied, the agent is authorized to explore the repository broadly. It should use staged discovery:

1. repository map and build systems;
2. entry points and deployable units;
3. major modules and dependency direction;
4. public APIs, schemas, configuration, and persistence;
5. lifecycle, security, operations, and tests;
6. targeted deep reads for ambiguous or high-risk areas.

The user accepts the token and runtime cost of an unscoped request. The agent must not claim complete coverage when it only sampled the repository.

## Authority order

During `mine-sync`, apply this order:

1. explicit current user instructions, including named design decisions to preserve;
2. current observable code, schemas, configuration, generated contracts, and runtime behavior;
3. tests and comments as evidence to inspect rather than unquestioned authority;
4. existing design only where repository behavior does not determine the answer;
5. model inference, clearly marked as inference.

Therefore, code wins by default when code and design disagree and the user has not protected the design decision.

This rule applies only to synchronization. `mine-arch` may create a target design that intentionally differs from current code.

## Synchronization procedure

1. validate repository, branch, marker, and working-tree conditions;
2. create and verify the local design backup;
3. inventory repository scope and evidence;
4. traverse existing design through indexes;
5. build a code-to-design traceability map;
6. classify discrepancies;
7. rewrite, split, add, move, or delete managed design documents as needed;
8. update every affected parent index and cross-link;
9. record uncertainties, suspicious behavior, and incomplete coverage;
10. run link, marker, ownership, anchor, and document-size validation;
11. write a local report under `.mine/runtime/sync/`;
12. when a plan workspace is active, optionally copy release-relevant evidence to its temporary reports directory.

## Discrepancy classes

| Class | Default action |
|---|---|
| Code differs from unprotected design | Update design to match code |
| User-protected design differs from code | Preserve design, report implementation drift, require planning before release |
| Design missing for implemented behavior | Add design |
| Design describes removed behavior | Delete or rewrite design |
| Code is ambiguous or dynamically generated | Record uncertainty and inspect generated/runtime evidence |
| Suspicious or unsafe implementation | Document actual behavior and prominently flag risk; do not silently redesign code |
| Current code cannot be built or inspected | Mark coverage incomplete and do not claim clean sync |
| Material user decision required | Ask the user and block final attestation |

## Output and attestation

The report records:

- repository, branch, and commit inspected;
- user-provided scope or confirmation that sync was unscoped;
- backup path and verification result;
- files, modules, schemas, APIs, and runtime evidence inspected;
- design documents added, rewritten, moved, split, or deleted;
- protected decisions;
- discrepancy classifications;
- incomplete coverage and unresolved risks;
- status: `SYNCHRONIZED`, `SYNCHRONIZED_WITH_WARNINGS`, or `BLOCKED`.

Only a full-release sync with no blocking uncertainty permits stable release closure.

## Safety boundary

`mine-sync` is destructive to managed design by intent. It is not authorized to:

- modify business code unless separately requested through planning and execution;
- delete arbitrary non-MINE documentation;
- follow links outside the repository;
- stage or commit unrelated changes;
- execute arbitrary shell deletion;
- use `git reset --hard`, `git clean`, blind stash, force push, or public-history rewriting.
