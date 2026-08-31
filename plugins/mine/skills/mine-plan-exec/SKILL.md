---
name: mine-plan-exec
description: Execute one repository implementation plan end to end in its MINE-prepared linked worktree. Use when the user invokes mine-plan-exec with a plan ID or file, asks to implement a governed plan, or wants dependency checks, code changes, verification, reporting, commits, and graph updates. Never implement directly on stable or dev.
version: 0.1.13
---

# MINE Plan Execute

MINE Is Not Everyone's. Treat the supplied plan as an immutable execution contract. Implement it on the MINE-managed `plan/<id>-<slug>` branch, verify it, report it, and leave the
current branch ready for review. Do not merely describe the work.

## Integration: MCP tools and CLI fallback

`mine-plan-exec` queries and transitions plan state through two paths, in this
order of preference:

1. **MCP tools (preferred)** - when the current Agent runtime exposes the
   MINE MCP server (`mine mcp serve`), call the typed MCP tools. They return
   the same DTOs as the JSON CLI and never touch the execution-graph files.
2. **JSON CLI (deterministic fallback)** - when MCP is unavailable, call
   `mine --format json` commands. Never parse human output.

Never invent an MCP tool, CLI command, flag, JSON field, or lifecycle
transition that the current binary does not expose. Never edit
`docs/plan/execution-graph.toml` or `docs/plan/execution-graph.md` directly.

The accepted MCP tools `mine-plan-exec` may use:

- `mine_graph_status` (no arguments) - read the current revision (carry
  `data.revision` as `expected_revision` before mutating).
- `mine_plan_show` (`id`) - read the target node and verify it is `READY` or
  already `IN_PROGRESS` for this implementation.
- `mine_plan_start` (`id`, `owner`?, `run_id`?) - transition `READY` ->
  `IN_PROGRESS` before any production-file edit.
- `mine_plan_mark_implemented` (`id`, `report`, `commits`) - record the
  implementation report and commit evidence (`IMPLEMENTED` status).
- `mine_graph_validate` (no arguments) - validate the graph after transitions.

Operations `mine-plan-exec` needs that are intentionally **CLI-only** (no MCP
tool exposes them):

- `mine graph show --format json` - obtain the complete authoritative node
  inventory for user-facing invocation resolution.
- `mine plan prepare --id <exact-stored-id> --format json` - create or safely
  reuse the registered linked worktree before start.
- `mine plan release --id <id> --format json` - if a predecessor was just
  accepted and the target node is still `BLOCKED`, release is the gate into
  `READY`. There is no MCP tool for release; it is a CLI fallback. (Normally
  `mine-plan-create` or `mine-plan-review` performs release; `mine-plan-exec`
  only does so when explicitly authorized to advance a just-unblocked node.)

When a required operation has no MCP tool, fall back to the JSON CLI and state
the fallback explicitly.

## Resolve the requested plan

Accept exactly one `<plan-id-or-registered-path>`. Strip only an
invocation-level `@`, then call `mine graph show --format json` from Root
authority and match against the returned node IDs and registered paths.

For each graph node, the accepted inputs are:

- its exact stored ID or exact registered path;
- for a canonical stored ID beginning with `plan`, the ID with that prefix
  removed or with that prefix replaced by one uppercase `P` (`planNN`, `NN`,
  `PNN`; `planNN-CNN`, `NN-CNN`, `PNN-CNN`);
- for the one legacy bootstrap node whose stored ID uses the `NN` placeholder,
  accept `NN`, `PNN`, and `planNN`, all resolving to that exact stored ID.

Collect matches by exact equality and require exactly one distinct graph node.
Fail clearly on zero or multiple matches. Matching is case-sensitive. Never
use prefix/substring matching, guess a filename, fill in a missing
compensation ordinal, select the latest compensation, or pass shorthand to a
binary command. Inputs such as `NN-C` do not mean `planNN-CNN`. After unique
resolution, use only the exact stored ID for `mine plan show`, `mine plan
prepare`, and every lifecycle command. Never select another node merely
because it is READY.

## Load governance and evidence

Before modifying anything, locate the repository root and read these sources completely in this order:

1. Root `AGENTS.md`.
2. The design knowledge base rooted at `docs/design/index.md` (and the relevant leaves named by `AGENTS.md`).
3. Query the plan and graph through `mine_plan_show` (MCP) or `mine plan show --id <id> --format json` (CLI fallback), and `mine_graph_status` (MCP) or `mine graph status --format json` (CLI fallback); use `docs/plan/execution-graph.md` only as a generated readable view.
4. The requested plan.
5. `docs/plan/parallel-execution-protocol.md` when the plan declares a parallel lane.
6. Every hard-predecessor acceptance report and the commits named by those reports.
7. Fetch and read the official sources or best-practice references explicitly registered by the plan; do not implement from the plan's paraphrase alone.
8. Only the implementation files, tests, and documentation needed to verify current reality for this plan.

Apply repository governance in this order: `AGENTS.md` → architecture → registered official sources → requested plan. A newer explicit user
instruction may narrow or override the requested operation, but never silently rewrite the immutable plan.

Convert the plan into an execution checklist before coding. For every implementation step identify its inputs, target files/interfaces,
edge cases, verification, and acceptance item. Do not rediscover or change resolved product decisions.

## Enforce the execution gate

Read the execution graph rather than inferring readiness from filenames.

- Require every hard predecessor to be `ACCEPTED` and backed by an implementation commit plus an acceptance report.
- Require the requested node to be `READY` or already `IN_PROGRESS` for this same implementation.
- If the node is `BLOCKED`, `CONDITIONAL` without its gate, `REJECTED`, or already `ACCEPTED`, do not implement it. Report the exact state
  and missing evidence.
- Treat an uncommitted predecessor or missing report as not accepted.
- Detect conflicts between the graph, plan, architecture, reports, and code. Stop before mutation when the conflict changes scope, contracts,
  or acceptance criteria; explain the concrete conflict to the user.

Do not edit, rename, renumber, delete, or append to the requested plan or any earlier immutable plan. If implementation proves the plan or
architecture wrong, stop and request a compensating design/plan instead of improvising.

## Work in the assigned plan branch

Plan implementation runs on the MINE-managed `plan/<id>-<slug>` branch,
created or reused only by `mine plan prepare` - never directly on the stable
branch or on `dev`. Call `mine plan show --id <exact-stored-id> --format json`,
then `mine plan prepare --id <exact-stored-id> --format json`. Change the
process directory to the returned absolute worktree path and verify its
observed branch equals the registered branch before any write or `plan start`.
A scheduler- or user-supplied worktree is authoritative only when the same
prepare call reports that exact path and branch. A refusal blocks execution;
never fall back to direct Git branch or worktree creation.

Use the returned authoritative Root for graph commands. Root remains on the
integration branch and is never switched. Compensation nodes reuse the
binary-reported lineage directory. For an `IN_PROGRESS` continuation,
preparation is valid only when observed HEAD equals the registered branch tip
and the persisted `branch_anchor` is an ancestor of that tip; current Root HEAD
is never substituted as the anchor.
- Never implement on stable or `dev` directly.
- Never stash, reset, clean, restore, or checkout to discard existing
  changes; never force-push or rewrite shared history.
- Do not tell the user to merge a hidden implementation branch afterward.
- Do not delegate plan execution unless the user explicitly requests
  delegation.

Before editing, inspect `git status --short`, the current branch, and staged changes. Classify existing changes as:

- **same-plan work**: inspect and continue it rather than duplicating it;
- **unrelated work**: preserve it and exclude it from plan commits;
- **ambiguous overlap**: investigate with diffs and history; ask the user only if safe integration cannot be determined.

Dirty status alone is not a blocker. Shared files are a blocker only when existing edits overlap the plan and cannot be preserved or
integrated safely. Stage explicit paths only; never use `git add .` or `git add -A` in a dirty workspace.

Before editing production files, read the current revision: call `mine_graph_status` (MCP) or `mine graph status --format json` (CLI fallback) (carry `data.revision` as `expected_revision`) and start the plan: call `mine_plan_start` (MCP) with the exact stored `id`, `owner`, `run_id`; or `mine plan start --id <exact-stored-id> --owner <owner> --run-id <run> --format json` (CLI fallback). Proceed only after MINE returns `IN_PROGRESS` (exit 0) and the returned persisted `branch_anchor` equals the prepared worktree HEAD. The accepted MINE CLI and MCP tools read the current revision under the lock themselves; they emit `revision_before`/`revision_after` in the envelope. Never edit either execution-graph file directly. For parallel lanes, MINE remains the serialized status owner and the plan's path ownership rules remain mandatory.

For a parallel lane, obey its exclusive write paths and read-only dependencies exactly. Do not edit reserved shared files, even to register
an import, CLI command, dependency, or fixture; record those needs as integration requests for the final integration plan. Never run two
owners on the same lane or create a second implementation after a timeout—inspect and continue the existing diff.

## Implement the plan

Execute the plan steps in order unless the plan explicitly permits parallel order. Keep changes within its scope.

1. Establish or run the plan's failing regression evidence before the fix when requested.
2. Implement the smallest cohesive production change that satisfies the step and architecture.
3. Add or update deterministic tests for the changed contract.
4. Run the step's narrow verification before proceeding.
5. Update documentation/configuration generated or owned by the changed interface.
6. Re-read the plan checklist after each major step to prevent scope drift.

Use the repository's managed toolchain and editing conventions. Preserve restricted data, generated artifacts, credentials, checkpoints, and
user files according to `AGENTS.md` and the architecture. Never print, stage, commit, or copy secrets into reports.

When the plan authorizes cleanup, operate on the actual current workspace so local leftovers are visible. Prefer moving uncertain user
artifacts to the plan's ignored quarantine path over deleting them. Never use broad destructive commands such as `git clean -fdx`.

Do not add compatibility aliases, migrations, or shims unless the plan or user explicitly requires them. Do not perform remote execution,
uploads, submissions, deployments, or pushes unless the plan and user authorize that external mutation.

If execution proves an immutable requirement infeasible, stop. Preserve and,
when safe, commit a minimal reproduction; leave Root and the Plan worktree
clean; report the exact evidence and keep the graph `IN_PROGRESS`. Never claim
`IMPLEMENTED`, fabricate implementation evidence, edit the Plan, reject the
Plan yourself, or create its compensation. Independent review owns the
evidence-bound active rejection.

## Commit implementation safely

Follow the plan's suggested commit boundaries when they remain cohesive. Otherwise use a small number of Conventional Commits organized by
concern.

Before every commit:

1. Inspect `git status --short` and `git diff`.
2. Stage only explicit files belonging to the active plan.
3. Inspect `git diff --cached --check`, `--stat`, and `--name-status`.
4. Confirm no unrelated changes, restricted data, artifacts, models, archives, logs, or credentials are staged.
5. Commit without rewriting previous shared history.

In a parallel execution wave, staging and commit form a serialized critical section. If the index already contains files from another lane,
do not commit, unstage, or alter them; wait for that owner. Confirm every cached path belongs to the current plan before committing.

Do not push. Do not amend an earlier plan or another agent's commit.

## Verify the completed implementation

Run every verification command required by the plan. Also run the repository gates that apply to the affected scope:

- focused tests, followed by the broader suite required by the plan;
- the formatter, linter, static-analysis, and type-check gates defined for the affected scope by `AGENTS.md` and the architecture;
- integration/smoke commands required by the plan;
- `git diff --check` and a final staged-scope audit.

Use exact commands from the managed environment. Never call a timeout, skip, missing dependency, non-zero exit, or unrun command a pass.
Fix in-scope failures. Record unrelated or environmental failures precisely; do not weaken tests, typing, lint configuration, or acceptance
thresholds merely to obtain green output.

## Write the acceptance report

Create the report path required by the plan under `docs/plan/reports/`. The implementation agent reports evidence but does not grant itself
independent acceptance.

Include:

- plan path and execution date;
- baseline commit/branch and implementation commits;
- files/interfaces changed and why;
- each plan step and acceptance item with evidence;
- exact commands, exit status, and concise observed output;
- skipped, failed, timed-out, or unavailable checks;
- deviations from the plan and why they were necessary;
- remaining risks, external actions, and user decisions;
- current working-tree state and explicitly preserved unrelated changes.

Set the implementation report conclusion to:

- `IMPLEMENTED` when all implementation work and required verification are complete and committed, pending reviewer acceptance;
- `IN_PROGRESS` when required in-scope work remains;
- never `ACCEPTED` merely because the implementing agent wrote the report.

`ACCEPTED` requires the reviewer process defined by `AGENTS.md`. After committing the implementation and report, record the implementation evidence: call `mine_plan_mark_implemented` (MCP) with `id`, `report`, `commits`; or `mine plan implemented --id <id> --report <report path> --commit <hash> --format json` (CLI fallback) (repeat `--commit` for each implementation commit) - the accepted MINE CLI and MCP tool read the current revision under the lock and emit `revision_before`/`revision_after`. The reviewer later performs the accept/reject transition through MINE. Do not release downstream plans while the node is only `IMPLEMENTED`.

Commit the report and, only when owned, final graph status using explicit paths. Parallel lane agents never stage the graph. Verify the resulting commit and confirm the assigned branch contains every implementation/report commit. For a non-parallel plan, no hidden merge step may remain. For a parallel lane, report the exact branch/worktree, commits, and declared join artifact required by the integration owner; do not merge it yourself unless the plan assigns integration ownership.

## Finish with a self-contained handoff

Report:

- the plan executed and its final graph status;
- the implementation and report commit hashes;
- the important behavior/files changed;
- verification commands and outcomes;
- failures, skipped checks, remaining risks, and required reviewer/user actions;
- unrelated working-tree changes preserved;
- for non-parallel work, state that the work is already on the assigned branch with no hidden merge step; for a parallel lane, state the exact integration handoff and join artifact.

Lead with the outcome. Do not claim the plan is accepted until the repository's reviewer has actually accepted it.

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
