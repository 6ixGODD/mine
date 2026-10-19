# Plan NN: Short title

## Status
`DRAFT | BLOCKED | READY | IN_PROGRESS | IMPLEMENTED | ACCEPTED | REJECTED`

## Goal

## User-visible outcome

## Canonical Plan identity
- ID: `planNN(?:-NN)*(?:-CNN)?`
- Filename: `.mine/plan/<id>-<lower-kebab-slug>.md`
- Lineage/worktree identity: derive from the typed Plan-ID contract; do not
  duplicate parsing or compensation-number logic in the Plan.

## Governing design references
- `.mine/design/<area>/<leaf>.md#<anchor>` (cite exact design leaves and anchors under `.mine/design/` — progressive disclosure rooted at `.mine/design/index.md`; MINE owns `.mine/design/`)

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

## Responsibility and immutable/effective authority
- Namespace/responsibility, supported behavior and explicit exclusions:
- Real entry point -> producer -> consumer and invariant:
- Physical mapping for navigation/admission (not a frozen internal file inventory):
- Shared declarations/integration owner; excluded algorithms/schema/migrations/locks:
- Generator transaction: inputs -> complete owned outputs -> seal/join gates:
- Conservative nonempty mapping, or actual approved policy/ADR byte binding:
- Original Plan/body digest and declarations; approved clarification tail if any:
- Material decisions requiring owner ADR approval rather than local autonomy:

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
- Responsibility/contract and initial physical locations (internal file splits are autonomous within authority):
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

## Native admission and recoverable progress
- Observed authoritative Root revision and complete held/selected owner snapshot:
- Actual CLI/MCP contract and installed version, not unshipped bootstrap commands:
- Explicit prepare actor/operation identity, generation and checkpoint completion:
- Start admission and same-operation query/resume after unknown results:
- Root integration branch; registered linked worktree and stopped compensation lineage:
- Own inputs/report committed before graph writes; no manual checkpoint fast-forward:
- Acceptance requires exact reviewed merge, actual integration gates and safe cleanup:

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

Record RUN/REUSED/INVALIDATED/NON_REUSABLE/NOT_RUN separately, with exact inputs,
command, policy, tools/environment/platform/features and artifact identities.
Reuse applicable existing evidence without duplicate runs; independent/native
requirements and changed inputs still need their actual probes. Resource absence,
warnings, nonzero exits and an exit0 invalidation are never green evidence.

## Acceptance checklist
- [ ] Every requirement is traced to architecture, implementation, and evidence.
- [ ] Every registered source was opened and understood by executors/reviewer.
- [ ] Held and selected owners are checked by native admission; approved semantic partition/shared-entry rules justify any physical overlap.
- [ ] Whole generators/shared mutable outputs have one owner and complete gate closure.
- [ ] Required quality gates pass or failures are accurately recorded.
- [ ] Obsolete implementation is removed or explicitly scheduled for removal.
- [ ] No unapproved external mutation occurred.

## Report path
`.mine/plan/reports/<plan-name>-implementation.md`

## Suggested commits
