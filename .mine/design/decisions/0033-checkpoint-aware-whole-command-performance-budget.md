# ADR-0033: Checkpoint-aware whole-command performance budget

## Status and authority

The native ADR registry is authoritative for status and approved body digest.
This technical decision is made under the repository owner's explicit delegation
to the coordinator to resolve technical ADRs and complete 0.2.0 autonomously.
It is not a claim that the owner personally reviewed this new body. It supplements
ADR-0015 and ADR-0022; it changes the old pre-checkpoint write-test numeric oracle,
not the immutable implementation Plan or the safety contracts.

## Evidence and decision

Whole-command traces 041, 049 and 050 are retained, including original failures.
Independent read-only audit of 050 found 115 Git launches, 23.638452 seconds,
peak one, and three batch-check processes: two graph-anchor observations with
different replacement semantics and one new candidate proof. Four other cat-file
calls read two blobs and their checkout transformations. Calling all seven
cat-file invocations anchor batches is incorrect. The original <=10 whole-write
oracle predates native checkpoint effects and cannot stand in for their budget.
Neither 115 launches nor the earlier 122/126 launches is accepted by this decision.

For the existing CLI/MCP whole native withdraw performance fixtures, set the
complete-command upper bound to **96 launches**, independent of 0/10/100/1000
nodes and duplicate/unique anchors. This is a concrete implementation ceiling,
not a measured result or a claimed irreducible minimum. Its derivation from the
audited 115 is: remove fourteen repeated index-representation queries across
seven observation groups; retain two queries for the first proof of newly
generated index bytes; remove four redundant tree/blob queries; replace the
four-query candidate proof with one equivalent exact proof: 115-14+2-4-3=96.

| Named phase | Launch allowance |
|---|---:|
| Root discovery and first snapshot | 5 |
| Initial admission, controlled anchors, Plan/ref/worktree | 17 |
| Plan-effects fresh observation | 10 |
| Graph/view persisted observation | 7 |
| Private index, tree/blob and candidate | 18 |
| Index-lock acquired observation | 7 |
| Written lock, pre-CAS observation | 7 |
| CAS and result confirmation | 2 |
| Post-CAS observation | 7 |
| Post-ref callback, pre-publication observation | 7 |
| Completion including first new-index representation proof | 9 |
| **Whole command** | **96** |

If the new-index representation is checked earlier, its two launches move to
that phase, not disappear or get counted twice. No other unexplained phase
overrun is hidden by unused allowance. Count all launched Git children, including
discovery, effects and proof. Do not split requests to evade totals. Recovery
paths are reported separately and preserve full evidence checks; this specific
fresh-withdraw ceiling is not a universal cap permitting other commands to grow.
Existing read/show caps remain unchanged.

## Mandatory implementation boundaries

- Keep all actual fresh mutable safety boundaries and observer-induced drift
  tests. Index witnesses bind repository/common directory, exact index bytes,
  self-contained representation and flags. Reuse only after fresh byte identity
  checks; changed/unknown/recovered index bytes receive complete proof. Config
  and attributes do not become immutable merely because index bytes match.
- Fetch both graph tree entries together and batch their raw blobs. Require full
  delta, exact mode/type, canonical-byte equality and checkout-transformation
  evidence. Once actual blob bytes equal canonical bytes, separately hashing the
  same canonical input is not an additional proof. Preserve bounded IO.
- One native Git candidate observation must prove exact candidate commit ID,
  exactly one expected parent and the expected tree. Reject tag peeling,
  wrong-type and malformed/foreign outputs; do not trust commit-tree stdout alone.
- Each graph snapshot and replacement interpretation receives at most one
  anchor batch. For these fixtures, at most two graph-anchor batches are allowed
  (ordinary and controlled no-replacement contexts), zero for empty anchor sets.
  Do not share naked object caches across those contexts. Candidate proof and
  the one raw-blob batch are separately classified and still counted in the
  whole-command total. The original replacement-refresh regression remains.
- One public-request deadline/cancellation/counter context spans all phases and
  index views. Retain 10-second child/30-second request/1-MiB stream defaults
  unless an actual validated project policy says otherwise. Failure after
  effects preserves truthful pending/recovery state. No automatic budget reset.

## Acceptance and alternatives

Implement the specified reductions and run one representative complete command
first. Then prove the unchanged read/show gates, all sixteen CLI/MCP write cells,
phase attribution, exact batch purposes, peak one, shared-timeout/cancellation,
replacement and index drift, candidate rejection and same-operation recovery.
Independent review must inspect those proofs. No implementation test is claimed
by this ADR, and this decision alone does not make Plan19 IMPLEMENTED or ACCEPTED.
Retain old RED evidence; label the new oracle as this approved contract, never
rewrite history as though the old <=10 check passed.

Rejected alternatives: raise the cap to the observed 115; report only snapshot
counts; delete fresh safety checks; disable replacement semantics globally;
require indefinite optimisation without a concrete stopping criterion. Further
improvements below 96 are welcome but not a reason to move this ceiling upward.

## Primary sources

Opened 2026-10-18: [Git cat-file](https://git-scm.com/docs/git-cat-file) distinguishes
batch object/content queries and checkout filters;
[Git show](https://git-scm.com/docs/git-show) documents commit presentation.
Use supported Git-baseline features and exact output verification. The numeric
budget is a local engineering decision derived from the audited call inventory,
not a guarantee made by Git.
