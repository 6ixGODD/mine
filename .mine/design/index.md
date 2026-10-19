# MINE Design Knowledge Base

## Purpose

This directory is the durable source of architectural truth for MINE. It uses progressive disclosure: humans and agents begin here and follow only the indexes needed for the current task.

The independently accepted 0.2.0 implementation owns `.mine/design/` and
`.mine/plan/` for fresh repositories. During self-development with installed
0.1.13, the source checkout retains `docs/design/`, its ownership marker and
`docs/design-backup-<UTC timestamp>/` backups until the reviewed terminal
cutover. The configured paths determine the current phase; see the
[namespace and cutover contract](governance/repository-layout.md).

## Product statement

MINE is a personal, opinionated engineering system for four coding-agent environments:

- Claude Code;
- Codex;
- Pi;
- OpenCode.

It combines five model-driven Skills with one deterministic Rust executable:

| Layer | Responsibility |
|---|---|
| Skills | Requirement-first architecture, code-first synchronization, plan creation, implementation, and independent review |
| `mine` CLI/MCP | Setup, graph state, validation, revision control, rendering, installation, diagnostics, and safe workspace operations |

Unsupported platforms may adapt the public repository in their own forks.

## Core workflows

### New repository

```text
mine init
    ↓
mine-arch <requirements>
    ↓
mine-plan-create
    ↓
mine-plan-exec / mine-plan-review
    ↓
final mine-sync
    ↓
stable release without the owned ephemeral Plan workspace
```

### Existing repository

```text
mine init   (fresh .mine layout; refuse foreign/active legacy governance)
    ↓
mine-sync [optional scope]
    └─ code -> descriptive design baseline
    ↓
mine-arch <new requirements>
    └─ baseline → target design
    ↓
planning, execution, review, final sync, release
```

## System invariants

1. Design is long-lived, MINE-owned, marker-validated, and branch-accurate: `.mine/design/` in the approved 0.2.0 layout, `docs/design/` during the current bootstrap.
2. The Plan workspace is temporary and excluded from the stable release tree and imported stable history: `.mine/plan/` in 0.2.0, `docs/plan/` during bootstrap. It remains Git-trackable on development branches.
3. Every plan references exact design documents and anchors.
4. Agents never mutate execution-graph files directly; they use `mine` MCP tools or JSON CLI commands.
5. The execution graph is TOML, Git-reviewable, and scoped to one internal plan workspace.
6. CLI and MCP share one application and domain implementation.
7. MINE supports only Claude Code, Codex, Pi, and OpenCode.
8. Before `mine-sync` rewrites existing design, a local ignored timestamped backup is created.
9. During `mine-sync`, explicit user instructions win; otherwise current repository behavior is authoritative over stale design.
10. During `mine-arch`, user requirements define target behavior even when current code differs.
11. Design indexes remain small; detailed documents are split before they become context hazards.
12. “Destructive” authority is bounded to managed design and temporary MINE artifacts; arbitrary repository destruction is forbidden.
13. Material architecture decisions are explicitly approved by the repository
    owner and digest-bound before their scope becomes planning-ready.

## Design map

- [Principles and quality attributes](principles.md)
- [System context](system/index.md)
- [Execution graph](execution-graph/index.md)
- [Public interfaces](interfaces/index.md)
- [Agent integrations and distribution](integrations/index.md)
- [Operations](operations/index.md)
- [Governance and documentation lifecycle](governance/index.md)
- [Architecture decisions](decisions/index.md)
- [External source register](sources.md)
- [1.0 Project control-plane target](v1/index.md) — owner-approved `v1.0.0` scope,
  not current0.2 implementation. Includes global projects, SQLite,
  identities, durable coordination, runtime selection, recovery and unified UI.

## Reading guidance

### Approved 0.2.0 namespace change

[ADR-0023](decisions/0023-mine-local-governance-layout.md) was explicitly
approved on 2026-10-13 and supersedes ADR-0006 and ADR-0002 while preserving
their ownership and retention intent. The [detailed namespace contract](governance/repository-layout.md)
defines Git-trackable `.mine/design/` and `.mine/plan/`, effective ignore
checks, and the old-binary release cutover. The implementation is independently
accepted; acceptance does not itself relocate the source checkout or publish a
release. Frozen Plans and the live graph retain their source paths until the
specified terminal boundary.

### Approved contracts and candidate implementation

ADR-0015 through ADR-0021 were explicitly approved on 2026-10-09. Their
[approval and Design handoff](governance/architecture-improvement-proposals.md)
links the exact decisions, body digests and detailed contracts. The installed
0.1.13 installation does not implement these extensions. The candidate source
and its five synchronized Skills implement the native ownership, preparation,
checkpoint, operation and verification consumers. Candidate implementation,
completed focused checks, independent acceptance and release are separate facts;
consult the implementation evidence before claiming any of the latter. Live
self-development still uses installed commands and a serialized Root owner.

ADR-0022 is also explicitly approved, replacing ADR-0010's pre-start rule while
retaining its active anchors and recovery guarantees. Its
[durable preparation](governance/ready-preparation.md) and
[native graph checkpoint](governance/root-graph-checkpoints.md) targets close the
real parallel prepare/start gap. Their candidate implementation is mandatory
before 0.2.0; an earlier candidate join alone does not authorize release.

| Task | Required design entry points |
|---|---|
| Initialize or onboard a repository | this file, governance design knowledge base, Skill contracts |
| Synchronize an old repository | governance design sync, design knowledge base, relevant domain indexes |
| Change architecture | this file, principles, affected domain index, governance design sync |
| Change execution graph | execution-graph index and relevant leaves |
| Change CLI or MCP | interfaces index and relevant interface contract |
| Change Skills | integrations/skills, governance documents, affected interface contract |
| Change installation/distribution | integrations/distribution, operations testing/release |
| Prepare release | branch lifecycle, design sync, operations testing/release |

## Document-size rules

- Index documents should normally stay below 250 lines.
- Leaf documents should normally stay below 400 lines.
- Split a document when it has more than eight major sections, mixes independent ownership boundaries, or routinely forces agents to load irrelevant material.
- Every child directory has an `index.md`.
- Links are repository-relative and validated.
- Contracts have one authoritative leaf; indexes summarize and link.
