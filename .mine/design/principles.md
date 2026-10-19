# Architecture Principles and Quality Attributes

## Principles

### Deterministic mechanisms, model-driven judgment

LLMs decide architecture, exploration scope, planning detail, implementation choices, and review findings. Rust enforces setup markers, graph structure, state transitions, concurrency, path safety, stable outputs, installation state, and protocol contracts.

### Progressive disclosure

No architecture document becomes a universal context dump. Indexes orient; leaf documents specify. Skills load the smallest sufficient design set.

### Distinguish descriptive truth from target intent

`mine-sync` is descriptive and code-first. Unless the user explicitly protects a design decision, current code, schemas, configuration, public behavior, and runtime evidence override stale design.

`mine-arch` is prescriptive and requirement-first. It may update target design away from current code, after which plans describe the transition.

### Destructive, but bounded

MINE may replace inaccurate managed design, delete obsolete design leaves, purge its own temporary plan workspace, and delete its own accepted temporary branches. It must create the required local design backup before synchronization.

MINE never interprets this authority as permission for arbitrary filesystem deletion, unbounded shell execution, `reset --hard`, `git clean`, blind stash, force push, or writes outside the repository.

### Branch-accurate design

On the stable branch, design describes released behavior. On `dev`, design may describe the approved target for current work. Temporary plans explain how implementation reaches that target.

### Evidence over confidence

A command, report, test name, or design assertion is a claim until evidence is inspected. Timeouts, missing tools, skipped commands, non-zero exits, incomplete repository scans, and unverified client discovery are not passes.

### No accidental compatibility debt

MINE does not migrate arbitrary legacy `docs/design/` layouts, preserve obsolete internal contracts, or add compatibility shims without an explicit external obligation.

### Safe failure

When path ownership, branch state, revision, marker ownership, or evidence is ambiguous, MINE stops without destructive recovery.

## Quality attributes

| Attribute | Requirement |
|---|---|
| Correctness | Illegal graph transitions and dependency states are rejected deterministically |
| Auditability | Durable state is text-based and Git-reviewable |
| Context efficiency | Agents operate through indexes and focused leaves |
| Synchronization fidelity | `mine-sync` records scope and uncertainty and does not claim full coverage without evidence |
| Safety | Writes are repository-scoped, ownership-checked, locked where required, revision-checked, and atomic |
| Portability | 0.2.0 delivers Windows and Linux x86_64; macOS delivery is deferred under ADR-0034 |
| Interoperability | The same Skills operate on Claude Code, Codex, Pi, and OpenCode |
| Maintainability | CLI and MCP are adapters over shared application services |
| Release hygiene | Stable branches contain design and product state, not plan workspaces or backups |
| Recoverability | Design synchronization creates local backup before rewrite; interrupted state writes remain complete |
| Testability | Domain rules are independent from filesystem, Git, CLI, MCP, and agent adapters |

## Hard non-goals for the 0.2.0 core

These bounds do not revoke the separately protected [1.0 target](v1/index.md).

- Web UI, remote service, accounts, or cloud collaboration;
- SQLite, remote databases, or custom binary graph storage;
- automatic invocation of coding agents by `mine init`;
- arbitrary shell execution through MCP;
- automatic compatibility migration of legacy design directories;
- support for Cursor, Windsurf, Cline, or other harnesses;
- permanent storage of historical plans on the stable branch.
