# Native Root Graph Checkpoints

## Authority and bounded responsibility

Accepted contract under [ADR-0022](../decisions/0022-durable-ready-preparation-and-root-graph-checkpoints.md).
Installed 0.1.13 still requires its serialized Root integration owner to commit
generated graph changes; do not invoke candidate checkpoint APIs on real Root.
Candidate source implements this protocol. Native completion/recovery evidence
is required; a persisted graph write or implementation label is not completion.

Extend the existing graph transaction and ADR-0019 operation service with one
narrow capability: persist and locally checkpoint the exact configured graph
source/view for that operation on Root integration branch. Every ordinary graph
write uses it, including registration, prepare/start, implementation, acceptance,
rejection/rewire/release/withdrawal, clarification and supported repairs. It is
not permission to commit Plan bodies, reports, ADRs, code, arbitrary paths or a
caller-supplied Git tree. No shell runner, push/tag, branch switch or daemon.

Skills still own authorized product/report commits, merges/conflict resolution,
verification and cleanup. Commit those owned inputs first and observe clean
Root before requesting graph mutation. Never consume foreign index/worktree
changes, change global Git configuration or bypass a required signing policy.

## Transaction and operation stages

Use one Root graph/coordinator lock, not nested acquisition. Under it: reload,
check caller revision and pending graph/render/checkpoint or unknown workspace
write outcomes; capture fresh Root identity,
HEAD/index/worktree; record typed operation intent; validate/persist the graph
once; render the same snapshot; perform the narrow checkpoint and verify its
completion. Bounded Git children use the plan04 adapter. No merge, tests or
long-running unrelated work belongs under this lock.

Operation progress distinguishes intent, Git workspace mutation if any, graph
persisted, render complete, candidate prepared, ref advanced, index published
and complete. These are journal stages, not Plan statuses. Record exact graph
before/after revision and source/view digests, original Root commit/index identity,
candidate tree/commit OIDs, generated index identity, owned lock identity and
stage revision. Persist intent/inputs before mutation; persist newly produced
candidate OID/lock identity immediately afterwards and before ref/index
publication. These results cannot be known before their creation.
Derive operation kind/identity for existing typed graph writes through the
operation service; never accept arbitrary commands or path whitelists.
The [ordinary-write attribution contract](ordinary-write-attribution.md) defines
explicit actor sources, generated ordinary IDs and the existing finalize recovery
entry; it does not add1.0 identities or future bootstrap commands.

No cross-file/ref/index transaction is claimed. A graph write followed by render,
identity, signing, lock, ref or index failure remains committed/pending-checkpoint.
Expose its actual before/after revision and concrete recovery evidence, even if
the public request returns failure. Never report a rollback or completed start.

## Plan Git effects before graph publication

Prepare, explicit reprepare/legacy recovery and unstarted withdrawal compose
existing approved Git effects with this same checkpoint. Their pre-Git intent
must retain the original Plan checkout/ref evidence; Root HEAD/index and graph
before/after alone cannot recover a branch or checkout already changed/removed.
Use a narrow tagged runtime payload in the existing operation journal, not a
second journal, generic command list, graph schema or new lifecycle state.
The existing runtime may reference a private typed structure; its concrete Rust
name and file split are local implementation choices.

### Frozen intent and monotonic progress

Before the first Git effect, under the same Root coordinator lock, freeze:

| Fact | Required binding |
|---|---|
| Operation | Existing ID, actor, kind/mode, repository/workspace, original observer revision and Root checkpoint identity |
| Plan | Exact node ID, body/path/policy identity, original status, lineage, registered ref and contained worktree locator; original generation or explicit absence |
| Baseline | Exact observed Root commit; intended prepared tip; for compensation, exact rejected predecessor ID/ref/tip and declared replacement relation |
| Before state | Explicit absent/present ref with full commit OID; explicit absent/present checkout with Git admin identity, symbolic branch, HEAD, index/tree identity and verified clean/unique inventory facts |
| Intended result | Exact target ref/tip and checkout attachment/absence; intended generation ordinal and timestamp, or withdrawal reason/timestamp; exact candidate graph delta |
| Effect progress | Closed typed steps, each with before/after predicates, durable attempted marker before invocation and reconciled result evidence afterward |

These are semantic minimum facts, not extra caller-supplied paths/OIDs. Derive
them from validated Root registration/config and fresh Git observations. Reuse
the outer journal/checkpoint fields instead of duplicating independent authority.
Store bounded relevant inventory facts, not entire repository contents. Missing,
unreadable, corrupt or unsupported state is not `Absent`; absent legacy
generation/anchor remains absent and is never reconstructed from a vanished ref.
Use checked Git queries returning absent/present/error; a convenience boolean
that turns command failure into `false` cannot establish ref absence.
Clean means verified under existing worktree safety, with no user-owned ignored
files that removal/checkout could lose; unsupported sparse/submodule/filter or
administrative state refuses before destructive effects.

Initial admission validates the full operation, including lossless withdrawal
or non-destructive fast-forward proof, before saving intent. Save the complete
candidate graph/source/view and fixed times once; do not recompute them from a
later Root on retry. No graph write occurs until all required Git effects have
been observed complete. A pending Git effect is a workspace mutation/start/
closure barrier just like pending checkpoint publication. Query/status expose
the operation ID and exact pending step even when graph revision is unchanged.

### Allowed effects and recovery

The closed effect set is limited to create the exact missing Plan ref, attach
its linked checkout, switch the stopped compensation lineage checkout, explicitly
fast-forward an eligible base checkout, remove the exact lossless unstarted
checkout, and delete its exact unstarted ref. An operation uses only the needed
subset in its prescribed order. There is no force reset/rebase, product commit,
Root switch, rejected-ref deletion, arbitrary shell command, recursive directory
cleanup or automatic prune/repair. Pure reuse has no Git-effect sequence.

Before each effect, recheck Root identity, frozen graph/config inputs, relevant
refs, checkout/admin identity, containment and cleanliness. Persist `attempted`
before invoking Git; success or failure of the child process is evidence, not
the final truth. Reobserve before recording completion. On same-ID resume:

- Exact recorded before-state permits retry only that unfinished effect after
  all its live safety checks. A completed step never repeats its mutation.
- Exact intended after-state may reconcile a lost response only when that
  effect was durably attempted, the full bound evidence still matches and no
  conflicting/replaced resource is observed. Mere name/path existence, absence,
  age, matching tip alone or process exit is insufficient.
- Mixed/partial checkout files, changed index, foreign branch/path, locked or
  prunable registration, changed preserved ref, unexpected disappearance or
  unprovable creation identity remains pending with a typed diagnostic. Do not
  erase/recreate it, adopt it as a new generation, or silently undo earlier work.

Create refs with absent-before protection. Before attaching, check both branch
ownership across all worktrees and path absence. After attachment, record the
Git-derived admin identity as soon as it exists; an unrecorded ambiguous partial
creation must refuse rather than infer ownership. Compensation switching binds
the old checkout branch as well as HEAD/index and the new branch; only that
registered stopped lineage may switch. Its initial branch must be the exact
declared rejected predecessor ref at the preserved tip, not any clean branch
with the same OID; the rejected ref remains unchanged.
Base reprepare records old/new commits and proved fast-forward relation; neither
normal reuse nor compensation preparation can silently advance a READY tip.

Withdrawal removes the clean registered checkout first through ordinary Git
worktree removal, then deletes only the frozen target ref with expected-old OID
protection and no symbolic-ref dereference. Lossless ancestry/equality and no
checkout anywhere are still required; expected-old deletion is not a replacement
for those proofs. Never use an unconditional `-D` after an earlier tip check.
Bind the compensation's preserved rejected ref and recheck it around cleanup.
Do not delete branch configuration or other refs through a broad cleanup helper.
An unprepared compensation does not own the predecessor's shared checkout:
its mere presence at the lineage locator is never permission to remove it.
Keep the existing wrong-branch/inconsistent-artifact refusal, not a cleanup
exception for that case.

For an interrupted removal, complete absence of both directory and relevant Git
registration can reconcile only its attempted removal and unchanged remaining
bound facts. A branch still at the recorded tip then permits its guarded delete.
An absent branch can reconcile only its attempted deletion; do not reinterpret
this as a never-prepared legacy node. Partial removal or a replacement directory
is preserved/refused. Once Git effects are complete, publish the frozen graph
once and use the existing checkpoint recovery; never perform cleanup again just
because graph persistence or the final response failed. Return truthful pending
facts on error, not a rollback or a completed withdrawal.

Same-operation query/resume returns/reconciles its stored result without a second
transition; a **new** withdrawal of an already WITHDRAWN node still fails the
existing state gate. Recovery checks original request identity before treating a
newer live revision as stale; it accepts only its own exact before/after graph
and checkpoint progression, never general revision bypass. Missing/corrupt intent
blocks recovery; no inference from Git reflog or a later generation is allowed.

### Integration and evidence boundary

This is the detailed implementation of ADR-0012 lossless cleanup and
ADR-0019/0022 journal-before-effect recovery, not additional Git authority.
Plan19 may extend the existing `graph_checkpoint` runtime/kernel and direct
`operation_store` serialization/validation, Plan/graph service composition,
bounded Git adapter and CLI/MCP operation query/resume mappings plus their focused
tests. Keep ordinary graph-only operations explicit about no Plan Git effects;
never decode missing required intent as an empty safe sequence. Existing journal
versions either validate their original no-effect contract or refuse unsupported
recovery; do not invent a general migration engine. Plan20 consumes the resulting
help/DTO/recovery flow. Frozen Plans and live bootstrap graph remain unchanged.

Real-Git public-entry tests inject failure/lost response before/after each effect,
graph persistence and checkpoint publication: base create/attach, compensation
switch, base explicit refresh, legacy READY without generation/anchor, withdrawal
with no artifacts/branch only/attached checkout, removal then delete failure,
and cleanup complete then graph failure. Assert same operation, exact original
facts, one generation/transition/checkpoint, Root dev and preserved rejected refs.
Negative tests cover dirty/ignored files, ref advancement or symbolic substitution,
worktree elsewhere, replaced/missing admin identity, partial removal/creation,
changed actor/request and corrupt intent. Unknown outcomes stay pending and block
unrelated mutations; unchanged focused evidence need not be rerun gratuitously.
These runtime checks are NOT_RUN by this Design clarification.

Primary sources opened 2026-10-16: Git [worktree](https://git-scm.com/docs/git-worktree)
documents independent checkout/admin state, porcelain inventory and normal clean
removal; [update-ref 2.45.0](https://git-scm.com/docs/git-update-ref/2.45.0)
supports expected-old creation/deletion checks; [switch 2.43.0](https://git-scm.com/docs/git-switch/2.43.0)
documents checkout changes and refusal protections. Use only supported Git
features. These primitives do not transact graph, refs and files together; the
typed intent and reconciliation policy above is MINE's explicit composition.

## First workspace graph is an absent-before checkpoint

Fresh `workspace open` is included in the native graph-write contract. It is
not an exception requiring the Skill to commit the first graph manually, and
must not write an unjournaled seed graph to make the existing kernel constructor
succeed. `mine init` still owns configuration/Design scaffolding, not workspace
graph creation. The Skill commits that scaffolding and establishes clean Root
on the configured integration branch before opening the workspace; the native
checkpoint does not create/switch branches or consume those setup changes.

Extend the same checkpoint kernel/runtime with a typed **absent-before** case,
distinct from present source/view bytes. Absence is never an empty string,
parse error, unreadable file, fabricated revision-zero graph or synthetic
historical workspace. A required tagged runtime representation must distinguish
Absent from Present explicitly; missing/corrupt serialized fields refuse
recovery, rather than defaulting to absence. This is an internal journal detail
of the approved staged persistence protocol, not a new Plan/graph state.

Under the existing single Root coordinator lock:

1. Validate Root configuration/identity, paths, actual integration HEAD H,
   index and clean checkout, actual stable baseline, and pending operations
   **before** admitting creation. Both configured source/view must be absent
   from filesystem and H/index. A lone file, unreadable entry, foreign marker,
   escaping link or corrupt graph is not a fresh workspace. Existing workspace
   handling must first observe pending barriers; it cannot return idempotent
   success while its first checkpoint is incomplete.
2. Require caller observer revision 0 for fresh admission. This denotes the
   observed absence only; generate the real workspace UUID once, build/validate
   the empty initial graph at revision 1, and bind config repository identity,
   workspace UUID, stable baseline, paths and candidate bytes to operation intent.
   Construct the operation store from those Root-validated facts, not by first
   reading a graph that has not been created. No caller-controlled replacement
   repository/workspace identity is admitted through the bootstrap constructor.
3. Persist that intent and complete candidate source/view bytes before either
   graph file appears. Carry before=0/after=1 truthfully. Publish only those two
   files, then reuse the existing render/candidate/index-lock/ref-CAS/index
   publication protocol. The first reachable checkpoint has exactly parent H
   and a tree delta adding only the configured graph pair. No fabricated parent
   graph, extra seed commit or second initial revision is allowed.
   Status validation may admit `??` only for this initial operation's exact
   source/view paths whose bytes match its frozen candidate; every other
   untracked path still refuses. The resulting tree proof remains mandatory.
4. Re-observe ownership immediately before each initial file publication and
   use no-clobber creation for an absent target. If another writer has created
   unexpected bytes, refuse without replacing/deleting them. Under a persisted
   same-operation intent, exact already-published after-bytes may be reconciled;
   a source-only intermediate result is a pending outcome, not a new workspace.
5. Recovery can open/query the same operation while the graph is still absent
   or only one file has been published. Obtain workspace identity and original
   before/after facts from its validated Root-bound journal, never regenerate
   the UUID/candidate graph or infer success from file existence. Preserve all
   ordinary HEAD/index/config/candidate/ref/signing checks and global barriers.
   Once source revision 1 exists but checkpoint is unfinished, reads expose that
   real revision and pending operation; no start or ownership grant follows.
   A missing/corrupt journal or unprovable partial creation is an explicit recovery
   refusal. Later operations cannot replace or silently adopt it.

After completion, repeat open preserves workspace identity and revision 1
without another commit. Concurrent initial callers cannot create two UUIDs or
two reachable initial commits; a stale caller reobserves the winning result,
and same-operation retries use its original facts. A valid existing graph stays
on the existing-present path and its normal revision rules; never reinterpret
legacy or deleted graph bytes as a pristine absent-before workspace.

An authorized completed release is the distinct post-release case, not unexplained
graph deletion. Before admitting a new UUID, apply the
[repository journal and predecessor-release proof](lifecycle-receipts.md#repository-journal-history-and-the-next-workspace).
The repository catalog retains and validates prior workspace history; same-ID
recovery remains available before any graph exists. Freeze that predecessor
proof in this initial operation's runtime intent, then use the same 0 -> 1
protocol above. Do not reset the catalog or bypass a foreign-workspace error
by filtering old records without validating their completed release.

This bounded Plan19 integration correction permits changes in
`src/infrastructure/graph_checkpoint/{kernel,runtime,mod}.rs` and their direct
`operation_store` serialization/constructor call sites only as needed for the
explicit initial case, plus focused `tests/graph_checkpoint_kernel.rs`,
`tests/operation_journal.rs` and the already-owned workspace/native-checkpoint
entry tests. It does not authorize a second checkpoint engine, a general journal
migration system or a change to accepted ref/index publication safety. The live
0.1.13 graph and frozen Plan18/19 bodies remain untouched.

Required real-entry tests: clean fresh open, repeated open, two initial contenders,
pre-existing source-only/view-only or ignored collisions, foreign staging, and
injected loss after intent, each file write, ref advancement and index publication.
Prove exact 0 -> 1, stable workspace identity, one reachable graph-only commit,
same-operation recovery with no graph file, no clobber, and no success/start grant
while pending. Existing-present checkpoint regression remains mandatory. These
implementation checks are NOT_RUN by this Design clarification.

## Standalone generated-view restoration

`graph render` restores a derived projection; it does not perform a lifecycle
transition. Do not feed an unchanged graph to `CheckpointKernel::begin`, invent
a revision increment, or relax that kernel's canonical-before/revision+1 gates.
Pending graph/render/checkpoint or unknown workspace-write outcomes must use the
existing exact-operation recovery path. Standalone render cannot complete,
replace or hide that operation, even if its desired Markdown bytes are known.
Ordinary Plan-local review holds alone are not new global barriers.

The target CLI is `mine graph render --expected-revision <revision> --format json`.
The flag is required, matching observer-supplied mutation/repair revisions; a
missing/invalid value is a usage error. Do not read a newer revision on behalf
of the caller or retry a conflict silently. No `--force`, caller-supplied path,
replacement bytes, actor or new operation ID is required or accepted by this
restoration path. It is CLI-only; no new MCP tool is introduced.

Application contract: `GraphService::render(expected_revision: u64)` returns a
typed `GraphRenderOutcome`, rather than `()`. It clears request-local snapshots,
acquires the existing Root coordinator lock once and delegates to a narrow
`restore_view_under_lock` capability in the checkpoint infrastructure. The
capability borrows that exact lock and takes the expected revision; configured
Root identity/paths and actual Git observations are derived internally. It
accepts neither a caller graph candidate nor an arbitrary filesystem target.
Use the existing bounded Git adapter and shared identity/path proofs; do not
create another checkpoint engine. Concrete private helper/type placement is a
local implementation choice, but the following contract is mandatory:

1. Reload/validate the source and check expected revision under the lock, before
   expensive Git observation. Fully validate pending evidence and reject the
   barriers above, returning the operation ID/stage when available. Do not use
   the stale service snapshot or acquire a second graph lock.
2. Prove actual Root on configured dev, real HEAD H, index tree equal to H,
   no sequencer/conflict/foreign index lock or unsupported Git redirection, and
   a checkout clean except for possible absence of the exact configured view.
   Both graph paths must already be recorded as regular files in H and index.
   The live source must match that recorded source under the supported storage
   transformation, and the recorded view must equal the current renderer's
   canonical projection of that same source. Reject unsupported filters/modes,
   sparse/skip-worktree ambiguity, path escapes, links/reparse points and
   unprovable checkout/storage equivalence before writing.
3. Exact existing canonical view is an `unchanged` no-op after the same checks.
   A genuinely absent view may be `restored`. An existing different file,
   directory, unreadable entry, staged deletion or uncommitted source is not
   missing projection. Preserve it and refuse, even if it resembles an old
   generated view. A renderer-version change making H's view noncanonical is
   not this restoration case either.
4. Build the complete canonical checkout bytes in an owned same-filesystem
   temporary file, propagate flush/sync failure, recheck the bound identities,
   then publish atomically **without replacing an existing target**. Reuse a
   maintained safe no-clobber primitive; the current overwrite-style
   `atomic_write::write` is not sufficient. Do not stream into the final path
   using create-new: interruption could leave a partial view. If another writer
   creates the target, preserve it and refuse (a fresh fully validated retry
   may establish an unchanged no-op). Do not steal unknown temporary/lock files.
5. Reobserve source/revision, Root branch/HEAD/index, canonical view and clean
   checkout before success. On post-publication drift/failure, report that the
   view was published and the observed failure; do not claim rollback or erase
   foreign changes. The Root lock coordinates MINE writers, not arbitrary Git or
   filesystem writers, so no cross-resource atomicity is promised.

Success uses the normal envelope with the actual workspace ID and equal
`revision_before`/`revision_after`; data contains `action` (`unchanged` or
`restored`) and `rendered_path`. Preserve the existing revision-conflict error.
Other refusals distinguish pending recovery, uncheckpointed/legacy evidence and
foreign view content; none returns success or stages/checkpoints those files.
Source bytes, graph revision, index, refs and commit history remain unchanged.
No new operation journal is needed: interruption leaves the target absent or
fully canonical, and a new request freshly proves restoration/no-op from Git.
Temporary-file cleanup is limited to provably owned artifacts; unknown leftovers
are reported rather than broadly removed.

Uncheckpointed legacy source/view or arbitrary changed Markdown remains an
explicit refusal requiring a separately supported bounded repair or the Root
owner's classification of that work. Do not invent a general legacy repair,
`--force` switch or manual discard recipe in Plan19. Initial creation and pending
publication already have their journaled paths; neither uses this helper.

Plan19 may implement this bounded integration in graph service/store,
`graph_checkpoint` and its directly needed safe file-publication helper, plus
CLI args/dispatch/handler, validation diagnostics and focused entry tests.
Keep immutable Plan bodies and accepted kernel mutation guards unchanged.
Test real checkpointed fixtures: missing restore, repeated no-op, stale revision,
pending resume-only refusal, foreign/old view preserved, staged deletion,
HEAD/index/source drift, destination creation race, flush/publication failure,
and lost response followed by no-op. Prove zero source/revision/ref/index/commit
mutation and that concurrent newer graph publication is never overwritten.
These runtime checks are NOT_RUN by this Design clarification.

## Safe Git checkpoint protocol

Use Git-produced indexes/objects; no custom index, object database or pack parser.
The baseline is Root symbolic HEAD on configured dev, commit H, clean actual
index tree equal to H and no foreign staged/unstaged/untracked files. Recheck
after graph persistence, allowing only this operation's exact source/view bytes.
Reject merge/rebase/sequencer state and unknown external Git redirection.
Resolve the actual Root index with Git in a controlled environment, not a
guessed `.git/index` or inherited foreign `GIT_INDEX_FILE`.

1. Build a private Git-produced index from the observed clean Root index,
   preserving its semantics. Prove its initial tree equals H. Stage only the
   exact two configured graph paths into that private index using bounded Git.
2. Inspect the **resulting tree**, not just the add arguments. Relative to H,
   only those paths may change; their actual blobs/modes must match the
   operation-bound canonical source and deterministic view under the declared
   Git storage transformation. Validate checkout/source equivalence explicitly.
   Filters, attributes, skip-worktree or omitted staging must not forge success;
   an unsupported transformation is a pre-publication refusal.
3. Generate one candidate commit with that tree and exactly parent H using
   `commit-tree`. Respect configured identity and required signing. Persist the
   exact resulting OID and generated index identity in the journal before ref
   update; do not regenerate a different timestamped commit on uncertain retry
   after that identity has been recorded or ref CAS has been attempted.
4. Acquire the actual Root `index.lock` with exclusive create. Verify original
   index/HEAD/branch and current graph/worktree again. Write the complete
   Git-produced index bytes into **that held lock file**, flush/close them and
   retain operation ownership evidence. An empty marker lock plus overwrite
   from another file is not the Git lockfile publication protocol.
5. Update only `refs/heads/<integration>` with `update-ref NEW H` expected-old
   compare-and-swap. Afterwards recheck symbolic HEAD/branch, actual ref,
   operation-owned graph bytes and foreign worktree/index changes, including
   changes made by a reference-transaction hook. An index lock does not lock
   HEAD or ordinary file writers.
6. Only while those identities remain exact, atomically publish the held
   `index.lock` itself over the Root index. Recheck commit/tree/index/branch and
   clean Root before marking complete and returning success. Cross-platform
   publication uses safe maintained primitives, never business-code `unsafe`.

Sparse/split index or other index extensions must be preserved through supported
Git-produced representations or rejected explicitly before ref mutation. Prove
the private index is self-contained or preserve/verify its referenced shared
index dependencies until publication/recovery completes; never delete those
dependencies as temporary garbage. No manual byte-level index interpretation.
Normal product commit hooks are not a general code-execution API of MINE;
machine checkpoint semantics and signing failures must be visible, not silently
weakened by disabling repository policy.

## Pending barriers, recovery and cleanup

Pending render/checkpoint is a workspace-wide mutation barrier, not only a
prepare error. Show/status/wave/release/workspace DTOs expose operation, stage,
actual graph revision and required recovery. Reads report the authoritative TOML
but cannot convert pending evidence into execution or release authorization.
Scheduling grants no new owner and releases no effective held owner on a
pending terminal write. Workspace purge/release closure refuses until reconciled.

Ordinary long-running review/integration operations are not global barriers;
they retain only their Plan's effective ownership. A disjoint Plan may start
while another review is incomplete if Root and all native write outcomes are
clean. Same-operation query/resume can enter its typed recovery path; the
pending barrier must not block its own reconciliation.

Reuse existing typed operation show/stage/finalize for same-identity resume.
Resume validates the journal against exact graph/Git/index/ref observations:
H or recorded candidate commit, original or recorded published index, and exact
operation-owned lock. Reconcile completed stages once, without duplicate graph
transition, reachable checkpoint commit or generation. If candidate creation
succeeded but its OID was not journaled, recovery may rebuild only after proving
no ref CAS was attempted and ref still equals H. Unpublished unreachable objects
are not duplicate reachable checkpoints. Once the OID is recorded or CAS
attempted, never change that candidate identity. Interrupted lock creation with
unprovable ownership leaves an unknown-lock refusal, not permission to steal it.
If ref advanced but index publication failed, retain that fact; do not reset
the ref or publish into a changed Root checkout.
If identities disagree, remain pending with a typed refusal and evidence.

Never delete/steal an unknown lock, infer ownership from age/PID disappearance,
undo foreign staging, regenerate a recorded candidate after a lost response, or
create compensation merely because transport failed. Clean only exact owned
temporary artifacts after completion or a provably stopped safe recovery.
Missing/corrupt journals are explicit recovery blockers, never authorization.

## Verification and primary sources

Inject failure/lost response at each boundary with real Git repositories:
creation, graph/render, candidate generation, lock acquisition/write, ref CAS,
post-ref observation, index publication and final response. Verify same-operation
resume, zero duplicate commits/transitions, truthful pending graph states, and
global scheduling/closure barriers. Cover foreign staging/untracked files,
unknown index.lock, changed Root HEAD/ref, hooks, filters/skip-worktree, split
index, signing/identity failure and successful clean checkpoints on all supported
native platforms. The protocol does not promise isolation from malicious
uncoordinated file writers; it detects drift and refuses without discarding work.

Primary sources opened 2026-10-09, supported Git 2.45.x features:

- [Git lockfile API](https://git-scm.com/docs/api-lockfile/2.2.3): exclusive
  lock creation, write into lock, same-filesystem rename publication; writers
  coordinate but readers do not take that lock.
- [rev-parse 2.45.0](https://git-scm.com/docs/git-rev-parse/2.45.0): resolve real
  Git paths including relocation variables and split-index locations.
- [commit-tree](https://git-scm.com/docs/git-commit-tree),
  [write-tree](https://git-scm.com/docs/git-write-tree),
  [read-tree](https://git-scm.com/docs/git-read-tree) and
  [Git environment](https://git-scm.com/docs/git): Git creates tree/commit/index
  data and `GIT_INDEX_FILE` selects an isolated index.
- [update-ref 2.45.0](https://git-scm.com/docs/git-update-ref/2.45.0): expected
  old OID guards the named ref, not a cross-resource transaction.
- [hooks 2.45.0](https://git-scm.com/docs/githooks/2.45.0): reference transactions
  can run hooks; successful ref CAS is not proof that Root HEAD stayed on dev.

Reopened 2026-10-14 for initial workspace admission: commit-tree, update-ref 2.45.0
and the lockfile API support using the same real parent commit, expected-old ref
check and index publication for a tree that adds the graph pair. None makes
graph-file creation and ref/index publication one atomic transaction; the
absent-before intent and same-operation recovery remain necessary.

Opened 2026-10-14 for standalone projection restoration:
[git-restore 2.43.0](https://git-scm.com/docs/git-restore/2.43.0) distinguishes
worktree restoration from index restoration and permits explicit source trees.
It does not supply this protocol's no-clobber/admission proof; a blind restore
command is not the implementation. The lockfile API above likewise documents
cooperating-writer publication, not a lock against arbitrary worktree edits.
