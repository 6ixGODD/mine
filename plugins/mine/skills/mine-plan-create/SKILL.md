---
name: mine-plan-create
description: "Create or update an evidence-backed, architecture-governed software implementation plan that is precise enough for independent coding agents to execute without rediscovering design decisions. Scope-first: an explicit user scope defines the planning boundary; research-backed: substantive planning requires mandatory scope-bounded web research comparing established practice; evidence-on-demand: repository inspection is proportional to scope. Use for initial project planning, incremental features, refactors, migrations, agent workflows, data/retrieval/tool contracts, or compensating plans. Routes material Design gaps back to mine-arch; never silently overrides accepted Design."
version: 0.1.13
---

# MINE Plan Create

Create an implementation-ready plan from requirements, current repository evidence, the architecture source of truth, and freshly verified external documentation.

A completed plan is an executable engineering contract, not a brainstorm, backlog, generic checklist, or restatement of the user's request. Make it precise enough that a weak implementation agent can execute it without inventing product decisions, interfaces, algorithms, file ownership, tests, or verification commands.

## Integration: MCP tools and CLI fallback

`mine-plan-create` registers and queries plans through two paths, in this
order of preference:

1. **MCP tools (preferred)** - when the current Agent runtime exposes the
   MINE MCP server (`mine mcp serve`), call the typed MCP tools. They return
   the same DTOs as the JSON CLI and never touch the execution-graph files.
2. **JSON CLI (deterministic fallback)** - when MCP is unavailable, call
   `mine --format json` commands. Never parse human output.

Never invent an MCP tool, CLI command, flag, JSON field, or lifecycle
transition that the current binary does not expose. Never edit
`docs/plan/execution-graph.toml` or `docs/plan/execution-graph.md` directly.

The accepted MCP tools `mine-plan-create` may use:

- `mine_graph_status` (no arguments) - read the current revision, branches,
  and plan count (carry `data.revision` as the expected revision on writes).
- `mine_graph_validate` (no arguments) - validate the graph after registration.
- `mine_graph_ready` (no arguments) - read the ready frontier.
- `mine_plan_show` (`id`) - look up a plan node.
- `mine_design_readiness` (`scope`?) - verify that applicable planning-blocking
  ADRs and the detailed Design are ready for the requested scope.
- `mine_adr_show` (`id`) / `mine_adr_list` / `mine_adr_validate` - inspect the
  digest-bound ADR registry before planning.
- `mine_plan_add` (`id`, `path`, `title`, `design_references`,
  `exclusive_write_paths`?, `hard_predecessors`?, `planning_scope`?) -
  register a new `DRAFT` plan node. `planning_scope` (ADR-0013) is the
  non-empty scope recorded immutably on the node; when omitted, the node
  records the configured design root directory (for this repository,
  `docs/design`).
- `mine_design_validate` (no arguments) - confirm design references resolve.

Operations `mine-plan-create` needs that are intentionally **CLI-only** (no
MCP tool exposes them, because they are release-gate transitions outside the
registered-add path):

- `mine plan release --id <id> --format json` - move a newly registered
  `DRAFT` plan into the startable frontier (`DRAFT` -> `READY` or `BLOCKED`).
  There is **no MCP tool for release**; `mine_plan_add` always creates `DRAFT`,
  so release is a mandatory CLI fallback after registration. Release re-runs
  planning readiness for the node's persisted `planning_scope` and records
  `readiness_checked_at` and `adr_registry_revision` as audit evidence
  (ADR-0013); pass `--planning-scope <scope>` to `mine plan add` when the
  plan's scope differs from the design root.
- `mine plan withdraw --id <exact-stored-id> --reason <reason> --format
  json` - retire a stale unstarted (`DRAFT`/`BLOCKED`/`READY`) registration
  (ADR-0012; CLI only). Use it instead of re-planning over obsolete
  registrations; it never applies to a started Plan.
- `mine workspace open|close` - ephemeral plan-workspace lifecycle (CLI only).

When a required operation has no MCP tool, fall back to the JSON CLI and state
the fallback explicitly.

### Canonical identity and readiness gate

Before registering a Plan, require `mine design readiness --scope <scope>`
through the read-only MCP equivalent when available. Stop and report every
applicable unresolved planning-blocking ADR; do not propose or infer approval.

Allocate or validate one canonical ID matching
`planNN(?:-NN)*(?:-CNN)?`, with two-digit numeric segments and an uppercase
`C` compensation suffix only at the end. The filename must be exactly
`docs/plan/<id>-<lower-kebab-slug>.md`; compensation Plans use the next
contiguous sibling `C` ordinal in the same lineage. Do not register legacy
IDs or create aliases. Use the actual binary's validation error as the
authoritative diagnostic when an ID or path is invalid.

## Required repository artifacts

Use these exact repository paths unless an existing repository convention is stricter:

- Design knowledge base root: `docs/design/index.md` (progressive disclosure; MINE owns `docs/design/`)
- Execution plans: `docs/plan/`
- Execution graph machine source: `docs/plan/execution-graph.toml`
- Generated graph view: `docs/plan/execution-graph.md`
- Implementation and review reports: `docs/plan/reports/`
- Repository working agreement: root `AGENTS.md`

Read the bundled templates before producing artifacts:

- [Plan template](references/plan-template.md)
- [Execution graph template](references/execution-graph-template.md)
- [Parallel execution protocol template](references/parallel-execution-protocol-template.md)

Do not reference a bundled or repository file unless it actually exists. Links in a completed plan must resolve to a real local path or a real external URL that was opened and verified during this run.

## Non-negotiable rules

1. Inspect the actual repository evidence relevant to the requested scope before designing. For an explicit-scope invocation, inspection is proportional to that scope.
2. Inspect the Design relevant to the requested scope deeply, not just its headings or search snippets. Respect accepted architecture by default; do not broaden into unrelated Design leaves.
3. Perform real web search and open the authoritative pages during every substantive planning run, bounded to the requested scope and its real dependencies.
4. Prefer official documentation, standards, specifications, source repositories, release notes, and primary research over secondary summaries.
5. Record verified links and the exact claim each source supports inside the plan.
6. Never finalize an implementation-ready plan when required web research or page fetching is unavailable. Report the missing research capability and leave the plan explicitly `DRAFT` or do not create it.
7. Respect accepted architecture and Design by default. Inspect SOLID concerns (ownership, dependency direction, new abstraction/interface/component boundaries) only when the requested scope actually crosses a boundary or introduces such a structure. Do not use a plan to silently redesign the system.
8. If the target work requires an architecture change, update the `docs/design/` knowledge base first (the affected leaf/index), then make the plan cite the updated design paths and anchors. If planning reveals a material Design gap, stop that portion and route the decision back to `mine-arch`; do not silently solve it inside the Plan.
9. Do not preserve obsolete implementations merely because an earlier plan created them. Unless the user explicitly requires compatibility, change the target implementation directly and schedule cleanup of superseded fields, interfaces, parameters, adapters, migrations, aliases, and shims.
10. Preserve unrelated user changes and never invent evidence, commands, files, APIs, tool names, test results, or external behavior.

## Scope-first principle

`mine-plan-create` is **scope-first, research-backed, evidence-on-demand**:

- **Explicit user scope is the authoritative planning boundary.** When the
  invocation names a requirement, component, previous discussion, Design
  change, or implementation target, treat that scope as authoritative. Stay
  within it: inspect only the Design, code, tests, configuration, graph
  state, reports, and external material needed to make that scope
  executable. Do not broaden into unrelated repository areas, audit
  unrelated Design leaves, or inspect unrelated accepted reports. Uncertainty
  inside the scope may expand evidence collection.
- **Bare invocation may enter discovery mode.** A bare invocation with no
  explicit planning target may perform broader discovery to identify the next
  unplanned or actionable Design work - but prefer the smallest coherent
  planning frontier. Do not turn every bare invocation into an unconditional
  full-repository audit.
- **Research stays mandatory and scope-bounded.** See Phase 4.

## Phase 1: Establish the planning mode

Classify the request before editing:

### Explicit-scope mode (primary)

The user supplied a concrete scope. In this mode:

1. Resolve the scope: restate the concrete target (requirement, component,
   Design change, discussion topic) from the invocation.
2. Read the relevant accepted Design (the leaves and necessary parent/index
   context that govern the scope) - not the whole tree.
3. Inspect the relevant implementation, tests, configuration, graph state,
   and predecessor reports only when they materially affect the requested
   work.
4. Perform mandatory scope-bounded external research (Phase 4).
5. Create precise Plan(s) for that scope; register and release them.

Do not: audit unrelated modules, reread the entire Design tree, inspect every
accepted report, research unrelated technologies, or perform a ceremonial
repository-wide review.

### Bare-invocation discovery mode

No explicit planning target. In this mode, broader discovery is allowed to
identify what should be planned next:

1. Inspect current Design and the execution graph frontier.
2. Consider recent accepted work and repository state relevant to the next
   unplanned or actionable Design work.
3. Identify the smallest coherent planning frontier and plan that.

### Zero-plan / initial architecture mode

Treat the repository as zero-plan when one or more of these are true:

- no accepted execution plans exist;
- the execution graph does not exist or contains no accepted baseline;
- architecture is absent, skeletal, contradictory, or not grounded in current code;
- the user is establishing the first implementation sequence for a new project.

In zero-plan mode:

1. Ensure `mine-arch` has produced or refreshed the architecture source of truth and repository quality gates.
2. Prefer multiple bounded research agents in parallel when the host supports subagents, teams, delegation, or isolated agent sessions.
3. Research the system broadly enough to establish a coherent baseline before decomposing implementation.
4. Produce the initial execution graph and identify the smallest useful accepted baseline.
5. Maximize safe parallelism, but do not parallelize unresolved shared contracts or files with unclear ownership.

### Incremental mode (within any of the above)

In an established repository with an accepted baseline:

1. Identify the accepted baseline and the target plan frontier for the scope.
2. Inspect the architecture, reports, interfaces, and code touched by the requested change - not unrelated areas.
3. Detect whether the request is a new feature, direct correction, refactor, migration, cleanup, or compensating plan.
4. Do not revise an immutable handed-off or executed plan. Update architecture and create a new next-numbered compensating plan only for a
   substantial correction (material Design change, replaced core approach, a new independent work package, or major scope expansion). A narrow,
   local, fully-verifiable correction — including one discovered during independent review or during release closure — is the reviewer's or
   executor's direct fix, documented in its own report, not a new plan. Do not create a plan merely to preserve reviewer/implementer role
   purity.

When an immutable `IN_PROGRESS` Plan is objectively infeasible, do not edit,
replace, or pretend to complete it. Planning may define the approved
stored-anchor/schema recovery and the later compensation sequence only after
Design and any material ADR decision authorize that stored-anchor schema recovery. The stopped
Plan remains frozen until an independent reviewer records canonical evidence
and uses the bounded active-rejection transition; only then register the next contiguous sibling compensation and reuse the existing lineage worktree.

## Phase 2: Gather evidence proportional to scope

Read, in order, only what the requested scope requires:

1. User requirements and supplied artifacts (the explicit scope is authoritative).
2. Root `AGENTS.md`.
3. The Design leaves governing the requested scope, plus the necessary parent/index context - not the whole tree. In discovery mode, read the index and relevant frontier leaves.
4. Query the execution graph through the final `mine` MCP tools or `mine --format json`; use the generated Markdown only as a readable view.
5. Predecessor plans and implementation/review reports only when they materially affect the requested work.
6. Manifests, lockfiles, toolchain files, CI, deployment files, and Git status relevant to the scope.
7. Relevant source code, tests, schemas, and generated artifacts for the scope.
8. Current official documentation and best practices fetched from the web (scope-bounded; see Phase 4).

Do **not** require by default: reading the entire Design tree; reviewing every
accepted Plan or report; auditing every manifest, CI file, deployment file, or
unrelated subsystem; or repository-wide evidence matrices. Expand repository
inspection only when a concrete planning question requires more evidence.

Keep these categories visibly separate:

- current implemented reality;
- accepted architectural target;
- new requested target;
- assumptions;
- unresolved material decisions;
- bounded local implementation decisions.

An evidence matrix is optional and scope-scaled: include one when the scope
spans several areas or subsystems; do not produce a repository-wide matrix by
default.

## Phase 3: Parallel research (optional, scope-scaled)

When parallel agents are available **and** the requested scope genuinely spans
several independent research questions, assign bounded research lanes with
explicit questions and expected output. Useful lanes include:

1. Repository reality for the scope.
2. Architecture boundaries, ownership, and lifecycle relevant to the scope.
3. Official framework/library/API documentation and version constraints.
4. Data model, transactions, concurrency, consistency, and migrations relevant to the scope.
5. Security, privacy, authorization, secrets, and failure handling relevant to the scope.
6. Toolchain, static checks, tests, CI, build, deployment, and observability relevant to the scope.
7. Parallel execution decomposition and shared-file collision analysis.

Each lane must return evidence, source links, concrete implications, unresolved decisions, and risks. Synthesize the results yourself; do not paste mutually contradictory subagent reports into the plan.

A single coherent scope does not require parallel lanes. When the host cannot run subagents, perform the needed research sequentially. Do not name or invoke a tool that the host does not expose.

## Phase 4: Mandatory external research (scope-bounded)

External research is **mandatory for every substantive planning run** and is
part of MINE's engineering discipline. It is also **bounded to the requested
scope**: search for the external technologies, protocols, standards,
services, or architecture patterns the requested scope depends on - not
unrelated repository subsystems.

Research should answer questions such as:

- How is this problem normally solved in mature systems?
- Is there already a standard abstraction, protocol, pattern, or library for this?
- What do the official framework or platform docs recommend?
- How do mature open-source projects implement the same mechanism?
- What failure modes are already well known?
- What implementation conventions should be followed instead of inventing a local mechanism?

The purpose is **not** to satisfy a documentation ritual: it is to avoid
closed-world design and unnecessary invention. Research compares established
approaches (concurrency control, transactional outbox, retry/backoff,
idempotency, auth/session lifecycle, migration strategies, locking models,
worker scheduling, state machines, test isolation, configuration ownership,
and similar) - not merely to verify API syntax.

For example, a Passkey-login scope reasonably includes WebAuthn/Passkey
standards, the selected framework's integration guidance, credential and
challenge lifecycle, and known security/compatibility constraints - but does
not justify researching unrelated storage, CI, deployment, or observability
subsystems unless the scope actually depends on them.

For each source:

1. Open the actual page; do not rely on a search-result snippet.
2. Confirm it applies to the repository's selected version or current supported version.
3. Record the page title, organization, URL, date accessed, verified claim, and concrete design or implementation implication.
4. Prefer sources in this order:
   - official product/framework documentation;
   - normative standards and specifications;
   - official source repositories, examples, release notes, and migration guides;
   - primary research;
   - reputable secondary sources only when primary material is insufficient.
5. Record conflicting guidance rather than hiding it.
6. Never cite a source that was not opened.

The plan must contain a **Research source register**. A raw list of links is insufficient; every link must support a specific plan decision, step, test, or risk.

## Responsibility boundary with `mine-arch`

- `mine-arch` owns: requirements interpretation; target architecture;
  durable engineering decisions; component boundaries; public contracts;
  important data, lifecycle, security, consistency, deployment, or ownership
  decisions; changing accepted Design.
- `mine-plan-create` owns: turning accepted Design into executable
  implementation work; resolving bounded implementation details;
  decomposing work; determining dependencies and write scopes; defining
  verification; identifying safe parallelism.
- If planning reveals a missing decision that materially changes product
  behavior, architecture, public API, persistent data semantics, a security,
  ownership, or compatibility boundary, deployment topology, or another
  durable engineering contract: **stop planning that portion and route the
  decision back to `mine-arch`**. Do not silently solve it inside the Plan.
- Research may validate accepted Design, refine implementation details, or
  reveal risks and incompatibilities - but it must **never silently replace
  accepted Design** because another project or article uses a different
  approach. If research shows accepted Design is materially wrong or
  incomplete, report the conflict and return that decision to `mine-arch`.

## Phase 5: Resolve decisions

Ask the user before finalizing when a decision materially changes any of these:

- product behavior or scope;
- architecture style or component ownership;
- persistent data model, migration, or rebuild policy;
- public API, event, file format, model/tensor, prompt, or tool contract;
- authorization, privacy, safety, secret, or trust boundary;
- compatibility requirements;
- deployment topology, external mutation, cost boundary, or operational responsibility;
- acceptance criteria.

Do not interrupt for bounded implementation choices that do not change those contracts. Resolve such choices using current architecture, official documentation, repository convention, and the smallest maintainable design. Record them under **Local decisions made by the planner**.

## Phase 6: Architecture gate (conditional, no checklist theater)

Respect accepted architecture by default. Before writing implementation steps, verify traceability from the requested scope to its governing architecture, and inspect deeper only when the scope actually crosses a boundary or introduces a new abstraction/interface/component/dependency structure:

- ownership and dependency direction for the affected boundary;
- state ownership, lifecycle, idempotency, concurrency, retries, and failure semantics for the affected component;
- no duplicate source of truth in the new structure;
- no speculative abstraction or interface with only imagined consumers;
- SOLID concerns (Single Responsibility, Open/Closed, Liskov, Interface Segregation, Dependency Inversion) applied to newly introduced or changed abstractions, not as a repository-wide checklist.

Do not require a full SOLID checklist or a repository-wide architecture audit on every Plan. Inspect architecture more deeply when the requested scope crosses a real boundary; check ownership/dependency direction when affected; check SOLID concerns when a new abstraction, interface, component boundary, or dependency structure is actually being introduced or changed.

If the accepted design knowledge base cannot support the request cleanly, update the relevant `docs/design/` leaf/index first (or route the decision to `mine-arch` when it is a material gap). Include exact design paths (and anchors where applicable) in the plan.

## Phase 7: Design for parallel execution

When any plan contains parallel implementation lanes, create or update `docs/plan/parallel-execution-protocol.md` from the bundled template. The protocol is a repository scheduling contract and must name reserved shared files, lane ownership, integration ownership, and duplicate-owner prevention.

Maximize useful parallelism so multiple agents can work efficiently without corrupting shared contracts.

1. Build a dependency DAG of work packages.
2. Separate contract-defining work from independent implementation work.
3. Identify parallel groups and explicit join gates.
4. Give each work package a single owner and a non-overlapping primary file scope.
5. Assign a single serial owner for shared root files and high-conflict artifacts, including as applicable:
   - package manifests and lockfiles;
   - workspace configuration;
   - central dependency-injection registries;
   - database migrations and generated schemas;
   - root lint/test configuration;
   - shared API specifications and generated clients;
   - execution graph and reports.
6. State which files an agent may edit, may read but not edit, and must not touch.
7. Define the artifact each package hands to downstream packages.
8. Define integration order and exact join verification.
9. Use isolated branches/worktrees when available. If agents share one worktree, serialize all overlapping files and root configuration changes.

Parallelism is not a goal when it causes duplicated work, conflicting schemas, premature stubs, fake interfaces, or repeated lockfile churn.

## Phase 8: Write the executable plan

Create one realistically sized plan for the next coherent increment under `docs/plan/` using a numbered kebab-case name, for example:

`docs/plan/03-session-storage-and-recovery.md`

Use the bundled [plan template](references/plan-template.md). Every work package and implementation step must specify:

- purpose and governing architecture sections;
- prerequisites and accepted upstream evidence;
- exact target files, directories, symbols, interfaces, schemas, or generated artifacts;
- current behavior and required final behavior;
- input, output, error, lifecycle, transaction, concurrency, and security semantics;
- algorithm or state transition where non-trivial;
- configuration and dependency changes;
- deletions and cleanup of superseded implementation;
- edge and failure cases;
- deterministic tests and fixtures;
- exact narrow verification commands and broader integration gates;
- expected observable outcome, not merely “tests pass”;
- artifacts delivered to downstream work;
- suggested cohesive commits.

A step such as “implement the service,” “add tests,” “update the API,” or “follow best practices” is invalid unless expanded into concrete files, contracts, behavior, cases, and verification.

Do not guess physical database columns, API responses, source document fields, runtime tool names, framework options, or command flags. Verify them first or mark the plan `DRAFT` with the exact missing evidence.

## Phase 9: Quality and verification matrix

Use the actual project quality gates defined by architecture and `AGENTS.md`. Do not impose Python-only tools on Go, TypeScript, Rust, or mixed-language projects.

The plan must include a verification matrix:

| Scope | Command | Preconditions | Expected evidence | Owner/work package |
|---|---|---|---|---|

Cover as applicable:

- formatter and formatting check;
- linter and static analysis;
- type checking;
- unit, integration, contract, migration, end-to-end, security, and smoke tests;
- generated artifacts and schema drift;
- build/package/container validation;
- deployment configuration validation;
- runtime probes for lifecycle, concurrency, error paths, and recovery;
- `git diff --check` and explicit changed-file audit.

Never label an unrun command, timeout, unavailable dependency, ignored diagnostic, or non-zero result as passing.

## Phase 10: Register the plan through MINE

Do not edit `docs/plan/execution-graph.toml` or `docs/plan/execution-graph.md` directly. After the plan document is complete:

1. Read the current graph revision: call `mine_graph_status` (MCP) or `mine
   graph status --format json` (CLI fallback). The envelope's `data.revision`
   is the current revision; carry it as `expected_revision` on the write.
   Before the registration write, run scoped design readiness and confirm the
   requested canonical Plan ID/path. A failed readiness check is a planning
   blocker, not permission to register a DRAFT node.
2. Register the plan: call `mine_plan_add` (MCP) with `id`, `path`, `title`,
   `design_references`, and optional `exclusive_write_paths` /
   `hard_predecessors`; or `mine plan add --format json` (CLI fallback) with
   `--id`, `--path`, `--title`, at least one `--design-ref`, and any `--write`
   / `--hard` (repeat for multiple values). Registration always creates a
   `DRAFT` node.
3. **Release the plan** (CLI-only - no MCP tool exposes release): call
   `mine plan release --id <id> --format json` to move the new `DRAFT` node
   into the startable frontier (`DRAFT` -> `READY` when every hard predecessor
   is `ACCEPTED`; `DRAFT` -> `BLOCKED` otherwise). This is a mandatory CLI
   fallback because `mine_plan_add` always creates `DRAFT`.
4. Validate the graph: call `mine_graph_validate` (MCP) or `mine graph
   validate --format json` (CLI fallback) after registration and release.
5. Report the returned revision before/after and the new plan's status
   (`DRAFT`, `READY`, or `BLOCKED`).

The accepted MINE CLI and MCP tools read the current revision themselves
before mutating under the lock, so an explicit `expected_revision` argument is
**not** required; every mutation envelope emits `revision_before`/
`revision_after`. Never parse human output. Never edit the graph files
directly. If the installed command contract differs from this draft, use the
actual implemented contract and update this Skill before release.

## Final review gates

Before finishing, verify all of the following:

- The cited `docs/design/` leaves/anchors exist and every cited section is real.
- Architecture changes were made before the dependent plan; material Design gaps were routed back to `mine-arch`, not silently solved.
- Mandatory **scope-bounded** web research was completed using opened authoritative pages.
- Every external link is real, current enough for the decision, and tied to a claim.
- The plan does not contain a dead local reference.
- Accepted architecture is respected; SOLID/speculative-abstraction concerns were checked where the scope actually introduces or changes a boundary.
- Current reality, target design, assumptions, and decisions are separate.
- Every work package has explicit ownership, file scope, dependencies, output, tests, and join gate.
- Shared files have one serial owner.
- Parallel groups are safe rather than merely numerous.
- Every implementation step is concrete enough to execute without rediscovering design decisions.
- Language-specific checks come from the repository's architecture and actual toolchain.
- Obsolete implementation is removed or explicitly scheduled for removal; no accidental compatibility debt is introduced.
- MINE successfully registered the plan and the validated graph accurately represents dependencies and status.
- No handed-off or executed immutable plan was rewritten.
- No unrelated files are modified or staged; the invocation scope was respected.

Finish with the created or updated architecture sections, plan path, graph status, parallel groups, unresolved gates, and the exact next executable work packages. Do not claim readiness when any required evidence is absent.

## Shared toolchains and Plan-isolated mutable outputs

Reuse Root-managed toolchains, dependency downloads, virtual environments, and
only caches proven read-only or safely content-addressed. Do not create a
second `.venv`, `venv`, `target`, `node_modules`, or equivalent environment or
cache directory in a linked Plan worktree.

Mutable build, test, generation, coverage, packaging, and runner outputs must
use `<Root>/.mine/build/<plan-id>/<tool-or-output-class>/`. For Rust, set
`CARGO_TARGET_DIR` to the exact Plan namespace, for example
`<Root>/.mine/build/<exact-plan-id>/cargo-target`; never use Root `target` or a
worktree-local `target`. If a measured output is predictably very large,
record the estimate and pause for explicit owner agreement on serialization,
a coarser exclusive namespace, and cleanup; never fall back automatically to a
globally shared mutable directory.
