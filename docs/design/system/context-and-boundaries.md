# System Context and Trust Boundaries

## Actors

| Actor | Role |
|---|---|
| Repository owner | Supplies requirements, optional sync scope, and protected design decisions |
| Architecture agent | Uses `mine-arch` to create or evolve target design |
| Synchronization agent | Uses `mine-sync` to reconcile repository reality into design |
| Planning agent | Uses `mine-plan-create` to create temporary plans |
| Implementation agent | Uses `mine-plan-exec` to implement one plan |
| Review agent | Uses `mine-plan-review` to independently accept or reject |
| `mine` executable | Enforces deterministic setup, ownership, graph, backup, lifecycle, and integration rules |
| Git | Provides branch, commit, ancestry, and tracked-file evidence |
| Agent harness | Claude Code, Codex, Pi, or OpenCode |

## Context

```text
Repository owner
      │ requirements, sync scope, protected decisions
      ▼
MINE Skills inside an agent harness
      │ typed MCP calls or JSON CLI
      ▼
`mine` application services
      │
      ├─ design namespace and backup service
      ├─ execution graph store
      ├─ Git evidence and managed-branch gateway
      ├─ design/plan path validation
      ├─ distribution and installer adapters
      └─ deterministic human/JSON/MCP output
```

## Trust boundaries

### User input

Paths, scope, protected decisions, plan identifiers, commits, branch names, and installation targets are untrusted until parsed and validated.

### Repository filesystem

The repository may contain legacy design content, malicious paths, symlinks, junctions, dirty state, generated files, and concurrent changes. Writes and backups must not escape repository boundaries.

### Existing design

Existing design is trusted only when a valid matching MINE marker exists. During `mine-sync`, its claims are subordinate to explicit user instructions and current repository behavior.

### Git

Git is invoked as argument arrays without shell interpolation. Managed branch mutations are limited by governance; destructive recovery and public-history rewriting are excluded.

### MCP transport

Malformed or stale requests receive the same validation and revision protection as CLI. stdout is protocol-only.

### Installed agent configuration

Installers preserve unrelated user state, back up before mutation, and refuse ownership conflicts.

### Model output

Skill instructions, reports, and inferred architecture are claims. Sync reports distinguish inspected evidence, sampling, inference, and uncertainty.
