# MINE Design Knowledge Base

## Purpose

This directory is the durable source of architectural truth for MINE. It uses progressive disclosure: humans and agents begin here and follow only the indexes needed for the current task.

MINE owns the `docs/design/` namespace. A managed tree contains `.mine-design.toml`. When `mine init` encounters an existing unmarked `docs/design/`, it moves the legacy directory aside to a timestamped `docs/design-backup-<UTC timestamp>/` backup and creates a fresh managed root; it does not abort and does not guess how to migrate arbitrary legacy content. `mine-sync` and later operations refuse an unmarked or foreign-owned `docs/design/` only when `mine init` has not been run.

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
stable release without docs/plan
```

### Existing repository

```text
mine init   (auto-backs-up legacy non-MINE docs/design/)
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

1. `docs/design/` is long-lived, MINE-owned, marker-validated, and branch-accurate.
2. `docs/plan/` is temporary and must not be present on the stable release tree or imported stable history.
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

## Reading guidance

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
