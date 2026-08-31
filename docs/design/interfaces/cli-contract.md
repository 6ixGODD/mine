# CLI Contract

## Human-facing core

Normal users should need only:

```text
mine init
mine status
mine doctor
```

All other commands are primarily invoked by Skills or advanced users.

## Command groups

```text
mine setup
mine update
mine uninstall
mine init
mine status
mine doctor
mine workspace open|status|close
mine graph validate|render|status|ready|wave|show|repair-provenance
mine plan add|show|prepare|start|implemented|accept|reject|release|rewire-compensation|withdraw
mine adr add|show|list|propose|approve|reject|supersede|validate
mine design backup|validate|status|render
mine design readiness
mine repository version show|suggest|set
mine agent config|install|uninstall|status
mine dist sync|verify
mine scan plan-refs
mine release
mine mcp serve
```

These names are the current stable contract; Skills and documentation are
regenerated from it before release. The `v0.1.13` lifecycle-completeness
release shipped the four formerly planned additions: `mine plan withdraw`
(ADR-0012), `--planning-scope` on `mine plan add` plus the release-time
readiness re-check (ADR-0013), `mine design render`, and
`mine adr supersede --dry-run`.

## Machine-level vs repository-level commands

MINE has three distinct levels of operation; do not confuse them:

- **Machine-level Agent integration**: `mine setup` installs Skills and MCP
  configuration into coding-agent client directories (run once per machine;
  rerun to add or remove integrations). `mine update` / `mine uninstall`
  manage the installed binary and Agent integrations at machine level.
- **Machine-level installation status**: `mine agent status` lists the managed
  Agent installations and their health independent of any repository; it does
  not require an initialized MINE repository.
- **Repository-level initialization**: `mine init` creates `.mine/config.toml`,
  the design namespace, and governance at a repository root (run once per
  repository).
- **Repository-aware diagnostics**: `mine doctor` / `mine doctor --agents all`
  run inside an initialized repository and check `.mine/config.toml`, design
  marker/index, execution graph, Git branch, and (with `--agents`) per-Agent
  installation state. They are not machine-level commands.

A fresh machine-level install is verified with `mine agent status`; `mine doctor`
requires an initialized repository.

## `mine init`

`mine init`:

- discovers repository root and stable branch;
- initializes or validates `.mine/config.toml`;
- creates a repository UUID when unmanaged;
- creates `docs/design/` scaffold and `.mine-design.toml` when absent;
- backs up an unmarked or foreign-owned existing `docs/design/` to `docs/design-backup-<timestamp>/` and creates a fresh managed root;
- creates MINE sections in `AGENTS.md` without erasing unrelated content;
- configures supported agents when requested;
- initializes repository version from existing MINE state, reliable root version evidence, or `0.1.0`;
- performs no source scan, architecture generation, plan creation, agent invocation, business-code change, branch creation, commit, merge, or release.

## Workspace commands

`mine workspace open` creates the temporary `docs/plan/` workspace on the configured integration branch and generates an internal UUID. It takes no user-supplied release version.

`mine workspace close` validates closure and may purge only the ownership-marked `docs/plan/` tree with explicit expected workspace identity. Version determination is separate.

This internal workspace is distinct from per-Plan Git linked worktrees, which
are prepared by `mine plan prepare --id <plan-id>`.

## Plan lookup and linked-worktree preparation

`mine plan show --id <plan-id>` resolves the authoritative graph from the main
integration worktree even when invoked inside a linked worktree. JSON includes
the complete node, registered branch/worktree locators, resolved absolute
worktree path, observed state, caller location, authoritative revision, and the
persisted `branch_anchor` when the Plan has started.

`mine plan prepare --id <plan-id>` creates or idempotently reuses the registered
branch and lineage-linked worktree under `[plan].worktrees_root`. Compensation
nodes reuse the base lineage directory and switch only that linked checkout.
It requires a clean
Root integration worktree and stable porcelain `-z` evidence. It never forces,
switches Root, prunes, or deletes an unverified path.

Anchor validation is lifecycle-aware. A branch that has not yet been
attached/switch-selected must equal its base `dev` or rejected-predecessor
initial anchor exactly. `READY` remains exact-initial-anchor only. After start,
`IN_PROGRESS` and `IMPLEMENTED` reuse permits ordinary Plan commits only when
the **persisted** `branch_anchor` remains an ancestor and porcelain HEAD equals
the branch tip. Current Root HEAD is never reused as an active Plan's anchor.
Missing anchors, non-descendant history, mismatched observed HEAD, or applying
the initial exact-anchor rule to a legitimately advanced active/reviewable
branch are invalid.

Plan IDs are canonical `planNN(?:-NN)*(?:-CNN)?`; paths are exactly
`docs/plan/<id>-<slug>.md`. Compensation registration/rejection validates the
same lineage and next contiguous `C` ordinal.

## Plan start and branch anchor

`mine plan start --id <plan-id> --owner <owner> --run-id <run-id>` accepts only
`READY`. Under the Root graph lock it rechecks revision/predecessors and the
exact prepared linked-worktree state. For a base Plan, branch tip and observed
HEAD must equal current Root integration HEAD. For a compensation, they must
equal the rejected predecessor branch tip named by the declared compensation
link. The commit is resolved through Git without a fixed object-ID length.

A successful transition stores that commit as immutable `branch_anchor` with
owner/run/timestamps and `IN_PROGRESS` in the same graph revision. Absent,
dirty, detached, wrong-path/branch, stale-tip, or changed Git evidence returns a
typed failure and writes nothing. Skills must not mutate Plan files before this
success.

## Execution-discovered Plan rejection

`mine plan reject` supports both ordinary `IMPLEMENTED -> REJECTED` review and
the evidence-bound `IN_PROGRESS -> REJECTED` recovery defined by ADR-0011. New
rejections require:

```text
mine plan reject --id <plan-id> --reason <reason> \
  --review-report <docs/plan/reports/...-review.md> \
  --compensating-plan <next-sibling-id>
```

The report must be the canonical Root-visible review path for that Plan and
must exist before mutation. For `IN_PROGRESS`, it must document immutable Plan
infeasibility and independent/owner authority; no implementation report or
commits are synthesized. The operation validates current status, revision,
report path, same-lineage contiguous compensation identity, and graph
semantics under the lock, then records rejection evidence atomically. Any other
source state or missing evidence returns `MINE_INVALID_TRANSITION` or the
specific validation error without writes. No generic cancel/set-status command
is added.

## Execution-graph provenance repair

`mine graph repair-provenance --expected-revision <revision>` is the only
operation allowed to read a supported older graph without first passing current
graph validation. Under the authoritative Root graph lock, it checks the
expected revision, derives missing canonical branch/lineage/worktree metadata
for v1 and `branch_anchor` for v2, refuses conflicting non-empty values, and
upgrades to fail-closed `mine-graph-v4`. A `mine-graph-v3` graph needs no
derivation (the v4 fields are optional with defaults) and upgrades as a
version-only migration.

Anchor derivation is limited to the validated first parent of the first ordered
implementation commit, or an active compensation's exact declared rejected-
predecessor tip when no implementation commits exist. Git objects, commit
ordering, declared lineage, and ancestry to observed tip are verified.
Ambiguous active base Plans and inconsistent evidence write nothing. A real
migration increments revision once and renders Markdown; repeating against
the current schema is idempotent byte-preserving success with no revision
increment.
Unsupported schemas, revision conflicts, unsafe paths, missing objects, and
inconsistent provenance write nothing.

## ADR and Design readiness commands

ADR commands implement the state machine in
`docs/design/governance/adr-lifecycle.md`. All JSON results include ADR registry
revision before/after, ID, status, content digest, blocking scopes, and approval
or supersession evidence. `approve`, `reject`, and `supersede` are explicit
authority transitions; illegal transitions or digest drift write nothing.

`mine design readiness [--scope <scope>]` combines Design validation and
applicable planning-blocking ADR state. It returns `ready`, blocking ADR DTOs,
digest mismatches, unresolved scopes, and registry revision. A `SUPERSEDED`
entry is satisfied only when its `superseded_by` chain transitively reaches an
`APPROVED` entry that still applies to the requested scope; superseding with an
equal-scope approved replacement never reduces readiness.

`mine adr supersede --dry-run` previews post-supersession readiness for every
affected scope without writing; there is no `unsupersede` (reversal stays
forward-only through a new ADR and `supersede`). `mine design render`
deterministically regenerates `docs/design/decisions/index.md` from the
registry; `mine adr validate` distinguishes an invalid registry from a stale
generated index and names the repair.

## Design backup

`mine design backup` is the deterministic backup mechanism used by `mine-sync`.

It:

- validates the design marker and repository ownership;
- creates `docs/design-backup-<UTC timestamp>/`;
- copies managed design without following external links;
- writes `*` to the backup root `.gitignore`;
- verifies copy completion;
- emits a structured manifest and backup path;
- performs no design mutation.

## Output modes

- default: concise human-readable output;
- `--format json`: stable machine-consumable envelope;
- `--quiet`: suppress non-error human output where meaningful;
- `--no-color`: deterministic plain text.

## JSON output

```json
{
  "ok": true,
  "command": "plan.start",
  "repository": "D:/work/project",
  "workspace_id": "8dcd1df5-...",
  "revision_before": 7,
  "revision_after": 8,
  "data": {},
  "warnings": []
}
```

Errors use the same envelope with `ok: false`, stable `error.code`, human message, and structured details.

## Exit codes

- `0`: success;
- `2`: invalid invocation;
- `3`: repository, branch, namespace, or workspace gate failure;
- `4`: validation failure;
- `5`: revision or lock conflict;
- `6`: external dependency or Git evidence failure;
- `7`: partial success requiring repair;
- `1`: unexpected internal failure.

Exact values become public contract when released.

## Plan registration scope

`mine plan add [--planning-scope <scope>]` records the node's planning scope
(ADR-0013). When omitted, the node records the configured design root
directory (for this repository, `docs/design`) — the fail-safe superset under
which every design-path-scoped ADR applies. The scope is immutable afterwards
and is exposed by `mine plan show`. Registration requires planning readiness
for the recorded scope; the former lineage-id scope derivation is removed.

## Plan release

`mine plan release --id <plan-id>` moves a newly registered plan from `DRAFT`
into the startable frontier, deterministically. Registration (`mine plan add`)
always creates a `DRAFT` node; release is the explicit gate between
registration and execution. See
`docs/design/execution-graph/state-machine-and-algorithms.md#plan-release`
for the full algorithm.

- Accepts only a `DRAFT` plan; returns `MINE_INVALID_TRANSITION` for any other
  status and mutates nothing.
- Re-runs planning readiness for the node's persisted `planning_scope`
  (ADR-0013) before transitioning; a blocked readiness fails with the blocking
  ADR DTOs and writes nothing. On success the node records
  `readiness_checked_at` and the satisfied `adr_registry_revision` as audit
  evidence.
- Transitions `DRAFT -> READY` when every hard predecessor is `ACCEPTED`
  (including a plan with no hard predecessors); transitions `DRAFT -> BLOCKED`
  when one or more hard predecessors are not yet `ACCEPTED`.
- Never alters `IN_PROGRESS`/`IMPLEMENTED`/`ACCEPTED`/`REJECTED`/`WITHDRAWN`
  plans.
- Goes through the shared transaction (`lock -> reload -> revision check ->
  semantic validation -> mutation -> atomic write -> deterministic render`);
  every successful release increments the graph revision exactly once.
- Not idempotent-success: re-running on an already-released node returns
  `MINE_INVALID_TRANSITION` and writes nothing.

```json
{
  "ok": true,
  "command": "plan.release",
  "revision_before": 18,
  "revision_after": 19,
  "data": {
    "plan": "09",
    "status_before": "DRAFT",
    "status_after": "READY",
    "hard_predecessors": ["03"],
    "unsatisfied_predecessors": []
  },
  "warnings": []
}
```

Errors reuse stable `MINE_*` codes (`MINE_PLAN_NOT_FOUND`,
`MINE_INVALID_TRANSITION`, `MINE_REVISION_CONFLICT`, `MINE_LOCK_TIMEOUT`).

## Plan withdrawal

`mine plan withdraw --id <plan-id> --reason <reason>` retires an unstarted
plan (`DRAFT`/`BLOCKED`/`READY`) into terminal `WITHDRAWN` (ADR-0012). See
`docs/design/execution-graph/state-machine-and-algorithms.md#plan-withdrawal`
for the full algorithm, preconditions, and Git-cleanup proofs.

- Requires a non-empty reason; never accepts a review report or compensating
  plan (withdrawal closes registration, not execution).
- Performs provably-lossless cleanup when a prepared-but-unstarted
  branch/worktree exists (base: tip equals or is an ancestor of the
  integration branch tip; compensation: tip equals the rejected predecessor's
  preserved tip; worktree clean) and refuses with typed errors otherwise.
- Withdrawn hard predecessors keep successors unsatisfied; withdrawn nodes
  never block workspace closure and are never revived.
- Not idempotent-success: re-running returns `MINE_INVALID_TRANSITION` and
  writes nothing.

## Compensation rewiring

`mine plan rewire-compensation --id <rejected-plan-id>` reroutes downstream
dependencies from an explicitly rejected plan onto its registered compensating
plan. It is the deterministic, CLI-managed closure of `mine plan reject` (which
set `compensating_plan`), and the only supported rerouting path after the
bootstrap exception ended. See
`docs/design/execution-graph/state-machine-and-algorithms.md#compensation-rewiring`
for the full algorithm and preconditions.

- The single input flag is `--id` (the rejected plan id). The replacement is
derived from the rejected plan's `compensating_plan`; the caller never supplies
it, so substitution can never be triggered by a similar id.
- Rewiring goes through the shared application/persistence transaction
  (`lock -> reload -> revision check -> semantic validation -> mutation ->
  atomic write -> deterministic render`); the graph TOML and its generated
  Markdown change atomically from the caller's perspective.
- It verifies the original is `REJECTED`, `compensating_plan` names an existing
  non-rejected replacement, every affected successor is still mutable
  (`DRAFT`/`BLOCKED`/`READY`), no cycle is introduced, and unrelated predecessors
  and successors are unchanged.
- Repeating a completed rewiring is safe idempotent success: it writes
  nothing, bumps no revision, and returns `affected_successors: []`.
- It never weakens the immutability of accepted or active plans: successors in
  `IN_PROGRESS`/`IMPLEMENTED`/`ACCEPTED`/`REJECTED` are never touched.

### Result envelope

```json
{
  "ok": true,
  "command": "plan.rewire-compensation",
  "revision_before": 17,
  "revision_after": 18,
  "data": {
    "rejected_plan": "05",
    "compensating_plan": "05-1",
    "affected_successors": ["06"]
  },
  "warnings": []
}
```

Errors reuse the stable `MINE_*` codes; rewiring-specific failures use
`MINE_REWIRE_SUCCESSOR_LOCKED` and the existing `MINE_GRAPH_CYCLE`. No
arbitrary graph-editing CLI is introduced.

## Design validation

`mine design validate` checks:

- marker exists and repository ID matches;
- `docs/design/index.md` exists;
- every index link resolves;
- child directories have indexes;
- no duplicate document IDs;
- plan anchors exist;
- size thresholds produce warnings;
- no duplicate document IDs;
- plan anchors exist;
- size thresholds produce warnings;
- stable branch contains no `docs/plan/`;
- no `docs/design-backup-*` path is tracked or staged for release.

## Stale-plan-reference scan

`mine scan plan-refs` is a read-only validation command, parallel to `mine graph validate` and `mine design validate`, that detects temporary historical Plan references (e.g. `Plan NN`) in tracked implementation content before stable release. It is the **authoritative cross-platform scanner**: a native Rust implementation with no Bash, WSL, or Git Bash dependency, so the release/review path works on Windows without WSL, Windows without `bash` on PATH, Linux, and macOS.

### Semantics (preserved from the accepted scanner contract)

- inspects **tracked** repository content (`git ls-files`), never an uncontrolled filesystem walk;
- detects temporary historical Plan references matching `(^|[^[:alnum:]_])[Pp]lan[[:space:]-]*[0-9]`;
- excludes temporary planning state and accepted documentation: `docs/plan/**`, `docs/design/**`, `docs/design-backup-*/**`, user-facing docs (`docs/concepts*`, `docs/user-guide*`, `docs/README.md`), root `README.md` / `README.zh-CN.md`, `tests/fixtures/**`, `**/testdata/**`;
- honors the explicit fixture exemption marker `mine-release-allow-plan-reference:` on the immediately preceding line;
- never rewrites source; it only reports evidence;
- reports exact `file:line` findings;
- `--check` mode exits non-zero when unexempted findings exist (release gate); without `--check` it prints findings and exits zero (repair mode);
- operates against the repository selected by normal CLI context, including `--repo`;
- supports `--format json` with stable machine-readable findings.

### Exit codes

- `0` no unexempted findings (or, without `--check`, findings reported for repair);
- `1` `--check` mode with unexempted findings;
- `2` usage error;
- non-zero without findings when the target is not a Git repository or Git inspection fails (fail-closed).
