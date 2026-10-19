# Durable READY Preparation

## Authority and implementation boundary

Accepted target under [ADR-0022](../decisions/0022-durable-ready-preparation-and-root-graph-checkpoints.md),
approved at ADR registry revision 40; revision 41 supersedes ADR-0010 without
editing either approved body. This target is mandatory before 0.2.0 and is
**not implemented by installed 0.1.13**. Existing active anchors, compensation
provenance and [worktree safety](worktree-and-integration-lifecycle.md) remain.
Candidate source and matching Skills implement persisted generations and native
prepare/start checkpoint consumers. Their evidence is separate from the old
binary's actual Root lifecycle, independent review and final release readiness.
The ADR's revision-37 plan10 facts are a historical incident snapshot, not live
status; plan10 subsequently started at graph revision 38.

Preparation becomes persisted provenance, not a new Plan lifecycle state.
Use the existing bounded Git adapter, graph transaction, ownership evaluator
and [operation journal](lifecycle-receipts.md), not a second workflow engine.

## Preparation generations and schema

Following the planned v5 ownership and v6 acceptance schemas, implement these
facts in `mine-graph-v7`. Actual Root bootstrap remains v4 until release;
candidate migrations run only in isolated repositories.

Each node retains ordered append-only `preparations` generations. A generation
contains its positive monotonically increasing ordinal, operation ID, actor/time,
repository/workspace/Plan identity, canonical Plan path and body SHA-256,
preparation-policy SHA-256, lineage/branch/repository-relative worktree locator,
exact `prepared_branch_commit`, exact `observed_root_commit`, and graph revision
observed before preparation. Git OIDs use the repository's object format.
The latest successfully completed generation is effective. Its graph write
increments once; a valid repeat does not change graph bytes/revision or Git.

The policy digest binds configured graph/worktree/branch authority, relevant
approved preparation decisions and this node's immutable/effective contract:
dependencies, ownership and applicable clarification facts. It excludes ordinary
graph revision/readiness timestamps and other nodes' changing status/occupancy.
Those live admission facts are independently rechecked at start. Never hash
the entire mutable graph as preparation policy.

New starts persist `started_preparation_generation` with `branch_anchor`,
owner/run/status in one graph mutation. `branch_anchor` equals the selected
generation's prepared branch commit and never changes afterwards. Migration
preserves older active anchors and acceptance/ownership evidence without
inventing a historical preparation generation. Legacy READY evidence is
explicitly absent, not derived from reflogs, merge-base or arbitrary ancestors.
Older writers must refuse v7 before mutation; supported earlier provenance and
compensation repair guarantees must not be lost in the new reader/migration.

## Initial prepare, reuse and explicit refresh

Initial base preparation uses exact accepted Root dev HEAD; compensation uses
the stopped declared rejected predecessor's exact preserved tip and its same
lineage worktree. Capture Root HEAD separately: compensation tip need not
contain subsequent Root graph commits. Validate clean Root and Plan checkout,
unique inventory/branch/path ownership, containment and all existing safety gates.

Journal intent before Git mutation, create or verify the exact branch/worktree,
persist one generation, render and complete its native
[Root graph checkpoint](root-graph-checkpoints.md). Return completion only after
revalidation, with actual graph revision, operation ID, generation, prepared and
observed Root commits, paths and checkpoint result. An uncertain creation or
graph/checkpoint result is recovered under that same operation identity.

For READY reuse require actual clean checkout HEAD and registered branch tip
to equal the recorded prepared branch commit exactly. Compare current body,
policy and locator identities. Then prove the complete Root history below.
Graph-only churn never moves the prepared branch or creates another generation.
Premature READY commits, dirty files, replaced worktrees or changed identity
are refusals, not reasons to silently accept a newer branch tip.

An explicit reprepare against a changed approved baseline is a typed operation,
not normal reuse. Require a clean checkout exactly at its prior prepared tip,
fresh accepted-policy admission and provable non-destructive ancestry. For a
base, an explicit safe fast-forward to the new accepted baseline may precede
the appended generation; never reset/rebase/merge or discard divergent work.
For compensation retain its exact declared predecessor tip; refresh only the
observed Root/policy baseline when the declared lineage remains valid. Journal
old/new facts before mutation. Preserve previous generations and stopped refs.

Legacy recovery is separate: establish a fresh initial generation only when
current exact Git evidence satisfies initial preparation against the current
accepted baseline (or exact declared rejected tip). Missing evidence does not
license attaching an arbitrary stale checkout or fabricating its original origin.
If those proofs fail, refuse with the missing evidence and safe next action.

Legacy compensation additionally requires the explicit current
[rejection handoff observation](lifecycle-receipts.md#explicit-current-handoff-observation-for-legacy-rejection)
before preparation may switch its lineage checkout. A missing old journal is not
itself a stopped-writer proof; the recovery route does not recreate old events.

## Compensation body materialization after start

A compensation body registered on Root after rejected tip P cannot already be
present in P. Preserve the exact compensation preparation rule: prepare and
READY checkout remain at P, and completed start records `branch_anchor = P`.
Do not merge Root G, cherry-pick its registration commit, import graph files,
move the preserved rejected ref, or advance the compensation branch while READY
just to make its new body appear. Prepare/start obtain authoritative body
identity from the Root registration/preparation, not a required premature copy
in the compensation checkout. A missing new compensation body at P is not itself
a preparation failure; this does not bypass any other identity/cleanliness gate.

The **assigned implementation executor, through mine-plan-exec**, materializes
only its own exact registered compensation body after native start and its
checkpoint have completed, before production edits or verification capture:

1. Resolve the same repository/workspace/Plan, owner/run, completed start,
   preparation generation, anchor P and registered lineage checkout. Read the
   frozen body at its canonical Root path and prove it equals the registered
   SHA and the body recorded in the preparation's observed Root commit G.
   G is separate from P; no ancestry G -> P is assumed. Missing/mismatched
   authority is a refusal, never permission to invent or re-freeze the body.
2. Require the actual compensation checkout/branch to be clean at P and the
   rejected ref unchanged. Copy the exact bytes to the same canonical relative
   path within this checkout using contained, no-clobber byte-preserving file
   publication. No text normalization, templating or path rewriting is allowed.
   An already-present identical committed body needs no write/extra commit;
   a different body, link, foreign path or ambiguous dirty state is preserved
   and refused, not overwritten.
3. Stage only this exact body path; before committing, prove the index blob
   payload and raw checkout bytes match the frozen Root body under actual Git
   attributes. Then make one ordinary executor-owned Plan commit whose parent
   is P and whose tree delta is only that addition. Reverify raw body/blob bytes
   and that the resulting checkout is clean. No native automatic
   product commit, new graph revision, preparation generation or anchor update
   is involved. Record G, P, body identity and materialization commit in the
   implementation report. Subsequent implementation commits use normal active
   ancestry rules; the reviewer requires the body at the actual reviewed tip.

This is a narrow governance-artifact materialization right for the assigned
executor, not permission to edit an immutable Plan or widen its product write
scope. The only allowed body change is absent -> exact registered bytes at its
own canonical path. It does not authorize copying other Plans, reports, policies,
Design, config or Root graph state. Missing non-body prerequisites remain their
own scope/dependency issue. Root remains dev; the stopped writer and preserved
rejected branch remain unchanged. Root integration must retain the same body
bytes rather than resolve a conflict by changing the frozen Plan.

Interrupted materialization is not a new native lifecycle operation. If copy
or staging succeeded but commit did not, `mine-plan-exec` first performs a
read-only same-start/owner/run/anchor/branch/worktree and exact-body audit. Only
when the sole dirty/staged path is this exact addition, all available bytes equal
the frozen body and HEAD is still P may it finish that same body-only commit.
Then normal clean prepare/reuse resumes. Do not call generic dirty-prepare a
success, rerun start, stage unrelated files, reset the branch or erase a partial/
foreign file. If commit succeeded but its response was lost, prove the actual
body-only child of P and reuse it without another commit. Later normal active
continuation validates the committed body and existing ancestry, not HEAD == P.

No Root-body fallback is added to verification, closure or ordinary source-file
reads. Until materialization is committed, those checkout-dependent operations
still refuse missing bodies. The executor reads the authoritative Root Plan for
planning before start; verification afterward reads the actual chosen source.

Plan19 owns native prepare/start/verification/closure boundary tests and any
necessary adapter correction within those existing responsibilities. Plan20's
Skill integration must teach this exact ordering and narrow interrupted-copy
recovery; Plan23 preserves it under the new configured paths and byte rules.
No immutable Plan or new public command is required. Real Git/public-entry
regressions must cover G-created body absent at P, READY untouched, completed
start then exact body-only commit, successful capture/closure at a descendant,
same-copy/commit lost-response recovery, raw-byte drift/collision refusal and
preserved Root/rejected refs. Runtime checks remain NOT_RUN by this Design.

## Complete graph-only history proof

Require the recorded Root commit to be a real ancestor of current Root.
For **every** intervening commit and its parent:

1. Require exactly one parent and complete actual object/history availability;
   no shallow gap, replace/graft interpretation, rewrite, divergence or merge.
2. Read the full tree delta, not a graph-path-filtered or endpoint-only diff.
   Changed paths must be only the exact two configured graph source/view paths;
   preserve all other product tree entries and modes.
3. Validate each graph version, workspace identity and increasing revision,
   deterministic source/view pairing, and unchanged prepared Plan/body/policy.
   Use that version's compatible deterministic renderer; unknown history fails
   closed rather than skipping validation.

A product/Design/config/Plan change followed by revert still fails at its
intermediate commit. A message containing "graph" is no proof. Use bounded
batched parent/object acquisition and NUL-delimited path evidence; do not spawn
hundreds of one-object Git children or keep permanent stale validation caches.

## Start, scheduling and public contract

Under the existing Root transaction lock, reject stale observer revision and
pending checkpoint first, then validate READY/predecessors/approved effective
policy, held ownership, current preparation and fresh Root/Plan Git evidence.
Record start and its immutable prepared anchor once, then complete its graph
checkpoint before returning execution admission. An unfinished checkpoint keeps
the operation pending and blocks new mutations/starts; it is not rollback.
An ordinary unfinished review is only a per-Plan ownership hold, not that global
barrier; same-operation recovery remains admitted through its typed entrypoint.
IN_PROGRESS/IMPLEMENTED prepare reuse keeps the stored-anchor ancestry rule
and fresh owned checkout checks, not dynamic Root HEAD equality.

Future CLI `plan prepare` has its own mutating request, distinct from read-only
`plan show`: canonical `--id`, caller `--expected-revision`, `--operation-id`,
required `--actor`, and mutually exclusive explicit `--reprepare` /
`--recover-preparation` modes.
These are target flags, unavailable on 0.1.13. Active/reviewable inspection
does not append generations. Reuse still rechecks its supplied observation;
stale callers re-observe rather than substitute a newer revision internally.
CLI currently owns prepare; no existing `mine_plan_prepare` MCP tool is claimed
or new tool required. Existing MCP start/show/wave and JSON DTOs expose matching
generation/checkpoint identities and refusal reasons through shared services.

### Preparation actor attribution

The pending prepare request carries a required non-whitespace `actor`, using
the existing operation actor validation. This is the caller's attributable
declaration, **not authentication**, a scheduling grant, or an ADR approval.
It is provided explicitly by the invoking Skill/agent or human, not inferred
from Git user.name/email, OS login, an empty READY owner, or a future start.
There is no default such as `system` or `unknown`. Missing/empty input refuses
before journal, Git or graph mutation. The typed application request carries
the same value; CLI help and all consuming Skills must agree. Read-only show
does not acquire this argument, and no preparation MCP tool is introduced.

For an operation that creates a generation, freeze the supplied actor once in
both its operation intent and that generation. Native time is captured once,
not refreshed on retry. Same-operation resume must match the original actor,
Plan, mode and original observer/request identity; it cannot relabel ownership
or substitute today's Root baseline. A different actor may perform a fresh,
fully validated no-op reuse using a fresh request identity, but that does not
rewrite the previous generation's actor or create a new generation/checkpoint.
Thus an independent reviewer uses their own identity without impersonating the
preparer. Start's owner/run remains a separate existing contract.

This resolves the previously unspecified input of the already-required
generation actor, within ADR-0019/0022; it adds no graph schema, Plan status or
authority. Plan19 owns request/DTO/help and focused public-entry tests; Plan20
owns all five source/generated/embedded Skill consumers. Installed 0.1.13 is
unchanged. Test missing/blank actor with no effects, exact attribution, changed
actor on same-ID resume refusal, distinct reviewer no-op reuse and preservation
of generation actor/time. These runtime checks are NOT_RUN by this Design.

All five Skills/templates use actual help, returned revisions and same-operation
query/resume. No ordinary lifecycle prompts, manual graph-only fast-forward,
duplicate executor or early global candidate installation. Before graph writes,
Skill-owned Plan/Design/report/product edits must be committed by their owner;
the binary never auto-stages them. Root dev, independent integration/cleanup,
rejection preservation and exact-node mutable-output isolation remain mandatory.

## Required evidence and primary sources

Real Git/public CLI/MCP tests cover prepare B at A -> prepare/start A -> repeated
Root graph checkpoints -> prepare/start B with unchanged B HEAD; simultaneous
conflicting and disjoint contenders; all dirty modes; wrong paths/identity;
change-and-revert, merges and rewrites; legacy recovery; compensation's distinct
Root/prepared commits; active-anchor reuse; interrupted creation/checkpoints;
same-operation resume without extra revision/commit; and old-writer refusal.

Opened 2026-10-09: [Git worktree](https://git-scm.com/docs/git-worktree) supplies
independent checkout evidence; [merge-base](https://git-scm.com/docs/git-merge-base)
proves ancestry, not authorization; [diff-tree 2.45.0](https://git-scm.com/docs/git-diff-tree/2.45.0)
supplies whole-tree deltas and NUL-delimited names. Their composition into this
admission policy is MINE's explicit design, not a Git guarantee.

Reopened 2026-10-14 for compensation materialization: Git worktree and diff-tree
support independent checkout evidence and exact parent/tree-delta inspection.
They do not make a later Root file appear in the earlier rejected commit; the
post-start executor-owned body commit is the explicit MINE handoff.
