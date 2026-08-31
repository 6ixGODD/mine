---
name: mine-plan-review
description: Strictly review one implemented repository plan against AGENTS.md, architecture, the immutable plan, implementation commits, real runtime behavior, verification evidence, and downstream contracts. Use when the user invokes the host-specific mine-plan-review skill with a plan path, asks whether a plan can be accepted, requests an acceptance review, or wants review failures classified into direct small fixes versus a new compensating plan. Work in the current workspace; never trust an implementation report or green tests without independent evidence.
version: 0.1.13
---

# MINE Plan Review

MINE Is Not Everyone's. Act as an independent acceptance reviewer, not as the implementing agent's advocate. Try to falsify the implementation's claims. Accept only
when the plan's actual contract is demonstrated end to end.

## Integration: MCP tools and CLI fallback

`mine-plan-review` queries and transitions plan state through two paths, in
this order of preference:

1. **MCP tools (preferred)** - when the current Agent runtime exposes the
   MINE MCP server (`mine mcp serve`), call the typed MCP tools. They return
   the same DTOs as the JSON CLI and never touch the execution-graph files.
2. **JSON CLI (deterministic fallback)** - when MCP is unavailable, call
   `mine --format json` commands. Never parse human output.

Never invent an MCP tool, CLI command, flag, JSON field, or lifecycle
transition that the current binary does not expose. Never edit
`docs/plan/execution-graph.toml` or `docs/plan/execution-graph.md` directly.

The accepted MCP tools `mine-plan-review` may use:

- `mine_graph_status` (no arguments) - read the current revision.
- `mine_plan_show` (`id`) - read the target node, its predecessors, and status.
- `mine_plan_accept` (`id`, `review`) - transition `IMPLEMENTED` -> `ACCEPTED`
  and record the review report path.
- `mine_plan_reject` (`id`, `reason`, `review_report`,
  `compensating_plan`) - evidence-bound transition from `IMPLEMENTED` or an
  objectively infeasible `IN_PROGRESS` Plan to `REJECTED`.
- `mine_plan_withdraw` (`id`, `reason`) - owner-authorized retirement of an
  unstarted (`DRAFT`/`BLOCKED`/`READY`) registration into terminal
  `WITHDRAWN` (ADR-0012). Withdrawn nodes are retired: they never block
  workspace closure and are never revived.
- `mine_graph_validate` (no arguments) - validate the graph after transitions.
- `mine_design_validate` (no arguments) - confirm design references.

Operations `mine-plan-review` needs that are intentionally **CLI-only** (no
MCP tool exposes them, because they rewire downstream dependencies outside
the single-node accept/reject path):

- `mine graph show --format json` - obtain the complete authoritative node
  inventory for user-facing invocation resolution.
- `mine plan prepare --id <exact-stored-id> --format json` - safely enter the
  registered review worktree.
- `mine plan rewire-compensation --id <rejected-plan-id> --format json` -
  reroute downstream dependencies from a rejected plan onto its registered
  compensating plan. There is **no MCP tool for rewiring**; after
  `mine_plan_reject`, rewiring is a mandatory CLI fallback when downstream
  nodes must be rerouted.
- `mine plan release --id <id> --format json` - release a compensating plan
  after it is registered (CLI only).

When a required operation has no MCP tool, fall back to the JSON CLI and state
the fallback explicitly.

## Resolve the review target

### Plan-review mode (default)

Accept exactly one `<plan-id-or-registered-path>`. Strip only an
invocation-level `@`, call `mine graph show --format json` from Root authority,
and match by exact equality against each returned exact stored ID and exact registered path.
For canonical IDs, accept the exact ID, its removed-`plan` form, or the form
with one uppercase `P`: `planNN` / `NN` / `PNN` and `planNN-CNN` / `NN-CNN` /
`PNN-CNN`. For the legacy bootstrap stored ID represented by `NN`, accept
`NN`, `PNN`, and `planNN`, all resolving to that exact stored ID.

Require exactly one distinct graph-node match and use only that exact stored
ID for `mine plan show`, `mine plan prepare`, and all lifecycle commands. Fail
clearly on no match or ambiguity. Matching is case-sensitive; never use
prefix/substring matching, guess a filename, fill a missing compensation
ordinal, select the latest compensation, or pass shorthand to the binary.
`NN-C` never means `planNN-CNN`. If no target is supplied, ask before mutation.

### Release-closure mode

When the repository owner has completed the final `mine-sync` (Phase A) and every Plan is terminal, invoke the Skill in release-closure mode:

```text
mine-plan-review complete release closure
```

**Entry discrimination (executable):** if the invocation argument is exactly `complete release closure`, enter release-closure mode; never resolve that literal as a plan file path. Any other single argument is treated as a Plan path (Plan-review mode). If no argument is supplied in Plan-review mode, ask for a path before mutation.

This mode does **not** require a Plan path and does **not** re-review or re-accept any Plan. It detects the all-terminal graph, verifies the final sync completed, and carries the mechanical closure steps below. It may also be used when the final Plan was already accepted in an earlier session.

`WITHDRAWN` nodes are retired and do not block closure: a graph whose only non-`ACCEPTED` nodes are `WITHDRAWN` is closable, while `REJECTED` nodes still require their accepted compensation chain. A stale unstarted registration encountered during closure is retired explicitly through `mine plan withdraw` (or `mine_plan_withdraw`) with the repository owner's authorization; never hand-edit the graph to drop it.

## Read the governing evidence

Read completely, in order:

1. Root `AGENTS.md`.
2. The design knowledge base rooted at `docs/design/index.md` (and the relevant leaves named by `AGENTS.md`).
3. Query the target node and graph revision through `mine_plan_show` (MCP) or `mine plan show --id <id> --format json` (CLI fallback), and `mine_graph_status` (MCP) or `mine graph status --format json` (CLI fallback); use the generated Markdown only as a readable view.

Every reviewer-initiated transition of graph state must go through the accepted MINE CLI (`mine plan accept` / `mine plan reject` with `--format json`); never edit `docs/plan/execution-graph.toml` or `docs/plan/execution-graph.md` directly (`AGENTS.md` documents this rule; the bootstrap exception has ended).
4. The target plan.
5. Every hard-predecessor acceptance report and referenced commit.
6. The target implementation report, implementation commits, diff from its accepted baseline, and suggested downstream consumer plan.
7. Fetch and read the official sources and best-practice references registered by the plan; do not rely on the plan's paraphrase alone.
8. Relevant production code, schemas, tests, CLI/configuration and generated-artifact contracts.

Govern review in this order: `AGENTS.md` → architecture → registered authoritative sources → immutable plan. Treat implementation comments,
test names and the implementation report as claims to verify, not authority.

## Establish a clean review subject

Resolve the node with `mine plan show --id <exact-stored-id> --format json`,
call `mine plan prepare --id <exact-stored-id> --format json`, enter the
returned absolute path, and verify its observed branch and HEAD. For an active
or implemented node, the persisted `branch_anchor` must be an ancestor of the
exact registered branch tip. Use the returned authoritative Root for graph
operations and never switch Root. A scheduler-supplied worktree must match the
same prepare evidence; refusal blocks review without a direct Git fallback.

Review the committed implementation named by its report. Inspect current branch, worktree list, `git status`, staged state and concurrent
changes before running commands.

- Work in the prepared workspace; do not directly create a worktree or branch.
- Never stash, reset, clean, checkout, restore or discard user/agent changes.
- Separate target-plan changes from unrelated concurrent changes using commit diffs and blobs.
- Do not attribute an unrelated dirty-worktree failure to the reviewed plan.
- If concurrent changes make a command unreliable, run the closest valid check against the managed environment or committed artifact and
  document the deviation. Do not call it a pass if equivalence is unproven.

## Build a traceability matrix

For every goal, implementation step, deliverable and acceptance checkbox, record:

- governing architecture section;
- implementation file/interface and commit;
- independent test or runtime probe;
- observed result;
- status: `PASS`, `FAIL`, `UNVERIFIED`, or `NOT_APPLICABLE` with reason.

One failed hard contract is sufficient to reject. Do not average severe defects against many passing cosmetic checks.

## Review adversarially

Use this evidence priority:

1. Real boundary behavior and independently inspected artifacts/data.
2. Production code and serialized/runtime round-trip behavior.
3. Independent tests that would fail for a plausible wrong implementation.
4. Existing tests.
5. Implementation report prose.

At minimum:

- Re-run the plan's exact commands when the environment permits.
- Inspect error/fallback paths, not only happy paths.
- Check empty, malformed, unknown, boundary-size, timeout, corruption and partial-failure cases applicable to the plan.
- Verify persisted data through write → load → consumer round-trips, including identity, ordering, hashes, dtype and shape.
- Compare real source counts/content when the plan makes data-specific claims.
- Check privacy/information boundaries by changing hidden inputs and observing all outputs.
- Check both symmetric roles/seats for game or multi-party logic.
- Run the focused tests, integration/smoke checks, formatter, linter, static analysis, type checks, build checks, and other repository gates required by `AGENTS.md`, the architecture, and the plan.
- Inspect whether downstream code can consume the produced interface without decoding guessed semantics or adding a compatibility shim.
- Confirm reports state exact commands/results and do not mark skips, timeouts, missing tools or warnings as passes.

### Remove temporary-plan references before release closure

Before the final stable-candidate integration, run the native stale-plan-reference scan against the repository under review:

```text
mine scan plan-refs --check --format json
```

`mine scan plan-refs` is the **authoritative cross-platform scanner**: a native Rust implementation inside the `mine` executable with **no Bash, WSL, or Git Bash dependency**. It works on Windows without WSL, Windows without `bash` on PATH, Linux, and macOS. The release/review path never requires an external POSIX shell for this scan.

Semantics (per `docs/design/interfaces/cli-contract.md` - "Stale-plan-reference scan"):

- inspects **tracked** repository content (`git ls-files`), never an uncontrolled filesystem walk;
- detects temporary historical Plan references such as `Plan NN`;
- excludes temporary planning state and accepted documentation (`docs/plan/`, `docs/design/`, design backups, READMEs, fixture/testdata directories);
- honors the `mine-release-allow-plan-reference:` exemption marker on the immediately preceding line;
- reports exact `file:line` findings; never rewrites source;
- `--check` exits non-zero when unexempted findings exist (release gate); without `--check` it prints findings and exits zero (repair mode);
- supports `--repo` and `--format json`.

It is layout-agnostic: it works for Go, Python, TypeScript, monorepo, and any other tracked layout.

**Legacy Bash helper:** the repository previously shipped a Bash helper at `skills/mine-plan-review/references/scan-plan-refs.sh` (also copied into installer-managed Skill directories). It is retained **only** as a manual compatibility helper for Unix-only use and is not the authoritative implementation; the normal MINE release/review path uses `mine scan plan-refs`. If you encounter the helper in an installed Skill directory, do not treat its absence as a defect - the native CLI is authoritative. The target repository never needs a local `references/` directory for this check.


This scans tracked implementation, test, workflow, Skill, and distribution assets while excluding temporary `docs/plan/` and design documentation. It rejects stale `Plan NN` references because stable behavior must be intelligible without the ephemeral planning history. Rewrite a historical comment as an enduring contract; for example:

```rust
// Bad: A stale plan-number comment attributing behavior to a historical plan.
// Good: Checksums prove artifact integrity only; binary reproducibility is not claimed.
```

An intentional fixture literal may be exempted only by an immediately preceding line with a concise reason:

```rust
// mine-release-allow-plan-reference: protocol fixture
let input = "Plan 08-2";
```

Never use exemptions for implementation comments, workflow behavior, public diagnostics, or prose that can be expressed as a durable contract. Review every exemption, record its path/line/reason in the closure report, and rerun the scan after every correction. The scanner is a release-closure gate, not a substitute for semantic review.

Do not accept because “tests pass.” Determine whether the tests assert the intended semantics rather than restating the implementation.
Write a small independent probe when a key acceptance claim lacks discriminating evidence.

## Classify findings

Classify by impact, not line count. **A reviewer is responsible for bringing submitted work to an acceptable, mergeable state, not merely for issuing an immediate binary verdict.** Independence means independently inspecting and validating the implementation end to end — it does not mean refusing to correct a defect once it is found. Do not spawn a compensating plan, and do not consume another full implementation/review cycle, merely to preserve reviewer/implementer role purity for a narrow, well-understood correction.

### Fix directly during review (the normal path for narrow findings)

Apply a direct reviewer fix whenever all are true:

- architecture and plan already specify the correct behavior unambiguously (or the fix is a release/workflow/manifest/documentation/test correction that does not change any product or design decision);
- the defect is local and has no effect on persisted schema, a public API/tool contract, a security/privacy boundary, cross-component ownership/lifecycle, or a downstream design decision;
- the fix and its regression coverage (a new/strengthened test, a corrected workflow step, a corrected manifest/doc line) are cohesive and fully verifiable in the current review session;
- fixing it does not hide a false implementation claim, rewrite an immutable plan, or silently weaken a gate the plan was supposed to satisfy.

This explicitly includes, without needing a second reviewer or a compensating plan: typos, wrong imports, missing local guards, incorrect error messages, small off-by-ones against an existing clear contract, missing focused tests for behavior already correctly implemented elsewhere, incorrect or ineffective CI/CD workflow steps (for example a masked failure condition, a missing platform in a required matrix, an incorrect path/flag), stale or contradictory documentation/Skill/template wording, a narrow release-blocking defect discovered during release-closure validation (for example a diagnostic command that discards data it should preserve, an ineffective detection gate, a version-resolution edge case), and coherent updates to generated distribution copies performed only through the accepted synchronization mechanism (never by hand-editing a generated copy directly).

Patch it, add or strengthen the regression test, run every affected gate (not just the one you touched), commit the correction **separately** from the plan's own commits (a clearly labeled `fix:`/`docs:`/`chore:` commit authored by the reviewer), then re-run the complete acceptance matrix against the corrected HEAD. Record the exact reviewer-authored change, its rationale, and its revalidation evidence in the review report. Never conceal a reviewer-authored change, fold it silently into the implementer's own commits, or accept without rerunning every gate the change could affect.

### Require a compensating plan (reserved for substantial issues)

Reject and create a compensating plan only when a finding is genuinely substantial — not merely inconvenient to fix inline. Reserve this path for findings that:

- require a material Design change or replace the plan's core approach/methodology;
- change a persistent data/catalog/checkpoint schema, provenance, or migration/rebuild behavior;
- change a public API/tool input-output contract or a security/privacy/secret/hidden-information boundary;
- introduce or require a substantial independent work package, a major scope expansion, or coordinated changes across multiple coupled modules and a downstream dependency;
- reveal missing real integration evidence central to the plan's own claim, such that no bounded, same-session correction could responsibly close the gap;
- cannot be safely and fully verified within the current review session (for example, the fix itself would need its own design decision, its own dependency-aware work-package sequencing, or realistically exceeds what one reviewer can independently validate in one pass).

When genuinely uncertain whether a finding is narrow or substantial, weigh it against the criteria above rather than defaulting to rejection: a change that is small in line count can still be architecturally substantial (create a compensating plan), and a change that touches many files can still be narrow and mechanical (fix it directly) — for example, a corrected CI workflow, a batch of strengthened Skill-contract tests, or a synchronized set of generated distribution copies produced by the one accepted sync mechanism are narrow even when they touch several files, because they carry no new design decision and are each independently, fully verifiable in the same session.

## Accept a correct implementation

Accept only when every hard acceptance item is `PASS` (after any direct reviewer fixes are applied and revalidated), required verification is reproducible, artifacts/reports are accurate, no unresolved finding can invalidate downstream work, and all implementation/report/reviewer-fix commits exist on the current branch.

Then:

1. Create a separate review report under `docs/plan/reports/<plan-name>-review.md` rather than overwriting implementation evidence. Include any reviewer-authored fixes: what changed, why, and its revalidation evidence.
2. Record inspected commits, traceability matrix, independent commands/results, reviewer fixes and remaining non-blocking risks. Commit reviewer fixes separately, then commit the review report on the Plan branch; stage explicit paths only.
3. Require a clean Root `dev` and clean prepared Plan worktree, validate the registered branch/anchor/ancestry again, merge the Plan branch into Root `dev` with `--no-ff`, and rerun every required integration gate on the merged tree. A failed merge or gate leaves the node `IMPLEMENTED`, not accepted.
4. After the merge and integration gates succeed, remove only the exact clean registered linked worktree and delete only the now-merged local Plan branch. Never force removal or delete a rejected, dirty, checked-out, or unexpectedly unmerged branch.
5. Call `mine plan accept --id <exact-stored-id> --review <review report path> --format json` (MCP: `mine_plan_accept` with exact stored `id` and `review`). The accepted binary transitions `IMPLEMENTED`→`ACCEPTED`, records the review report, and releases eligible downstream nodes. Commit only the Root-owned graph result with explicit paths and verify the reported revisions.

## Review an infeasible active Plan

Enter this mode only when the owner explicitly asks for review of a stopped
`IN_PROGRESS` Plan whose executor reported objective immutable-Plan
infeasibility. Independently reproduce the failure against the preserved
branch/worktree and prove that the persisted anchor and Git topology are not
being confused with ordinary implementation drift.

If a material replacement decision is required, its ADR must already be
approved. Write and commit the canonical Root-visible
`docs/plan/reports/<plan-name>-review.md` as a regular file contained by Root;
record the immutable requirement, independent reproduction, reason, authority,
and next contiguous sibling compensation ID. Then call `mine plan reject` with
the exact stored ID, non-empty reason, exact `--review-report` path, and exact
next compensation ID. This transition records no implementation report or
implementation commits. Do not merge the rejected branch, remove its lineage
worktree, or start/register the compensation implicitly.

## Bring release closure to completion

Release closure is two phases. Phase A: the repository owner runs the final `mine-sync prepare this repository for stable release` (the reviewer does **not** invoke `mine-sync`). Phase B: the mechanical closure below, carried out by the reviewer - either continuing from a final acceptance, or in a fresh session via the explicit `mine-plan-review complete release closure` invocation.

### Entering release-closure mode

Release-closure mode is distinct from Plan-review mode:

- **No Plan path is required.** Do not require a Plan that is still `IMPLEMENTED`; the final Plan may already be `ACCEPTED` in an earlier session.
- **Enter only when** `mine graph status --format json` reports every Plan terminal (`ACCEPTED` or `REJECTED` with accepted compensation).
- **Never re-accept.** An already-`ACCEPTED` Plan must not be transitioned again; closure does not call `mine plan accept` unless a genuine new `IMPLEMENTED` plan is being reviewed.

**Freshness verification (Phase A evidence, mandatory and reproducible).** A terminal graph plus a structurally valid Design is NOT sufficient proof that the final `mine-sync` ran after the last Plan was accepted and integrated, or that Design matches the current `dev` implementation. Before any closure step, verify Phase A completion with explicit, independently checkable evidence — never rely on `mine design validate` alone as a semantic sync proof:

1. **Sync report exists and covers the final state.** Locate the most recent mine-sync report under the target repository's `.mine/runtime/sync/` (written by Phase A per `docs/design/governance/design-sync.md`). It must record the repository, the branch it reconciled, the commit inspected, and a status of `SYNCHRONIZED` (or `SYNCHRONIZED_WITH_WARNINGS` with no blocking uncertainty). If no report exists, or its recorded commit predates the final accepted plan's integration commit, the final sync is **missing or stale**: report the gap explicitly and do **not** proceed.
2. **Design reconciles the final accepted plan's contracts.** Take the last-accepted Plan (or the full set of plans accepted since the recorded sync commit). For each, verify that the Design leaves referenced by its `design_references` describe the post-implementation behavior — not the pre-implementation description. Concretely: read the plan's implementation diff (its recorded `implementation_commits`), read each referenced Design leaf, and confirm the leaf reflects the implemented contracts, schemas, and behavior. If any referenced leaf still describes the pre-change behavior, the Design is stale: report the gap and do **not** proceed.
3. **Independent consistency spot-check.** For at least the final Plan's changed surface (files, CLI contracts, schemas, behavior touched by its implementation commits), independently re-derive from the current `dev` code that the Design describes what the code does. A leaf that validates structurally but contradicts the code (e.g., still documents the old flag, old default, or old schema) is stale regardless of `mine design validate`.

Record the freshness evidence (sync report path + recorded commit, final Plan id, referenced leaves checked, spot-check observations) in the closure report. If the graph reports non-terminal state, the sync report is absent or pre-integration, or any freshness check fails, the final `mine-sync` is missing or stale: report the gap explicitly and do **not** proceed to closure.

### Discover the decisive validation suite

Run whatever decisive validation the **repository under review** requires. Discover it in this authority order:

1. explicit current user instructions;
2. repository governance such as `AGENTS.md`;
3. accepted Design and release contracts;
4. the active Plan and its acceptance criteria;
5. detected project build, test, lint, packaging, and integration systems.

Never invent Cargo, Python, Node, Go, or any other toolchain command. Commands valid for developing MINE itself belong in MINE-local governance, Design, or CI - never in this portable Skill contract. Complete and repeated validation may be required when the target repository's own contract justifies it.

### Mechanical closure steps

1. Confirm the repository owner has run the final `mine-sync prepare this repository for stable release` against the complete `dev` tree (Phase A), per the freshness verification in "Entering release-closure mode" above (sync report with recorded post-integration commit + referenced Design leaves reconciled + independent consistency spot-check). The reviewer does not run `mine-sync`.
2. Merge the accepted plan branches into `dev` with `--no-ff` (only branches not yet integrated) and re-run the decisive validation suite discovered above directly on `dev`, plus `mine design validate --format json` and `mine graph validate --format json`.
3. Call `mine release --format json` (CLI-only; no MCP tool exposes release preflight) as a diagnostic before candidate construction. Its development-tree gates are decisive immediately: terminal plan state, accepted compensation for every rejected plan, valid graph/render, valid design, no dirty tree, no pending Agent transaction, and the authoritative resolved version from `.mine/config.toml`. Before stable integration, `can_release:false` is expected solely when the existing stable branch still contains its old `docs/plan/` workspace; do not misreport that pre-integration stable-tree fact as a failed `dev` validation. After curated stable integration, run the preflight again from `dev` and require every gate, including no `docs/plan/` or `docs/design-backup-*` on the stable branch, to pass. `mine release` is validation-only; it must never itself claim that `master`, tags, publication, or cleanup occurred.
4. If `mine release` (or any other decisive check) fails on a narrow, release-scoped defect - not a new design decision - fix it directly in this same session exactly as in "Fix directly during review" above (own commit, own regression coverage, full revalidation), rather than opening a compensating plan solely to perform the closure.
5. Run the bundled scanner from this Skill's installed directory (see "Remove temporary-plan references before release closure"), with the target repository as the working directory. Correct every unexempted temporary-plan reference and record every line-local fixture exemption before candidate construction. Never require the target repository to contain `references/scan-plan-refs.sh`.
6. In an isolated clone or Git worktree (never the reviewer's own live checkout), construct the exact stable candidate tree per `docs/design/governance/branch-and-plan-lifecycle.md`: the accepted `dev` tree with the ephemeral `docs/plan/` workspace removed and no tracked `docs/design-backup-*` path. Run the repository's decisive validation plus `mine design validate` against the candidate; do **not** run `mine graph validate` there because the graph-less stable tree intentionally has no `docs/plan/` workspace. Graph/render validation is instead a decisive gate on `dev` before candidate construction. Candidate cleanliness is verified by `mine release`'s stable-tree gates; MINE product-distribution checks (four-agent installation, `mine doctor --agents all`, MCP tool-count assertions, bootstrap smoke tests) apply **only** when the repository under review is the MINE source repository itself, per MINE-local governance - never as a universal requirement.
7. Only after every candidate check passes, perform the Design-authorized local stable-branch integration (squash or curated commit so temporary plan history is not imported into the stable branch - never a plain merge of `dev`), determine and record the resolved release version from `.mine/config.toml`, and perform only the MINE-owned local cleanup the Design authorizes: delete local `plan/*` branches that are merged/accepted (never a branch with unexpected ancestry, an unmerged branch, or a checked-out worktree), and delete the local `dev` branch only after stable integration succeeds.
8. Never push, create a remote release, publish a package, force-update history, or delete a remote or unrelated/user branch. Report the exact final local stable-branch commit, the resolved version, the final stable-tree contents, and every local branch or artifact removed.

## Reject and compensate

For a material failure:

1. Do not edit the immutable target plan or rewrite its implementation report/history.
2. When correction changes a material decision, update the `docs/design/` knowledge base and complete the ADR approval lifecycle before declaring the replacement target.
3. Create and commit the canonical Root-visible `docs/plan/reports/<plan-name>-review.md` with `REJECTED`, exact findings, commands, evidence, affected downstream nodes, and the next contiguous sibling compensation ID. The path must be a regular file contained by authoritative Root, not the implementation report or an escaped/symlinked path.
4. Call `mine plan reject --id <exact-stored-id> --reason <rejection summary> --review-report <review report path> --compensating-plan <comp-id> --format json` (MCP: `mine_plan_reject` with exact stored `id`, `reason`, `review_report`, and `compensating_plan`). The binary may transition `IMPLEMENTED`→`REJECTED`, or the explicitly reviewed infeasible `IN_PROGRESS` edge above, and records all rejection evidence atomically. It never fabricates implementation evidence. Commit only the Root-owned graph result with explicit paths. Downstream nodes stay blocked; registration and rewiring remain separate operations.
5. Create the repository's next compensating plan number/name (for example `02-1-...`) that identifies the rejected implementation and
   changes target code directly—no alias, shim or migration solely to preserve the rejected behavior.
6. Give the compensation node hard predecessors, deliverables, concrete steps, verification, acceptance criteria and graph edges. Route
   downstream release through the compensating plan.
7. Register/release the compensation and rewire only through the accepted MINE interface; reuse the preserved lineage worktree when execution is later invoked. Do not implement the compensating plan during review unless the user separately asks for implementation.
8. Stage only architecture, review, compensation-plan, and CLI-generated graph files owned by this operation; inspect the cached diff and commit with a Conventional Commit. Never hand-edit graph files.

If material product decisions remain unresolved, write the rejection report and ask the user before changing architecture/creating the plan.
Do not invent the decision merely to finish review.

## Review report requirements

Every review report must include:

- target plan, baseline, implementation/report commits and current branch;
- review scope and any concurrent-worktree caveat;
- findings ordered by severity with file/interface evidence;
- acceptance traceability matrix;
- exact commands, exit codes and concise outputs;
- direct reviewer fixes, if any;
- passed, failed, skipped, timed-out and unavailable checks;
- security/data handling statement;
- final `ACCEPTED` or `REJECTED` decision;
- downstream nodes released or kept blocked;
- remaining risks and required user/external actions.

Never claim independent acceptance when the same unresolved defects remain, and never soften `FAIL` into “remaining risk” to release a
downstream plan.

## Finish the handoff

Lead with `ACCEPTED` or `REJECTED`. Summarize decisive evidence, reviewer-authored fixes and their revalidation, report/plan paths, commit
hash(es), graph effect, verification results, unrelated changes preserved, whether local release closure was carried out in this session, and
next action. Say explicitly whether downstream work is released.

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
