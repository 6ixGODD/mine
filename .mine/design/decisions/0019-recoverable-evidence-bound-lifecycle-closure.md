# ADR-0019: Recoverable Evidence-Bound Lifecycle Closure

## Status and scope

Repository-owner review candidate. Preserves independent review, accepted
integration cleanup, immutable Plans and compensation-lineage requirements.

## Context and evidence

`src/application/plan_service.rs:706-747` accepts IMPLEMENTED, records a report
string and releases successors, but does not verify a real report, integrated
commits, Root role, or worktree/branch cleanup. Rejection validates report
evidence at line 788. The review Skill requires the omitted acceptance facts.

Ordinary lifecycle CLI/MCP requests do not carry the observer's expected
revision; handlers/service obtain separate revisions. A lock protects an
internal write, not the human/agent's original decision snapshot. LODE's
46-C01 review records a reject that eventually succeeded after a long wait,
and an add whose completion had to be checked before retrying. Timing cause
and simultaneous Git-process counts were not measured there.

`release_service.rs:295-298` defines design-valid through file existence;
`386-393` defines stable-unchanged through a nonempty SHA. Neither proves the
property advertised. Stable integration legitimately changes the baseline,
so a phase-aware comparison is required rather than a blanket equality check.

## Proposed decision

1. Lifecycle writes bind the caller-observed revision and exact Plan/body/tip
   evidence. Expose a durable local operation identity, stage and queryable
   result for multi-step/destructive closure. A timed-out caller learns whether
   execution was refused, committed, pending, or outcome-unknown before retry.
2. Keep the graph as lifecycle authority and existing Skills as Git integration
   owners. Add a small Root-owned receipt/journal for interrupted operations,
   not a daemon or general-purpose workflow engine. Accepted evidence becomes
   graph/report-bound; an ignored runtime cache alone cannot authorize acceptance.
3. Independent positive review pins the reviewed tip, immutable Plan identity
   and a contained regular report with digest. Positive verdict authorizes
   integration, not ACCEPTED by itself. Merge/conflict resolution is followed
   by applicable integrated-tree checks and a recorded integration result.
4. Acceptance verifies the reviewed tip is included in Root dev ancestry, the
   report/evidence matches, the managed worktree is gone, and the merged local
   branch is gone. Root has no unrelated changes; operation-owned graph/report
   changes are explicitly classified rather than globally ignored. Later dev
   advancement must preserve the proven integration and verification inputs.
5. Finalize ACCEPTED/release successor ownership only after those checks. Each
   completed stage is reverified on resume. Failure after merge or cleanup
   continues the same operation; never remerge blindly, falsify acceptance,
   delete user work, or manufacture compensation for an infrastructure failure.
6. Reject/compensation/rewire/release handoffs share an evidenced resumable
   operation identity, while preserving each graph transition and lock revision.
   Replacement design approval and the next sibling compensation remain required
   when behavior/architecture truly changes. Unknown write outcome is not REJECTED.
7. Release preflight reports checks it actually performs: validated Design/ADR
   readiness, graph facts and phase-aware stable/candidate ancestry. Before
   integration compare the stable baseline; afterward verify the authorized
   integrated candidate. Existing publish authorization remains unchanged.

## Alternatives and consequences

- Trusting a report path/string permits acceptance without integration.
- Holding one lock across merge/tests blocks other agents and cannot make
  filesystem/Git/graph changes one atomic transaction.
- Retrying opaque writes risks duplicate effects; creating new Plans for each
  interruption obscures evidence and costs tokens without changing behavior.

The chosen staged receipt preserves the user's merge-and-cleanup rule while
making recovery truthful. Receipt storage/retention and CLI/MCP DTOs need explicit
detailed Design; no new status or command is implied to exist today.

## Verification obligations

Refuse missing/unsafe report, wrong tip/digest, unmerged code, dirty unrelated
work, remaining managed artifacts, stale revision and foreign cleanup targets.
Inject interruption at every stage, including committed graph/render failure;
resume once without duplicate merge or history loss. Test conflicting merges,
terminal replay, preserved rejected branch/lineage, and stable pre/post-integration
checks. Review is independent even when validation receipts are reused.

## Primary sources

Opened 2026-10-09.

- Git project, [git-merge-base](https://git-scm.com/docs/git-merge-base):
  `--is-ancestor` tests ancestry, not branch naming or a nonempty SHA.
- Git project, [git-worktree](https://git-scm.com/docs/git-worktree): inspect
  machine-readable inventory and remove owned clean linked checkouts normally;
  never force-delete unrelated/dirty state to finish a receipt.

## Approval question

Approve observer-bound writes, queryable interruption recovery and verified
integration/cleanup receipts before ACCEPTED, without adding an orchestration
service or relaxing independent review?
