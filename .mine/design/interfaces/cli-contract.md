# CLI Contract

## Human-facing core

This command inventory describes the 0.2.0 candidate, not installed 0.1.13.
Wave/show expose effective ownership snapshots; ordinary lifecycle writes carry
the caller's observed revision and explicit initiating identity. See
[active scheduling](../execution-graph/active-ownership-scheduling.md) and
[lifecycle receipts](../governance/lifecycle-receipts.md).
Typed ownership clarification, operation and verification adapters are present
in candidate source and matching Skills. Do not pass their arguments to 0.1.13
or globally replace the bootstrap installation during self-development.

The 0.2.0 verification adapter target is specified separately in
[capture/record/current-context contracts](../operations/verification-receipts.md#native-capture-record-and-current-context-bridge).
Operation begin/stage/finalize uses the pinned review report's
[local closure assertion](../governance/lifecycle-receipts.md#local-independent-review-and-stopped-writer-assertion),
not an unvalidated stopped-writer boolean. Candidate CLI-only adapters do not
introduce new MCP tools or themselves run checks, merge, clean up or publish.

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
mine graph validate|render|status|ready|wave|show|repair-provenance|repair-compensation-predecessors
mine plan add|show|prepare|start|implemented|accept|reject|release|rewire-compensation|withdraw|clarify
mine operation begin|stage|finalize|show
mine verification capture|record|check
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

These names are the candidate contract; Skills and documentation are
regenerated from it before release. The `v0.1.13` lifecycle-completeness
release shipped the four formerly planned additions: `mine plan withdraw`
(ADR-0012), `--planning-scope` on `mine plan add` plus the release-time
readiness re-check (ADR-0013), `mine design render`, and
`mine adr supersede --dry-run`.

## Machine-level vs repository-level commands

Lifecycle examples below show their operation-specific arguments. In candidate
0.2.0, ordinary Plan writes and graph repairs also require `--actor <actor>` and
`--expected-revision <r>` (start uses `--owner` instead of actor). Withdrawal
additionally requires `--operation-id <uuid-v4>`. Acceptance uses `--id`,
`--expected-revision`, `--operation-id` and typed `--input`, not `--review`.
Ownership clarification uses `plan clarify --id --expected-revision --input`.
Operation begin/stage/finalize take `--expected-revision --input`; show takes
`--operation-id`. Verification readers observe externally executed checks;
they do not run tests or grant release authority. Exact typed inputs live in
the linked lifecycle/verification contracts and candidate help.

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
- creates the configured Design scaffold and marker (`.mine/design/` by default);
- refuses foreign/unmarked configured governance and existing legacy layout
  without writes; unrelated user `docs/` is preserved and init is not migration;
- establishes bounded child Git ignore/byte-preserving attributes after ownership
  validation, refusing conflicting effective foreign rules;
- creates MINE sections in `AGENTS.md` without erasing unrelated content;
- configures supported agents when requested;
- initializes repository version from existing MINE state, reliable root version evidence, or `0.1.0`;
- performs no source scan, architecture generation, plan creation, agent invocation, business-code change, branch creation, commit, merge, or release.

## Workspace commands

`mine workspace open --actor <actor> --expected-revision <r>` creates the
configured temporary Plan workspace (`.mine/plan/` by default) on the integration
branch and generates an internal UUID. It takes no user-supplied release version.

`mine workspace close` validates closure and may purge only the configured,
ownership-marked Plan tree with explicit expected workspace identity and caller
revision. Version determination is separate; pending checkpoints block closure.

This internal workspace is distinct from per-Plan Git linked worktrees, which
are prepared by `mine plan prepare --id <plan-id>`.

## Plan lookup and linked-worktree preparation

`mine plan show --id <plan-id>` resolves the authoritative graph from the main
integration worktree even when invoked inside a linked worktree. JSON includes
the complete node, registered branch/worktree locators, resolved absolute
worktree path, observed state, caller location, authoritative revision, and the
persisted `branch_anchor` when the Plan has started.

`mine plan prepare --id <plan-id> --actor <actor> --expected-revision <r>
--operation-id <uuid-v4>` creates or safely reuses the registered
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
`<configured-plan-root>/<id>-<slug>.md`. Compensation registration/rejection validates the
same lineage and next contiguous `C` ordinal.

## Plan start and branch anchor

`mine plan start --id <plan-id> --owner <owner> --run-id <run-id>
--expected-revision <r>` accepts only
`READY`. Under the Root graph lock it rechecks revision/predecessors and the
exact prepared linked-worktree state. For a base Plan, branch tip and observed
HEAD must equal the completed durable preparation baseline (not every later
Root graph checkpoint). For a compensation, they must
equal the rejected predecessor branch tip named by the declared compensation
link. The commit is resolved through Git without a fixed object-ID length.

A successful transition stores that commit as immutable `branch_anchor` with
owner/run/timestamps and `IN_PROGRESS` in the same graph revision. Absent,
dirty, detached, wrong-path/branch, stale-tip, or changed Git evidence returns a
typed failure. Skills must not mutate Plan files before completed start
admission. A durable graph write alone is insufficient while the native
checkpoint is pending; recover the same operation rather than starting again.

## Execution-discovered Plan rejection

`mine plan reject` supports both ordinary `IMPLEMENTED -> REJECTED` review and
the evidence-bound `IN_PROGRESS -> REJECTED` recovery defined by ADR-0011. New
rejections require:

```text
mine plan reject --id <plan-id> --reason <reason> \
  --review-report <configured-plan-root/reports/...-review.md> \
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

## Execution-graph view restoration

Candidate ADR-0022 implementation:
`mine graph render --expected-revision <revision> --format json` requires the
caller's observed revision. It is a locked projection restoration, not a new
graph transition. Only a canonical no-op or recovery of a missing view already
proved in actual Root HEAD/index may succeed; both return equal before/after
revisions, workspace ID and data `{action, rendered_path}`, with action
`unchanged` or `restored`. Pending graph/render/checkpoint outcomes require
same-operation recovery. Foreign/stale bytes and uncheckpointed legacy pairs are
preserved/refused, never force-overwritten. No new actor/operation-ID, force,
path or content argument is added. Installed 0.1.13 does not implement this
target yet; do not invoke its new flag on live Root. See the complete
[restoration contract](../governance/root-graph-checkpoints.md#standalone-generated-view-restoration).

## Execution-graph provenance repair

`mine graph repair-provenance --expected-revision <revision>` is the only
operation allowed to read a supported older graph without first passing current
graph validation. Under the authoritative Root graph lock, it checks the
expected revision, derives missing canonical branch/lineage/worktree metadata
for v1 and `branch_anchor` for v2, refuses conflicting non-empty values, and
upgrades to fail-closed `mine-graph-v7`. Supported v3-v6 snapshots with complete
existing provenance require no derivation and receive a validated version-only
upgrade; reading an older snapshot does not fabricate preparation or acceptance.

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

## Compensation-predecessor repair

`mine graph repair-compensation-predecessors --id <compensation-id>
--expected-revision <revision> --reason <non-empty>` is the ADR-0014 recovery
for an invalid, unstarted compensation registration. It accepts no caller-
supplied edge list: the exact replacement is derived from the one rejected
node that declares the compensation.

The target must be `DRAFT`, `BLOCKED`, or `READY` with no start,
implementation, review, withdrawal, or child-compensation evidence, and its
own branch must not exist or be checked out. MINE replaces only its ordered
hard-predecessor list, validates the complete candidate graph, persists TOML
atomically, renders deterministic Markdown, and reports old/new dependencies
and status. Rendering is separately repairable as specified by
[persistence](../execution-graph/persistence-and-concurrency.md).
Empty reasons, revision conflicts, ambiguous lineage, existing Git
artifacts, unrelated graph invalidity, and every started/terminal target fail
without writes. An already-exact target is an idempotent no-op.

## ADR and Design readiness commands

ADR commands implement the state machine in
the [ADR lifecycle](../governance/adr-lifecycle.md). All JSON results include ADR registry
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
deterministically regenerates the configured ADR index from the
registry; `mine adr validate` distinguishes an invalid registry from a stale
generated index and names the repair.

## Design backup

`mine design backup` is the deterministic backup mechanism used by `mine-sync`.

It:

- validates the design marker and repository ownership;
- creates a timestamped child of `.mine/runtime/design-backups/` (installed
  0.1.13 instead uses `docs/design-backup-<UTC timestamp>/`);
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
directory (from Root configuration) — the fail-safe superset under
which every design-path-scoped ADR applies. The scope is immutable afterwards
and is exposed by `mine plan show`. Registration requires planning readiness
for the recorded scope; the former lineage-id scope derivation is removed.

For a compensation ID, registration additionally derives the one rejected
node that declares it and requires the same lineage, next contiguous sibling
ordinal, and exact ordered hard-predecessor list before insertion (ADR-0014).
The command never normalizes or silently repairs a caller-supplied mismatch.

## Plan release

`mine plan release --id <plan-id>` moves a newly registered plan from `DRAFT`
into `READY`/`BLOCKED` after scope readiness and predecessor checks. See the
[release CLI details](cli-plan-lifecycle.md#plan-release) for gates, errors
and the result envelope.

## Plan withdrawal

`mine plan withdraw --id <plan-id> --reason <reason>` retires an unstarted
plan into terminal `WITHDRAWN` (ADR-0012). See the
[withdrawal CLI details](cli-plan-lifecycle.md#plan-withdrawal) and
[lossless cleanup contract](../execution-graph/plan-withdrawal.md).

## Compensation rewiring

`mine plan rewire-compensation --id <rejected-plan-id>` reroutes downstream
dependencies from an explicitly rejected plan onto its declared compensation.
See the [rewiring CLI details](cli-plan-lifecycle.md#compensation-rewiring)
for exact replacement rules, idempotency, errors and the result envelope.

## Design validation

`mine design validate` checks:

- marker exists and repository ID matches;
- the configured Design index exists;
- every index link resolves;
- child directories have indexes;
- no duplicate document IDs;
- plan anchors exist;
- size thresholds produce warnings;
- stable branch contains no owned Plan workspace;
- no managed backup or runtime path is tracked or staged for release.

## Stale-plan-reference scan

`mine scan plan-refs` is a read-only validation command, parallel to `mine graph validate` and `mine design validate`, that detects temporary historical Plan references (e.g. `Plan NN`) in tracked implementation content before stable release. It is the **authoritative cross-platform scanner**: a native Rust implementation with no Bash, WSL, or Git Bash dependency, so the release/review path works on Windows without WSL, Windows without `bash` on PATH, Linux, and macOS.

### Semantics (preserved from the accepted scanner contract)

- inspects **tracked** repository content (`git ls-files`), never an uncontrolled filesystem walk;
- detects temporary historical Plan references matching `(^|[^[:alnum:]_])[Pp]lan[[:space:]-]*[0-9]`;
- excludes the configured Plan and Design roots (canonical `.mine/plan/` and
  `.mine/design/` when unconfigured), `.mine/runtime/design-backups/`, user-facing
  docs (`docs/concepts*`, `docs/user-guide*`, `docs/README.md`), root `README.md` /
  `README.zh-CN.md`, `tests/fixtures/**`, and any `testdata` path segment; there is
  no blanket `.mine/` or arbitrary legacy backup-name exemption;
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
