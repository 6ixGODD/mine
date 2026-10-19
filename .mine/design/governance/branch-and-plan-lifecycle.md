# Branch and Plan Lifecycle

## Layout applicability

Literal `docs/design/`, `docs/plan/` and backup paths below describe the
installed 0.1.13 bootstrap. The approved [0.2.0 namespace contract](repository-layout.md)
replaces them with `.mine/design/`, `.mine/plan/` and
`.mine/runtime/design-backups/` at its defined cutover, without changing
branch roles or retention. Never purge all of `.mine/`. The live old-binary
graph and frozen Plans remain at their current locations until closure.

## Branch roles

### Stable branch (`main` or `master`)

- detected and recorded by `mine init`;
- contains stable product state, configuration, root README files, and `docs/design/`;
- must not contain `docs/plan/` or tracked `docs/design-backup-*` paths;
- direct plan implementation is forbidden.

**Stale recorded branch (migration contract).** A `.mine/config.toml` written by an
older MINE version may record a stable branch (e.g. `master`) that no longer
exists in the repository (e.g. a `main`-only repository). Re-running `mine init`
detects this: when the recorded `branches.stable` provably does not exist in git
while a different branch is detected, `mine init` repairs the recorded value to
the detected branch and records an explicit `repaired-stable-branch` action. A
recorded branch that still exists is kept (it may be intentional). Downstream
commands never silently use a missing configured branch: `mine release` reports
it as a decisive, actionable error (run `mine init` to repair), and the doctor
and workspace commands consume the repaired value after initialization.

### `dev`

- temporary integration branch for the active body of work;
- created from the latest accepted stable baseline by an authorized MINE Skill;
- owns the active `docs/plan/` workspace;
- receives independently accepted plan branches;
- remains checked out in the main (Root) worktree throughout unreleased work;
- is deleted after stable release integration.

### `plan/<id>-<slug>`

- short-lived implementation/review branch based on accepted `dev`;
- is checked out only in its registered linked worktree, never by switching the Root;
- owns one plan unless an explicit parallel-lane design says otherwise;
- is merged into `dev` only after independent acceptance;
- is deleted after successful integration.

## Standing Git authorization

By invoking a MINE Skill, the repository owner grants the active agent authority to:

- inspect Git state and remote/default-branch metadata;
- create and switch the MINE-managed `dev` and `plan/*` branches;
- commit explicit files owned by the current operation;
- merge an accepted plan branch into `dev`;
- delete an accepted and merged local `plan/*` branch;
- perform final squash or curated integration into the stable branch after all release gates pass;
- create a release tag when configured;
- delete the local managed `dev` branch after release.

The authorization excludes:

- unrelated or unknown branches;
- force push;
- `reset --hard`;
- `git clean`;
- blind stash;
- rewriting public/shared history;
- discarding unrelated work;
- deleting remote branches unless the user explicitly requests it.

Dirty or ambiguous worktree state blocks branch mutation until safely classified.

The exact Root/linked-worktree ownership, graph authority, preparation,
acceptance merge, conflict recovery, and cleanup contracts are specified in
[Plan worktree and integration lifecycle](worktree-and-integration-lifecycle.md).

## Repository initialization

`mine init` does not create `dev`, `plan/*`, or `docs/plan/`. It establishes configuration, the design namespace, governance, templates, and integrations only.

The first model-driven action is invoked explicitly by the user:

- `mine-arch` for requirement-first design;
- `mine-sync` for code-first onboarding or reconciliation.

## Compensation and downstream rewiring

When a plan is rejected (`mine plan reject --compensating-plan <id>`), closing
the rejection has two independent, CLI-managed steps. **Neither edits the
execution graph by hand**; the bootstrap exception that allowed manual
rerouting (e.g. Plan `02` -> `02-1` during bootstrap) has ended.

1. **Register the compensating plan** with `mine plan add`, whose hard
   predecessor is the rejected plan's accepted upstream (not the rejected
   plan itself).
2. **Rewire downstream successors** off the rejected plan onto the
   compensating plan with
   `mine plan rewire-compensation --id <rejected-plan-id>`
   (see
   `docs/design/execution-graph/state-machine-and-algorithms.md#compensation-rewiring`).
   The replacement is derived from the rejected plan's `compensating_plan`
   field; the caller never supplies a replacement, so no similar-id
   substitution can occur.

A rejected plan is terminal; it is not revived or re-statused. Accepted and
active successors (`IN_PROGRESS`/`IMPLEMENTED`/`ACCEPTED`/`REJECTED`) are
never rewritten by compensation rewiring, preserving their immutability.
Downstream work that depended on the rejected plan may only resume after the
rewiring is accepted into `dev` and the compensating plan is itself accepted.

### Infeasible active Plan

An executor that proves an immutable `IN_PROGRESS` Plan infeasible stops and
preserves its branch/worktree; it never marks the node `IMPLEMENTED` merely to
reach rejection. After a Root-visible independent report and explicit owner
approval of any material replacement decision, `mine plan reject` may perform
the evidence-bound `IN_PROGRESS -> REJECTED` transition defined in the graph
state machine. It requires the next sibling compensation and uses the same
terminal rejection, preservation, registration, and rewiring contracts above.
There is no hand-edited graph, revived node, or separate cancellation lineage.

If recovery infrastructure itself must change before that transition is
available, the stopped Plan's write lane is explicitly frozen and one
serialized governance-recovery Plan may take ownership of the overlapping
central paths. No executor may resume the frozen Plan concurrently. After the
recovery Plan is accepted, the original Plan is rejected through the new
validated command and its compensation resumes in the preserved lineage
worktree.

## Plan workspace creation

`mine-plan-create` ensures `dev` exists in the Root worktree and opens an
internal plan workspace when absent. It never switches Root to a Plan branch.

The workspace has a generated `workspace_id`; the user does not supply a workspace version. The workspace marker records repository ID, stable baseline commit, integration branch, creation time, and MINE ownership.

## Registration and release

`mine plan add` **registers** a plan as `DRAFT`: it records identity, design
references, write paths, and dependencies, but makes no claim about whether
the plan may execute. `mine plan release --id <plan-id>` is the explicit,
deterministic gate that moves a `DRAFT` plan into the startable frontier
(`READY` when all hard predecessors are `ACCEPTED`, otherwise `BLOCKED`). The
distinction between registration and release is deliberate and must be
preserved: a freshly added plan never becomes silently executable.

Automatic successor release inside `mine plan accept` is unchanged: accepting
a plan may release `BLOCKED` successors whose hard predecessors are all now
accepted, in the same accept transaction. `mine plan release` covers the
standalone case (a newly registered `DRAFT` plan with no pending accepted
upstream), which the accept pass cannot reach.

An unstarted plan (`DRAFT`/`BLOCKED`/`READY`) that will never execute — for
example after its architecture was replaced by an approved decision — is
retired through `mine plan withdraw` (ADR-0012): terminal `WITHDRAWN`, reason
and timestamp recorded, provably-lossless cleanup of a prepared-but-unstarted
lane, no compensation semantics. Withdrawn nodes do not block workspace
closure; successors referencing them are explicitly withdrawn or re-planned.

## During development

- Design changes precede plans.
- Plan files are immutable after execution starts.
- Reports and graph state are temporary but committed on managed development branches for reviewability.
- `docs/design/` on `dev` describes approved target behavior.
- Every plan references exact design paths and anchors.
- Execution agents commit scoped implementation in the registered Plan
  worktree but never self-accept.
- Design changes precede plans.
- Plan files are immutable after execution starts.
- Reports and graph state are temporary but committed on managed development branches for reviewability.
- `docs/design/` on `dev` describes approved target behavior.
- Every plan references exact design paths and anchors.
- Execution agents commit scoped implementation but never self-accept.
- Review agents integrate positively reviewed work into Root `dev` before
  recording `ACCEPTED`; only then may they remove the clean linked worktree and
  merged local Plan branch.

## Lightweight maintenance boundary

**MINE governs engineering change, not every repository edit.** A coding agent
may perform direct, bounded, behavior-preserving repository maintenance
without creating a MINE execution Plan when the correct result is unambiguous
and no durable engineering contract changes.

### Maintenance that may bypass Plan creation

Examples: typo fixes; prose cleanup; translation; broken-link repair; README
improvements; user-facing documentation additions; formatting-only
documentation changes; examples that merely describe already-accepted
behavior; comments that do not change or establish behavioral contracts;
other clearly behavior-preserving editorial maintenance.

Such work still must:

- respect `AGENTS.md`;
- preserve unrelated changes;
- stage explicit files;
- run relevant validation;
- use normal commit discipline;
- never silently change product behavior while claiming to be docs-only.

It does **not** require `mine-arch` → `mine-plan-create` → `mine-plan-exec` →
`mine-plan-review` solely to preserve procedural formality.

### Changes that still require the MINE lifecycle

Use normal Design → Plan → Execute → Review when a change alters or
establishes durable engineering behavior, including: business/runtime
behavior; architecture; public API; CLI semantics; MCP contracts; Skill
workflow behavior; execution-graph/state-machine behavior; release behavior;
branch lifecycle; persistence/schema/migration behavior;
security/privacy/authority boundaries; deployment/operational contracts;
durable Design decisions.

A documentation file is **not automatically lightweight maintenance**. If
documentation changes the authoritative contract rather than merely
describing already-accepted behavior, it belongs in normal MINE governance.

## Reviewer authority to correct, not only to verdict

A reviewer's independence is about independently inspecting and validating a submitted implementation, never about a prohibition on correcting
a defect once found. A reviewer is responsible for bringing submitted work to an acceptable, mergeable state:

- Reviewers may directly apply localized corrections — a local code fix, a strengthened or added test, a corrected CI/CD workflow step, a
  corrected manifest or generated-copy synchronization, a documentation/Skill-wording correction, or a narrow release-closure blocker — when
  the correct behavior is already unambiguous from the accepted Design and the plan, the fix carries no new product/design decision, and it is
  fully verifiable in the same review session.
- Reviewer-authored corrections are committed as their own explicit, separately labeled commit(s) and are described in the review report
  (what changed, why, and its revalidation evidence); they are never concealed, folded silently into the implementer's commits, or accepted
  without rerunning every gate the change could affect.
- A localized reviewer correction does not require a second reviewer, and does not by itself require a new compensating plan solely to
  preserve reviewer/implementer role separation.
- `REJECTED` plus a compensating plan is reserved for material Design changes, replacement of the core approach, a substantial independent
  work package, major scope expansion, or a finding that cannot be safely and fully completed within the current review session.
- During final release closure, a narrow release blocker (for example an ineffective validation gate, a missing CI matrix leg, or a
  diagnostic command that drops data it should preserve) is normally fixed and documented directly in the same closure session rather than
  deferred to another plan.

## Release closure

Release closure has two distinct phases owned by different actors:

### Phase A - final design reconciliation (repository owner)

After the last plan is accepted and integrated into `dev`, the repository owner explicitly invokes a full-repository `mine-sync`:

```text
mine-sync prepare this repository for stable release
```

This reconciles accepted implementation into `docs/design/`, resolves or reports every incomplete area, and validates the complete repository. It is a separate, deliberate session; the review agent does not perform it automatically.

### Phase B - mechanical release closure (mine-plan-review)

After the final sync, the repository owner invokes `mine-plan-review` in release-closure mode. This is a distinct invocation from single-Plan review: the final Plan may already be `ACCEPTED`, the owner has run a separate `mine-sync` session, and the original reviewer session no longer exists. The executable entry path is:

```text
mine-plan-review complete release closure
```

The Skill distinguishes release-closure mode from Plan-review mode by this invocation. No Plan path is required; the Skill reads the graph to confirm every Plan is terminal. Closure proceeds only when the final `mine-sync` is provably fresh; a terminal graph plus a structurally valid Design is **not** sufficient proof. The mechanical closure steps are:

1. confirm every plan is accepted and integrated into `dev` and the final `mine-sync` has completed (Phase A) with reproducible freshness evidence: (a) the Phase A sync report under `.mine/runtime/sync/` records a post-integration commit and `SYNCHRONIZED` status with no blocking uncertainty; (b) the final accepted Plan's `design_references` leaves describe post-implementation behavior; (c) an independent Design-vs-code consistency spot-check on the final Plan's changed surface passes. If any evidence is absent, stale, or contradictory, the Skill reports the final sync as missing or stale and does not proceed;
2. run `mine release --format json` preflight plus the repository's own decisive gates discovered from `AGENTS.md`, the architecture, and the plan;
3. run the native stale-plan-reference gate `mine scan plan-refs --check --format json` (authoritative cross-platform scanner; no Bash/WSL dependency) and record every line-local fixture exemption;
4. determine the next repository version from accepted changes and current managed version (`mine repository version suggest --format json`);
5. safely purge the MINE-owned `docs/plan/` workspace;
6. verify no tracked or untracked release-bound plan workspace or design backup enters the stable tree;
7. integrate the final tree through squash or curated commits so temporary plan history is not imported;
8. tag when configured;
9. delete local managed `plan/*` and `dev` branches.

The reviewer never pushes, creates a remote release, publishes a package, force-updates history, or deletes a remote or unrelated/user branch. Remote publication remains explicitly outside MINE's authority unless the user separately requests it.

## Why squash or curated integration

A normal merge makes temporary plan commits reachable from stable history even after file deletion. Squash or curated integration keeps stable history focused on accepted product and design state.

## Hotfix exception

A hotfix may use `hotfix/<slug>` from the stable branch when explicitly requested, but it still updates design, uses temporary planning for non-trivial changes, passes independent review, and leaves no plan workspace on the stable tree.
