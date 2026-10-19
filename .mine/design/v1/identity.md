# Roles, Assignments and Traceable Handoff — Approved 1.0 Target (ADR-0025)

## Three roles, one project lead assignment

Keep understandable names **Architect, Executor, Reviewer**. Project Lead is an
assignment to one Architect principal, not a fourth all-powerful agent role.
The human owner retains administration and business/budget authority.

| Role | Allowed responsibility | Forbidden implication |
|---|---|---|
| Architect | arch, plan-create, sync; technical blockers and coordination | Cannot self-accept implementation or impersonate human approval |
| Executor | plan-exec on explicitly assigned Plan | Cannot take another owner by supplying their ID |
| Reviewer | plan-review on assigned Plan, integration/cleanup under normal gates | Cannot review its own prior implementation under a new session/ID |

Shared read/status/inbox capabilities are project-scoped. Architect foundation
coding requires a normal execution assignment/Plan and independent review;
an Architect designation alone is not arbitrary Root production-write permission.
Permissions are enforced by the binary/service, not only Skill prompts.

## Principal versus session

Stable internal UUID plus a unique short display ID, e.g. `EXEC-1DF3XA`,
`ARCH-78PIKZ`, `RVWR-29QW7B`. The prefix is a creation label; stored role grants
are authority. Collision-check random allocation, no ID reuse. No caller may
authenticate by quoting a short ID. Enrollment uses a scoped, one-use grant
issued by the owner or delegated Project Lead policy. Provider binding records
provider/profile, native session/thread ID, attempt generation and nonsecret
configuration/model selection provenance; credentials remain protected externally.
One project owns a principal; cross-project participation uses explicit grants.

Plan stores assigned executor/reviewer and assignment revision. Actual mutable
admission also requires current attempt, epoch, scope and execution state. An
unassigned executor gets a transfer-required diagnostic, not an implicit takeover.
Reviewer independence checks contribution/assignment history, not merely current
role or a different display ID. New IDs cannot launder self-review.

## Handoff is an operation and history

Two distinct events:

- **Session replacement**: same principal/assignment, new attempt/provider ID.
- **Owner transfer**: A to B, increment assignment revision/epoch; preserve all
  previous owners, attempts, implementation contributions and review provenance.

An explicit owner instruction “B continues this Plan” supplies transfer authority
and need not trigger a second ceremonial approval. Otherwise only recorded
delegation policy/authorized Project Lead can transfer; B cannot self-grant it.
Record immutable handoff ID, Project/Plan, from/to principal and attempt, authorizer,
reason, request/commit times, before/after assignment revisions, source branch/head,
worktree identity, dirty-file manifest, current work node/checkpoint, pending
operations/commitments, evidence IDs and receiving acknowledgment. Record failure
and recovery events too; an unsuccessful handoff must not look completed.

Transfer stages: requested -> old writer quiescing -> checkpoint verified ->
ownership CAS committed -> new admission -> acknowledged. Missing source session
does not require a fictitious acknowledgment; record forced recovery evidence.
Stop/revoke old session and child commands; raise fencing epoch before new tool
admission. Timeout is not proof of process death. If actual old filesystem writer
cannot be stopped/fenced, pause that node and report UNCERTAIN; never run B in
the same worktree anyway. Do not stash/reset/clean abandoned dirty edits: preserve
and hand them over with hashes and an explicit unfinished-work boundary.

The compact handoff brief is pointers plus decisive state, not the full transcript.
B revalidates exact observed facts and resumes existing operations; it does not
restart the Plan, erase prior failure, rerun every test or create compensation
solely because session resume failed. Node-local pending work changes addressee
through audited reassignment, retaining original sender/request/commitment IDs.

## Trust and proof

MINE tokens/epochs constrain MINE-mediated operations. Same-OS-user agents with
arbitrary filesystem access can bypass them; this is not an OS sandbox claim.
Managed host isolation/child-process control is required for safe replacement;
hostile-agent isolation would need stronger OS controls and separate credentials.
Tests cover two contenders, stale epoch/revision, transfer lost response, old
process still alive, dirty handoff, explicit owner replacement and self-review
attempts after principal/session changes. UI shows the complete ordered chain.
