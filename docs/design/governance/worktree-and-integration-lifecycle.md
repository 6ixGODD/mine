# Plan Worktree and Integration Lifecycle

## Purpose and target invariant

Parallel Plans must never share or switch the main Git worktree. While an
internal Plan workspace is open, the repository's main (Root) worktree is the
single integration worktree and remains checked out on the configured
`branches.integration` branch. Each Plan node owns one branch; each canonical
Plan lineage owns one linked Git worktree reused by every compensation node.

The stable branch may be checked out in the main worktree only when no internal
Plan workspace is open, or during the explicit stable-release integration
phase. Its configured name may be `main`, `master`, or another detected name.

## Sources of truth and resource ownership

| Fact or resource | Owner/source of truth |
|---|---|
| Stable and integration branch names | `.mine/config.toml` |
| Default linked-worktree parent | `[plan].worktrees_root` in `.mine/config.toml` |
| Shared toolchain/dependency root and Plan-isolated mutable-output parent | main Root worktree, selected by `[plan].shared_environment_root` |
| Plan branch and canonical lineage | registered `PlanNode` and typed Plan ID |
| Repository-relative worktree path | derived from configured root + lineage ID |
| Main/linked worktree registration and checked-out branches | `git worktree list --porcelain -z` |
| Plan lifecycle status and scheduling | execution graph in the main `dev` worktree |
| Code/index/HEAD for one Plan | that Plan's linked worktree |

The default `worktrees_root` is `.mine/worktrees`. Canonical IDs and locators are:

```text
base id:          planNN(?:-NN)*
compensation id: <base>-CNN
branch:           plan/<id>-<slug>
worktree_path:    .mine/worktrees/<base-id>
```

Each `NN` is `01..99`; `00` is invalid. `C` occurs at most once, last, and uses
contiguous sibling ordinals `C01`, `C02`, ... within one lineage. `<slug>` is
lowercase kebab-case. The exact Plan file is `docs/plan/<id>-<slug>.md`.
Registration rejects unsafe, mismatched, duplicate, absolute, or
repository-escaping identity/locators. The configured worktree root is
repository-relative and must remain inside the repository after
symlink/junction-aware resolution. Absolute machine-local paths are never
persisted in configuration or the graph.

The default `shared_environment_root` is `.` (the main repository Root). It is
repository-relative and containment-validated. Linked worktrees contain source,
their Git administrative state, and Plan-owned reports only; they do not own a
second language environment, dependency installation, or mutable build/test
output tree. Root ownership does not mean that parallel Plans share one mutable
output directory.

## Main worktree and graph authority

Git's main worktree is the first record returned by `git worktree list
--porcelain -z`; MINE does not infer it from the caller's current directory or
`.git` spelling. During unreleased work it must be the configured integration
branch and clean before any branch/worktree creation or integration mutation.

All graph reads and writes, including calls launched from a linked worktree,
resolve the main worktree first and use the graph, lock, configuration, and
revision there. A linked Plan worktree never becomes an independent graph
authority. This preserves one revision sequence and one filesystem lock across
parallel agents.

## Plan preparation

`mine plan prepare --id <id>` is the deterministic linked-worktree operation
used by execution and review Skills. It:

1. parses the canonical ID, derives its base lineage, and loads the Plan from
   the authoritative main-worktree graph;
2. requires the Plan to be startable or already active/reviewable;
3. validates that the main worktree is clean and on the integration branch;
4. validates unique branch/path ownership from porcelain `-z` evidence;
5. for a base node, creates its branch from current accepted `dev`; for a
   declared compensation, preserves rejected refs, creates the compensation
   branch from the rejected predecessor's HEAD, and switches only the clean
   shared lineage worktree;
6. creates the linked worktree without `--force`;
7. returns Plan ID/path, branch, absolute worktree path, observed state, and
   authoritative graph revision.

The required initial anchor is current accepted `dev` for a base Plan and the
rejected predecessor HEAD for a compensation. Before a registered branch is
first attached or a compensation is first switched into the lineage worktree,
its HEAD must equal that anchor exactly. A `READY` Plan remains pinned to the
same dynamically resolved initial anchor.

`mine plan start` revalidates that exact prepared state and persists the commit
as `branch_anchor` in the same transaction as `READY -> IN_PROGRESS`. Once the
registered worktree is already on the registered branch, normal execution
commits are expected to advance it: an `IN_PROGRESS` or `IMPLEMENTED` Plan is
an idempotent reuse only when observed worktree HEAD equals registered branch
tip and the **stored** `branch_anchor` is an ancestor of that tip. Current Root
HEAD is never substituted after start. A missing anchor, unrelated history, or
history rewritten away from the anchor is a typed refusal; legitimate Plan
commits must never be mistaken for a stale pre-preparation branch.

A compensation must equal the rejected node's declared `compensating_plan`,
use the next contiguous ordinal, and inherit its hard predecessors. A branch checked
out elsewhere, a path owned by another worktree, dirty or wrong-branch Root,
locked/prunable/broken registration, or occupied unowned directory is a typed
refusal. MINE never force-adds/removes, deletes an unverified directory, or
uses `git worktree prune` as normal cleanup.

## Plan lookup and Skill invocation

Single-Plan execution and review resolve the user's invocation through the
authoritative graph before doing work. The user may supply the exact stored ID,
the exact registered Plan path, or the equivalent numeric/`P` shorthand: for
example, `03`, `P03`, and `plan03` identify the same canonical Plan, while
`02-C02`, `P02-C02`, and `plan02-C02` identify the same compensation. The one
legacy bootstrap node similarly accepts `01`, `P01`, and `plan01` but resolves
to its exact stored ID `01`.

This is Skill-level invocation normalization, not a second persisted identity
grammar. A Skill reads `mine graph show --format json`, requires exactly one
matching ID/path, and then calls `mine plan show --id <exact-stored-id>
--format json` and all lifecycle commands with that exact ID. It never guesses
a Markdown filename, uses prefix/substring matching, fills in a missing
compensation ordinal, or selects a latest compensation. The binary's `--id`
contract and graph storage remain canonical and case-sensitive. The Plan DTO
includes authoritative `path`, registered branch/path, resolved absolute
worktree path, observed worktree state and HEAD/branch, caller location, and
authoritative graph revision. Release closure retains the explicit phrase
`complete release closure`.

## Execution contract

`mine-plan-exec <plan-id>` prepares or reuses the registered linked worktree,
changes its own process directory there, verifies the Plan branch, and performs
`mine plan start` before any implementation mutation. Start revalidates the
prepared Git evidence and records `branch_anchor`; failure leaves the Plan
unmodified and blocks execution. All implementation writes and commits occur
in the linked worktree. The executor never switches the Root. Graph transitions
still target Root `dev`. Implementation may conclude `IMPLEMENTED`, but never
merges, accepts, or rejects itself.

## Independent review and accepted integration

`mine-plan-review <plan-id>` reviews the submitted branch in its lineage
worktree. Reviewer corrections are committed there. A positive verdict is not
yet `ACCEPTED`; integration is part of acceptance:

1. commit the review report and reviewer fixes on the Plan branch;
2. require the Plan worktree and Root `dev` to be clean;
3. re-read both HEADs and require authoritative graph status `IMPLEMENTED`;
4. merge the Plan branch into Root `dev` with normal Git merge semantics;
5. resolve conflicts in Root according to Design and Plan, stage explicit
   resolutions, complete the merge, and rerun affected narrow and broad gates;
6. after complete merge and green integration, remove the clean linked
   worktree with normal `git worktree remove`, then delete the merged local
   Plan branch;
7. only after cleanup succeeds, run `mine plan accept` against Root and commit
   graph state on `dev`.

Fast-forward is permitted when valid; divergent histories produce a merge
commit. Evidence records which occurred. MINE does not rewrite public history
to force one topology.

If conflicts cannot be safely resolved, the reviewer aborts the merge when Git
can restore the clean pre-merge state, leaves the Plan `IMPLEMENTED`, preserves
its worktree/branch, and reports the blocker. If abort or cleanup cannot be
proven safe, it stops with recovery evidence and performs no acceptance or
forced deletion. If merge succeeds but worktree or branch cleanup fails, the
integrated code remains on `dev` but the Plan stays `IMPLEMENTED` with recovery
evidence until cleanup completes. `ACCEPTED` therefore means independently
approved, integrated into `dev`, and locally cleaned up.

## Shared environment and isolated mutable-output contract

All Plan worktrees reuse Root-owned toolchains, dependency downloads, virtual
environments, and only those caches that are demonstrably read-only or safely
content-addressed for concurrent consumers. Skills resolve Root first and run
repository commands with that environment explicitly selected. Examples
include invoking the interpreter from Root `.venv`, reusing Cargo's downloaded
crate source/cache, or using a Root package-manager dependency store rather
than installing beneath a linked worktree.

Every mutable output that can affect build or verification truth is isolated
by exact Plan node, not merely by lineage or worktree. The default namespace is:

```text
<shared_environment_root>/.mine/build/<plan-id>/<tool-or-output-class>/
```

This includes compiler output, test binaries, incremental state, generated-code
intermediates, coverage databases, packaging/staging directories, test-runner
caches, and comparable language-specific products. For Rust, for example,
`CARGO_TARGET_DIR` for `plan01-C01` resolves to a directory such as
`<Root>/.mine/build/plan01-C01/cargo-target`; it never points all concurrent
Plans at Root `target`. A base Plan and its C01/C02 compensations reuse one Git
worktree but use different mutable-output namespaces so rejected binaries and
test state cannot contaminate compensation evidence.

An executor must not create `.venv`, `venv`, `target`, `node_modules`, or an
equivalent environment/dependency/cache tree inside a Plan worktree. If the
required Root environment does not exist or a dependency mutation is required,
execution stops and requests the serialized Root/integration owner; parallel
Plan agents do not race to initialize shared environment state.

Per-Plan isolation is the default whenever disk capacity permits. If measured
output size makes it impractical—for example, independently materializing a
full operating-system kernel build for every Plan—the executor must stop before
the build and present the estimated space, concurrency risk, and cleanup cost
to the repository owner. Execution resumes only after an explicit agreement on
a safer alternative such as serialization, a coarser exclusive namespace, or
bounded cleanup. An agent never silently falls back to a globally shared
mutable directory.

Cargo officially supports selecting the generated-artifact directory through
`build.target-dir` / `CARGO_TARGET_DIR`; this is the Rust adapter for the
language-neutral ownership rule, not the rule itself:
https://doc.rust-lang.org/cargo/reference/config.html#buildtarget-dir

## Rejection and cleanup

Rejection is recorded in the Root graph and review report. Rejected code is not
merged. Its branch ref and shared lineage worktree remain. A registered
compensation executes in that exact directory on its own branch; no second
worktree is created. If compensation is rejected, the next sibling ordinal
reuses it again. Final accepted integration removes the lineage worktree;
earlier rejected refs remain until release closure. Dirty, locked, missing, or
unmerged worktrees are never force-removed automatically.

When an immutable Plan is proven infeasible during `IN_PROGRESS`, the executor
stops without claiming implementation. After independent evidence and explicit
owner approval of any material replacement decision, `mine plan reject`
records the canonical review report, reason, and next sibling compensation in
one Root graph transition. The same preservation and compensation-worktree
rules apply; there is no separate cancelled lineage and no manual graph repair.

## Failure and concurrency semantics

- Linked worktrees isolate HEAD, index, and working files; Root graph revision
  checks and its filesystem lock serialize lifecycle mutations.
- `git worktree lock` protects a temporarily unavailable worktree from pruning;
  it is not an execution mutex.
- Branch/path evidence is revalidated immediately before mutation.
- Git mutation and graph transition are ordered recoverable steps, not a
  fictitious atomic transaction; graph `ACCEPTED` is always last.
- Commands invoked from any checkout or subdirectory resolve the same Root
  authority before acting.

## Required verification

Temporary real-Git tests on Windows, Linux, and macOS cover two parallel
lineages; base -> C01 -> C02 reuse of one directory with distinct preserved
branches; Root staying on `dev`; shared Root environments plus per-Plan mutable
output isolation with no worktree-local environment directories or cross-Plan
test contamination; porcelain `-z` path handling; exact-match
idempotence; collisions; dirty/wrong Root refusal; linked invocation using the
Root graph; Plan-show location fields; fast-forward/divergent merge; real
conflict resolution and abort; rejected preservation; and cleanup refusal for
dirty, locked, missing, or unmerged worktrees.

## Official sources

- [Git worktree documentation](https://git-scm.com/docs/git-worktree), accessed
  2026-08-21: linked worktrees isolate HEAD/index; porcelain `-z` is the stable
  script interface; Git rejects branch reuse; normal remove requires clean state.
- [Git rev-parse documentation](https://git-scm.com/docs/git-rev-parse), accessed
  2026-08-21: current top-level and common Git directory distinguish linked
  checkout context from shared repository storage.
- [Git merge documentation](https://git-scm.com/docs/git-merge), accessed
  2026-08-21: integration starts clean; conflicts complete with `--continue` or
  where safe restore with `--abort` before graph acceptance.
