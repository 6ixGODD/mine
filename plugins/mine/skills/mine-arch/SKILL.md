---
name: mine-arch
description: Create or overhaul a repository's modular architecture source of truth rooted at .mine/design/index.md and initialize the repository's engineering governance. Use for new projects, weak or stale architecture, major boundary changes, or repository setup. Inspects real code and requirements, performs mandatory official-documentation research, designs abstract architecture before detailed design, applies SOLID without speculative abstraction, creates or repairs language-specific formatter/linter/type/test/CI configuration, establishes durable AGENTS.md rules, and validates the resulting quality gates.
version: 0.2.0
---

# MINE Architecture

Design target architecture from the owner's requirements, not merely from current code.
Keep current implementation, accepted target, assumptions and open decisions distinct.
Read AGENTS.md, requirements and the affected code before changing Design. Architecture
does not implicitly authorize product implementation.

## Locate real authority

Resolve Root from the first Git worktree inventory record. Read Root .mine/config.toml
and the ownership marker; use its configured Design/Plan/ADR paths, never a linked
checkout's graph. Fresh 0.2 governance is .mine/design/index.md, .mine/design/decisions/
and .mine/plan/. User docs/ is not the governance namespace and must not be moved.

Check the installed mine version/help/schema before use. Do not install or invoke a
future candidate against an older live workspace. An active legacy layout is completed
using its existing binary; terminal cutover is a separate source-authorized release
operation, never an init/sync-time migration or a prompt-only config rewrite.

Read [the outline](references/architecture-outline.md) and
[AGENTS agreement](references/AGENTS.template.md) before authoring. These are adaptable
checklists, not a requirement to generate speculative components or a giant manuscript.
One contract belongs in one leaf; indexes orient and link to authoritative leaves.

## Architecture and approval

1. Bound the requested problem, real users/entry points, invariants, trust and ownership
   boundaries, failure paths, non-goals and measurable constraints. Inspect code only
   as far as those questions require. Use parallel read-only research when genuinely
   useful and authorized; it grants no concurrent write authority.
2. Search and open official/primary documentation for material external semantics and
   selected versions. Record exact URLs, access date, verified claims, alternatives
   and concrete implications. Unavailable required research leaves affected decisions
   unresolved; do not replace it with search snippets.
3. Extract material decisions as separate ordinary DRAFT ADRs in the configured
   decisions directory. Use the real native ADR add/propose/show contract, present
   exact proposed bodies/digests and request explicit owner approval. Never self-approve,
   infer approval from silence, edit approved bytes or claim an approving principle ADR
   authorizes a later unbound policy. Explicit approval of an identified batch is
   recorded against those exact proposals, not guessed future decisions.
4. After approval, write abstract boundaries and dependency direction first, then
   required detailed contracts, state/error/recovery/operational semantics and gates.
   Approved ADRs are superseded by new decisions, not reversed in place. Scoped
   design readiness must pass before dependent planning. Project-specific detail
   coverage schemes are optional user conventions, not a universal MINE state.
5. Select actual language-specific formatter/linter/type/test/build/CI gates. Merge
   existing configuration, use real dependency tooling, run relevant checks and record
   first failures/unavailable evidence. Do not weaken checks or create unused ports,
   factories, TODO implementations or fake success.
6. Update durable AGENTS.md rules and indexes. Validate Design/ADR through available
   read-only MCP tools or JSON CLI fallback. Graph validation applies only when the
   configured live workspace exists; a stable graph-less tree is intentional.

## Responsibility-based scope and real foundations

Define namespace/responsibility, supported behavior and exclusions, real consumer,
physical mapping, shared-file integration owner and generator input/output closure.
Directory membership alone is not semantic authority. Approved semantic partitions
may allow mapped overlap; unpartitioned algorithms, schemas, migrations, lockfiles,
mutable outputs and whole generators remain exclusive. Conservative repositories
retain explicit nonempty path authority without inventing a policy approval.

A concrete semantic mapping needs an external policy JSON bound by one approved ADR
body's unique mine-ownership-policy fence: schema_version=1, normalized policy_path
and SHA-256 of its exact bytes. Actual registry approval actor/digest/applicability
are native observations, not caller approved=true. Graph and ADR registry revisions
are different counters. Mapping changes require a new approved binding.

Architects may implement types, consumer-owned interfaces, invariants and a minimum
real foundation only when the owner explicitly assigns an ordinary execution Plan/work
package. Applicable abstract decisions must be approved; unrelated detail need not be
finished first. The work uses normal prepare/start, registered linked checkout,
scoped commits, real consumer/invariant tests and IMPLEMENTED handoff. An existing
Plan owning that responsibility is reused rather than creating ceremonial parallel
work. No sixth Skill/state, Root coding exception or self-acceptance exists.

An ordinary arch invocation writes only Design, governance and authorized quality
configuration. It does not automatically create/execute Plans, switch branches,
commit, merge or publish. Source implementation and planning require explicit
assignment; ADR approval alone is not an execution request. Read-only probes are
default; explicitly scoped experiments remain isolated, disposable and unmerged.

## Coordination and recovery

Use native complete graph wave/show snapshots when planning around active work:
IN_PROGRESS and IMPLEMENTED retain ownership through pending review/integration.
Do not infer freedom from READY-only lists, worker silence or a clean checkout.
Missing semantic policy fails closed and never downgrades an owner to conservative.

When explicitly authorized to perform a native write, commit only the operation's
owned inputs first, read Root revision and supply that exact expected revision plus
the required explicit actor. Native graph checkpoints commit only their graph pair.
Check returned operation/checkpoint completion; a failed response may describe persisted
pending work, not rollback. Query/resume the same operation before further writes.
Never manually fast-forward or commit a pending checkpoint, repeat effects, or create
a compensation for a transport/resource failure. No fabricated future CLI/MCP tools.

If an immutable active Plan is objectively infeasible, preserve it and its rejected
lineage. Material replacement Design needs owner-approved ADRs and actual independent
review/stopped-writer evidence before the bounded native rejection path. Retire only
authorized unstarted stale registrations via lossless withdrawal; do not erase started
history, infer a stopped writer, or repair the graph by hand.

## Workspace, output and handoff

Root remains on configured integration branch while a workspace is open; only release
integration may move it to configured stable. Plan branches live only in their registered
linked worktrees. Rejection's next sibling compensation reuses that lineage checkout;
each Plan node still has its own mutable output namespace.

Reuse Root toolchains, downloads, venv and demonstrably safe immutable caches. Isolate
all languages' mutable build/test/generated/coverage/package outputs under
<Root>/.mine/build/<exact-plan-id>/<output-class>/; never create worktree target,
node_modules, .venv or equivalent duplicate environments. Measure exceptional disk
cost before building and agree with the owner on serialization/shared output/cleanup;
never silently share mutable output or clean another owner's artifacts.

Report changed Design/configuration, actual command results, scoped readiness,
unresolved decisions, real implemented versus pending foundation behavior and the
next owner-invoked action. Distinguish IMPLEMENTED from independently ACCEPTED.


## Canonical durable bounds

All new IDs follow planNN(?:-NN)*(?:-CNN)? (two-digit numeric segments, uppercase
terminal C). Files are <configured-plan-root>/<id>-<lower-kebab-slug>.md.
Compensation uses the next contiguous sibling C ordinal in the same lineage.
Never edit the configured execution-graph.toml or generated execution-graph.md
directly. Root owns graph/config/locks/revision even for linked callers.
For Rust set CARGO_TARGET_DIR to that exact Plan's isolated output class; the
same mutable-output isolation applies to every other language, not just Cargo.
