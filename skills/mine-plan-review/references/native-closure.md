# Native independent closure

Read with the invoking review Skill. This describes the shipped typed protocol,
not a shell automation engine or permission to execute unchecked Git effects.
Inspect the installed version/help/schema before using it. An older live workspace
must finish with its installed workflow; candidate contracts run only in isolated
fixtures until released. Never copy this document's placeholders into real evidence.

## Before begin

Resolve the exact registered Plan, authoritative Root config/graph, implementation
owner/run, lineage/branch/worktree, current tip and immutable body identity. Obtain
the actual executor stop and relinquishment of checkout/output, then independently
review. A clean tree, elapsed time, IMPLEMENTED or a vanished process is not a handoff.
Actor labels are local attributable declarations, not authentication or an OS fence.

The canonical report is derived from the registered Plan path, not an arbitrary
report selector. It contains one unambiguous Verdict: ACCEPTED or Verdict: REJECTED
and exactly one fenced block with info string mine-review-closure. Its strict JSON
object has these fields, no duplicate/unknown keys:

| Field | Actual evidence |
|---|---|
| schema_version | 1 |
| plan | Existing typed PlanIdentity, exact repository/workspace/Plan/path/body digest/lineage/branch |
| writer_owner / writer_run_id | Exact original authoritative implementation identity |
| reviewer | Independent operation actor, different from writer_owner |
| independent_review | true only after real independent review |
| writer_stopped | true only after actual identified handoff |
| stopped_at | Validated UTC handoff timestamp |
| stop_basis | Concrete bounded executor acknowledgement/controller observation |

Do not manufacture a historic body digest, owner/run, stop event or approval. A
known writer resuming invalidates the handoff even if the frozen declaration remains.
Existing output-user locks independently gate cleanup and cannot be overridden.

Commit all report/fix inputs before begin. For acceptance the report is committed
on the reviewed Plan branch; pin its resulting tip outside the report. ReviewEvidence
is {reviewed_tip: exact commit OID, report: {path, bytes, sha256}}. FileIdentity.sha256
uses lowercase 64-hex and actual byte length. The final tip cannot name itself from
within its own committed report. Rejection report evidence is committed in clean
Root dev, while rejected source P stays at its preserved tip; do not substitute
integrated_commit merely to change the report's source location.

## Native JSON requests

These are strict data shapes. Supply real observed values, canonical IDs and
contained paths. Never construct trusted proof objects from serialized reports.

BeginInput has schema_version:1, actor and intent. Its supported intents are:

- acceptance: kind, plan_id, review (ReviewEvidence).
- rejected_handoff: kind, plan_id, review, compensation (actual PlanIdentity).
- release: kind, candidate (exact C OID), candidate_root (explicit registered
  checkout locator), attribution (genuine selected Plan IDs), policy_path.

Use the actual CLI fallback; there are no operation/verification MCP tools:

~~~text
mine operation begin --expected-revision <R> --input <Root-contained-json> --format json
mine operation show --operation-id <returned-id> --format json
mine operation stage --expected-revision <current-R> --input <stage-json> --format json
mine operation finalize --expected-revision <current-R> --input <finalize-json> --format json
~~~

StageInput has schema_version:1, expected_stage_revision, operation (the existing
typed LifecycleOperation with only its permitted next facts), verification (GateInput
or null where allowed). Do not invent a graph checkpoint subcommand or jump stages.
GateInput has policy_path and checks; each GateSelection has check, receipt
(FileIdentity, not a path string), fresh current context and fresh_observation
(optional FileIdentity). Its schema is not a request to execute tests.

Read the existing operation before forming each next request. Retain repository/
workspace/body/tip/report/binding/actor and immutable evidence. Increment only the
actual permitted stage revision, select the actual observed next stage/outcome and
retain required evidence. Native validation proves those facts again; a caller's
stage label does not authorize missing effects. Graph observer revision and stage
revision are separate. A post-purge release may observe graph revision zero; never
invent a live graph or silently reuse an old nonzero observer.

## Acceptance phases

Reviewed -> Integrated -> Verified -> Cleaned -> Accepted.

- Reviewed begins from actual independent review/stopped-writer report and pins
  exact Plan tip/report identity. IMPLEMENTED ownership remains held.
- The reviewer merges that exact source into Root integration branch with ordinary
  no-ff Git, resolves conflicts without changing frozen contract/body, then records
  actual integrated commit. Stage does not merge or resolve anything itself.
- Verified requires applicable complete policy/check evidence against the actual
  integrated source, not just another source tree or status-zero process exit.
- Before Cleaned recheck any still-present branch at the frozen reviewed tip,
  clean registered worktree including untracked data, common repository/inventory,
  integration ancestry and output users. Remove only its exact merged linked
  checkout/local branch without force. A changed tip, foreign attachment or dirty
  checkout blocks cleanup; absence alone does not prove a lost removal response.
- Finalize Accepted with the existing closure ID and current whole-gate selection.
  plan accept --id <id> --expected-revision <R> --operation-id <same-id>
  --input <typed-finalize-json> --format json is the guarded Plan alias; MCP
  mine_plan_accept uses id, expected_revision, operation_id and input.
  Native terminal graph checkpoint completion is required before successors gain
  execution authority. Do not manually commit/fast-forward its graph pair.

A terminal checkpoint changes the exact graph inputs. The native phase-aware gate
may retain the observed preterminal check through its one verified known transition;
this does not make a Full-policy receipt applicable to arbitrary later trees. Never
strip graph paths from the policy or relabel historical acceptance as a fresh run.

## Rejection handoff

Intent -> Rejected -> CompensationRegistered -> Rewired -> Released -> Complete.
These stages observe the actual separate rejection/register/rewire/release graph
transitions. Each uses current explicit actor/observer and completes its own graph
checkpoint. Do not stage labels in place of effects or rerun completed transitions.

Keep rejected P/ref/worktree unchanged and never merge/delete it. The next contiguous
sibling compensation has the complete ordered predecessor list of the rejected
node, not an edge to that rejected node; it shares the lineage checkout, not mutable
output. Preparation switching happens only after completed stopped-writer handoff.
Ordinary compensation execution is a separate request; do not start it during review.

An already-REJECTED migrated legacy node may have no old operation. Only the explicit
current-observation route may begin a new rejected_handoff when exact declared
READY compensation, completed rewiring, original owner/run, preserved reviewed P,
fresh independent handoff/report and complete valid operation inventory prove it.
This is not reconstruction of historical journal/timestamps. Existing operations
must be queried, corrupt/missing referenced records refused, never replaced as
"legacy absence". A clean exact already-switched compensation at P may be observed
only under the native legacy rules, not adopted from another branch/path.

If permitted unpinned historical report evidence needs a current section, preserve
all original bytes/verdict, append a clearly dated actual observation and unique
assertion, then commit before begin. Never alter a report already pinned by evidence.
Observe existing facts one stage at a time and finalize this same new operation;
do not rerun old graph transitions or fabricate historical preparation generations.

## Unknown results and checkpoints

Persisted stages/effects are not rolled back because a response failed. Query the
same operation, inspect actual bound before/after resources and resume only the
unfinished typed step. Exact attempted intent plus matching fresh evidence can
reconcile a lost response; mere matching name, absence or OID cannot. Do not repeat
merge/removal, replace operation identity, infer old facts from reflogs, manufacture
compensation or clean unknown artifacts.

Graph checkpoint and unknown workspace writes are global mutation/start barriers.
Ordinary pending review holds its own Plan resources. Native prepare carries its
original actor/observer/mode/UUID; normal graph mutations generate their own returned
operation IDs. Use the actual same-ID finalize recovery contract after query, not
a guessed command or manual graph commit. Stale new decisions reobserve; old pending
requests retain their original identity rather than silently refreshing it.

## Verification and release after purge

Use verification capture before the external check; record its real command outcome
and artifacts afterward. Requests live under Root .mine/build/<exact-plan-id>/
<output-class>/<file>. Capture/record/check do not execute checks or authenticate
external claims. Check fresh command/source/body/policy/tools/env/platform/features
and artifacts before reuse. JSON success may say INVALIDATED/NON_REUSABLE; inspect
disposition and reason. Required independent/native probes cannot be cached away.

Release binds terminal accepted source S and exact candidate C plus its registered
dedicated checkout, current candidate policy and genuine attribution. Its source/
candidate/evidence roles remain distinct: actual command inputs at C; frozen original
Plan/config/report bodies in S; receipts/artifacts in the operation's evidence root
using original logical Plan paths. No live-graph reconstruction, arbitrary archive,
Root-body fallback or caller-defined transformation whitelist.

Intent -> Preflight -> Integrated -> Complete uses current GateInput. Before stable
integration verify actual stable baseline; afterward verify authorized result tree/
ancestry and fresh context, not equality to the old baseline. Root may be stable
only in that authorized release phase. mine release --operation-id <id> is diagnostic
only, can_release=false, and never substitutes for stage/finalize admission.

An active Release bars unrelated mutations. Safe explicit Refused at Intent requires
actual unchanged source/index/stable and no release effects; preserve journal/evidence
without cleanup. Missing/defective candidate prepared before begin need not prevent
that no-effect refusal. After Preflight or unknown effects use same-operation recovery,
not cancellation or editing the bound candidate. Publication/upgrade remains separate
explicit authority; a completed local Release does not prove push/tag/CI/native-host
work or replace the user's installed tools.
