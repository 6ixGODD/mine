---
name: mine-sync
description: Reconcile MINE-owned design with repository reality using code-first synchronization. Use when onboarding an existing repository, after substantial out-of-band changes, when design drift is suspected, before stable release, or when the user requests a repository/design audit. Creates a verified local backup before rewriting design, then updates design to match current code unless the user explicitly protects a decision. Does not modify business code without a separate architecture/plan/execute flow.
version: 0.2.0
---

# MINE Sync

Reconcile managed Design with observable code. This is code-first, unlike requirement-
first mine-arch; do not silently redesign business code or erase explicitly protected
accepted targets. Scope may be a named subsystem or an explicitly broad release audit.

## Locate ownership and coordinate

Read AGENTS.md, requirements, Root .mine/config.toml and its Design ownership marker.
Resolve authoritative Root from the first Git worktree inventory record. Fresh Design
is .mine/design/index.md and .mine/design/decisions/; fresh Plans/graph/reports are
.mine/plan/. Use actual configured contained paths, not a linked checkout's graph,
a guessed docs/ path or the existence of a directory.

Check installed binary/help/schema before use. No future binary/Skill governs active
older workspaces. Do not redirect a live legacy config or move user docs/ during sync.
An unmarked/foreign/malformed configured Design root is refused, not automatically
backed up and claimed by init. Report exact ownership conflict and safe next action.

With an active workspace Root remains on configured integration branch; release may
later use stable. Read the complete native graph wave/show snapshot to see held
IN_PROGRESS/IMPLEMENTED owners, effective responsibilities/exclusions/mapping,
policy/provenance, generators/outputs and pending closure/checkpoint. Do not write a
Design area concurrently owned by execution or infer that stopped/silent owners have
released it. Sync can read dirty work but must classify preserved unrelated changes.

Prefer available read-only mine_design_validate/mine_graph_validate/mine_graph_status,
mine_graph_wave and ADR/readiness tools. Use JSON CLI fallback for design status/backup
and absent tools. Never hand-edit graph, manufacture approvals, start/review Plans,
commit/stage/push, install global assets or execute arbitrary shell deletion.

## Ordered procedure

1. Record actual repository/branch/HEAD, configured namespace and marker identity,
   user scope, protected decisions, active holds and pending operations. A pending
   checkpoint is not rollback or permission for a manual graph-only commit; its owner
   queries/resumes the same operation before conflicting writes.
2. Before any Design rewrite, run mine design backup --format json. The native backup
   lives under .mine/runtime/design-backups/<timestamp>/, verifies ownership,
   containment/collision and exact copied bytes, and excludes itself from tracking.
   Record returned path/file count/bytes and verification. Failed/missing backup
   blocks all Design mutation. Do not implement another backup or follow escaping/
   cyclic links. Do not infer copied bytes from successful process exit alone.
3. Inventory current entry points, producers/consumers, public contracts, config,
   persistence, generation, lifecycle/error paths, tests and operations relevant to
   scope. Broaden only for concrete dependencies; explicitly broad release sync
   covers the complete release, not a sampled subsystem.
4. Traverse Design indexes progressively. Build code -> contract leaf -> discrepancy
   trace; separate actual enforcement, accepted pending target, assumptions and
   suspicious behavior. Named interfaces without real consumers are not completion.
5. Apply authority: explicit owner protection first; actual code/runtime/config next;
   tests/comments as inspected claims; existing Design only where behavior is unknown;
   inference last and labelled. Code-first does not permit editing approved ADR bytes
   or silently reversing approved policy. Preserve protected target and report drift.
6. Rewrite only managed unprotected descriptive Design after the verified backup.
   Maintain one contract per leaf, indexes/links and honest implemented-versus-pending
   wording. Describe original/effective ownership, native observer/checkpoint/recovery,
   receipt/native evidence and ordinary architect-foundation boundaries from actual
   code; do not document a future feature as already available.
7. Validate Design, ADRs and, when live, graph through native read tools/JSON CLI.
   Graph-less stable candidate validation does not recreate an ephemeral graph.
   Recursively detect tracked backup/runtime material anywhere; do not exempt all
   .mine or claim clean release from structural Design validation alone.
8. Write a local report under .mine/runtime/sync/ with exact repository/branch/HEAD,
   scope, backup manifest/verification, inspected and sampled evidence, protected
   decisions, changed Design leaves, discrepancies, actual commands/exits,
   unresolved uncertainty and SYNCHRONIZED / SYNCHRONIZED_WITH_WARNINGS / BLOCKED.

## Evidence and target-change boundaries

A required code change is a planning/execution handoff, not sync-time implementation.
An uncovered target architecture/public/persistent/security/ownership/deployment or
acceptance change returns to owner-approved ADR/Design work. Do not "fix" code or
activate arbitrary semantic policy while calling it synchronization.

Responsibility/namespaces and exclusions govern scope; paths locate it, not every
internal filename decision. Native clarification preserves original body/declarations
and exact approved policy/tail/actor/revision provenance; it is not permission to edit
immutable Plans, rehash changed bodies or backdate absent history. Graph and ADR
registry revisions are distinct. Invalid semantic policy never drops a held owner.

Use existing exact applicable evidence without launching duplicate full suites.
Check input-bound receipts with fresh current context through real native verification;
exit0 may still mean INVALIDATED/NON_REUSABLE. Preserve RUN/REUSED/INVALIDATED/
NON_REUSABLE/NOT_RUN, first failures and missing native/independent evidence. If a new
check is necessary, it needs explicit owned output authority and exact results; do not
substitute older candidate/platform artifacts.

A full release sync report must cover the final independently accepted integration
commit and all affected contracts with no blocking uncertainty. It is Phase A evidence,
not phase-B stable integration, acceptance, publication or user installation. Handoff
to independent mine-plan-review complete release closure is explicit owner action.

## Output isolation and final report

Reuse Root toolchains, downloads, venv and safe immutable caches. Never create a
duplicate environment/target/node_modules in a linked checkout. All languages'
mutable build/test/generated/coverage/package outputs are isolated by exact Plan node
under <Root>/.mine/build/<plan-id>/<output-class>/; compensation gets a separate
namespace despite lineage checkout reuse. Local backup/sync recovery uses its exact
Root runtime namespace. Exceptional measured disk cost requires owner agreement
before relaxing isolation; never delete foreign outputs or silently share writable
parallel builds.

Report changes and coverage, backup proof, protected/pending contracts, failures and
current live reviewable/executing/ready/blocked/pending-operation/accepted/withdrawn
status plus the next action. Do not stage, commit, transition the graph or announce
release readiness when coverage, independent review, native gates or uncertainty
remain unresolved.


## Canonical durable bounds

All new IDs follow planNN(?:-NN)*(?:-CNN)? (two-digit numeric segments, uppercase
terminal C). Files are <configured-plan-root>/<id>-<lower-kebab-slug>.md.
Compensation uses the next contiguous sibling C ordinal in the same lineage.
Never edit the configured execution-graph.toml or generated execution-graph.md
directly. Root owns graph/config/locks/revision even for linked callers.
For Rust set CARGO_TARGET_DIR to that exact Plan's isolated output class; the
same mutable-output isolation applies to every other language, not just Cargo.
