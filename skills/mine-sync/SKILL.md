---
name: mine-sync
description: Reconcile MINE-owned design with repository reality using code-first synchronization. Use when onboarding an existing repository, after substantial out-of-band changes, when design drift is suspected, before stable release, or when the user requests a repository/design audit. Creates a verified local backup before rewriting design, then updates design to match current code unless the user explicitly protects a decision. Does not modify business code without a separate architecture/plan/execute flow.
version: 0.1.13
---

# MINE Sync

MINE Is Not Everyone's. `mine-sync` is MINE's code-first, high-cost synchronization Skill that makes `docs/design/` an accurate description of the repository that actually exists. It is destructive to managed design **by intent** and bounded by strict safety rules. It is the code-authoritative counterpart to `mine-arch` (which is requirement-first and may deliberately diverge from current code).

This Skill's behavior is governed by `docs/design/governance/design-sync.md` and ADR-0005 ("Code Is the Default Authority During `mine-sync`"). This document states the exact procedure; any summarized version elsewhere does not override it.

## Integration: MCP tools and CLI fallback

`mine-sync` validates the design knowledge base through two paths, in this
order of preference:

1. **MCP tools (preferred)** - when the current Agent runtime exposes the
   MINE MCP server (`mine mcp serve`), call the typed MCP tools.
2. **JSON CLI (deterministic fallback)** - when MCP is unavailable, call
   `mine --format json` commands.

Never invent an MCP tool, CLI command, flag, or lifecycle transition that the
current binary does not expose. Never edit `docs/plan/execution-graph.toml` or
`docs/plan/execution-graph.md` directly.

The accepted MCP tools `mine-sync` may use:

- `mine_design_validate` (no arguments) - validate the design namespace
  (marker, index links, anchors, size thresholds).
- `mine_graph_validate` (no arguments) - validate the execution graph when a
  plan workspace is active.
- `mine_graph_status` (no arguments) - read the current revision and workspace.

Operations `mine-sync` needs that are intentionally **CLI-only** (no MCP tool
exposes them, because they create local recovery state or read marker
identity outside the graph):

- `mine design backup --format json` - create the verified local design
  backup **before any mutation** (no MCP equivalent; backups are local
  recovery material, never tracked or exposed over MCP).
- `mine design status --format json` - marker/identity confirmation.

The backup is mandatory and must precede any design rewrite. Use the CLI for
it even when MCP is available. When a required operation has no MCP tool,
fall back to the JSON CLI and state the fallback explicitly.

## When to use `mine-sync`

- onboarding an existing repository;
- after substantial manual or out-of-band changes;
- when design drift is suspected;
- before stable release;
- when the user explicitly requests a repository/design audit.

Do **not** confuse `mine-sync` with `mine-arch`: `mine-arch` designs target architecture from requirements and may intentionally differ from current code; `mine-sync` makes design describe the repository that already exists. They answer different questions in opposite directions.

## Safety boundary (non-negotiable)

`mine-sync` is destructive to managed design by intent. It is **not** authorized to:

- Never edit `docs/plan/execution-graph.toml` or `docs/plan/execution-graph.md` directly (`AGENTS.md` documents this rule; the accepted MINE CLI is the only path that mutates graph state).

- modify business code unless separately requested through planning and execution;
- delete arbitrary non-MINE documentation;
- follow links outside the repository;
- stage, commit, or push any changes (unrelated or otherwise);
- execute arbitrary shell deletion;
- use `git reset --hard`, `git clean`, blind stash, force push, or public-history rewriting;
- edit `docs/plan/execution-graph.toml` or `docs/plan/execution-graph.md` directly.

It writes only to MINE-managed design (after a verified backup) and to its local sync report under `.mine/runtime/sync/`. Handoff to planning, execution, and review is an explicit user action. When drift reveals a needed **target** change, hand off to `mine-arch`; when it reveals needed **code** change, hand off to `mine-plan-create`.

## Procedure

Exactly this order. Do not skip or reorder.

### 1. Validate repository, branch, marker, and working-tree conditions

- Confirm the repository root and that a MINE-managed tree exists: `docs/design/.mine-design.toml` must parse, its `managed_by` is `MINE`, and its `repository_id` matches the configured repository identity.
- Refuse **legacy unmarked `docs/design/`**: if `docs/design/` exists but `.mine-design.toml` is absent, stop. This is a namespace conflict (ADR-0006). Do not guess whether old documents are authoritative, compatible, or safe to overwrite. The deterministic resolution is `mine init`, which auto-backs-up the legacy directory to `docs/design-backup-<UTC timestamp>/` and creates a fresh managed root; after that, sync may proceed. The user is never told to move or delete the legacy directory by hand. Warn the user explicitly.
- Reject a marker belonging to another repository (`MINE_DESIGN_OWNERSHIP_MISMATCH`) or a malformed marker (`MINE_DESIGN_MARKER_INVALID`).
- Record the current branch and HEAD commit (read-only Git evidence; no mutation).
- A non-clean working tree does not block a read+design-write sync, but report it; never stage or commit to "clean up".

### 2. Create and verify the local design backup (mandatory, before any mutation)

When managed design contains real content, synchronization begins by creating a backup **before** rewriting any design document:

- `docs/design-backup-YYYYMMDDTHHMMSSZ/` (UTC, sortable timestamp; the timestamp is the backup directory name — no user-controlled path).
- Verify the source marker and repository ownership first.
- Verify the destination does not already exist.
- Copy regular files and repository-internal links **without following links outside the repository**.
- Write `.gitignore` containing exactly `*` in the backup root so the backup is never tracked or staged for release.
- Verify the copied file manifest (and hashes where practical).
- Record the backup path in the local sync report.
- Perform **no** design mutation until backup verification succeeds. A failed backup blocks synchronization.

Use `mine design backup --format json` to perform this backup when available; its JSON envelope reports `backup_path`, `file_count`, and `total_bytes`. Do not reimplement the backup by hand.

### 3. Inventory repository scope and evidence

Use the discovery scope below (user-scoped or unscoped). Inventory:

- repository map and build systems;
- entry points and deployable units;
- modules and dependency direction;
- public APIs, schemas, configuration, persistence;
- lifecycle, security, operations, tests.

### 4. Traverse existing design through indexes

MINE design uses **progressive disclosure** rooted at `docs/design/index.md`. Begin at the root index, identify affected areas, and load only the leaves and direct dependencies relevant to the requested change. Do not recursively read the entire tree unless an unscoped audit explicitly requires it.

### 5. Build a code-to-design traceability map

For every material area, map: code → design document → discrepancy class. Distinguish:

- current implemented reality;
- existing design description;
- whether the user explicitly protected the design decision.

### 6. Apply the authority order

During synchronization, apply this strict order (ADR-0005):

1. explicit current user instructions, including named design decisions to preserve;
2. current observable code, schemas, configuration, generated contracts, and runtime behavior;
3. tests and comments as evidence to inspect, not unquestioned authority;
4. existing design only where repository behavior does not determine the answer;
5. model inference, clearly marked as inference.

Therefore **code wins by default** when code and design disagree and the user has not protected the design decision. This rule applies only to synchronization; `mine-arch` may create a target design that intentionally differs from current code.

### 7. Classify discrepancies and act

| Class | Default action |
|---|---|
| Code differs from unprotected design | Update design to match code |
| User-protected design differs from code | Preserve design, report implementation drift, require planning before release |
| Design missing for implemented behavior | Add design |
| Design describes removed behavior | Delete or rewrite design |
| Code is ambiguous or dynamically generated | Record uncertainty and inspect generated/runtime evidence |
| Suspicious or unsafe implementation | Document actual behavior and prominently flag risk; do not silently redesign code |
| Current code cannot be built or inspected | Mark coverage incomplete; do not claim clean sync |
| Material user decision required | Ask the user and block final attestation |

### 8. Rewrite, split, add, move, or delete managed design documents

Rewrite the modular design tree to match code. Maintain the progressive-disclosure structure:

- every design area owns an `index.md`;
- root index links to domain indexes; domain indexes link to component indexes; leaves contain exact contracts;
- one contract has one authoritative leaf; indexes summarize but do not duplicate;
- split documents that become oversized or mix ownership;
- update every affected parent index and cross-link;
- delete or rewrite obsolete design directly unless the user explicitly protects it (no-historical-baggage, per `AGENTS.md`).

### 9. Record uncertainties, suspicious behavior, and incomplete coverage

Never hide uncertainty. The sync report must state explicitly what was fully inspected, sampled, inferred, or left uncertain. Do not claim complete coverage when only sampling.

### 10. Validate the resulting knowledge base

Run link, marker, ownership, anchor, and document-size validation. Prefer MCP
tools when the runtime exposes the MINE MCP server; fall back to `mine --format
json` CLI otherwise:

- `mine_design_validate` (MCP) or `mine design validate --format json` (CLI)
  - returns `valid` and any warnings, including tracked `docs/design-backup-*`
    paths;
- `mine_graph_validate` (MCP) or `mine graph validate --format json` (CLI) when
  a plan workspace is active;
- `mine design status --format json` (CLI only - no MCP tool) for
  marker/identity confirmation.

A tracked `docs/design-backup-*` path is a release gate failure: backups are
local recovery material and must not be committed.

### 11. Write the local sync report under `.mine/runtime/sync/`

Record:

- repository, branch, and commit inspected;
- user-provided scope, or confirmation that sync was unscoped;
- backup path and verification result;
- files, modules, schemas, APIs, and runtime evidence inspected;
- design documents added, rewritten, moved, split, or deleted;
- protected decisions;
- discrepancy classifications;
- incomplete coverage and unresolved risks;
- status: `SYNCHRONIZED`, `SYNCHRONIZED_WITH_WARNINGS`, or `BLOCKED`.

Only a full-release sync with no blocking uncertainty permits stable release closure. When a plan workspace is active, optionally copy release-relevant evidence to its temporary reports directory.

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

When no scope is supplied, the agent is authorized to explore the repository broadly using staged discovery:

1. repository map and build systems;
2. entry points and deployable units;
3. major modules and dependency direction;
4. public APIs, schemas, configuration, and persistence;
5. lifecycle, security, operations, and tests;
6. targeted deep reads for ambiguous or high-risk areas.

The user accepts the token and runtime cost of an unscoped request. The agent must **not** claim complete coverage when it only sampled the repository.

## No business-code mutation

`mine-sync` does not modify business code without a separate architecture/plan/execute flow. Drift that reveals needed code change is a handoff to `mine-plan-create`, not a sync-time edit. Suspicious or unsafe implementation is documented and flagged, never silently redesigned in code during sync.

## Final attestation

A full sync report records repository, branch, commit, scope, backup path, inspected evidence, design changes, protected decisions, discrepancy classifications, incomplete coverage, and status (`SYNCHRONIZED`, `SYNCHRONIZED_WITH_WARNINGS`, or `BLOCKED`). Only a full-release sync with no blocking uncertainty permits stable release closure.

## Shared toolchains and Plan-isolated mutable outputs

Reuse Root-managed toolchains, dependency downloads, virtual environments, and
only caches proven read-only or safely content-addressed. Do not create a
second `.venv`, `venv`, `target`, `node_modules`, or equivalent environment or
cache directory in a linked Plan worktree.

Mutable build, test, generation, coverage, packaging, and runner outputs must
use `<Root>/.mine/build/<plan-id>/<tool-or-output-class>/`. For Rust, set
`CARGO_TARGET_DIR` to the exact Plan namespace, for example
`<Root>/.mine/build/<exact-plan-id>/cargo-target`; never use Root `target` or a
worktree-local `target`. If a measured output is predictably very large,
record the estimate and pause for explicit owner agreement on serialization,
a coarser exclusive namespace, and cleanup; never fall back automatically to a
globally shared mutable directory.
