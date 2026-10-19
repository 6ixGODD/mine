# Skill Contracts

## Skill set

Accepted target extensions are [semantic ownership](../governance/semantic-ownership.md),
[architect foundations](architect-foundation.md),
[active scheduling](../execution-graph/active-ownership-scheduling.md),
[resumable lifecycle](../governance/lifecycle-receipts.md) and
[verification reuse](../operations/verification-receipts.md). Candidate source,
generated plugin copies and embedded inventory consume these contracts together.
This does not upgrade the installed bootstrap binary or prompts. Candidate
implementation is not independent acceptance or release evidence.

ADR-0022 adds mandatory [durable preparation](../governance/ready-preparation.md)
and [native checkpoint recovery](../governance/root-graph-checkpoints.md)
consumption across all five Skills/templates and generated/embedded payloads.
Routine shipped lifecycle writes require neither manual graph commits nor owner
reapproval; candidate self-development still uses the installed previous binary
and its actual capabilities until release. All five candidate Skills and their
bundled templates now describe the native consumers, not manual substitutes.

MINE v1 has exactly five first-class Skills:

1. `mine-arch`;
2. `mine-sync`;
3. `mine-plan-create`;
4. `mine-plan-exec`;
5. `mine-plan-review`.

Installation, graph manipulation, diagnostics, backup mechanics, and distribution remain deterministic CLI responsibilities.

## Shared rules

Every Skill:

- locates Root governance and the configured Design index (`.mine/design/index.md`
  for fresh 0.2.0 repositories; legacy paths remain authoritative during bootstrap);
- validates `.mine-design.toml` ownership;
- loads relevant design leaves through indexes;
- treats the configured Plan root (`.mine/plan/` by default) as temporary workspace;
- never edits execution-graph TOML or generated Markdown directly;
- prefers typed MCP tools and falls back to `mine --format json`;
- fails explicitly when neither invocation path is available;
- records exact design references in plans and reports;
- obeys standing managed-branch authorization and preserves unrelated changes;
- does not invent unsupported harness behavior.
- parses only canonical Plan IDs (`planNN(?:-NN)*(?:-CNN)?`) and derives neither
  filenames nor compensation IDs with ad-hoc string conventions.
- reads complete held ownership, not just READY advice; declares responsibility,
  exclusions and physical mapping without ceremonial per-internal-file approval;
- carries the caller's observed graph revision and explicit actor through native
  writes, distinguishes ADR revision, and resumes the same unfinished operation;
- obeys preparation generations and checkpoint barriers; never writes graph files
  or commits a shipped native graph checkpoint by hand;
- isolates language-independent mutable outputs per exact Plan node and reuses
  Root environments, not another Plan's build products;
- checks verification evidence against current inputs and disposition; a receipt
  reader's exit zero does not prove a check passed or authorize release.

## `mine-arch`

`mine-arch` is requirement-first.

### New or minimally implemented repository

It:

1. audits repository reality and user requirements;
2. researches current official technology contracts;
3. creates or expands the modular target design;
4. creates or updates repository governance;
5. validates links, markers, and document boundaries;
6. prepares handoff to `mine-plan-create`.

### Existing managed design

It:

1. reads the root and affected child indexes;
2. loads only design relevant to the requested change;
3. compares current design, code baseline, and user requirements;
4. gives explicit current user requirements authority over both code and existing design;
5. updates target design before planning;
6. splits documents that become oversized or mix ownership;
7. updates parent indexes and cross-links;
8. validates design and hands off to planning.

### ADR-first approval gate

Before detailed Design becomes planning-ready, `mine-arch` extracts each
material decision into a DRAFT ADR, proposes digest-bound review candidates,
and stops for explicit user approval. It never self-approves or infers batch
approval. After recorded approval/rejection, it completes Design consistent
with approved ADRs and runs scoped Design readiness. Complex projects may add
a project-specific detailed-design coverage matrix; the Skill does not impose
one universal maturity schema.

When architecture discovery proves an already `IN_PROGRESS` immutable Plan
infeasible, `mine-arch` may, only after recording the replacement ADR approval
and a canonical independent report, invoke the bounded active rejection
transition. This is not generic graph administration: it may only preserve and
reject the named stopped Plan with its next sibling compensation. It never
marks implementation evidence, rewires arbitrary successors, or resumes
execution.

It does not silently treat current code as target architecture when the user's requirement changes that target.

## `mine-sync`

`mine-sync` is code-first, high-cost, and potentially destructive to managed design.

It:

1. refuses an unmarked or foreign configured Design root;
2. creates and verifies a local ignored timestamped design backup before rewrite;
3. uses user-named scope when provided;
4. explores broadly when scope is absent, with the user's acceptance of cost;
5. compares repository behavior with design;
6. preserves only design decisions explicitly protected by the user;
7. otherwise updates design to match code, schemas, configuration, and observable behavior;
8. creates a descriptive baseline when meaningful design is absent;
9. records uncertainty and suspicious behavior instead of hiding it;
10. validates the resulting knowledge base and emits a sync report.

It does not modify business code without a separate architecture/plan/execute flow.

## `mine-plan-create`

It:

- ensures the managed `dev` branch and internal plan workspace exist;
- requires current validated target design;
- creates immutable plans under the configured Plan root;
- records exact design paths and anchors;
- registers graph nodes through MCP or JSON CLI;
- allocates canonical hierarchical IDs and filenames and, for compensation,
  the next sibling `C` ordinal in the rejected lineage;
- refuses registration/release until applicable blocking ADRs are approved and
  Design readiness succeeds;
- defines ownership, dependencies, verification, and reports;
- registers compensation nodes with exactly the rejected node's ordered hard
  predecessors; a newly discovered dependency belongs in a separately
  accepted predecessor before the rejection lifecycle, not as an extra
  compensation edge;
- refuses planning on the stable branch.

When an older writer has already persisted an invalid unstarted compensation,
Skills never edit the graph or relax exact inheritance. After owner-approved
ADR-0014 governance, they may invoke the deterministic CLI/MCP compensation-
predecessor repair with the observed revision and a recorded reason, then
require `graph validate`, `plan prepare`, and `plan start` evidence.

For governance recovery, it may allocate one serialized base Plan whose write
scope overlaps a stopped infeasible Plan only when the accepted Design names
the recovery, the user has frozen the old lane, and no agent may resume it. The
planner records this ownership transfer and the exact post-recovery
compensation sequence; it does not pretend the two Plans are parallel-safe.

### Scope-first planning contract

`mine-plan-create` is **scope-first, research-backed, evidence-on-demand**:

- **Explicit user scope is the authoritative planning boundary.** When the
  user invokes the Skill with a clear requirement, component, previous
  discussion, Design change, or implementation target, the planner stays
  within that scope: it inspects only the Design, code, tests,
  configuration, graph state, reports, and external material needed to make
  that scope executable, and does not broaden into unrelated repository
  areas. Uncertainty inside the scope may expand evidence collection.
- **A bare invocation may enter discovery mode.** With no explicit planning
  target, the Skill may perform broader discovery to identify the next
  unplanned or actionable Design work, preferring the smallest coherent
  planning frontier rather than an unconditional full-repository audit.
- **Research remains mandatory and scope-bounded.** Substantive planning
  requires external research within the requested scope: official
  documentation, normative standards, mature implementations, and known
  failure modes. Research exists to compare established practice and prevent
  unnecessary invention - not to satisfy a documentation ritual, and not to
  broaden into unrelated subsystems. Research may validate, refine, or
  reveal risks in accepted Design, but it cannot silently override it; a
  material Design conflict returns to `mine-arch`.
- **Evidence collection is proportional.** Repository inspection scales with
  scope: relevant Design leaves, graph state, implementation, tests, and
  predecessor reports only when they materially affect the requested work.
  No repository-wide evidence matrices, full Design-tree reads, or audits of
  every accepted report are required by default.
- **No ceremonial SOLID review.** Accepted architecture is respected by
  default; architecture, ownership, dependency direction, and SOLID concerns
  are inspected when the requested scope crosses a real boundary or
  introduces a new abstraction/interface/component.
- **Plans stay implementation-ready.** Scope-first planning does not make
  Plans vague: exact scope, Design references, write boundaries,
  implementation targets, behavior, semantics, acceptance criteria,
  verification commands, and commit boundaries remain required.

### Responsibility boundary with `mine-arch`

The accepted architect-foundation target permits source work assigned through
a normal Plan work package after scoped decision approval and an explicit
execution request. The architect follows that package's execution obligations
and independent review; this adds no sixth Skill or automatic code edits during
architecture approval. See [the exact role boundary](architect-foundation.md).

`mine-arch` owns requirements interpretation, target architecture, durable
engineering decisions, component boundaries, public contracts, and changes to
accepted Design. `mine-plan-create` turns accepted Design into executable
implementation work: resolving bounded implementation details, decomposing
work, determining dependencies and write scopes, defining verification, and
identifying safe parallelism. If planning reveals a missing decision that
materially changes product behavior, architecture, public API, persistent
data semantics, a security/ownership/compatibility boundary, deployment
layout, or another durable engineering contract, planning stops for that
portion and the decision routes back to `mine-arch`.

### Research philosophy

The broader MINE research philosophy, shared by `mine-arch` (broadest),
`mine-plan-create` (scope-bounded implementation research), and `mine-sync`
(understanding observed reality, never overriding it), is:

```text
observe repository reality -> compare established practice -> make a project-specific decision
```

## `mine-plan-exec`

It:

- is invoked as `mine-plan-exec <plan-id-or-registered-path>`, normalizes the
  accepted numeric/`P` shorthand by unique match against `mine graph show`,
  and resolves the authoritative Plan document through `mine plan show` using
  the exact stored ID, never by guessing a filename or compensation ordinal;
- prepares or reuses the registered linked worktree with `mine plan prepare`;
- reuses the base lineage worktree for every compensation node, switching only
  that clean linked checkout while preserving rejected branch refs;
- performs all implementation writes and commits there and never switches the
  Root worktree away from `dev`;
- validates workspace, graph, predecessors, design references, and revision;
- starts through MINE only after durable preparation and completed checkpoint
  admission, then verifies the persisted anchor and effective authority;
- materializes a compensation's own frozen body only after completed start,
  preserving its rejected lineage and separating Root graph commits from Plan
  history; ambiguous partial copy/commit recovery never silently overwrites;
- implements only declared scope;
- commits scoped work and writes an ephemeral report;
- records implementation evidence through MINE;
- never self-accepts, self-rejects, or merges itself;
- when an immutable requirement is proven infeasible, stops with committed
  minimal reproduction evidence when safe, preserves clean Root/worktree state,
  and reports `IN_PROGRESS`; it never fabricates `IMPLEMENTED` to reach review.

## `mine-plan-review`

Single-Plan mode is invoked as `mine-plan-review
<plan-id-or-registered-path>`. It uses the same graph-backed, unique-match
numeric/`P` shorthand normalization as execution, then resolves the immutable
Plan and linked worktree through the binary using the exact stored ID. Release
closure retains its explicit phrase-based invocation.

It:

- independently inspects design, plan, commits, code, tests, and runtime behavior;
- discovers the decisive validation suite from the repository under review, in this authority order: explicit current user instructions; repository governance (`AGENTS.md`); accepted Design and release contracts; the active Plan and its acceptance criteria; detected project build, test, lint, packaging, and integration systems - it never presumes Cargo, Python, Node, Go, or any other toolchain;
- accepts or rejects through MINE;
- may apply only unambiguous local fixes under strict rules;
- before final stable integration, runs the native stale-plan-reference scan via `mine scan plan-refs --check` (the authoritative cross-platform implementation; no Bash/WSL/Git Bash dependency), rewrites historical comments as durable contracts, and records narrowly scoped fixture exemptions;
- creates compensation plans for material failures;
- reviews and corrects only inside the registered Plan worktree;
- after a positive verdict, merges the Plan branch into clean Root `dev`, owns
  conflict resolution and revalidation, and records `ACCEPTED` only after the
  merge and integration gates succeed;
- deletes only the clean accepted linked worktree and merged local branch;
- uses typed begin/stage/finalize and the pinned independent report assertion;
  stages describe observed effects, not permission to manufacture them; uncertain
  responses resume the same operation after fresh observation;
- on rejection, declares the next canonical sibling compensation ID and
  preserves the shared lineage worktree for its execution;
- when explicitly asked to review a stopped `IN_PROGRESS` Plan for
  infeasibility, verifies the executor's reproduction independently, writes the
  canonical Root-visible review report, and may invoke the evidence-bound
  rejection transition; it does not apply ordinary implementation acceptance
  semantics to that mode;
- carries mechanical release closure via the explicit `mine-plan-review complete release closure` invocation after the repository owner has run the final `mine-sync` (Phase A); it does not itself invoke `mine-sync`;
- requires only generic release gates (design, graph, terminal state, clean tree, candidate cleanliness); it does not require MINE plugin distribution, four-client installation, or MCP tool-count verification unless reviewing the MINE source repository itself;
- does not preserve rejected behavior with compatibility shims by default.

## Contract synchronization

After CLI or MCP contracts change:

1. update root Skill sources to actual implemented names, arguments, output, and error codes;
2. validate every command/tool reference against the built binary and MCP schema;
3. synchronize generated plugin copies;
4. compare hashes;
5. run discovery smoke tests on Claude Code, Codex, Pi, and OpenCode.
