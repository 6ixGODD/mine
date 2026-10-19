# Component Architecture

## Dependency direction

```text
CLI Adapter ─┐
             ├──> Application Services ───> Domain
MCP Adapter ─┘             │                  ▲
                           ├──> Repository Port│
                           ├──> Git Port       │
                           ├──> Clock/ID Ports │
                           └──> Agent/Dist Ports
                                      │
                               Infrastructure Adapters
```

The domain does not depend on clap, rmcp, filesystem APIs, Git subprocesses, agent configuration formats, or Markdown rendering.

## Components

### Domain

Owns plan-workspace identity, plan states, legal transitions, dependency validation and graph-cycle detection, readiness, parallel waves, write-path conflicts, revisions, safe paths, marker ownership, and workspace closure invariants.

### Application services

- `init_service` and `workspace_service` own repository initialization and the
  temporary Plan-workspace lifecycle;
- `graph_service` and `plan_service` own graph queries, transactional graph
  mutation, Plan lookup, Plan lifecycle transitions, start-time branch-anchor
  capture, and evidence-bound active rejection;
- `adr_service` and `design_service` own ADR readiness and read-only Design
  status/validation;
- `agent_service` and `doctor_service` orchestrate the four supported agent
  adapters and diagnostics;
- `operation_service`, `native_lifecycle`, `native_plan_workspace` and the native
  preparation adapter validate recoverable lifecycle stages and exact Git facts;
- `verification_service` and `native_verification` capture/record/check external
  verification evidence; private `native_gate` and `native_evidence` validate
  current selections and contained committed evidence;
- `release_service` owns retained source/candidate observations and bounded
  curation composition, including the concrete terminal layout producer/validator;
  its diagnostic entry is read-only, while typed release stages persist progress;
- `scan_service` scans tracked implementation for stale Plan references.

Repository-version, distribution-sync, and Design-backup commands currently
compose domain/infrastructure operations directly in the CLI adapter; there
are no separate application-service modules for those commands.

### Infrastructure

Implements TOML and ADR-registry persistence, atomic writes, locks,
repository/worktree discovery, bounded Git evidence and typed checkpoint/Git
effects, operation journals and verification artifacts, safe Design backups,
deterministic graph/ADR rendering, embedded Skills, and system clock/UUID
sources. Agent configuration adapters live under `agent_setup/` rather than
the infrastructure module.

The read-only Git adapter exposes fixed-argument commit resolution and ancestry
queries. The checkpoint kernel separately owns narrowly typed recoverable Git
effects, not arbitrary shell commands. `plan_service` owns when an initial anchor
must equal Root/predecessor state and when a stored anchor may have descendant
Plan commits. This keeps Git subprocess mechanics out of the domain without
duplicating policy in CLI/MCP adapters.

ADR registry persistence and rendering is a separate repository adapter over
the same atomic-write/locking primitives. It owns neither graph status nor
architecture judgment; the ADR application service owns transition/readiness
policy and `mine-arch` owns model-driven decision discovery.

### CLI and MCP adapters

Adapters map typed requests to the same application services and contain no duplicate state-machine, path, backup, or branch policy.

## Transaction pattern

Every graph write:

1. locates repository and validates marker/branch/workspace gate;
2. acquires the fixed lock;
3. reloads configuration and graph;
4. validates expected revision;
5. gathers and revalidates any transition-specific external evidence (for
   `plan start`, exact Root/worktree/branch/commit state);
6. executes one domain transition;
7. persists exact operation intent before its supported effects;
8. publishes the graph pair and its narrow Git checkpoint through the locked
   checkpoint kernel, with current index/ref checks and durable progress;
9. records completed checkpoint evidence before returning a completed mutation;
10. releases the lock and returns structured output, or a typed pending result
    for same-operation recovery when publication is incomplete.

These steps describe native 0.2.0. Installed 0.1.13 remains the separate bootstrap
writer until terminal self-host cutover. Graph/index/ref effects are recoverable,
not one cross-resource atomic transaction; see [Root checkpoints](../governance/root-graph-checkpoints.md).

Design backup is a separate deterministic operation that completes and verifies before any model-driven design rewrite begins.
