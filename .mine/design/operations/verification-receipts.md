# Input-Bound Verification Receipts

## Authority and implementation boundary

Accepted target under [ADR-0020](../decisions/0020-input-bound-verification-evidence-reuse.md).
Candidate source implements receipt validation/reuse and CLI capture/record/check
adapters. Installed 0.1.13 does not provide these typed guarantees. Reports remain
observational evidence, not native prerecords or a manufactured release authority.

## Ownership and receipt data

Repository policy selects required checks; the receipt service does not invent
a universal Rust/Python/Node suite or become a test orchestration engine. An
executor/reviewer launches the actual command and records its observed result.
The validator checks attributable inputs/artifacts, not cryptographic proof of
execution by an untrusted actor.

A schema-versioned receipt records generated receipt ID, repository/workspace
and exact Plan identity, actor, command/ordered argv/cwd, start/end times, exit
code, outcome, policy identity, input-manifest digest, toolchain/dependency-lock
identities, platform/architecture/features, relevant environment identities,
and contained evidence paths/byte lengths/SHA-256. Environment secrets are never
stored; secret-dependent or unobservable inputs make reuse unavailable.

Store portable receipts and bounded decisive logs with the Plan's report evidence,
under `<configured-plan-root>/reports/verification/<plan-id>/`. Executors produce them in their
linked checkout; the reviewer/integration owner preserves the required evidence
in Root before cleanup. Large disposable outputs stay under exact Plan-node
`.mine/build/` namespaces and are not retained merely to prove a green result.
Missing artifacts are explicitly non-reusable, never presumed passed.

## Input key and invalidation

The default manifest conservatively hashes the full relevant repository input
tree, including relevant untracked/ignored inputs, policy and command selection.
Exclude only declared receipt/output locations and demonstrably non-input files;
otherwise records would include themselves. Git commit identity is context, not
the cache key. Content/mode/relative path, not mtime, defines file identity.

`FileVerificationStore` omits otherwise-empty ancestor directories created only
by excluded outputs, including its configured verification store. It retains
an ancestor containing real inputs and ordinary empty directories outside those
output ancestors. Writing receipts therefore does not invalidate their own Full
key; nearby real prose, directories, configuration and dependency locks still do.

A repository may narrow the manifest through explicit validated check policy.
That policy must prove graph/reports/prose exclusions are not read by the command
or any generator/tool it invokes. A graph-only commit therefore does not require
a new full suite when the check's complete input key is unchanged. An identical
curated stable product tree may reuse the same evidence under the same proof.
Undeclared external data, mutable HOME or network state makes a check non-reusable.

Toolchain/features/dependency/config/generator-input changes, conflict resolution,
packaging inputs and platform changes invalidate affected checks. Platform evidence
does not substitute for a separately required platform. A changed input policy
invalidates old keys. Failed/skipped/timeout/warning-disqualified runs are not
successful receipts; repository policy, not the receipt, defines decisive warnings.

## Coordinator locks are infrastructure outputs

Root-owned `.mine/locks/` is mutable operation infrastructure, like the existing
`.mine/runtime/` and `.mine/build/` output namespaces. It is not a source input
to `InputScope::Full`. Exclude that exact directory and descendants before
opening or hashing their files, including when a Root integrated check observes
fresh inputs while holding the coordinator lock. Creating/acquiring/releasing
the lock or changing its bytes must not change the source manifest, including
through otherwise-empty ancestor directory entries. This applies on every
platform, not as a Windows-only catch-and-ignore of a sharing/lock error.

This is a bounded correction of the existing non-input/output classification,
not a new policy escape hatch. Do not exclude arbitrary `*.lock`, sibling paths
such as `.mine/locks-extra`, all `.mine/`, actual dependency lockfiles, config,
verification policy, Design or graph inputs. Those remain governed by the normal
Full/dependency-closure rules. Preserve path containment and platform alias
checks; spelling/case aliases must not admit the reserved namespace as input.

An explicit dependency root, generator input, dependency-lock path, dependency
audit or command cwd inside `.mine/locks/` is refused, not silently dropped to
produce an apparently valid narrower key. Check the reserved path before file
identity/content reads, so an explicit reference to a held lock returns the
typed scope refusal rather than an OS read error. A root such as `.mine/` may
still cover its legitimate inputs while the reserved descendants are pruned.
Checks intentionally dependent on live coordinator state cannot claim that state
is covered by a reusable source key; retain their separate runtime evidence.

Keep the existing Root lock held throughout lifecycle admission and its fresh
manifest/context validation. Do not capture before locking, release/reacquire
the coordinator, reuse the capture as current context, skip fresh validation,
or treat unreadable ordinary input files as absent. Existing receipts remain
immutable and go through normal exact-key applicability checks; do not rewrite
old manifests or relabel them green after changing this classification.

Plan19 may make this correction only in
`src/infrastructure/verification_store/filesystem.rs` (the reserved-output set,
its existing traversal/explicit-scope admission order and directly necessary
helpers), `tests/verification_receipts.rs` and the existing real public-path
regression in `tests/lifecycle_operations.rs`. No lock primitive, graph schema,
receipt schema, policy threshold or public request contract changes are needed.
Test manifest equality before/while/after a real coordinator lock; lock-only
directory creation; explicit root/generator/dependency-lock/audit/cwd refusals;
nearby genuine input changes still invalidating; and the Windows CLI Verified
positive path with the lock held. Preserve the original failure log and rerun
the focused failures first. These runtime tests are NOT_RUN by this Design
clarification; no successful runtime result is inferred from Design validation.

## Role and output semantics

Execution, review, sync and release report each check as RUN, REUSED, INVALIDATED,
NON_REUSABLE or NOT_RUN with original receipt and reason. A reuse decision is
read-only and launches no test process. On missing/invalid evidence, run the
relevant check; do not weaken it or claim the complete lifecycle passed.

Independent review still inspects contracts and performs new risk-selected probes.
A Plan/policy that explicitly requires independent or native-platform execution
cannot be satisfied solely by the implementer's receipt. A full suite is not
automatically repeated merely because the actor or lifecycle phase changed.

No mutable output directory is shared across Plans to obtain a cache hit.
Toolchain/dependency downloads may be shared only when read-only or safely
content-addressed. Compensation keeps the lineage checkout but gets a fresh
mutable-output namespace. Exceptional disk cost still requires explicit owner
agreement on serialization/exclusive output and cleanup.

## Native capture, record and current-context bridge

This is the 0.2.0 adapter contract under ADR-0020, not a command available in
installed 0.1.13. The accepted kernel already supplies `CaptureFields`,
`RunObservation`, `VerificationContext`, and `VerificationService`; the adapter
must use these types and their validation rather than inventing another receipt.
The default live policy is **`.mine/verification-policy.json`**, already owned
by the verification implementation. No second policy under Design is introduced.
Policy identity uses the kernel's validated serialization/digest, not an
incompatible adapter-specific hash convention.

Target CLI and strict JSON request bodies (`schema_version` is 1; reject unknown
or duplicate fields, malformed identities and oversized/non-regular inputs):

| Entry | Request fields beyond schema_version | Result |
|---|---|---|
| `mine verification capture --input <json-file>` | `plan_id`, `actor`, `check`, optional `policy_path`, `context: VerificationContext`, `started_at: UtcTimestamp` | Native `CaptureFields` in the JSON envelope; no command execution or receipt publication |
| `mine verification record --input <json-file>` | `capture: CaptureFields`, `observation: RunObservation`, optional `policy_path` | Native record decision and generated receipt identity; RUN describes an observation, not necessarily a pass |
| `mine verification check --receipt <relative-path> --input <json-file> [--policy <relative-path>]` | `plan_id`, `actor`, `check`, `context: VerificationContext`, optional `independent_of` | Native applicability decision/reason; read-only, no test or version-probe process |

An omitted policy selector resolves to the default above; an explicit selector
must resolve to a contained regular current policy file. CLI JSON envelope
success means the request was processed, not that a verification gate passed.
Consumers must inspect disposition, reason and original outcome/exit code;
INVALIDATED, NON_REUSABLE and NOT_RUN never become a green check by exit-code
interpretation alone. Malformed/unsafe requests use normal error envelopes.

The invocation's actual checkout is the source tree: Root for integrated checks,
or the exact registered Plan worktree for execution checks. Resolve that checkout
and its registration independently of the input JSON. Relative command cwd,
policy, manifest and receipt paths are rooted in this source checkout; Root
repository/workspace/Plan identity and lock authority remain separate. Refuse
a foreign/unregistered checkout or a different Plan's linked worktree. This
does not grant a caller permission to write arbitrary evidence into Root.

A compensation's newly registered body may be absent at its rejected-tip
preparation anchor. The executor completes the
[post-start exact-body handoff](../governance/ready-preparation.md#compensation-body-materialization-after-start)
before capture or closure. Keep actual checkout/reviewed-tip body checks; do not
substitute Root bytes for a missing body or ordinary source file.

### Root-owned control request files

For these three verification commands, `--input` names a **Root-authority-relative
control request**, not a source-checkout-relative input. Its owner is the current
Plan's executor/reviewer runner. Store request JSON, exported captures and mutable
current-context/observation JSON under
`<Root>/.mine/build/<exact-plan-id>/verification/` (or another output-class child
of that same exact Plan namespace). A compensation uses its own node ID, not the
base lineage ID. This remains true when the command is invoked from the linked
checkout; do not create a second output tree there or put mutable request files
into source merely to satisfy the parser. The binary reads the supplied request;
it does not invent observations, run checks or create an output runner.

The CLI path remains a contained `RepoPath`, for example
`.mine/build/plan01-C01/verification/check-request.json`. It is resolved only
against the independently established authoritative Root, never against cwd,
the source checkout, an absolute caller path or a fallback search. Before reading,
validate the reserved per-Plan output-path shape, containment and regular-file/
non-link/bounded-read requirements. Strictly decode once, then resolve the actual
registered Plan: `plan_id` for capture/check, `capture.plan.plan_id` for record.
Require that canonical Plan identity and its exact output namespace agree before
using the request in verification or writing a receipt. Foreign Plan namespaces,
traversal, aliases that escape containment, malformed/duplicate JSON and missing
Root requests refuse; a same-named linked-worktree file never rescues the call.
Existing current-source registration, owner/actor and receipt identity checks
remain mandatory. No JSON field, schema version or CLI flag changes are needed.

This distinction applies only to the external control request and runner-owned
mutable output. `--receipt`, `--policy`, JSON `policy_path`, command cwd, ordinary
source files, manifest/dependency inputs and portable retained artifact paths
keep their existing source-checkout semantics. In particular, the strict stored
receipt reread used for known-invalidation checks stays source-relative; do not
bulk-replace every `input(...)` helper call with Root resolution. A mutable
request or exported capture is not itself portable receipt evidence and cannot
substitute for the existing source/evidence checks. `.mine/build/` remains outside
the source manifest, so emitting these requests does not invalidate the run.

Native verification implements localized request-path routing/Plan-namespace checks in
`src/application/native_verification.rs`, using existing strict containment/read
helpers; only a directly needed narrow helper in `native_evidence.rs` is permitted,
not a global change to that shared helper's root semantics. Update the existing
verification entry fixtures in `tests/lifecycle_operations.rs`. Matching
Skill examples preserve the same Root/output authority after layout
cutover. Tests must invoke capture/record/check from the real linked checkout
with only Root control files, prove source body/policy/artifacts still come from
that checkout, reject foreign namespaces and worktree-local fallback, and prove
request emission leaves Full input keys unchanged. These tests are NOT_RUN by
this Design clarification.

### Capture, record and check observations

For capture, resolve canonical Plan identity and current body from Root, confirm
the checked-out body matches, load the current policy, select command/argv/cwd
from its named check, then call `capture` and export. Do not accept caller-supplied
manifest hashes or command overrides in this request. The runner must capture
before running, hold inputs/context stable during execution, and retain the
capture separately from the source inputs (Plan-owned mutable output namespace).
These local timestamps/context values are trusted runner observations, not
cryptographic execution attestation. Endpoint equality cannot detect an
edit-and-revert during a run; never claim it can.

For record, independently resolve current Root identity and source body, reload
the selected live policy, and reject mismatches with the captured Plan/policy
before importing the capture. Preserve its original pre-run manifest/context;
never create a replacement capture after execution. Use existing artifact
containment/hash/retention checks and output leases; the runner retains decisive
logs before record. Source changes during execution remain non-reusable through
the kernel's `inputs_stable` fact. A refused record leaves the runner's original
capture/logs available; it does not authorize fabricated success or lost evidence.

For check, construct the request from the current Root Plan and live policy,
not from receipt content. The receipt path must be the exact canonical evidence
path for that Plan and parsed receipt ID; validate path/ID agreement before
calling the native store. Missing/corrupt safe evidence follows the kernel's
non-pass decision. The input context must be freshly observed by the caller's
current runner environment, independently of the stored receipt; do not fill
missing tools/platform/features/environment from old evidence or the capture.
Unknown external state is explicitly unobservable, never an invented hash.
An independently required check binds `independent_of` to the actual implementer
identity in lifecycle integration; a caller cannot weaken that requirement by
supplying another actor. Policy-selected native-platform checks remain required.

Lifecycle verification consumers use the same current-context service adapter.
Stage/finalize requests must supply the current runner observations needed for
their selected checks; an old operation/receipt's context is not a default.
Current policy/check coverage is validated for the gate, not merely one green
receipt supplied by a caller. A fresh non-reusable run can count only as freshly
observed evidence for that gate under the actual policy, never as REUSED.

Plan19's existing CLI/output/application composition and tests own this bridge;
small new adapter modules under those owned namespaces are permitted. No kernel
schema extension, new MCP tool, graph migration or arbitrary command runner is
needed. Production tests must cover capture-before-run then record/check, changed
policy/source/context, forged receipt/path, omitted context, independent actor,
failed/timeout/warning observations, and zero check-command launches on reuse.
All such implementation checks are NOT_RUN by this Design clarification.

## Fresh observations at a lifecycle gate

A stored receipt with unobservable inputs cannot establish present applicability.
The production lifecycle adapter may accept a **newly performed run for this
specific gate**, under ADR-0020's local trusted-runner model. This is not a
`fresh=true` override, a time-based cache window, or a new REUSED outcome. The
runner observes the operation/gate before capture and execution, then submits
the resulting evidence; it must not retrospectively label an unrelated old run.

At a new verification gate, each current check selection contains its receipt
`FileIdentity`, independently
observed current `VerificationContext`, live policy/check selector, and optional
`fresh_observation` claim identity. Without a fresh claim, only an actual native
REUSED decision can satisfy that selection. The optional claim is a strict,
bounded JSON document with these fields (unknown/duplicate fields refuse):

| Field | Required binding |
|---|---|
| `schema_version` | 1 |
| `operation_id` | Existing operation being advanced/finalized |
| `target_stage`, `target_stage_revision` | Existing typed stage and exact proposed operation stage revision being verified, not a caller-chosen new gate |
| `expected_graph_revision` | The caller's current observation required by this mutation; never silently refreshed |
| `check` | Exact check selected from the current live policy |
| `receipt` | Exact canonical stored receipt `FileIdentity`, including bytes/digest |
| `capture`, `observation` | Original `CaptureFields` and actual `RunObservation`, unchanged from that run |
| `attested_by`, `attested_at` | Actual runner actor (equal to capture/receipt actor) and validated UTC timestamp after the run |
| `statement` | Literal `executed_for_this_gate`; a trusted assertion that capture preceded execution for the named gate, not a cryptographic proof |

The selected operation's Root repository/workspace/Plan/body/integration identity
binds the claim through its ID and captured Plan; compare those real facts, not
just strings supplied by the claim. The target stage/revision is taken from the
already-validated lifecycle transition (or its exact replay); a caller cannot
mint an arbitrary revision to make old evidence fresh. Require ordered capture
start <= observed end <= attestation time and no future observation relative to
the current host clock. Clock inconsistency refuses rather than inferring a run.
Time values are consistency checks, not freshness proof; no maximum-age heuristic
can replace the explicit gate identity and trusted runner assertion.

Before counting RUN, independently verify **all** the following, without changing
or re-recording the original receipt:

- Actual receipt bytes/path/ID and `StoredVerification` internal bindings are
  valid. Claim capture/observation match stored Plan/actor/check/policy/command,
  pre-run manifest/context, times, exit/outcome and retained artifact identities.
  Warning-disqualification is derived by the actual selected policy, not ignored
  because observation.outcome says passed.
- The current Root Plan/body and live policy equal the captured/recorded ones;
  stored `inputs_stable` is true. Recompute current source/lockfile manifest and
  all artifact hashes/containment/limits. Compare actual current runner context
  against the run context; preserve secret/unobservable markers, never erase
  them or invent values. Source/policy/context drift is not this exception.
- Exit is zero and effective outcome is Passed. Failed, skipped, timed-out,
  warning-disqualified or changed-during-run evidence cannot satisfy this gate.
- Enforce independent-run actor against the real graph implementer and native
  platform requirements even when the generic reuse checker returns early.
  The only missing reuse property admitted here is external-input observability;
  an early `NON_REUSABLE/unobservable_inputs` decision alone proves none of these
  other properties and is never sufficient for admission.

Retain the claim as a separate immutable regular file containing strict JSON,
named `<generated-uuid>.gate-claim`, directly beside receipts in the Plan's
canonical verification evidence directory. Do not use a `.json` suffix or a
subdirectory: the accepted store interprets every `.json` sibling as a receipt
and rejects non-file entries during output cleanup. Existing bounded file and
retention limits apply to claims too.
The Skill/runner owns creating/committing this evidence before stage mutation;
the binary checks its declared FileIdentity and never trusts a mutable runtime
sidecar. Include both receipt and claim identities in the existing operation
`verification_evidence` list and retain them in Root through cleanup. No new
`OperationFields`, receipt or journal schema is required. Do not insert the
claim into the referenced receipt's own artifact list: the claim contains that
receipt's digest, so doing so would create a hash cycle. The check request names
the claim identity rather than duplicating a different unbound inline assertion.

Current-gate selection and historical evidence are different responsibilities.
The operation's evidence list is append-only. Validate every retained file's
bytes and internal identity, including old claims, but count only the request's
explicit current selections toward complete current-policy check coverage.
Historical stage claims remain attributable history, not fresh evidence for a
later stage. Their previous policy/context need not equal today's to remain
history; they cannot satisfy today's gate unless the underlying receipt now
qualifies through normal REUSED evaluation or there is a genuine new run.
The adapter validates the whole current selection/coverage before allowing the
per-file provider to succeed; recognizing an old artifact must not create an
always-valid production seam. Preserve file validation on every resume.

Do not create a new verification gate merely because a journal stage advances.
For acceptance, Verified is the integrated-check admission; Cleaned adds cleanup
facts and finalize revalidates that admitted evidence and observable input/
policy/context identities. They do not automatically demand another execution of
every unknown-input check. Retained Verified-stage evidence establishes that the
required run occurred at that gate, not that unknown external state remained
unchanged or a new run occurred at Cleaned/Accepted. Known drift invalidates the
affected evidence; an explicitly required later fresh/independent check creates
its own current selection and needs a real new run. Use the existing same-stage
progress/recovery mechanism for necessary refreshed evidence; never move an
operation backwards or change pinned review/integration identity. If refreshing
would require those frozen identities to change, refuse rather than repair them
through an evidence flag. Finalization still requires all its existing objective
Git/graph/cleanup gates; retaining a historical run is not a substitute for them.

An exact replay reports the previously recorded stage/run, not another execution.
Changing gate, graph observation, check or receipt requires a new attributable
observation; do not rewrite an old claim. A stale observer revision alone is not
an input change: reobserve current facts and, when this is still the same
unadmitted gate/run with matching complete observable keys, submit a newly
attributed claim for the current revision without calling it a second execution.
This does not assert that unknown external inputs stayed constant, relabel an
unrelated old run, or satisfy a different gate. If unknown external state prevents
reuse at a later required gate, report the specific check needing a fresh run,
not an automatic whole-suite rerun and not assumed continuity. No check executes
under the Root mutation lock. Standalone `verification check` remains conservative:
unknown inputs with otherwise matching observable evidence return NON_REUSABLE;
known current source/context/dependency/artifact invalidation still returns
INVALIDATED, including the post-terminal full-tree change. The native wrapper
must not let the kernel's early unobservable-input result hide that known drift.
This additional observation can only invalidate, never grant REUSED or a fresh
RUN. If it reloads stored evidence, validate and bind it to the same receipt and
request/policy/manifest identity already examined; do not mix two file versions.
Only the lifecycle admission result reports the supplied fresh RUN and references
its claim. Terminal query is read-only history, never a new gate pass on an
unrelated tree.

Plan19's concrete application provider/current-check DTOs own this bridge and
its whole-gate coverage validation, using the existing kernel types. A bounded
comment clarification on `VerificationEvidenceProvider::validate` may distinguish
historical artifact validation from current-gate admission; no unconditional
provider, signature/schema expansion or invented current context is authorized.
The comment lives in `src/application/operation_service/service.rs`; this exact
documentation-only clarification is included in the bounded integration scope.
Required tests include successful unknown-input fresh RUN, missing claim refusal,
cross-operation/stage/check/actor/revision replay, tampered capture/receipt/claim,
false fresh declarations with failed/warning/changed-input or wrong-platform runs,
independent-run mismatch, later-stage history retention without counting it,
incomplete current coverage, exact replay without a second RUN, and no mutation
on failed admission. Test normal output cleanup with retained gate-claim files as
well. These implementation tests are NOT_RUN by this clarification.

## Release attribution without a live Plan tree

Release verification separates three concrete roles: **Plan attribution** from
the real frozen terminal development state, **command inputs** from the actual
curated candidate, and **retained evidence** outside the tree being shipped.
This is the phase-aware ADR-0019/0020/0021 release bridge, not a synthetic Plan or
an automatic policy projection. No Plan/graph/report is recreated inside stable.

Before removing the live Plan workspace, the release owner must:

1. Validate/commit terminal development state and retain its exact source commit
   S, source configuration, repository/workspace identity, graph revision,
   real graph/render, actual
   selected Plan bodies/identities, acceptance/compensation evidence and decisive
   receipt/log/claim files. Select genuine governed Plan identities explicitly
   for attribution; never guess a replacement identity or an empty Plan body.
2. Freeze a bounded typed release evidence manifest under
   `.mine/runtime/release-evidence/<release-operation-id>/`. It binds schema 1,
   operation/repository/workspace IDs, source commit S, actual terminal graph
   revision, original logical paths and file identities, selected Plan identities,
   candidate verification policy identity, and the existing release binding's stable
   baseline and candidate commit/tree identities. Derive these from real files,
   config, graph and Git; caller hashes alone are not observations. Bind the
   manifest's contained `FileIdentity` through the existing release operation's
   evidence list before purge. Copies retain original bytes/digests; the archive
   is a retained evidence source, not a new writable execution graph.
3. Verify the export against S and actual retained file bytes. Keep the original
   development commit reachable through the existing development history until
   closure completes, or refuse if it cannot be read/proven. No hidden ref or
   new branch is created by the verifier. Missing/corrupt/foreign evidence blocks
   purge/closure; it does not authorize reconstructing approvals or Plan bodies.
   Do not copy secrets or arbitrary ignored trees into evidence. Required inputs
   not recoverable from legitimate bounded evidence make reuse unavailable.

The frozen manifest inventories the original exported facts. Later real candidate
receipts/claims are added through the existing append-only operation evidence
identities; never rewrite the original manifest to retrofit future observations.
Interpret retained original Plan paths under the bound source configuration,
not by pretending they were created under the candidate's potentially new layout.
This historical read grants no current Plan write authority or active legacy alias.

Candidate commit identity is produced only once a real curated tree exists;
release staging can prepare that tree without prematurely deleting the live
governance source. Freeze those identities before release admission, rather than
inventing a commit or changing immutable release binding after it is begun.
Existing staged-candidate/Skill ownership remains in force; the binary does not
gain general Git staging, merge or branch-switch authority.

At candidate verification, a narrow explicit Plan-body authority read obtains
the selected real body's identity from that validated retained source. Ordinary
source-file/lockfile reads, policy loading, cwd resolution and input-manifest
walking still read the **candidate checkout**, including relevant untracked/
ignored inputs under normal policy. Never make `file(path)` silently fall back
to the archive when a candidate file is absent. A command actually depending on
the removed Plan path still fails/misses that input; attribution does not inject
it into the command's filesystem or change its input key.

Receipt and artifact persistence/rehashing uses a separate contained evidence
root in the release bundle, retaining normal Plan-relative logical paths and
Plan identity. These are not candidate source files. Keep Root-owned locks and
per-Plan output isolation. Before purge, receipts may be retained from the live
Plan evidence root; after purge, only the explicitly bound release evidence
root is usable. The operation supplies and validates these distinct roots; no
arbitrary caller path can redirect source/identity/evidence lookup. Fresh gate
claims remain bound to the release operation and actual candidate checks.

Removing graph/Plans **invalidates a prior full-tree key**. Do not strip entries,
rewrite old policy/key, or call product-only comparison a full-policy REUSED
decision. A prior receipt is reusable only if its already-validated dependency
policy genuinely gives an equal complete candidate input key, with the other
context/platform/independent conditions satisfied. Otherwise run the required
affected checks on the actual curated candidate, capture a new truthful key,
and attribute those results to the retained genuine Plan and Release operation.
This may require full checks for a full policy; it does not justify repeating
them at each subsequent phase when that candidate's eligible inputs are unchanged.

Post-integration preflight compares actual stable tree/config/Design/artifacts
with the frozen candidate and source-to-candidate curation proof, plus baseline
ancestry and pending-operation state. Verify removals/relocations against the
authorized release/layout contract, not a blanket `docs/` or `.mine/` exemption.
Curated/squash integration need not make S an ancestor of the candidate: validate
the exact curation/tree relationship and configured stable baseline ancestry,
not a nonexistent source-to-candidate ancestry requirement.
Hash/equivalence of product paths proves the deployed candidate identity, **not**
that an old development full-tree receipt applied to it. No current live Plan
lookup or fabricated graph is required; terminal governance facts come from the
validated S snapshot. A runtime manifest alone without that source and bound
operation is insufficient. Preserve the release bundle through interruption and
successful closure; later deletion is explicit owned cleanup, not implicit purge.
It is ignored local evidence and does not enter stable or authorize public upload.

Bounded Plan19 integration scope: split Plan-body attribution from
`VerificationBackend::file` via an explicit `plan_body(&PlanIdentity)` port (or
an equivalently narrow typed authority input); update `validate_request` and
ordinary filesystem backend to preserve current checks. Add a concrete release
composition using independently validated source/evidence/attribution roots.
This permits necessary edits in `src/application/verification_service/mod.rs`,
`src/infrastructure/verification_store/{mod,filesystem}.rs`,
`src/application/operation_service/service.rs`, their direct adapters and
`tests/verification_receipts.rs`/existing lifecycle and release tests. It is not
permission to change receipt/input-manifest/graph schema, automatic exclusions,
or arbitrary fallback lookup. Accepted Plan bodies stay immutable.

Decisive tests: full policy invalidates after graph mutation while acceptance
still finalizes once through exact terminal-delta proof; wrong graph delta/other
source change refuses; resume after terminal write does not mint more receipts;
purged candidate has no Plan file yet verifies genuine retained attribution;
candidate missing input never falls back to archive; old full receipt refuses
reuse; new candidate run succeeds; corrupt export/source commit/body/policy or
wrong stable tree refuses; supported narrowed-policy equality reuses normally.
All implementation/runtime tests remain NOT_RUN by this Design clarification.

### Frozen candidate checkout locator

The native Release begin request requires `candidate_root: RepoPath` in addition
to the candidate commit and existing attribution/policy inputs. Resolve this
Root-relative locator once and freeze it in the typed release evidence manifest
under the existing operation evidence binding. It identifies the actual checkout
used for command inputs; a commit ID alone does not identify cwd, ignored inputs
or worktree-local state. This does not change graph or receipt schemas.

Admission verifies a real, Root-contained, registered linked worktree, sharing
Root's common Git directory, whose actual HEAD is exactly C and whose tracked
index/worktree matches C. Reject Root itself, a Plan-owned workspace, foreign or
unregistered repositories, prunable/missing checkouts, ambiguous inventory,
symlink/junction escape and overlap with retained evidence or mutable output
roots. Inventory confirms the explicitly selected checkout; never search for
the first or only checkout at C, and do not impose an undocumented runtime-only
directory convention. The release owner supplies a dedicated contained location.
The binary verifies it but does not create, move, repair or remove a worktree.

Before integration, capture/record/check and resumed stages revalidate this same
frozen locator, registration, repository identity and C. Resolve policy, cwd and
ordinary source inputs there; inspect relevant ignored/untracked inputs under
the existing verification policy. A clean tracked tree does not prove those
inputs unchanged. Missing, moved or replaced checkouts refuse without retargeting
the manifest or silently choosing another checkout. Recovery may restore the
original locator and exact valid checkout, then reobserve all inputs under the
same operation; old receipts remain subject to normal invalidation and cannot
be revived merely by recreating the directory.

At the existing Integrated stage, the authoritative source becomes actual Root
on the configured stable branch, with integrated commit/tree satisfying the
frozen release binding and candidate equivalence contract. This is an explicit
stage transition, not a fallback when candidate lookup fails. The frozen locator
remains historical evidence; candidate worktree absence after integration does
not by itself invalidate an otherwise provable stable closure. Reobserve actual
Root cwd/policy/ignored inputs: equal tracked trees alone do not authorize reuse
of checkout-dependent evidence. Do not rewrite receipt context or manifest.
Retain the source snapshot and evidence bundle throughout closure.

Required probes: two registered checkouts at C select only the explicit one;
wrong HEAD/common directory, escape, Plan ownership and evidence/output overlap
refuse; ignored-input drift invalidates applicable evidence; missing/moved
locator refuses before integration; same-locator recovery rechecks inputs;
Integrated validates actual stable inputs and never falls back to stale C cwd.
Opened 2026-10-17: [Git worktree](https://git-scm.com/docs/git-worktree) documents
shared repository data with per-worktree HEAD/index and porcelain inventory.
The explicit locator and containment rules are MINE's local release contract.

### Exact curation proof and authorization boundary

This clarifies the approved terminal layout cutover, not a general migration or
caller-programmable release framework. A curation manifest describes evidence;
it does **not** authorize the changes it lists.

The native release composition must validate the actual source S configuration,
ownership marker, terminal graph and independent acceptance evidence. For the
approved layout cutover it additionally validates the source-bound ADR approval
and exact approved body digest, and the accepted implementation identity covering
that transformation. In this repository those are ADR-0023 and Plan24; these
repository-specific IDs must not be hardcoded in the generic lifecycle engine.
Candidate C cannot grant itself authority through a new approval, a manifest
boolean, an arbitrary validator name, or an unrestricted path allowlist.
Acceptance of the implementation is not authorization to publish a release.

Keep the existing Release baseline/candidate binding and operation evidence
list. The bounded typed curation evidence, frozen by its `FileIdentity`, binds
S and C commit/tree identities, stable baseline, repository/workspace/operation
identity, source authorization file identities, and the complete tree delta.
Enumerate exact paths, entry types/modes and before/after content identities;
account for additions, deletions and relocations without relying on heuristic
rename detection. Recompute the delta from actual Git objects, and verify raw
checkout bytes where frozen-body approval or attributes require them. Reject
duplicates, omitted differences, unsupported entry types, escape and collisions.
Neither a caller-supplied hash nor a list of changed files proves permission.

A narrow internal curation validator consumes those verified observations and
returns a checked proof that cannot be deserialized or freely constructed by an
external caller. The trusted application composition chooses the validator;
there is no script/plugin loader or caller-selectable approval implementation.
The ordinary release validator permits only the exact owned ephemeral workspace
purge required by the source release contract, with all other entries unchanged.
The layout validator derives and checks the finite approved transformation:

- exact owned Plan workspace purge;
- byte-preserving durable Design relocation, including frozen ADR bodies and
  their unchanged approval digests;
- only the enumerated layout fields in configuration and path-bearing metadata;
- deterministic native regeneration of current indexes and exact, bounded
  current governance-reference edits, never rewriting historical decisions;
- the managed ignore/attributes changes required by the layout contract,
  preserving foreign rules and proving frozen-byte behavior.

The concrete validator must recompute expected results from S under those
rules, not merely compare C with hashes declared in the manifest. Its finite
field/reference inventory and deterministic transformation are reviewed code;
every unaccounted byte or mode/type change refuses, even inside an otherwise
permitted file. Product, version and ordinary Skill changes belong in accepted
S before cutover, not in a curation exemption. Non-layout configuration fields
and unrelated content stay unchanged.

The accepted implementation composes the generic full-delta/proof boundary,
retained-source binding and native release admission with the ordinary purge-only
validator and the concrete layout producer/validator. The layout composition
also verifies raw bytes and effective attributes. Its positive, negative and
interruption evidence establishes implementation behavior; the actual release
must still validate its own exact S/C and cannot substitute those fixtures.

Admission, resumed stages and post-integration validation use the same frozen
identities and revalidate the relevant observations. A changed manifest or
candidate refuses rather than overwriting evidence. Stable integration must
match frozen C and satisfy the configured baseline relation; S need not be an
ancestor of C. Retained evidence stays separate from candidate command inputs.

Required probes include valid old-layout S to purged new-layout C with frozen
CRLF ADR bytes; product-byte smuggling; extra deletion/mode/symlink changes;
non-layout configuration edits; forged approval/acceptance; manifest-only
authorization; missing source/target, collision/escape; replaced manifest or C;
interruption/replay; and stable-tree mismatch. Keep the independent receipt
test proving old full-tree evidence invalidates after purge and fresh actual-C
evidence can satisfy the release gate. These are implementation obligations,
not tests run by this Design clarification.

Primary sources reopened 2026-10-17:
[Git diff-tree](https://git-scm.com/docs/git-diff-tree) defines content and mode
comparison and raw tree-delta output; [Git attributes](https://git-scm.com/docs/gitattributes)
defines checkout/check-in transformations. The authorization policy above is
MINE's approved local contract, not a permission guarantee supplied by Git.

## Retention, safety and verification

Before accepted worktree/output cleanup, preserve receipts and decisive evidence
needed by acceptance/release. Remove only validated owned output paths with no
active users. Rejection preserves source/ref/worktree, not every disposable build
cache; cleanup must retain counterexamples and useful handoff evidence. Plan
receipts are ephemeral with `docs/plan/`; no default remote cache/upload is added.

Test unchanged reuse without spawning commands, changed source/toolchain/features/
environment/platform, relevant untracked files, excluded graph-only changes,
forged/missing/truncated artifacts, conflict resolution, full-suite policy,
independent probes and cleanup while an output has an active user. Show exact
run/reuse provenance and CLI/MCP parity wherever results are exposed.

Opened 2026-10-09: [Bazel remote caching](https://bazel.build/remote/caching)
illustrates action/input keys and undeclared-input risks. MINE adopts the
input-accountability principle, not Bazel or a remote cache service.

Reopened 2026-10-14 for the native bridge: the same Bazel documentation identifies
undeclared environment/tool dependencies and input mutation during execution as
cache-correctness risks. This supports independent current-context observation
and the explicit source-stability limitation, not trust in a stored receipt's
own claim that it still applies.

Opened 2026-10-14 for the terminal/release boundary:
[Git diff-tree](https://git-scm.com/docs/git-diff-tree) supplies actual tree-delta
evidence, distinct from commit-name equality; the Bazel source above supports
preserving complete input keys rather than silently projecting a changed tree.

Opened 2026-10-14 for locked fresh observation:
[Microsoft LockFileEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex)
documents that a second handle, including in the locking process, cannot access
the exclusively locked region. This supports excluding coordinator lock bytes
as non-input infrastructure, not weakening the lock or ignoring source IO errors.
