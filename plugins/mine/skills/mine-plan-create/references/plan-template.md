# Plan NN: Short title

## Status
`DRAFT | BLOCKED | READY | IN_PROGRESS | IMPLEMENTED | ACCEPTED | REJECTED`

## Goal

## User-visible outcome

## Canonical Plan identity
- ID: `planNN(?:-NN)*(?:-CNN)?`
- Filename: `docs/plan/<id>-<lower-kebab-slug>.md`
- Lineage/worktree identity: derive from the typed Plan-ID contract; do not
  duplicate parsing or compensation-number logic in the Plan.

## Governing design references
- `docs/design/<area>/<leaf>.md#<anchor>` (cite exact design leaves and anchors under `docs/design/` — progressive disclosure rooted at `docs/design/index.md`; MINE owns `docs/design/`)

## Requirements traceability
| Requirement | Design leaf/anchor | Work package | Acceptance evidence |
|---|---|---|---|

## Current evidence and baseline
| Area | Current implementation | Evidence path/commit | Verified behavior | Gap |
|---|---|---|---|---|

## Research source register
| Source title | Organization/version | URL | Accessed | Verified claim | Plan implication |
|---|---|---|---|---|---|

## Decisions
### Material user decisions
### Local decisions made by the planner
### Assumptions and unresolved gates

## Scope
### In scope
### Non-goals
### Historical baggage to remove

## Dependency and parallelism graph
```mermaid
flowchart LR
```

| Work package | Depends on | Parallel group | Exclusive write scope | Shared-file requests | Start gate | Join gate |
|---|---|---|---|---|---|---|

## Work packages

### WP1 — Name
- Purpose:
- Inputs and predecessors:
- Exact files/symbols/contracts:
- Current behavior:
- Required final behavior:
- Input/output/error/lifecycle semantics:
- Transactions, concurrency, retries, timeouts:
- Security/privacy considerations:
- Configuration/dependencies:
- Cleanup/removals:
- Edge and failure cases:
- Tests and fixtures:
- Narrow verification commands and expected evidence:
- Downstream artifact:
- Suggested commit:

## Mutable output ownership

Every Plan owns mutable build, test, generation, coverage, packaging, and
runner output beneath `<Root>/.mine/build/<plan-id>/<tool-or-output-class>/`.
Compensation nodes use a fresh namespace even when they reuse a lineage
worktree. Reuse Root-managed toolchains, dependency downloads, virtual
environments, and only demonstrably safe shared caches; never create a local
`target`, `.venv`, `venv`, `node_modules`, or equivalent output tree in a Plan
worktree. For Rust, verification must set `CARGO_TARGET_DIR` to the Plan's
`cargo-target` directory. If measured size is predictably exceptional, record
the estimate and pause for explicit owner agreement before relaxing isolation.

## Integration and join procedure

## Verification matrix
| Scope | Command | Preconditions | Expected evidence | Owner |
|---|---|---|---|---|

## Acceptance checklist
- [ ] Every requirement is traced to architecture, implementation, and evidence.
- [ ] Every registered source was opened and understood by executors/reviewer.
- [ ] Parallel write scopes are disjoint and shared files have one owner.
- [ ] Required quality gates pass or failures are accurately recorded.
- [ ] Obsolete implementation is removed or explicitly scheduled for removal.
- [ ] No unapproved external mutation occurred.

## Report path
`docs/plan/reports/<plan-name>-implementation.md`

## Suggested commits
