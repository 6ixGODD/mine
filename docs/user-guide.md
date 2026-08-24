# MINE User Guide

This guide covers the practical use of MINE.

For the design rationale behind Design, Plans, the Execution Graph, Review, worktrees, compensating Plans, and stable
state, see [Core Concepts](concepts.md).

---

## 1. Installation

### Windows

Run the following in PowerShell:

```powershell
irm https://raw.githubusercontent.com/6ixGODD/mine-is-not-everyones/master/scripts/bootstrap.ps1 | iex
```

### macOS / Linux

Run the following in your terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/6ixGODD/mine-is-not-everyones/master/scripts/bootstrap.sh | sh
```

After installation, restart your terminal and verify that MINE is available:

```sh
mine --version
```

Then install the Agent integrations:

```sh
mine setup
```

Check the current integration status with:

```sh
mine agent status
```

MINE currently supports Claude Code, Codex, Pi, and OpenCode.

For clients with MCP support, `mine setup` registers the local `mine mcp serve` server. When MCP is unavailable, Skills
automatically fall back to the JSON CLI interface. Pi does not include MCP in its minimal core, so it can use MINE
entirely through Skills + CLI.

### Updating

`mine update` replaces the binary **and** refreshes the Skills of every
installed Agent from the new version's embedded payload, so you do not need
to re-run `mine setup` after an upgrade:

```sh
mine update
```

`mine setup` is for **first-time installation** and for adding or removing
Agents (for example `mine setup --agents claude-code,codex`). After an
update, verify with `mine --version` and `mine agent status`.

### Pi shared Skills

Pi discovers Skills both in the shared Agent Skills directory
(`~/.agents/skills`, where Codex installs) and in its own directory
(`~/.pi/agent/skills`). To avoid Pi loading two copies (and the resulting
conflict warning), when the shared directory already contains a complete
MINE Skill set, MINE installs Pi's Skills into the shared directory and
removes any legacy MINE Skills from `~/.pi/agent/skills`.

### Installing a specific version

The bootstrap installer uses the latest Release by default. To install a specific version, set `MINE_REF`.

For example, to install `v0.1.4`:

#### Windows

```powershell
$env:MINE_REF = "v0.1.4"
irm https://raw.githubusercontent.com/6ixGODD/mine-is-not-everyones/master/scripts/bootstrap.ps1 | iex
```

#### macOS / Linux

```sh
MINE_REF=v0.1.4 \
sh -c "$(curl -fsSL https://raw.githubusercontent.com/6ixGODD/mine-is-not-everyones/master/scripts/bootstrap.sh)"
```

---

## 2. Initialize a Repository

Run the following from the root of a Git repository:

```sh
mine init
```

A repository normally needs to be initialized only once.

`mine init` creates the repository state required by MINE, including `docs/design/`. It does not design the system for
you or modify application code.

### ADR approval and Design readiness

Material architecture decisions are recorded as individual ADRs. The
registry is the machine source of status and binds approval to the exact
decision-body digest:

```sh
mine adr add --id ADR-0010 --path docs/design/decisions/0010-example.md --title "Example decision"
mine adr propose --id ADR-0010
mine adr show --id ADR-0010
mine adr approve --id ADR-0010 --by repository-owner --note "Approved for planning"
mine design readiness --scope plan01
```

`mine-arch` may prepare and propose a DRAFT ADR, but it cannot approve one or
infer approval. If an applicable planning-blocking ADR is unresolved,
readiness reports the blocker and planning must stop. Editing an approved ADR
body changes its digest and requires a new proposal/approval cycle.

---

## 3. Start an Engineering Change

Day-to-day work is driven primarily through five Agent Skills:

```text
mine-arch
mine-sync
mine-plan-create
mine-plan-exec
mine-plan-review
```

These names refer to **Agent Skills**, not subcommands of the `mine` CLI. Invocation syntax varies by Agent, so use the
form supported by your client.

Examples:

**Codex:**

```text
$mine-arch ...
```

**Claude Code (Marketplace plugin):**

```text
/mine:mine-arch ...
```

**Pi:**

```text
/skill:mine-arch ...
```

**Other Agents:**

```text
/mine-arch ...
```

The examples above use `mine-arch`, but the same naming convention applies to the other four Skills: `mine-sync`,
`mine-plan-create`, `mine-plan-exec`, and `mine-plan-review`.

---

## 4. Create Plans

Once the Design is clear:

```text
mine-plan-create <scope to plan>
```

Plans use canonical hierarchical identities such as `plan01-01` and
`plan01-02-C01`. The file must be exactly
`docs/plan/<id>-<lower-kebab-slug>.md`; compensation IDs are contiguous sibling
`C` ordinals in the same lineage. Legacy IDs such as `01` are not allocated
for new Plans.

For example:

```text
mine-plan-create Turn the Passkey login changes we just agreed on into an execution plan.
```

An explicit scope becomes the planning boundary. `mine-plan-create` inspects the relevant Design, code, tests,
dependencies, and established engineering practice, then creates one or more executable Plans.

If you invoke:

```text
mine-plan-create
```

without a scope, the Skill determines the next work to plan from the current Design, Execution Graph, and repository
state. This usually requires broader exploration.

If planning reveals that the current Design is insufficient to support the implementation, `mine-plan-create` may invoke
`mine-arch` itself, update the Design, and then continue planning. The user normally does not need to switch Skills
manually.

To inspect the currently executable Plans:

```sh
mine graph ready
```

To inspect a specific Plan:

```sh
mine plan show --id <id>
```

---

## 5. Execute and Review

For a Plan in the `READY` state:

```text
mine-plan-exec <Plan path>
```

After implementation, the Plan moves to:

```text
IMPLEMENTED
```

Then run:

```text
mine-plan-review <Plan path>
```

in an **independent Agent session**.

**Do not** continue directly into Review from the same context that just ran `mine-plan-exec`.

You may use:

- two separate sessions of the same Agent; or
- two different Agents.

For example:

```text
Codex session A
    → mine-plan-exec

Codex session B
    → mine-plan-review
```

Or:

```text
Claude Code
    → mine-plan-exec

Codex
    → mine-plan-review
```

Review can produce three outcomes:

- **ACCEPTED** — the implementation is accepted and integrated;
- **ACCEPTED after a local fix** — the Reviewer may directly fix a clear, local issue that does not change the
  engineering contract;
- **REJECTED** — the current Plan cannot be accepted. A compensating Plan is typically created, and affected downstream
  dependencies are updated.

If the reason for rejection requires a Design change, the Reviewer may invoke `mine-arch` directly. Users normally do
not need to edit the Execution Graph by hand.

---

## 6. Parallel Execution

If multiple independent Plans are `READY`, they may be executed in parallel.

MINE isolates Plans using separate branches and worktrees, for example:

```text
dev
├── plan/01-api       → worktree A
├── plan/02-storage   → worktree B
└── plan/03-ui        → worktree C
```

You normally do not need to manually:

- create Plan branches;
- create worktrees;
- edit the Execution Graph;
- manage graph revisions;
- manage report paths;
- integrate accepted Plan branches into `dev`.

MINE and the corresponding Skills manage these states automatically.

### Mutable build and test outputs

Plan worktrees reuse Root-managed toolchains and dependency downloads, but
mutable outputs are owned by the exact Plan node:

```text
<Root>/.mine/build/<plan-id>/<tool-or-output-class>/
```

For Rust, use `CARGO_TARGET_DIR=<Root>/.mine/build/<plan-id>/cargo-target`.
Do not use Root `target` or create a worktree-local `target`, `.venv`, `venv`,
`node_modules`, or equivalent cache. A compensation Plan receives a fresh
namespace even when it reuses its predecessor's lineage worktree. A measured
exceptionally large output requires an estimate and explicit owner agreement
on serialization, namespace coarsening, and cleanup before building.

---

## 7. Release

Once all planned work is complete, run the Final Sync:

```text
mine-sync prepare this repository for stable release
```

This reconciles `docs/design/` against the final codebase.

Then, from an independent Review context, run:

```text
mine-plan-review complete release closure
```

Release Closure performs the local release finalization, including:

- validating the Execution Graph and release gates;
- confirming that the final Design has been synchronized;
- constructing and validating the stable candidate;
- checking that product code contains no temporary Plan references from the current development cycle;
- removing `docs/plan/` and execution reports;
- integrating the final state into stable;
- cleaning up MINE-managed local branches and worktrees.

MINE **does not** automatically perform:

- `push`;
- Git tagging;
- GitHub Release creation;
- package publishing;
- any other remote release operation.

Those actions remain under user control.

---

## 8. Inspect Status and Diagnostics

### Agent integration

```sh
mine agent status
mine setup
```

### Current repository

```sh
mine status
mine doctor
```

### Design

```sh
mine design status
mine design validate
```

### Execution Graph

```sh
mine graph status
mine graph ready
mine graph wave
mine graph validate
```

### Plan

```sh
mine plan show --id <id>
```

### Release checks

```sh
mine release --format json
```

When a command fails, check the human-readable or JSON diagnostic returned by the CLI first.

For the design rationale behind these mechanisms, see [Core Concepts](concepts.md).
