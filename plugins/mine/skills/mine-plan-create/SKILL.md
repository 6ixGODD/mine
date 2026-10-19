---
name: mine-plan-create
description: "Create or update an evidence-backed, architecture-governed software implementation plan that is precise enough for independent coding agents to execute without rediscovering design decisions. Scope-first: an explicit user scope defines the planning boundary; research-backed: substantive planning requires mandatory scope-bounded web research comparing established practice; evidence-on-demand: repository inspection is proportional to scope. Use for initial project planning, incremental features, refactors, migrations, agent workflows, data/retrieval/tool contracts, or compensating plans. Routes material Design gaps back to mine-arch; never silently overrides accepted Design."
version: 0.2.0
---

# MINE Plan Create

Turn the owner's explicit scope and accepted Design into executable increments.
A bare invocation discovers the smallest useful unplanned frontier, not an automatic
full-repository audit. Do not implement the Plan or silently redesign accepted contracts.

## Authority and inputs

Resolve authoritative Root from Git inventory and read AGENTS.md, requirements,
Root .mine/config.toml/marker and relevant Design leaves. Fresh roots are
.mine/design/index.md and .mine/plan/; configured paths are authority, not filename
guesses or a linked checkout's stale graph. Read predecessor reports/commits when
they materially affect this scope. Distinguish implemented reality from accepted
target, local choices and unresolved material decisions.

Check installed binary/help/JSON/MCP schemas. Prefer available typed read/registration
tools and use JSON CLI where there is no tool. Do not invoke future candidate features
on live older workspaces, edit either graph file, or redirect active legacy layout.
Workspace open/release/prepare/clarify/operation recovery remain CLI-only where the
installed catalog has no corresponding tool; never invent a preparation MCP tool.

Read the [Plan](references/plan-template.md), [parallel protocol](references/parallel-execution-protocol-template.md)
and [generated view](references/execution-graph-template.md) references before authoring.

## Scope, research and decisions

1. Bound the user-visible increment, non-goals and entry point -> producer -> consumer
   chain. Inspect the affected source, config, tests and accepted Design rather than
   trusting filenames or a planner's paraphrase.
2. Every substantive planning run searches and opens scope-bounded primary sources.
   Compare established practice, actual versions, alternatives and failure modes;
   register title/organization/version/URL/date/claim/implication in the Plan. Missing
   mandatory research leaves the scope DRAFT or uncreated, never planning-ready.
3. Resolve internal implementation choices from Design and repository conventions.
   Escalate changed behavior, architecture, API, persistence, security/authority,
   compatibility, deployment responsibility or acceptance criteria to mine-arch.
   Update Design and complete applicable owner ADR approval before dependent planning.
   Never infer approval or change an approved body.
4. Define cohesive responsibility/namespaces and exclusions first. Physical paths
   locate the implementation; do not freeze every future internal file. An executor
   may split/add internal files inside approved responsibility/mapping without repeated
   permission if contracts, acceptance and shared ownership do not change.
5. Define real inputs/outputs/errors, invariants, cleanup, deterministic positive and
   negative tests, exact narrow and broader gates, downstream evidence and commit
   boundaries. Avoid vague "implement service" steps, unused abstractions or shims
   preserving abandoned internal designs.

## Identity and frozen contract

Allocate canonical planNN(?:-NN)*(?:-CNN)? IDs with two-digit numeric segments and
uppercase terminal C. Files are <configured-plan-root>/<id>-<lower-kebab-slug>.md.
Do not create numeric-only aliases or encode arbitrary stage labels in IDs.
Plans freeze on handoff/start/report; never edit/rename/replace them afterward.

Compensation is the next contiguous sibling C ordinal, inherits the rejected node's
complete ordered hard predecessors exactly and reuses its base lineage worktree.
Never depend on the rejected node itself or choose new scheduling edges. Its own
mutable output namespace differs from its predecessor. A narrow fully verifiable
local fix against already accepted behavior is not a reason to create another Plan.

An architect's real interface/invariant foundation is an ordinary explicitly assigned
work package with a demonstrated consumer and independent acceptance, not a sixth
lifecycle or a Root implementation shortcut. Use an existing owning Plan if applicable.

## Ownership and scheduling

Read the complete native graph wave/show snapshot: held IN_PROGRESS/IMPLEMENTED
owners, original/effective authority, policy/provenance, pending closure/checkpoint,
lineage/worktree, output and generator resources, selected/excluded IDs and reasons.
READY is eligibility, not ownership; advisory waves do not grant start authority.
Do not drop invalid/missing-policy owners, treat stopped workers as released, or
start another executor after a timeout.

Conservative registration requires a nonempty physical mapping. Semantic registration
uses the supported ownership-input JSON with policy_path, approving_adr and ownership,
not a caller-authored approval. The exact external JSON policy must be bound by the
approved applicable ADR's unique mine-ownership-policy fence (schema_version,
policy_path, policy_sha256). Native code derives policy actor/digests/provenance.
No arbitrary policy is authorized by a general principle ADR; registry and graph
revision counters must not be substituted for one another.

Safe parallel lanes have one owner per responsibility, explicit exclusions/mappings,
start/join gates and a named shared-file integration owner. Namespace overlap is safe
only under the actual approved partition/shared-declaration contract; schemas,
algorithms, migrations, lockfiles, mutable outputs and whole generators remain
exclusive absent that contract. Record generation inputs -> outputs -> seal closure;
one owner regenerates the whole combined set at integration, not stale lane copies.
Every required gate must have authority to produce its outputs.

A bounded mapping/generator-closure correction to a frozen Plan uses native
plan clarify --id <id> --expected-revision <observed> --input <contained-json>.
The request binds exact Plan/body/workspace/lineage/branch, actor/reason, approved
policy and clarification tail; preserve original declarations and append provenance.
It cannot change behavior/dependencies/acceptance/architecture. A missing historical
digest is absent history: only explicit approved first-baseline activation can attest
current bytes. No prompt-only scope override, rehash-after-tamper or fallback policy.

## Register and release

Finish scoped Design readiness first. Commit the exact Plan and other operation-owned
inputs explicitly before mutation so Root/index is clean. Never stage unrelated work.
Read Root graph revision immediately before each new write and provide that exact
expected_revision; actor is explicit, not Git user.name or a future executor.

Example CLI registration uses actual installed flags:
~~~text
mine plan add --actor <actor> --expected-revision <R> --id <id> --path <registered-path> --title <title> --design-ref <leaf> --write <mapping> --format json
mine plan release --actor <actor> --expected-revision <next-observed-R> --id <id> --format json
~~~
Use --ownership-input instead of inventing semantic flags; conservative --write is
repeatable and nonempty. MCP mine_plan_add carries actor, expected_revision, id,
path, title, design_references and its supported optional ownership_input/mappings/
predecessors/scope. It creates DRAFT; release is an explicit CLI fallback to READY
or BLOCKED, not an automatic start.

Inspect returned native operation/checkpoint completion and actual revision.
A persisted graph plus pending checkpoint is not a completed registration/release.
Query operation show --operation-id <same-id>, then use the real same-operation
recovery contract before any new write. Do not manually graph-commit/fast-forward,
refresh the old request's revision internally, repeat Git effects or register a
duplicate after a lost response. Truly stale new decisions require reread/re-evaluation.

Retire only authorized unstarted obsolete nodes through lossless withdrawal with
its explicit operation ID/actor/observer; never erase started history. Historical
compensation-predecessor repair is only the binary's bounded unstarted evidence
case, never an edge editor. Validate graph after completed mutations.

## Workspace, evidence and handoff

Root stays on configured integration while a workspace is open; Plan implementation
requires native prepare/start in its registered linked worktree, never shared Root
switching. Reuse Root environments/downloads; isolate every language's mutable outputs
under <Root>/.mine/build/<exact-plan-id>/<output-class>/. No worktree venv/target/
node_modules or shared writable parallel build directory. Exceptional measured disk
cost requires owner agreement on strategy and cleanup before building.

The matrix distinguishes RUN, REUSED, INVALIDATED, NON_REUSABLE and NOT_RUN.
Reuse only input-bound applicable evidence; independent/native probes remain required.
Report exact Plan paths, graph revision/status, dependencies/held blockers, startable
frontier and next assigned action. Do not claim registration/checkpoint completion,
execution, acceptance or release without its actual evidence.


## Canonical durable bounds

All new IDs follow planNN(?:-NN)*(?:-CNN)? (two-digit numeric segments, uppercase
terminal C). Files are <configured-plan-root>/<id>-<lower-kebab-slug>.md.
Compensation uses the next contiguous sibling C ordinal in the same lineage.
Never edit the configured execution-graph.toml or generated execution-graph.md
directly. Root owns graph/config/locks/revision even for linked callers.
For Rust set CARGO_TARGET_DIR to that exact Plan's isolated output class; the
same mutable-output isolation applies to every other language, not just Cargo.
