# Recoverable Evidence-Bound Lifecycle Operations

## Authority and implementation boundary

Accepted target under [ADR-0019](../decisions/0019-recoverable-evidence-bound-lifecycle-closure.md).
Candidate source implements typed closure operations, observer-supplied revision,
strict report assertions and recoverable Git effects/checkpoints. Installed
0.1.13 retains report-based acceptance and its older write contract; do not invoke
candidate operation APIs on that bootstrap installation. Implementation and
fixture passes do not prove this project's actual independent acceptance/release.

## Application and storage boundary

Graph TOML remains Plan lifecycle authority. A small operation service handles
typed begin/query/stage/finalize requests; Skills still perform authorized Git
merge/conflict resolution and invoke repository checks. No arbitrary shell tool,
daemon, workflow engine or extra Plan state is added.

ADR-0022 adds a narrow native exception to Skills-only local Git commits:
[native Root graph checkpoints](root-graph-checkpoints.md) own only the exact
operation-bound graph source/view. All product/report commits and merges remain
Skill-owned. A persisted graph with pending checkpoint is not a completed
operation: global mutation/start/ownership-release/closure barriers apply until
same-operation recovery. The preparation protocol is defined in
[durable READY preparation](ready-preparation.md).

A Root-owned atomic journal under `.mine/runtime/operations/` retains interrupted
progress. It is bound to repository/workspace/Plan identity and has a generated
operation ID, local stage revision, kind, initiating actor, observed graph revision,
Plan/body digest, reviewed tip/report digest, integration result and diagnostics.
Claim one active operation per Plan under the Root graph lock. Runtime stage data
alone cannot authorize acceptance: finalize re-observes authoritative graph/Git,
validates real report/verification artifacts, and persists portable acceptance
evidence with the Plan node in the next versioned graph schema.

Accepted evidence includes operation ID, reviewed commit, immutable Plan identity,
contained review report/digest, integrated commit, applicable verification receipt
identities and observed clean worktree/branch removal. It is purged with the Plan
workspace, never silently shipped into stable. Runtime journals are recoverable
local mirrors; corrupted/missing evidence is an explicit recovery refusal.

## Bounded journal observation

Production barrier/admission/summary reads must not retain all complete
`CheckpointRuntime` values simultaneously: each includes graph before/after
and index bytes, so collecting every `JournalRecord` multiplies peak memory by
operation history. Use a bounded visitation/compact-observation path in the
existing store. Validate the directory/catalog identity set once per locked
observation, then load, fully validate and release each bounded record before
loading the next. Retain only required IDs, barrier facts and typed operation/
closure summaries; fetch one pending record's full runtime by exact ID when
actually resuming it. Do not repeatedly parse the complete catalog per record.

Preserve all existing checks: bounded regular-file reads, catalog/ID equality,
repository/workspace binding, stage/outcome consistency, checkpoint runtime
validation and fail-closed handling of missing/corrupt evidence. A public result
or graph write is not successful until the entire required scan succeeds; finding
one terminal record is not permission to skip later malformed records. A visitor
used for observation cannot perform lifecycle side effects while scanning.
The same existing Root lock/snapshot rules protect admissions. No new persistent
index, retention deletion, schema, long-lived cache or validation bypass is added.

This is a local implementation choice within the accepted bounded-evidence and
pending-barrier design, not a new authority. Plan19 may extend
`src/infrastructure/operation_store/journal.rs` and its direct module exports,
update its `barriers`/`begin` and summary callers to the common read path, and
update `tests/operation_journal.rs` and `tests/graph_checkpoint_kernel.rs`.
Test equivalent barrier/identity/error outcomes with many large journal fixtures,
including corruption at the end and an early pending entry; demonstrate that
full runtime objects are not retained across the scan. Record measured resource
evidence before claiming any speed or memory improvement. A sequential scan
still has history-proportional IO; this change alone does not claim constant-time
admission or solve long-term retention. Runtime tests are NOT_RUN by Design.

## Repository journal history and the next workspace

The Root operation catalog is repository-wide history, not a file to reset when
workspace identity changes. Distinguish **repository observation** from
**workspace mutation authority**. Under the same Root coordinator lock, a compact
repository visitor validates the complete catalog/directory ID set and each
bounded record's schema, repository binding, original workspace identity,
stage/outcome and internal runtime bindings before yielding its summary. It may
read records for prior workspaces of this repository; it must not silently skip
them. A foreign repository, malformed/missing record or inconsistent catalog is
still a refusal, including when the offending record claims to be terminal.

The existing workspace-bound `query`/`begin`/`advance` paths keep their exact
workspace check. Split that check from repository-level record validation;
do not simply remove it from writes. Historical query is explicitly read-only
and returns the stored original workspace identity. The current writer binds to
the actual live graph workspace, or the validated initial checkpoint intent
before that graph exists. Recovering an old pending operation binds to that
operation explicitly, never by relabeling it as the new workspace.

For a new workspace after release, **sealed** means a derived proof from the
existing committed `Release/Complete` operation, not a new status, command or
mutable seal registry. Require the exact prior workspace's completed release,
its bound retained terminal authority manifest/graph, real candidate/stable
result and original repository identity; the retained graph must meet existing
terminal/compensation rules. Verify the release candidate belongs to the actual
stable history and that the owner has established clean Root dev on the current
stable baseline. Do not pick a seal by filesystem time or the last catalog entry.
Any historical predecessor workspace without a valid completed release proof, or
with a nonterminal/unknown operation, blocks admission of a different workspace.
This includes pending Acceptance and Release Intent, not only operations whose
current barrier summary is global. Completion of a Plan/rejection/checkpoint
alone, or terminal Refused records, is not workspace release.

Historical observation validates retained records and their bound release proof;
it does not replay old Git effects, demand removed worktrees, or read an old Plan
path from today's workspace. Completed release does not require its deleted dev
branch to be recreated. Original source commit reachability is required through
closure as specified by release attribution; after completion the validated
retained terminal snapshot plus immutable completion and actual stable-history
proof supplies this admission evidence. Rehash the required retained manifest
and terminal snapshot bytes and validate their original bindings; consuming this
proof is not rerunning the historical release gates. Missing essential retained seal evidence
blocks new admission; never synthesize it from current files or silently erase
the old catalog. This does not add an automatic journal/archive retention purge.

Use workspace-qualified ownership keys internally. Pending checkpoints/unknown
workspace writes remain repository-wide barriers regardless of workspace ID;
an unsealed old workspace's ordinary closure also prevents opening a new one.
Within the current workspace, ordinary review keeps its existing Plan-local
holding semantics. Validated sealed terminal history contributes no current
Plan hold, even when Plan IDs are reused in the next workspace. Catalog writes
preserve every existing operation ID; a global operation-ID collision still
refuses or resolves the exact same immutable operation, never overwrites history.

Fresh initial checkpoint admission records, inside its existing typed runtime
intent, the exact predecessor workspace IDs, completed release operation IDs,
terminal stage revisions and retained authority manifest identities used for
this proof. A required explicit empty
list means no prior workspace history was observed; an omitted/malformed field
cannot mean that. This adds bounded provenance to the absent-before case, not
another registry or graph schema. The predecessor set and newly generated UUID
freeze before graph creation. Recheck the same bindings on resume; do not select
a different predecessor after interruption. Validate all prior workspace groups,
not merely one convenient completed release. A pending initial intent resumes
its own UUID rather than being mistaken for an unsealed predecessor to bypass.
The new workspace UUID must not have appeared in history. Predecessor bindings
form a single succession chain: once a successor intent exists, another open
must resume that exact intent or refuse, never consume the same predecessor
again to fork a second workspace. Later completed successors become the new
frontier; absent graph files do not authorize restarting from an older seal.
Validate catalog continuity and all retained records; missing/corrupt evidence
cannot be treated as an empty history. This local protocol does not claim to
detect deliberate erasure of every local historical artifact.

Plan19 may separate repository observation from workspace-bound access in
`src/infrastructure/operation_store/{journal,mod}.rs`, add the compact summary/
visitor and initial-admission binding in `graph_checkpoint/{kernel,runtime}.rs`,
and wire its existing workspace/release services. No journal deletion, schema
guessing, new graph status or generic cross-workspace write API is authorized.
The initial runtime variant is the already-approved bounded checkpoint extension;
ordinary persisted operation identities remain unchanged. Regression tests must
cover release A -> absent graph -> open B (new UUID, revision 1), subsequent B
writes with intact A history, reused Plan IDs, pending/corrupt/foreign/unsealed A,
missing release manifest, wrong stable history, concurrent B opens, duplicate
UUID/seal consumption, two successive release/open cycles, release source
objects no longer reachable after completed closure, and a lost response while
B still has no graph. Runtime tests are NOT_RUN by Design.

## Revision and outcome semantics

All lifecycle writes accept the caller-observed expected graph revision. Reject
staleness before expensive queries; never replace the observer's revision with
a service's newer one. Return actual before/after revisions from the same
transaction. Each graph mutation increments once; journal stages do not fabricate
Plan transitions or change node status.

Queries distinguish not executed/refused, in progress, committed, and unknown
outcome. A transport timeout is not a failed transition. Retry first queries the
same operation identity and reconciles graph/Git evidence. A real committed write
whose Markdown render fails is committed-with-repair-needed, not rollback.

## Acceptance stages

| Stage | Required evidence / permitted next action |
|---|---|
| Reviewed | IMPLEMENTED, independently positive contained report, exact final reviewed tip and body identity; record the operation before destructive cleanup. |
| Integrated | Reviewer merges that tip into clean Root dev, resolves conflicts within authority, and records the resulting commit. Confirm reviewed tip ancestry, not branch-name equality. |
| Verified | Applicable integrated-tree evidence is green or validly reused under the verification contract; conflict changes invalidate affected checks. |
| Cleaned | Normal removal of the exact owned clean linked worktree, then deletion of the merged managed local branch; record fresh inventory evidence. |
| Accepted | Revalidate graph/review/integration/receipts/cleanup, persist acceptance evidence and ACCEPTED in one graph transaction; release successors only after its required native checkpoint completes. |

Stage names are operation progress, not graph statuses. The Plan remains IMPLEMENTED
and holds ownership through all unfinished stages. Root must have no unrelated
dirty state. Necessary operation-owned graph/report changes are explicitly listed
and committed by the integration owner; they cannot whitelist arbitrary files or
overwrite a path the merge will touch. The ignored journal itself does not dirty
the Git integration tree.

After merge/cleanup interruption, resume by rechecking observed facts: do not
merge an already-integrated tip again or force-remove a dirty checkout. If a merge
cannot be safely completed/aborted, stop with recoverable evidence. Rejecting an
implementation is not the answer to an uncertain tool result or cleanup failure.

## Terminal admission versus post-transition evidence

The acceptance transition has two verification targets, not a self-referential
test fixed point. Immediately **before** the terminal graph write, under the
Root coordinator lock and caller revision, validate current applicable evidence
against the real pre-terminal source inputs. Any required command ran outside
the lock. Retain the exact selected receipt/claim identities and their validated
input manifests, and bind admission to the observed Root HEAD H, graph revision
and exact immutable acceptance operation. Existing checkpoint intent/runtime
records the actual before/after graph bytes and original HEAD/index; do not
invent another independent graph snapshot authority.

Writing ACCEPTED and its evidence identities necessarily changes a full-policy
input graph. **After** this write, verify the exact same operation's authorized
graph transition/render and native checkpoint, not a claim that the old full
receipt matches the resulting full tree. Match the persisted acceptance operation
and evidence to the admitted candidate; prove its actual graph before/after and
commit tree delta are precisely the native transaction on the configured pair,
with unchanged non-graph inputs. Path names alone do not authorize arbitrary
graph edits. Pending publication stays pending and grants no successors.

The production provider must distinguish pre-terminal admission from historical
admission plus post-transition proof. Preserve/rehash actual evidence and verify
the pinned review/integration/cleanup facts in both phases. An exact replay uses
the recorded transaction and checkpoint identity; it neither re-runs checks nor
adds another receipt to the graph. A later unrelated source/policy change cannot
be hidden in this terminal-delta allowance. Missing before-state or a different
operation/candidate/ref/index remains a recovery refusal, not an automatic rerun
loop. Do not embed the final graph/commit digest in an artifact whose digest is
itself stored in that graph; use the existing subsequent checkpoint observation
to bind the produced result without a hash cycle.

Standalone verification remains unchanged: checking the old full-policy receipt
against the post-terminal tree reports input invalidation. Historical acceptance
proves checks at its actual admission boundary followed by the verified state
transition; it is not a portable full-policy cache hit for every later tree.
This phase distinction does not automatically exclude graph files from policy.

## Rejection and release closure

Reject -> register compensation -> rewire -> release remains a sequence of exact
existing graph transitions, associated with one recoverable handoff operation.
Each step retains its preconditions/revision and declared sibling lineage. No
automatic architecture approval, frozen Plan edit or guessed dependency is added.
The rejected branch is not merged; stop its writer before handing its worktree
to compensation. Routine interruption does not manufacture another compensation.

Generic release preflight must perform the validations it names: configuration,
Design/ADR readiness, terminal graph/compensation, pending operations and stable
candidate hygiene. Before stable integration, verify the stored baseline equals
the actual stable tip. After integration, verify the authorized candidate/result
and ancestry instead; legitimate stable advancement is not baseline corruption.
Public push/tag/publish authorization remains explicit and separate.

Post-purge release uses the distinct retained attribution and candidate-source
contract in [verification receipts](../operations/verification-receipts.md#release-attribution-without-a-live-plan-tree).
Only a validated, already-begun Release operation may use that retained terminal
snapshot. Ordinary Plan/graph commands still require their live authority; they
never fall back to archived state. Release Intent/Preflight observes Root dev;
Integrated/Complete may observe configured stable after authorized integration,
checking actual candidate/result tree and baseline ancestry. Do not retain the
kernel's unconditional live-graph/dev requirement for those post-integration
release stages, and do not relax it for acceptance/rejection or other operations.

### Standalone release diagnostics versus gate admission

The standalone `mine release --operation-id <id>` is a read-only, explicitly
selected phase-aware diagnostic for an existing Release operation. It validates
the selected repository/workspace/kind, journal and retained-source binding,
then observes the objective Git/source/evidence conditions applicable to its
recorded stage. It never guesses an operation or falls back to an arbitrary
archive when the live graph is absent. Wrong, corrupt or unavailable binding
is a refusal, not a reason to select a different operation.

Keep this bounded entry point diagnostic-only: do not add another `--input`
gate-admission interface or duplicate StageInput processing here. Existing gate
records may be identified as historical observations but cannot establish fresh
runner context or current applicability. In this operation-selected diagnostic
mode `can_release` remains false, with an explicit explanation that current gate
admission is not performed by this command. It must not label every gate passed
merely because objective Git checks pass or the operation previously completed.
Do not fill unknown facts with fabricated positive values.

The existing typed native Release stage/finalize path, using current StageInput
verification context and fresh observation, is the sole admission path for its
required gates and transitions. A diagnostic false does not contradict a prior
committed Release/Complete record: that record describes completed history, while
this command grants no new release permission. Read-only inspection does not
advance stages, append gate evidence, mint receipts or authorize publication.
The no-operation command retains its live-source preflight scope; it cannot
serve as a post-purge fallback or replace operation-bound closure gates.

Tests must prove operation-selected diagnostics work on retained post-purge
facts without choosing an archive implicitly; stale/foreign/corrupt selection
refuses; historical green claims do not produce `can_release=true`; diagnostic
invocation leaves graph/journal/evidence byte-identical; and the typed native
stage/finalize path still admits valid fresh context and refuses changed inputs.

### Active release barrier and side-effect-free refusal

A nonterminal Release is a repository-wide mutation barrier from persisted
Intent onward, not merely when its outcome is Unknown. Admission under the Root
lock must reject conflicting pending operations before freezing S and publishing
Intent. Ordinary graph writes and new closure/workspace operations observe that
barrier under the same lock; they must not advance the frozen source or create
another active release. Read-only query/status and same-operation verification,
stage/recovery remain available. Same-operation admission excludes only its own
barrier after validating its exact identity and revision, never unrelated pending
or corrupt records. This is a MINE admission barrier, not an OS fence against
manual Git or filesystem changes; fresh observation still detects such changes.
The release owner must not make unrelated Root commits after freezing S.

Expose the kernel's existing initial-stage `Refused` outcome through the native
typed stage/finalize path, without a new outcome, journal schema or release seal.
An explicit same-ID request may terminate Release at Intent only after fresh,
locked observation proves Root remains on dev at S, source graph/config and
tracked index/worktree remain unchanged, stable remains at the frozen baseline,
and no release purge, integration, pending index/checkpoint or uncertain effects
have occurred. Candidate/evidence preparation that predates begin is not a new
release effect. A defective or missing candidate is not itself a reason to
prevent safe refusal when all source/stable and no-effect conditions are proven;
retain the original journal, binding and evidence without cleanup or rollback.
Unknown effects or missing evidence of those conditions refuse this transition.

Persist Refused atomically using the observed stage revision and retain the
reason. Repeating the same completed request is idempotent and cannot reopen the
operation or increment state again; stale/conflicting requests refuse. This
terminal record ceases to hold the mutation barrier, but is neither successful
release nor workspace sealing and cannot authorize publication or a new workspace.
After Preflight, only the existing same-operation recovery path is allowed;
do not label partially executed or uncertain release work Refused. A candidate
replacement requires safe Intent refusal and a separately admitted operation,
never editing the old immutable binding.

Required evidence: a real concurrent graph-write/release-begin race yields one
serialized admission; active Intent blocks ordinary writes and another closure;
query and exact same-operation recovery remain usable; clean Intent refusal
unblocks writes with retained audit; changed S/index/stable, purge or Unknown
effects forbid refusal; Preflight refuses cancellation; stale revisions and
duplicate refusal are safe; Refused is never accepted as a completed release seal.
These tests are Plan19 implementation obligations, not executed Design evidence.

## Local independent-review and stopped-writer assertion

The 0.2.0 production bridge uses the existing canonical review report and its
`ReviewEvidence.report: FileIdentity`; it does not add a writer daemon, PID
registry, process scanner, separate mutable attestation file or journal schema.
The graph records an implementation `owner` and `run_id`, not an OS lease.
Consequently `writer_stopped` validates an attributable local handoff assertion,
not machine proof that an arbitrary process cannot write or resume. Independent
review remains a real human/agent responsibility; actor strings are not external
authentication. This elaborates ADR-0019's existing review/handoff evidence,
without substituting a declaration for observable Git/receipt/cleanup checks.

The reviewer records exactly one fenced `mine-review-closure` JSON object in
the canonical review report, with these required fields and no unknown/duplicate
fields (including duplicate or malformed matching fences):

| Field | Contract |
|---|---|
| `schema_version` | Integer 1 |
| `plan` | Existing typed `PlanIdentity`, including actual immutable body digest |
| `writer_owner`, `writer_run_id` | Nonempty exact original implementation identity from the authoritative graph node |
| `reviewer` | Nonempty independent reviewer; equals initiating operation actor and differs from writer_owner |
| `independent_review` | Boolean true; attests the independent review was actually performed |
| `writer_stopped` | Boolean true; attests the identified writer has stopped and relinquished use of the checkout/output |
| `stopped_at` | Existing validated `UtcTimestamp` of the observed handoff |
| `stop_basis` | Nonempty bounded explanation of the actual executor acknowledgement or controller-observed stop and handoff |

Silence, a timeout, clean Git status, IMPLEMENTED status, or disappearance of a
PID is not sufficient stop evidence. The reviewer must obtain the real handoff
before writing this assertion; unknown or still-active writers block closure.
The stopped writer must not resume against that handed-off checkout. If contrary
activity is known or observed, stop closure and reconcile it; never keep proceeding
merely because the frozen assertion exists. No universal OS process guarantee is
advertised. Existing real output-user locks still gate output cleanup and cannot
be overridden by this assertion.

The report is completed before operation begin and pinned using existing
`ReviewEvidence` (reviewed tip plus report path/length/digest). Do not embed that
final reviewed commit in a report committed by that same commit: the existing
pair supplies the binding without a self-referential commit hash. The begin
adapter parses the report, validates this assertion and the unique expected
verdict against current graph identity under the Root coordinator lock, then
passes the same existing typed operation into the accepted kernel. There is no
parallel unchecked `writer_stopped=true` input that bypasses the report.

Every stage/resume/finalize constructs its production evidence provider from the
same pinned operation and fresh graph snapshot, rechecks the actual report bytes
at the kernel's stage-appropriate source location, and validates the assertion
against that operation's exact Plan/body, actor and original node owner/run_id.
Missing, false, self-reviewed, stale-run, foreign, duplicate, changed or unreadable
evidence refuses progression. A compensation's new owner/run never replaces the
rejected node's identity for this test. Missing historical writer identity is not
filled with the current caller or a synthetic value.

The existing journal freezes the report identity across stages; successful
acceptance retains the same report and digest in Root-visible graph/report
evidence before worktree cleanup. The report must survive rejection handoff too.
No ignored runtime-only declaration can authorize terminal graph state. The
provider must still perform actual ancestry, registered inventory, clean-tree,
verification applicability and exact cleanup checks; it must never return
unconditional success in production. The declaration grants no force-delete,
kill-process, new approval, successor release or independent lifecycle transition.

In particular, after integration and before cleanup, recheck any still-present
Plan branch against the frozen reviewed tip and recheck the registered checkout
for dirty/untracked changes. Root ancestry alone does not detect a writer that
advanced the old Plan branch after its reviewed tip was merged. A changed tip,
dirty checkout or foreign attachment blocks progression; an interrupted removal
is reconciled through the existing same-operation cleanup evidence, not treated
as permission to skip checks. These checks belong to the concrete bridge even
where the earlier kernel only checked integration ancestry at that stage.

Plan19 may implement the concrete provider/parser in its already-owned
application/CLI composition namespaces; existing `OperationFields`,
`JournalRecord`, `ReviewEvidence` and the `writer_stopped` trait remain unchanged.
Add real-entry tests for valid acceptance and rejection assertions, every refusal
above, report retention across cleanup/resume, and unchanged graph/journal on
failed admission. These tests are NOT_RUN by this Design clarification.

## Explicit current handoff observation for legacy rejection

This is a bounded implementation of ADR-0019 evidence-bound handoff and
ADR-0022 explicit legacy recovery, not historical journal synthesis. A migrated
REJECTED node and its already-declared READY compensation may lack a native
handoff operation. Graph status and a preserved ref alone do not prove that its
writer stopped; neither prepare nor start may silently infer that proof.

Use the existing typed `RejectedHandoff` begin/query/stage/finalize route for a
**new current observation**, not a claimed resume of the original rejection.
The concrete begin adapter may admit this exact already-REJECTED case only when:

- Current repository/workspace/revision, original node owner/run, exact declared
  sibling compensation, READY state, completed successor rewiring, body/policy
  and same-lineage locator validate already at begin, not only at later stages.
  The rejected branch still points to exact reviewed source P. Original body
  `None` remains absent; a fresh observed digest is not backfilled as historical
  registration. Existing digests must match.
- The complete operation catalog validates. There is no existing operation for
  this node to resume/reuse, pending checkpoint, corrupt/missing referenced record,
  conflicting hold or unknown effect. Missing records are never classified as
  legacy absence. Existing valid native handoffs use their existing identity.
- An independent reviewer actually rechecks P and obtains the identified writer's
  stop/handoff. The canonical Root report contains the exact existing closure
  assertion with current attributable evidence; a missing original owner/run is
  not replaced by today's reviewer or compensation owner.

For rejection handoff, separate reviewed **source** from retained **report**:
the body/source and preserved ref bind P, while the canonical report is verified
against its exact committed bytes in clean Root dev and frozen `ReviewEvidence`
path/length/digest. A current report need not exist in the older P. This is not a
Root-body fallback, does not move P, and must not set `integrated_commit` merely
to select a different report location. Acceptance's source rules are unchanged.
Apply this explicit rejection-report source consistently at begin/stage/finalize
and preparation consumption; never try arbitrary fallback locations on failure.

If the historical report has no modern pinned evidence/assertion, its independent
reviewer may append a clearly dated current recovery section and one closure
assertion, preserving every original byte and verdict, then commit that exact
report on Root before begin. This is current evidence, not a claim that the old
report contained the assertion. Never edit a report already pinned by an operation
or other immutable evidence. Missing, contradictory or already-pinned unsuitable
evidence refuses with the exact unmet condition; no journal reset or synthetic ID.

The new operation starts at Intent with a new generated ID and the caller's
current observed revision. Advance existing stages one at a time by observing
the exact already-committed rejection/registration/rewire/READY facts, then
finalize Complete. Do not invoke those graph transitions again, fabricate their
old timestamps or jump directly to Complete. This observation changes only its
new journal, not graph bytes/revision, refs, checkout, historical evidence or
preparation generation. Lost responses query/resume this same new operation.

Before initial compensation switching, require a completed handoff plus freshly
revalidated report/identity/stopped-tip evidence. This is a lineage-local safety
gate, not a global scope-admission replacement. For a legacy checkout already
switched by an older preparer, current observation may recognize only the exact
registered declared compensation branch, clean at P, while the rejected ref
also remains P. This is observational admission, not permission to switch,
reset or adopt an arbitrary branch. Start still performs full native admission;
explicit preparation recovery creates only its own fresh generation afterwards.

Plan19 owns this bounded adapter/provider correction within its existing scope;
no new public command, graph/operation schema, role or Plan status is introduced.
Required real-entry tests: missing handoff prepare refusal with no effects;
legacy new observation -> Complete -> explicit preparation -> start; already
switched same-P legacy checkout; same-ID loss/replay; preserved P/original None;
missing owner/run/stop proof, wrong sibling/path/body/report/tip, dirty checkout,
stale revision, missing/corrupt/foreign/pending journals and changed evidence.
Tests remain NOT_RUN by this clarification; installed 0.1.13 is not upgraded.

Reopened 2026-10-16: [Git worktree](https://git-scm.com/docs/git-worktree)
and [diff-tree](https://git-scm.com/docs/git-diff-tree) support inventory and exact
tree observations, not proof of a stopped external writer or invented past events.

## Interfaces, tests and sources

The target operation service exposes typed queries through JSON CLI, with summary
progress in Plan/workspace DTOs. Acceptance and other lifecycle adapters gain
observer revision/operation identity where applicable; new help/schema/Skill
references ship only with implementation. Canonical report path checks remain
real-file, containment and identity checks, not string acceptance.

Inject a failure/lost response after every stage and verify one truthful resume,
no duplicate integration, no premature successor release and no history loss.
Refuse wrong tip/body/report, foreign cleanup target, active writer, stale revision,
dirty/unmerged artifacts and unverifiable receipts. Repeated finalization verifies
the stored operation/evidence and returns the committed result without another
transition; a different operation cannot overwrite accepted evidence.

Opened 2026-10-09: [Git merge-base](https://git-scm.com/docs/git-merge-base)
defines ancestry tests; [Git worktree](https://git-scm.com/docs/git-worktree)
defines inventory and normal clean-checkout removal. A journal never overrides
their observed results.

Reopened 2026-10-14: the same Git worktree documentation specifies clean-removal
and inventory semantics; `worktree lock` protects against administrative pruning,
not arbitrary process writes. The local handoff assertion therefore never claims
an OS writer fence or replaces those observable Git checks.
