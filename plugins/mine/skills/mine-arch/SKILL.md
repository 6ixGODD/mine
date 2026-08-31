---
name: mine-arch
description: Create or overhaul a repository's modular architecture source of truth rooted at docs/design/index.md and initialize the repository's engineering governance. Use for new projects, weak or stale architecture, major boundary changes, or repository setup. Inspects real code and requirements, performs mandatory official-documentation research, designs abstract architecture before detailed design, applies SOLID without speculative abstraction, creates or repairs language-specific formatter/linter/type/test/CI configuration, establishes durable AGENTS.md rules, and validates the resulting quality gates.
version: 0.1.13
---

# MINE Architecture

Create or update the repository architecture and make the repository capable of enforcing it.

MINE Is Not Everyone's. This skill is not limited to writing prose. It owns:

- architecture discovery and design;
- the modular, progressive-disclosure design knowledge base rooted at `docs/design/index.md`;
- repository-wide engineering agreements in `AGENTS.md`;
- selection and initialization of language-appropriate quality gates;
- creation or repair of missing formatter, linter, type-checker, test, build, pre-commit, and CI configuration;
- validation that the documented commands actually run.

`mine-arch` is **requirement-first** (governed by `docs/design/integrations/skills.md` and `docs/design/governance/design-knowledge-base.md`). It designs target architecture from user requirements and may intentionally differ from current code; it must **not** silently treat current code as the target architecture when the user's requirement changes that target. This makes it the requirement-first counterpart to `mine-sync`, which is code-first by intent.

## Fixed outputs (the MINE design knowledge base)

MINE owns `docs/design/` (ADR-0006). The design tree is a **progressive-disclosure knowledge base**, not a single manuscript:

```text
docs/design/
  .mine-design.toml       # ownership marker (schema_version, managed_by=MINE, repository_id, created_at)
  index.md                # root index, links to domain indexes
  <domain>/
    index.md
    <component>/
      index.md
      <leaf>.md           # exact contracts live here
```

- The root index is `docs/design/index.md`; every design area owns an `index.md`.
- Root index links to domain indexes; domain indexes link to component indexes; leaves contain exact contracts.
- One contract has one authoritative leaf; an index may summarize a child in one or two sentences but does not duplicate its full contract.
- Do not create a competing single-document architecture source, and do not rename `docs/design/index.md` to something else or introduce a competing `architecture-and-detailed-design.md` file.

Root `AGENTS.md` must name `docs/design/index.md` (and `.mine-design.toml`) as the design source of truth, not a fixed single-document path.

Read the bundled references before working:

- [Architecture outline](references/architecture-outline.md) — a completeness checklist, not a single-document template.
- [AGENTS.md template](references/AGENTS.template.md) — durable repository-wide agreements only.

These references are patterns, not text to copy blindly. Tailor them to the actual repository and remove inapplicable placeholders.

## Integration: MCP tools and CLI fallback

`mine-arch` integrates with the MINE execution graph through two paths, in this
order of preference:

1. **MCP tools (preferred)** - when the current Agent runtime exposes the
   MINE MCP server (`mine mcp serve`), call the typed MCP tools. They return
   the same DTOs as the JSON CLI and never touch the execution-graph files.
2. **JSON CLI (deterministic fallback)** - when MCP is unavailable, call
   `mine --format json` commands. Never parse human output.

Never invent an MCP tool, CLI command, flag, JSON field, or lifecycle
transition that the current binary does not expose. Never edit
`docs/plan/execution-graph.toml` or `docs/plan/execution-graph.md` directly.

The accepted MCP tools `mine-arch` may use:

- `mine_design_validate` (no arguments) - validate the design namespace.
- `mine_graph_validate` (no arguments) - validate the execution graph.
- `mine_graph_status` (no arguments) - read revision, branches, plan count.
- `mine_plan_reject` (`id`, `reason`, `review_report`,
  `compensating_plan`) - only for the bounded, evidence-backed rejection of
  an objectively infeasible `IN_PROGRESS` Plan after the replacement ADR is
  approved.
- `mine_plan_withdraw` (`id`, `reason`) - owner-authorized retirement of an
  unstarted (`DRAFT`/`BLOCKED`/`READY`) registration made stale by an
  approved architecture replacement (ADR-0012). It performs the same
  provably-lossless Git cleanup checks as `mine plan withdraw`.

Operations `mine-arch` needs that are intentionally **CLI-only** (no MCP tool
exposes them, because they initialize local environment or are one-shot
setup):

- `mine init` - repository and graph initialization (no MCP equivalent; it
  bootstraps the very configuration MCP depends on).
- `mine design status` - marker/identity confirmation (read-only CLI only).
- `mine graph render` - regenerate the Markdown view (CLI only).

When a required operation has no MCP tool, fall back to the JSON CLI and
state the fallback explicitly.

### ADR-first approval gate

Before detailed Design is treated as planning-ready, extract each material
decision into a bounded DRAFT ADR. Use the CLI to propose it, show the exact
body and digest, and stop for the repository owner's explicit approval:

```text
mine adr add ...
mine adr propose --id <adr-id>
mine adr show --id <adr-id>
mine adr approve --id <adr-id> --by <actor> --note <approval-note>
mine design readiness --scope <scope>
```

`mine-arch` must never approve its own ADRs, infer batch approval, or continue
planning when an applicable planning-blocking ADR is unresolved. After the
owner has approved the material decisions, complete the detailed Design and
run scoped readiness. The registry digest binds approval to the exact ADR
body bytes, so body edits require a fresh proposal and approval.

If architecture discovery proves an immutable `IN_PROGRESS` Plan infeasible,
preserve that Plan and its lineage. After the replacement decision completes
the ADR lifecycle and an independent reviewer writes the canonical Root-visible review report, `mine-arch` may invoke the evidence-bound rejection
with that report, a non-empty reason, and the next canonical sibling
compensation ID. It never fabricates implementation evidence, performs a
generic status change, rewires arbitrary successors, or resumes the stopped
implementation.

Unstarted registrations (`DRAFT`/`BLOCKED`/`READY`) made stale by an approved
architecture replacement are retired through the owner-authorized withdrawal
instead: `mine plan withdraw --id <exact-stored-id> --reason <non-empty
reason> --format json` (MCP: `mine_plan_withdraw`). Withdrawal closes
registration, not execution: it never accepts a review report or compensation
plan, refuses diverged or dirty Git evidence, and leaves the withdrawn node's
successors unsatisfied until they are themselves withdrawn or re-planned.
Withdrawal is never a way to erase started history: a Plan that has started
(`IN_PROGRESS` or later) keeps its evidence-bound rejection path.

## MINE execution-graph integration

Initialize repository graph governance with the accepted MINE CLI:

```bash
mine init --format json
```

`mine init` is CLI-only by design (it bootstraps the repository configuration
the MCP server itself depends on); there is no MCP tool for initialization.
After initialization, prefer `mine_design_validate` / `mine_graph_validate`
(MCP) or `mine design validate --format json` / `mine graph validate --format
json` (CLI fallback) to confirm the resulting configuration.

The machine source of truth is `docs/plan/execution-graph.toml`;
`docs/plan/execution-graph.md` is generated by `mine graph render`. Never edit
either graph file directly (`AGENTS.md` documents this rule). If `mine` is
unavailable, complete architecture work but report graph initialization as
blocked and provide the repository installation command.

## No automatic execution

`mine-arch` does not, on its own:

- invoke other Skills (`mine-sync`, `mine-plan-create`, `mine-plan-exec`, `mine-plan-review`);
- create plans, implementation reports, or execution-graph nodes;
- create, switch, or delete Git branches or worktrees;
- create commits, pushes, merges, or releases;
- run `mine` write commands that transition execution-graph state, except the
  bounded active rejection described above;
- modify business code to match a target design (that is `mine-plan-exec`'s role, through a plan).

It writes only to MINE-owned design, repository governance (`AGENTS.md`), quality-gate configuration, and the design ownership marker. It may run `mine init` to establish graph governance and validate the resulting configuration. The sole later graph-transition exception is the approved, independently evidenced active-Plan rejection described above. Handoff to `mine-plan-create` is an explicit user action, not an automatic next step.

## Non-negotiable principles

1. Inspect the current repository and requirements before designing.
2. Separate current reality, target architecture, assumptions, local decisions, and unresolved material decisions.
3. Design from abstract to detailed: goals and invariants first, then context and boundaries, then contracts and internals, then files/configuration/verification.
4. Perform real web search and open authoritative documentation for all material external technologies, standards, protocols, security controls, toolchains, and deployment practices.
5. Include verified official links and their concrete implications in the architecture document.
6. Do not finalize architecture as implementation-ready when mandatory research or repository evidence is unavailable.
7. Apply SOLID to actual change boundaries, not as an excuse for abstraction layers, empty interfaces, factories, or indirection without evidence.
8. New projects have no historical baggage unless the user explicitly requires compatibility. Change obsolete implementations directly; do not preserve old fields, parameters, interfaces, schemas, aliases, migrations, adapters, or shims merely because an earlier plan created them.
9. Architecture changes precede plans that depend on them.
10. Repository quality configuration must reflect the actual languages, package managers, source roots, generated artifacts, deployment model, and risk profile.
11. Never weaken checks, add blanket ignores, or fake commands to make a broken repository appear green.
12. Preserve unrelated user changes and secrets.

## Phase 1: Repository and requirements audit

Locate the repository root and inspect:

- user requirements, product documents, examples, and constraints;
- root and nested `AGENTS.md` or equivalent instructions;
- existing architecture, plans, reports, ADRs, diagrams, and API specifications;
- Git status and relevant history;
- manifests, lockfiles, workspace definitions, language/toolchain version files, and package boundaries;
- entry points, composition roots, service/process boundaries, dependency direction, and generated code;
- domain models, schemas, migrations, storage, indexes, queues, caches, and provenance;
- APIs, events, files, CLIs, tools, prompts, agents, and integration boundaries;
- authentication, authorization, PII, secrets, safety, and external mutations;
- lifecycle, concurrency, idempotency, retries, timeouts, recovery, and cleanup;
- logging, metrics, tracing, auditing, health/readiness, and operational runbooks;
- formatter, linter, static analysis, type checking, tests, builds, containers, deployment, and CI.

Create a repository evidence table:

| Area | Current evidence | Verified behavior | Target need | Gap/risk |
|---|---|---|---|---|

Do not infer an interface from a filename, comment, test name, or aspirational document. Inspect the implementation or mark it unknown.

## Phase 2: Parallel discovery

When the host supports subagents, teams, delegation, or isolated agent sessions, run bounded research lanes in parallel:

1. Requirements, domain, and user-visible closed loops.
2. Current code structure, dependency graph, and composition roots.
3. Data, consistency, migrations, concurrency, and lifecycle.
4. External APIs/protocols/security and official documentation.
5. Language toolchains, quality configuration, tests, CI, build, and deployment.
6. Agent/prompt/knowledge/tool contracts when applicable.
7. Risks, failure modes, observability, and operations.

Give each lane explicit questions and expected evidence. Synthesize conflicts yourself.

When no parallel-agent capability exists, perform the same lanes sequentially. Never assume a named subagent tool exists.

## Phase 3: Mandatory external research

Search and open official or primary sources for every material external dependency or practice, including as applicable:

- language and runtime versions;
- frameworks and libraries;
- databases, transaction and locking semantics;
- network protocols, API standards, authentication, authorization, and encryption;
- cloud/provider APIs and SDKs;
- agent frameworks, MCP/tool protocols, model constraints, and prompt/runtime behavior;
- formatter, linter, static analyzer, type checker, test framework, package manager, and CI configuration;
- container, deployment, migration, backup, recovery, and observability guidance.

For each source, record:

- page title and organization;
- exact URL;
- applicable version;
- date accessed;
- claim verified;
- architecture or repository-configuration implication.

Open the source page. Search snippets and uncited model memory are not evidence. Prefer official docs, standards, official source repositories, release notes, migration guides, and primary research.

If web search/fetch is unavailable, report the blocker and do not claim the architecture or toolchain initialization is complete.

## Phase 4: Resolve decisions

Ask the user when a decision materially changes:

- product scope or externally visible behavior;
- system boundaries, process topology, or ownership;
- persistent schemas, migration/rebuild policy, or data retention;
- public APIs/events/files/tools/prompts/model contracts;
- privacy, security, authorization, safety, or trust boundaries;
- compatibility requirements;
- external service cost/ownership or deployment operations;
- acceptance criteria.

For bounded choices that do not change these contracts, choose the simplest architecture-consistent option supported by official documentation and repository convention. Record it in the architecture under local decisions and trade-offs.

## Phase 5: Abstract architecture first

Before writing detailed classes, tables, routes, or files, establish:

1. Background, stakeholders, goals, non-goals, and system boundary.
2. Quality attributes and measurable constraints.
3. Domain language, invariants, state ownership, and source-of-truth decisions.
4. System context, external actors, and trust boundaries.
5. Architecture style and component/process boundaries.
6. Dependency direction and allowed/forbidden relationships.
7. Data flow and major lifecycle/interaction flows.
8. Operational model, failure domains, and deployment topology.

Do not jump from requirements directly to packages and tables. Detailed design must be traceable to these abstract decisions.

## Phase 6: SOLID and maintainability design

Apply SOLID explicitly:

### Single Responsibility

- Give each module, service, package, process, adapter, tool, configuration source, and resource owner one coherent responsibility.
- Keep protocol conversion, business policy, persistence, external SDK adaptation, orchestration, and composition distinct where they change independently.

### Open/Closed

- Introduce extension points only where the requirements or current provider variability show a real change axis.
- Prefer explicit replacement at a composition root over speculative plugin systems.

### Liskov Substitution

- Define behavioral contracts, including errors, cancellation, timeouts, lifecycle, ordering, idempotency, and side effects.
- Every implementation must preserve those semantics, not merely method signatures.

### Interface Segregation

- Give consumers narrow capability interfaces.
- Do not inject a root settings object, god service, broad repository, or generic tool bag when a smaller contract exists.

### Dependency Inversion

- Domain and application policy depend on abstractions.
- Frameworks, SDKs, storage, network transports, CLI/UI, and concrete configuration remain in adapters and composition roots.

Also enforce:

- one source of truth per fact/configuration/generated contract;
- explicit resource ownership and startup/shutdown order;
- explicit transaction, concurrency, retry, timeout, cancellation, and recovery semantics;
- strong internal types, with dynamic/untyped data contained and validated at boundaries;
- no dead abstraction, duplicate wrapper, pass-through layer, or compatibility shim without a demonstrated need.

## Phase 7: Detailed architecture knowledge base

Create or update the modular design knowledge base rooted at `docs/design/index.md` using the bundled outline as a completeness checklist (not a single-document template). Split the architecture across domain/component indexes and leaf contracts per the progressive-disclosure structure above.

The knowledge base must be detailed enough to govern later plans and reviews. Cover all applicable areas across the relevant leaves:

- technology decisions and official source register;
- overall architecture and component boundaries;
- repository and code organization;
- composition roots and dependency rules;
- domain model, state machines, data/provenance model, schemas, indexes, migration/rebuild/deletion behavior;
- API, event, tool, file, CLI, prompt, model, and generated-artifact contracts;
- workflows, lifecycle, transactions, concurrency, idempotency, retries, failure and recovery;
- authentication, authorization, privacy, secrets, safety, and audit;
- configuration sources and environment layering;
- logging, metrics, tracing, health/readiness, diagnostics, and retention;
- testing strategy and quality-gate matrix;
- build, packaging, deployment, migration, backup, rollback, and operations;
- risks, trade-offs, open decisions, and final closed-loop verification.

Use diagrams where they clarify boundaries, data flow, sequence, lifecycle, state, or deployment. Diagrams supplement exact prose and contracts; they do not replace them.

Do not accumulate individual plan-specific implementation history inside the design knowledge base. Keep current target design and migration/cleanup direction, while plans and reports preserve execution history. Maintain parent indexes and cross-links as leaves are added, split, moved, or deleted.

## Phase 8: No-historical-baggage policy

Unless compatibility is explicitly required:

- later architecture and plans override obsolete earlier implementation directly;
- remove unused reserved fields, compatibility parameters, aliases, wrappers, adapters, flags, routes, schemas, and migration scaffolding;
- do not retain an old interface solely to reduce immediate edit size;
- do not call an accidental internal implementation a public compatibility contract;
- when direct replacement is too large for one increment, create a new-feature plan and an explicit cleanup/removal plan with a hard dependency and acceptance gate;
- never leave cleanup as an unowned “future improvement.”

Document legitimate external compatibility obligations precisely: consumer, version, duration, migration path, deprecation signal, and removal condition.

## Phase 9: Repository quality-gate design

Architecture owns the repository's quality policy. Determine it from the actual repository rather than copying a universal tool list.

### Inventory first

For every language/package boundary, record:

- version and toolchain manager;
- package/workspace manager and lockfile;
- source roots and generated/vendor/excluded paths;
- formatter;
- linter/static analyzer;
- type checker where applicable;
- unit/integration/e2e/contract test framework;
- build/package commands;
- CI and local canonical commands;
- whether a gate is blocking, advisory, scoped, or temporarily non-green with documented debt.

### Python

Select and configure only what the project needs, typically from:

- `pyproject.toml` for packaging/dependencies and tool settings when it is the chosen canonical source;
- `.ruff.toml` or `[tool.ruff]`, never duplicated;
- `mypy.ini` or `[tool.mypy]`, never duplicated;
- `pytest.ini` or `[tool.pytest.ini_options]`, never duplicated;
- `.pylintrc` only when Pylint adds checks intentionally not covered by Ruff/Mypy;
- `.pre-commit-config.yaml` when pre-commit is part of the workflow;
- Python version files and uv/Poetry/pip-tools configuration as actually used.

Define source roots, namespace/package behavior, test paths, excludes, generated code treatment, plugin requirements, strictness, per-module exceptions, and precise commands. Do not enable blanket `ignore_missing_imports`, broad `Any`, wildcard excludes, or global ignores merely to hide problems.

### Go

Define:

- supported Go version, module/workspace boundaries, and dependency ownership;
- `gofmt`/`go fmt` policy;
- `go vet` and `go test` scopes;
- race, fuzz, integration, build-tag, coverage, or vulnerability checks when meaningful;
- `staticcheck` or `golangci-lint` only when selected and configured deliberately;
- generated-code and migration ownership.

Do not invent a configuration file for `gofmt`, which has none.

### TypeScript / JavaScript

Define:

- Node version and package manager;
- workspace/package boundaries and lockfile ownership;
- `tsconfig` hierarchy, strictness, path aliases, module target/resolution, build vs test configurations;
- ESLint using the configuration style supported by the installed version;
- Prettier and its relationship with ESLint without duplicated formatting rules;
- unit/component/e2e framework selected by the project;
- typecheck, lint, format-check, test, and build scripts;
- generated clients/assets and browser/server boundary rules.

### Rust

Define:

- Rust toolchain and workspace boundaries;
- `cargo fmt --check` and rustfmt configuration only when needed;
- `cargo clippy` lint level and justified allowances;
- unit, integration, doc, feature-matrix, build, and optional audit checks;
- generated code and unsafe-code policy where relevant.

### Cross-repository configuration

Evaluate and create or repair as applicable:

- `.editorconfig`;
- `.gitattributes` and line-ending/binary/LFS rules;
- `.gitignore`;
- `.pre-commit-config.yaml`;
- CI workflows;
- stable command wrappers such as package scripts, Make, Just, Task, or a typed project CLI when justified;
- dependency update and generated-artifact drift checks.

Use one canonical configuration source per tool. Document ownership and commands in architecture and `AGENTS.md`.

## Phase 10: Initialize or repair the repository

After the architecture defines the quality policy:

1. Create missing configuration files that the selected tools actually require.
2. Merge existing configuration instead of creating competing sources.
3. Add or pin development dependencies through the repository's real package manager.
4. Update lockfiles using the official package manager; never hand-edit dependency resolutions.
5. Add stable local/CI commands.
6. Add minimal representative tests or smoke checks only when needed to prove the toolchain is wired correctly and within user scope.
7. Update CI to run the selected blocking gates.
8. Run configuration parsers, format checks, linters, static analysis, type checks, tests, builds, and CI-equivalent commands appropriate to the repository.
9. Fix errors introduced by this initialization.
10. Record pre-existing failures exactly. Do not weaken the gate or claim green output when it is not green.

Do not install every possible tool. A small coherent gate set with clear ownership is better than duplicated linting and contradictory configuration.

## Phase 11: Create or update AGENTS.md

Use [AGENTS.md template](references/AGENTS.template.md) as a starting point, then tailor it to the repository.

`AGENTS.md` must name the design knowledge base as `docs/design/index.md` (with `docs/design/.mine-design.toml` as the ownership marker), not a fixed single-document path. `AGENTS.md` must include durable repository-wide agreements:

- exact source-of-truth paths;
- governance precedence;
- architecture-first changes;
- SOLID and dependency-boundary rules;
- no-historical-baggage policy;
- plan immutability and compensating-plan policy;
- requirement that plan executors open and understand every registered official-documentation/best-practice link before implementing affected steps;
- rule to ask the user for uncovered material decisions, while making bounded local choices and recording them in reports;
- parallel scheduling and shared-file ownership;
- actual project-specific format/lint/type/test/build commands and completion semantics;
- scope, data, generated artifact, secret, commit, and evidence discipline.

Keep package-specific schemas, route lists, tool catalogs, implementation details, and individual plan indexes out of `AGENTS.md`; those belong in architecture, plans, or reports.

Never leave template placeholders or commands for tools that are not installed and configured.

## Phase 12: Final validation

Before finishing, verify:

- `docs/design/index.md` exists and the progressive-disclosure structure (root/domain/component indexes, leaf contracts) is intact.
- The design proceeds from abstract design to detailed design.
- Current reality, target, assumptions, local decisions, and unresolved decisions are separate.
- All material external claims have opened authoritative sources and concrete links.
- Component responsibilities and dependencies satisfy SOLID without speculative abstraction.
- Data, APIs, tools, workflows, lifecycle, security, observability, testing, deployment, and failure handling are covered where applicable.
- The no-historical-baggage policy is explicit and cleanup is owned.
- `AGENTS.md` names the architecture path and contains only durable repository-wide rules.
- Selected quality tools match the actual languages and versions.
- Each tool has one canonical configuration source.
- Missing required configuration was created; duplicate configuration was consolidated.
- Dependencies and lockfiles were updated through official package managers.
- Documented commands were actually run and their results are accurately reported.
- No blanket ignores or weakened gates were added to disguise failures.
- No dead local references, invented commands, unrelated edits, or secrets were introduced.

Finish with:

- architecture sections created or changed;
- repository configuration created or repaired;
- exact quality commands and outcomes;
- `AGENTS.md` agreements added or changed;
- unresolved material decisions and external gates;
- the next recommended `mine-plan-create` target.

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
