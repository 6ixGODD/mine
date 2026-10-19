---
name: mine-plan-review
description: Strictly review one implemented repository plan against AGENTS.md, architecture, the immutable plan, implementation commits, real runtime behavior, verification evidence, and downstream contracts. Use when the user invokes the host-specific mine-plan-review skill with a plan path, asks whether a plan can be accepted, requests an acceptance review, or wants review failures classified into direct small fixes versus a new compensating plan. Work in the current workspace; never trust an implementation report or green tests without independent evidence.
version: 0.2.0
---

# MINE Plan Review

Independently verify actual behavior against the immutable Plan and accepted Design.
Bring narrow, unambiguous defects to a verified mergeable result; independence does
not require another compensation cycle for a small fully verifiable correction.
Do not accept from an implementation report, green label or tests alone.

## Mode, identity and real authority

The exact invocation "complete release closure" selects release mode, not a Plan path.
Otherwise resolve one exact graph ID/path or canonical aliases planNN / PNN / NN,
including numeric hierarchy and terminal -C01, against Root graph show JSON. Strip
one invocation @; require one distinct match and use only the exact stored ID.
Do not guess a path, incomplete C ordinal, latest compensation or another READY node.

Resolve Root from Git inventory; read AGENTS.md, requirements, Root .mine/config.toml/
marker, complete Plan, governing leaves, predecessor acceptance evidence, implementation
commits/report and registered primary sources. Fresh .mine/design/ and .mine/plan/
are defaults, not authority over another configured source. Graph reads/writes use
Root, never a linked checkout's copy. Check installed help/JSON/MCP schema first;
future candidate features must not govern a live older workspace.

Prefer available MCP reads/transitions with their actual required actor/observer/
operation inputs; use JSON CLI for prepare, operations, verification, clarification,
rewire/release and scanner where no tool exists. Never invent tools or hand-edit graph.

For review prepare, use own explicit actor, one retained operation UUID and caller
revision: plan prepare --id <id> --actor <reviewer> --expected-revision <R>
--operation-id <uuid> --format json. Enter the returned registered linked checkout
only after completed checkpoint. Verify actual branch/tip and stored anchor ancestry;
do not create a second checkout or switch Root.

## Independent evidence and economical checks

Obtain the actual executor stop/handoff for exact owner/run and checkout/output.
Silence, timeout, IMPLEMENTED or clean status is not proof the writer stopped.
Keep complete held-owner/wave facts: IMPLEMENTED and pending positive review retain
ownership until verified integration/cleanup/checkpoint completion. Invalid policy
does not remove a held owner or grant conservative fallback.

Build requirement -> Design -> implementation/consumer -> independent evidence trace.
Inspect real entry/producer/consumer/error behavior, raw persisted round trips,
security/ownership boundaries, generated closure and precise negative cases. Check
original and effective responsibility/mapping/provenance, not just diff paths.
Internal file splits within accepted responsibility do not themselves require a new
Plan. Public/persistent/security/behavior/acceptance changes are material.

Read existing exact input-bound receipts with fresh command/source/body/policy/tools/
environment/platform/features and artifact context before launching expensive checks.
Native verification check can exit0 with INVALIDATED/NON_REUSABLE: inspect disposition
and reason. Preserve RUN/REUSED/INVALIDATED/NON_REUSABLE/NOT_RUN, first failures and
warnings. Reuse unchanged applicable evidence; independently probe the central claim
and rerun changed-input/native/independence requirements. Never rerun every full suite
by ritual, call cross-run totals one green invocation or waive a missing native host.

Fix narrow already-specified defects directly with separate reviewer commits and
discriminating regression tests; rerun affected gates, invalidate affected receipts,
and record authorship/rationale. Do not rewrite Plan/implementation history or weaken
a gate. Material Design/core approach/public or persistent contracts/security/
coordination changes require owner-approved decisions and ordinary compensation.
If concurrent dirt makes evidence unreliable, classify it rather than discard it or
falsely attribute another owner's failure to this Plan.

## Acceptance is a recoverable operation

Read [native closure and evidence](references/native-closure.md) before any terminal
closure, including its strict report assertion and request contracts.

1. Finish independent verification and narrow fixes. Write a separate canonical review
   report with unique Verdict: ACCEPTED and exactly one attributable mine-review-closure
   JSON fence. Commit the report on the Plan branch, then pin the resulting reviewed
   tip and report FileIdentity; do not embed a self-referential final tip in its report.
2. With clean Root dev and exact clean registered Plan tip, begin the native acceptance
   intent using current observer and report evidence. Query its persisted operation
   identity. All retries use that same operation and original pinned facts.
3. The reviewer performs ordinary no-ff integration of that exact tip into Root dev,
   resolves conflicts within accepted contracts, preserves frozen bodies, then observes
   Integrated through typed operation stage. Stage records facts; it never merges.
4. Verify the merged source with actual applicable whole-gate evidence and fresh
   context, then record Verified. A failure keeps IMPLEMENTED and held ownership.
5. Recheck still-present Plan ref at frozen reviewed tip, clean tracked/untracked
   checkout, inventory/ancestry and no active output user. Remove only the exact
   accepted merged linked worktree and local branch, without force. Record Cleaned
   with actual typed cleanup facts. Changed tip/dirt/foreign attachment blocks cleanup.
6. Finalize acceptance through plan accept --id <id> --expected-revision <R>
   --operation-id <same-closure-id> --input <typed-finalize-json> --format json
   (or matching MCP mine_plan_accept). ACCEPTED/downstream release is complete only
   after the native terminal graph checkpoint completes. No old --review-only accept,
   manual graph commit, hidden merge step or self-accept is allowed.

After interruption, operation show --operation-id <same-id> reveals persisted stage/
revision/checkpoint. Reobserve actual resources and resume only its unfinished step;
do not repeat a merge/removal, fabricate cleanup from absence, refresh old evidence,
restart review or create compensation for transport failure. Ordinary pending review
is a per-Plan hold; pending graph publication is a global mutation/start barrier.

## Rejection and stopped active Plans

For material failure write a canonical Root-visible independent report with unique
Verdict: REJECTED, exact evidence and the same real stopped-writer assertion. Preserve
the rejected source/ref/worktree; never merge or delete it. If active immutable work
is objectively infeasible, reproduce it and require any replacement material ADR
approval before evidence-bound rejection. Never fabricate IMPLEMENTED evidence.

Use the typed rejected_handoff operation to associate the actual rejection,
next-sibling compensation registration, exact downstream rewiring and release stages.
Each graph write carries explicit actor/current observed revision and completes its
checkpoint. The compensation inherits the rejected node's complete ordered hard
predecessors, not a dependency on the rejected node; same lineage worktree, different
mutable output namespace. Do not execute compensation during review absent a
separate request. Do not edit a frozen Plan or register a duplicate after a timeout.

Legacy rejected history without a native handoff needs the explicit current observation
route described in the reference, not a fictional old journal. Existing operation or
corrupt/missing evidence must be queried/refused rather than replaced. No unrelated
new lifecycle or prompt-only stopped-writer override.

## Complete release closure

Require all Plans terminal, accepted compensation chains and no pending barriers.
WITHDRAWN is retired, not incomplete implementation. Do not re-accept terminal Plans.
Verify final mine-sync Phase A's exact post-integration source/report/coverage and
Design consistency; structural validation alone is not semantic synchronization.
A missing/stale Phase A blocks closure and does not authorize automatic sync.

Discover decisive gates from current user instructions, AGENTS, accepted Design and
actual project tooling. Run native plan-reference scanning directly; no Bash helper
or MINE-specific Cargo/install suite is imposed on other projects. Preserve bounded
fixture exemptions and recursive backup detection, including runtime backups, rather
than exempting all .mine.

Construct exact candidate C in a dedicated isolated checkout from terminal source S,
purging only its configured ephemeral Plan workspace and source-authorized finite
metadata changes. Root stays dev until authorized integration. For MINE's own terminal
layout cutover use its reviewed native staging helper/finite validator; the local
manifest is diagnostic evidence, never a whitelist or approval. Do not force-add
ignored files, renormalize approved ADR bodies or move user docs/.

Begin a native Release binding exact S/C/checkout/policy/genuine Plan attribution.
Candidate check inputs come from actual C; historical bodies/receipts use the
operation's frozen S/evidence roots, not a reconstructed live Plan tree. Follow
Intent -> Preflight -> Integrated -> Complete through typed stage/finalize with
current gate selections. Integrate curated/squashed stable history only after actual
candidate gates; do not plain-merge temporary Plan history. Root may then be stable.
Verify result tree/ancestry, exact artifacts and authorized local cleanup.

An active Release holds a global write barrier. Same-ID query/recovery is admitted,
not unrelated graph commits or a second Release. Safe Intent refusal requires actual
unchanged S/index/stable and no effects; after Preflight/unknown effects use same
operation recovery. mine release --operation-id <id> is read-only diagnostics with
can_release=false, not current gate admission or permission to publish.

Push/tag/upload/package publication/global binary or Skill replacement needs explicit
separate owner authorization. Release review does not automatically upgrade the user's
installed binary. Record exact stable result/version/native evidence and cleanup,
without claiming unperformed CI, unsupported native hosts or publication.

## Output and handoff

Reuse Root environments and immutable caches. Isolate every language's mutable build/
test/generated/package/coverage output by exact Plan node under .mine/build/, even
when compensation shares a checkout. No worktree-local environments/shared parallel
mutable outputs; exceptional measured size requires owner strategy before building.
Never clean another owner's data, force-remove a checkout, reset, blind stash or
rewrite public history.

Report ACCEPTED/REJECTED only after actual completed evidence-bound transition;
otherwise clearly say REVIEW IN_PROGRESS/BLOCKED and current persisted state/stage.
Include own fixes, exact commits, commands/exits/receipt applicability/native gaps,
report path, preserved unrelated work, cleanup and all live reviewable/executing/
ready/blocked/accepted/withdrawn statuses plus next action. A positive report alone
is not ACCEPTED or release permission.


## Canonical durable bounds

All new IDs follow planNN(?:-NN)*(?:-CNN)? (two-digit numeric segments, uppercase
terminal C). Files are <configured-plan-root>/<id>-<lower-kebab-slug>.md.
Compensation uses the next contiguous sibling C ordinal in the same lineage.
Never edit the configured execution-graph.toml or generated execution-graph.md
directly. Root owns graph/config/locks/revision even for linked callers.
For Rust set CARGO_TARGET_DIR to that exact Plan's isolated output class; the
same mutable-output isolation applies to every other language, not just Cargo.
