<!-- mine-managed-agents -->
# Agent Working Agreement

## Source of truth
- Design knowledge base root: `.mine/design/index.md` (progressive disclosure; MINE owns `.mine/design/`)
- Design ownership marker: `.mine/design/.mine-design.toml`
- Execution plans: `.mine/plan/`
- Execution graph machine source: `.mine/plan/execution-graph.toml`
- Generated execution graph view: `.mine/plan/execution-graph.md`
- Implementation and review reports: `.mine/plan/reports/`
- Project configuration: `.mine/config.toml`; configured contained paths are authoritative. The paths above are fresh defaults, never permission to redirect an active legacy workspace or move user `docs/`.
- Read requirements and current implementation evidence before changing design or code.

## Design rules
- Separate current implementation, accepted target design, assumptions, local decisions, and unresolved material decisions.
- Do not invent data fields, API behavior, tool names, command flags, or external semantics. Cite repository evidence or mark uncertainty.
- Verify external behavior against opened official or primary documentation and link the exact source in architecture and plans.
- Follow SOLID at real change boundaries. Do not create speculative interfaces, factories, plugin systems, or indirection without a demonstrated variation or testing boundary.
- Update architecture before creating a plan that depends on changed architecture.
- Material decisions enter planning only through digest-bound ADR approval;
  `mine-arch` never self-approves and must report scoped readiness.

## No historical baggage by default
This is a new project unless the user explicitly states otherwise. When a later accepted design conflicts with an earlier internal implementation, change the target implementation directly. Do not keep reserved fields, obsolete parameters, compatibility aliases, dead interfaces, transitional adapters, duplicate schemas, or shims solely to preserve an abandoned plan. If cleanup is too large, create an explicit follow-up plan to remove the obsolete design rather than silently retaining technical debt.

## Document boundaries
- `AGENTS.md` contains durable repository-wide agreements only.
- Package-specific schemas, contracts, implementation decisions, and plan details belong in the architecture document and relevant plan.
- Do not maintain an accumulating plan index here.

## Plan immutability
- A plan becomes immutable when handed to an implementation agent, execution begins, or a report records execution.
- Do not edit, rename, renumber, delete, or replace an immutable plan.
- Material replacement changes correct architecture/ADR approval first, then use the next contiguous sibling compensation. Narrow unambiguous fixes of already-accepted behavior do not require a ceremonial new Plan.

## Plan execution
- Read and understand the whole requested plan, governing architecture sections, predecessors, and registered official sources before editing.
- Fetch registered sources; do not implement from the planner's paraphrase alone.
- Ask the user when an uncovered decision changes product behavior, architecture, persistence, public contracts, security/privacy, compatibility, deployment responsibility, or acceptance criteria.
- Resolve bounded local implementation decisions using architecture, official documentation, and repository convention; record them in the implementation report.
- Preserve unrelated changes and stage only explicit in-scope files.
- Responsibility/namespaces, supported behavior and exclusions govern scope; physical mappings locate it. Internal file/symbol splits within that authority need no repeated approval. Cross-contract behavior/persistence/security/acceptance changes still require owner decisions.
- Conservative authority needs nonempty physical mapping. Semantic authority requires the exact applicable approved policy/ADR body binding. Never fabricate approval, downgrade invalid policy or replace a graph observation with the independent ADR registry revision.
- Frozen ownership clarification is append-only, body/policy/identity/actor/reason/revision-bound native evidence. It cannot change behavior/dependencies/acceptance or rehash tampered Plan bodies; absent historical identity is not invented.
- New Plan IDs and filenames use the canonical `planNN(?:-NN)*(?:-CNN)?`
  grammar and exact `.mine/plan/<id>-<lower-kebab-slug>.md` form; legacy aliases
  are not created.

## Lightweight maintenance

MINE governs engineering change, not every repository edit. Direct, bounded, behavior-preserving maintenance (typo fixes, prose cleanup, translation, broken-link repair, README improvements, user-facing documentation additions, formatting-only documentation changes, examples/comments describing already-accepted behavior) may proceed without a MINE execution Plan when the correct result is unambiguous and no durable engineering contract changes. Such work still must respect this `AGENTS.md`, preserve unrelated changes, stage explicit files, run relevant validation, use normal commit discipline, and never silently change product behavior while claiming to be docs-only. Changes that alter or establish durable engineering behavior (business/runtime behavior, architecture, public API, CLI semantics, MCP contracts, Skill workflow, execution-graph/state-machine behavior, release behavior, branch lifecycle, persistence/schema/migration, security/privacy/authority boundaries, deployment/operational contracts, durable Design decisions) require the normal MINE lifecycle. A documentation file is not automatically lightweight maintenance: if it changes the authoritative contract rather than describing already-accepted behavior, it belongs in normal MINE governance.

## Parallel execution
- Use MINE CLI/MCP to query and transition `.mine/plan/execution-graph.toml`; treat the Markdown graph as generated read-only output. Use the plan's work-package DAG.
- Parallel lanes require explicit responsibility/exclusions/mapping, one owner per shared file, start/join gates and integration ownership. Physical overlap requires its exact approved semantic partition/shared-entry rule; algorithms, schemas, migrations, lockfiles, generators and mutable outputs remain exclusive otherwise.
- Manifests, lockfiles, migrations, generated schemas/clients, root quality configuration, central registries, the execution graph, and reports are serialized unless a plan explicitly assigns one owner.
- Native wave/start compares complete held IN_PROGRESS/IMPLEMENTED owners and selected candidates. READY is advisory eligibility; stopped/silent writers do not release ownership. Invalid required policy never drops a held owner.
- Root remains on configured integration while a Plan workspace is open; only authorized release integration may move it to stable. Plan branches are checked out solely in registered linked worktrees. Rejection preserves its ref and reuses the lineage checkout for the next compensation.
- Prepare records explicit actor, original observer, operation identity and generation; production edits require completed start/checkpoint. After unknown effects query/resume the same operation, never duplicate execution or manual graph-only fast-forward.
- Acceptance requires independent stopped-writer handoff, pinned review, exact reviewed integration, actual applicable gates and safe removal of only the merged clean worktree/local branch before terminal checkpoint completion. No self-acceptance or forced cleanup.
- Generators have one input-to-complete-output-to-seal transaction owner. Regenerate once from combined accepted inputs, not stale lane output copies; every gate must have output authority.

### Shared environment and Plan-isolated mutable outputs

- Reuse Root-managed toolchains, dependency downloads, virtual environments,
  and only caches proven read-only or safely content-addressed.
- Never create `.venv`, `venv`, `target`, `node_modules`, or an equivalent
  environment/cache tree inside a linked Plan worktree.
- Mutable build, test, generation, coverage, packaging, and runner outputs use
  `<Root>/.mine/build/<plan-id>/<tool-or-output-class>/`. Rust adapters set
  `CARGO_TARGET_DIR` to the exact Plan namespace, such as
  `<Root>/.mine/build/<exact-plan-id>/cargo-target`.
- Before an exceptionally large build, record a measured size estimate and
  pause for explicit owner agreement on serialization, a coarser exclusive
  namespace, and cleanup. Never silently use a globally shared mutable path.

## Repository quality gates
`mine-arch` must replace this table with the actual repository commands and configuration sources.

| Scope | Format | Lint/static analysis | Type check | Tests | Build/package |
|---|---|---|---|---|---|
| `<scope>` | `<command>` | `<command>` | `<command or N/A>` | `<command>` | `<command>` |

- Run the narrowest relevant checks first, followed by required broader gates.
- Never call a timeout, missing tool, skipped command, warning, ignored diagnostic, or non-zero exit a pass.
- Do not weaken configuration, add blanket ignores, or change acceptance thresholds merely to obtain green output.

## Evidence and reports
- Record exact commands, exit codes, concise observed output, inspected artifacts, skipped checks, failures, remaining risks, and preserved unrelated changes in `.mine/plan/reports/`.
- Implementation agents may conclude `IMPLEMENTED`, never `ACCEPTED`.
- Independent review is required for `ACCEPTED`.
- Distinguish RUN/REUSED/INVALIDATED/NON_REUSABLE/NOT_RUN with exact input and artifact identity. Reuse applicable evidence without duplicate suites, but preserve changed-input, independent and native requirements, first failures and unavailable hosts.

## Commit discipline
- Use Conventional Commits and repository conventions.
- Stage explicit paths only; never use broad staging in a dirty worktree.
- Do not amend or rewrite another agent's shared history.
- Do not push, deploy, publish, upload, or perform other external mutations unless explicitly authorized.

## MINE graph discipline

- Never edit `.mine/plan/execution-graph.toml` or `.mine/plan/execution-graph.md` directly.
- Register plans and perform lifecycle transitions only through the installed `mine` MCP tools or `mine --format json` CLI.
- Carry the observed revision into every write operation and stop on revision conflicts.
- Commit only owned Plan/report/policy inputs before graph writes. Native checkpoints own the exact graph pair; an incomplete response can mean persisted pending state, not rollback. Inspect returned operation/checkpoint completion and resume that same operation before new conflicting writes. Never manually commit or fast-forward its pending graph state.
