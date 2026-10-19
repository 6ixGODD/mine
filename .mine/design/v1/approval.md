# 1.0 Approved Decision Handoff — 2026-10-16

## Subsequent timing/logging approval

The owner later explicitly instructed approval of all ADRs while delegating the
complete 0.2 coordination/release chain. This separately approves ADR-0032's
already-proposed body, SHA-256
`AF7C150AF5774EF90EFACD91698A0A9AB2D89127C7A959CA15E37E232FBA2AB1`.
Native approval records repository-owner attribution at registry revision 73;
no approved body was edited. Both `v1.0.0` and current `docs/design` readiness
are true. Timing/logging remains a 1.0 target, not active 0.2 implementation.
The eight-entry record below preserves the earlier distinct approval event.

All eight entries are **APPROVED**, not implemented. The repository owner
explicitly stated “ADR全部批准” on2026-10-16, approving the exact proposals below.
Installed MINE recorded each approval as repository-owner, registry62 ->70.
All raw-file SHA-256 values matched the proposed registry and this handoff before
approval. No approved body was edited, including its frozen whitespace.
Native readiness for `v1.0.0` is now true; current0.2 scope `docs/design` remains
separate. No1.0 execution Plan or implementation is authorized by this record.

## Exact approved decisions

The explicit batch approval binds these exact native body digests, not an older
proposal. Frozen ADR prose retains its original proposal wording; native status
is authoritative. Detailed leaves describe accepted target direction; refine exact
wire/DDL/platform contracts before dependent implementation planning.

| ADR | Body | SHA-256 |
|---|---|---|
| ADR-0024 | [Project Registry and Safe Deinitialization](../decisions/0024-project-registry-and-safe-deinit.md) | `D5D3D18FE700C8ADF1FAD09C2D2F837FCAB86AFDB2BE1E6C81DA65DB79AFB2D8` |
| ADR-0025 | [Principal Roles and Traceable Plan Handoffs](../decisions/0025-principal-roles-and-plan-handoffs.md) | `47A1A3E419DC15E687583767E84E181B75478864FC94C54204CCED92A6C6484E` |
| ADR-0026 | [SQLite Authority, Migration and Durable Effects](../decisions/0026-sqlite-authority-and-recovery.md) | `28D69EA36ADEC209FF5A6C8829E36E8B50E1EACB68246CCFD5E70CFE4CAD2EF1` |
| ADR-0027 | [Durable Commitments and Tiered Blocker Resolution](../decisions/0027-durable-coordination-and-escalation.md) | `CA287AC652F1232BC42BFD820E67A0FC4E21C840CEED786FDCBFF1E1594ECCE5` |
| ADR-0028 | [Provider Sessions, Model Selection and Recoverable Work Nodes](../decisions/0028-runtime-model-selection-and-recovery.md) | `558BAD902831FD22796BE2EDCAC00A1CCA8DC439831C6330ECF7292F89E86367` |
| ADR-0029 | [Unified Local Dashboard and Consistent Decisions](../decisions/0029-unified-local-project-dashboard.md) | `6A118D5885834F4413FDE7BBA6663EBD82D184177EBEBA3A39ECB146A25021AC` |
| ADR-0030 | [Experimental Bounded Autonomy](../decisions/0030-experimental-autonomy-policy.md) | `F9E95E2CC9D329D91E9F8C7DBDC0FB3E543A7F48FA4CBA856AB7E0211B708A1F` |
| ADR-0031 | [Cross-Project Resource Arbitration and Temporary Channels](../decisions/0031-cross-project-resource-coordination.md) | `E66579EDC35CAB46862FBD6376CAD04B167A2CE62A4ABC464C2B3CC2A4E2A4CE` |

## Owner-visible tradeoffs

- Project state lives locally in SQLite; Git clone alone no longer carries all
  approvals/history. Explicit verified backup/import is required for full recovery.
- Managed session creation/model configuration is provider-capability dependent;
  arbitrary existing desktop sessions cannot be promised attachable or visible.
- Fast/effort/model selection is Project-scoped, separately recorded and checked;
  unknown capability/cost cannot silently become supported/free.
- Durable coordination guarantees recorded obligations and failure escalation,
  not guaranteed task success. Waiting itself launches no LLM polling.
- Safe handoff/deinit can wait for a real writer/resource to stop; timeout alone
  never authorizes a duplicate filesystem writer or deletion of unknown data.
- Autonomy is off by default and cannot approve its own introduction. Human
  decisions retain one native authority whether made in Web or Architect session.

OpenCode stays supported in0.2.0 and is removed only in1.0.0. No stable branch,
remote publication, global installation or active0.2 migration is authorized by
this proposal. Exact b2jI visual output and pinned-provider native conformance
remain implementation acceptance work, not completed research evidence.

## Approval verification

Installed CLI approved ADR-0024 through ADR-0031 individually with explicit owner
attribution; each call exited0. No registry/graph hand editing or supersession.
`mine adr validate --format json`: exit0, valid=true, revision70, count31.
`mine design validate --format json`: exit0, valid=true, no warnings.
`mine graph validate --format json`: exit0, graph revision90 unchanged.
Native scoped readiness: docs/design=true and v1.0.0=true, registry70.
`python scripts/verify.py`: exit0. No product tests needed or run for approval.
Initial PowerShell digest preflight had a pipeline syntax error before writes;
corrected read-only preflight exited0 and verified all8 exact raw-file digests.
