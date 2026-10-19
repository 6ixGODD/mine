---
name: mine-plan-exec
description: Execute one repository implementation plan end to end in its MINE-prepared linked worktree. Use when the user invokes mine-plan-exec with a plan ID or file, asks to implement a governed plan, or wants dependency checks, code changes, verification, reporting, commits, and graph updates. Never implement directly on stable or dev.
version: 0.2.0
---

# MINE Plan Execute

Implement one immutable Plan end to end in its registered linked worktree, verify,
commit scoped work and hand off IMPLEMENTED for independent review. Never implement
on Root/dev/stable, self-accept, merge, push or publish as part of execution.

## Resolve identity and authority

Read AGENTS.md, requirements, Root .mine/config.toml/marker, the whole requested Plan,
governing Design leaves, predecessor acceptance reports/commits and registered
primary sources. Fetch the actual sources, not only a planner's paraphrase.
Fresh governance roots are .mine/design/ and .mine/plan/; use actual configured paths.

Resolve Root from Git's first worktree inventory record. Query its graph show JSON.
Match exact registered ID/path or the canonical alias forms: planNN / PNN / NN,
including layered numeric segments and terminal -C01. Strip one invocation @ only.
Require one distinct match, then use its exact stored ID in every command. Do not
guess filenames, use substring matches, fill an omitted C ordinal or select another
READY node. Legacy IDs are resolved only when actually present, never newly created.

Check installed version/help/MCP schemas first. Never bootstrap a future binary/Skill
onto live older graph/layout. Complete active legacy work with its installed workflow;
candidate commands belong only in isolated fixtures until actual release. Prefer
available typed MCP tools; prepare, clarify, operations and verification use actual
JSON CLI fallback where no corresponding MCP tool exists.

## Admission and preparation

Inspect status/index/branch/inventory and classify existing edits: continue same-Plan
work, preserve unrelated work, investigate ambiguous overlap. Dirty status is not
permission to discard or duplicate an executor. Never stash/reset/clean/restore
another owner's changes.

Require hard predecessors ACCEPTED with genuine reports/commits and this node READY
or already IN_PROGRESS under the same owner/run. Read the complete graph wave/show
snapshot, including held IN_PROGRESS/IMPLEMENTED owners, original/effective scope,
policy/provenance, pending closures/checkpoints, lineage/worktree and mutable output/
generator resources. READY/wave selection is advisory, not a write grant. Missing
required semantic policy fails closed, never downgrades or drops a held owner.

For a fresh prepare, explicitly declare caller actor, generate one canonical UUIDv4,
retain that identity and supply the exact observed Root revision:
~~~text
mine plan prepare --id <exact-id> --actor <actor> --expected-revision <R> --operation-id <uuid> --format json
~~~
Enter only its returned absolute linked worktree after completed checkpoint and
verify registered branch/HEAD. The scheduler's location must match the same evidence.
Record preparation generation, prepared branch commit, observed Root commit and
checkpoint result separately. Root stays on configured integration branch.

If a prepare/checkpoint response is uncertain, query the returned same operation ID
before retry; retain original actor/observer/mode/request. Never create a duplicate
checkout, manually fast-forward a graph checkpoint or copy graph files into the Plan.
A genuinely stale new request needs a fresh read/re-evaluation, not silent revision
substitution. Explicit --reprepare and --recover-preparation are only for their
native safe-baseline/legacy-evidence cases, never dirty/divergent checkout repair.

After prepare completes, read Root revision again and start through MCP
mine_plan_start(expected_revision,id,owner,run_id) or:
~~~text
mine plan start --id <exact-id> --owner <owner> --run-id <run> --expected-revision <R> --format json
~~~
No production edit is admitted until start and its native graph checkpoint complete.
The selected generation's prepared commit becomes the immutable branch_anchor.
An active continuation proves that stored anchor is an ancestor of the clean actual
registered tip; today's Root HEAD is not a substitute. A pending checkpoint may
already have persisted IN_PROGRESS: query/resume, do not rerun start.

## Compensation body after start

A new compensation body registered at Root G may be absent from rejected tip P.
Prepare/READY stay exactly at P in the same lineage worktree; do not advance READY,
merge Root G, cherry-pick registration or move the preserved rejected branch.

Only after completed start, the assigned same owner/run executor proves the exact
Root body equals its registered/preparation digest and its bytes in observed G.
Publish only that own canonical body path into the clean P checkout without clobber
or normalization. Verify raw working bytes, actual attributes and index blob; commit
one ordinary child of P whose sole tree addition is that body. An identical already
committed body needs no new commit. Different bytes, links or foreign dirty state
are preserved/refused. Do not copy other Plans/reports/policies/Design/config/graph.
Record G, P, digest and materialization commit. Verification reads the real committed
checkout body afterward; there is no Root-body fallback.

Interrupted body materialization first audits the same start/owner/run/anchor/ref/
worktree. Only its sole exact uncommitted addition, all bytes equal and HEAD still P,
may finish that same body-only commit. A lost commit response is reconciled from the
actual body-only child, not another commit. Generic dirty prepare is not a pass.

## Implementation authority

Follow the Plan's work-package DAG. Responsibility/namespaces, behavior and exclusions
are authority; physical mappings locate it. Make bounded internal file/symbol splits
inside that scope without repeated permission, recording decisions. Do not cross
public/persistent/security/architecture/acceptance boundaries or touch shared files
owned elsewhere. An architect assigned a foundation uses this same path and must
deliver a real consumer/invariant, not unused scaffolding or a Root exception.

A frozen mapping/generator-closure defect may use owner-authorized native clarification:
plan clarify --id <id> --expected-revision <R> --input <contained-json>.
Preserve original Plan bytes/digest/declarations; append exact identity/actor/reason/
policy/tail provenance, and let native admission check all held owners. Do not
rehash tampered Plan bytes, invent historic digests, broaden behavior/dependencies/
acceptance or use clarification as prompt-only policy activation.

Run the narrow regression first, implement cohesively, cover errors and downstream
consumers, remove obsolete internal behavior, then run required broader affected
gates. Coordinate shared setup/central manifests/generator inputs through their named
owner. One integration owner regenerates the complete combined output transaction.

## Evidence without duplicate work

Reuse Root toolchains/downloads/venv and safe immutable caches. Every language's
mutable build/test/generated/coverage/package/log output belongs under
<Root>/.mine/build/<exact-plan-id>/<output-class>/, separate even for compensation.
No worktree target/.venv/node_modules or shared parallel mutable directory. Measure
exceptional cost and agree owner strategy/cleanup before building; never silently
weaken isolation or delete foreign artifacts.

Use actual native verification capture/record/check JSON contracts with requests in
Root .mine/build/<exact-plan-id>/<output-class>/<file>. Capture current source/body/
policy/toolchain/environment/platform/features before running the external command;
record actual outcomes/artifact digests afterward. These commands never run tests.
Check old receipts with fresh context before reuse. A successful JSON process exit
can report INVALIDATED/NON_REUSABLE; inspect disposition/reason, not only exit0.
RUN/REUSED/INVALIDATED/NON_REUSABLE/NOT_RUN remain distinct. Preserve first failures,
warnings, timeouts and native-host absence. Reuse exact applicable evidence without
rerunning it, but never erase required independent/native or changed-input probes.

## Commits, completion and recovery

Stage explicit in-scope files only, audit cached paths/diff/check and use normal
Conventional Commits. Commit the implementation and canonical implementation report
before graph mutation. Report exact commands/exits/artifacts/input identities, local
decisions, untouched concurrent work, skipped/failed checks and remaining downstream
gaps. Do not call partial work IMPLEMENTED.

Read Root observer revision and use mine_plan_mark_implemented with actor,
expected_revision,id,report,commits, or the installed plan implemented CLI carrying
those exact fields. Native graph checkpoint owns only its graph pair; do not stage
it manually or hide pending publication. Validate returned completed state and graph.
If checkpoint publication is pending, query/resume the same operation, not a new
execution/compensation. Never self-accept.

Objective immutable-Plan infeasibility stops implementation without IMPLEMENTED.
Preserve evidence and scoped reproduction commits, keep the node IN_PROGRESS and
handoff independent review/material ADR decision. Infrastructure/transport failure
alone is not rejection. On final handoff explicitly stop relinquished checkout/output
use; the reviewer needs real owner/run stop evidence, not silence or clean status.

Report final live status: reviewable IMPLEMENTED, executing IN_PROGRESS, READY,
BLOCKED/pending operations and accepted/withdrawn nodes, plus exact Plan/worktree,
commits, verification and next reviewer action. Do not claim acceptance or release.


## Canonical durable bounds

All new IDs follow planNN(?:-NN)*(?:-CNN)? (two-digit numeric segments, uppercase
terminal C). Files are <configured-plan-root>/<id>-<lower-kebab-slug>.md.
Compensation uses the next contiguous sibling C ordinal in the same lineage.
Never edit the configured execution-graph.toml or generated execution-graph.md
directly. Root owns graph/config/locks/revision even for linked callers.
For Rust set CARGO_TARGET_DIR to that exact Plan's isolated output class; the
same mutable-output isolation applies to every other language, not just Cargo.
