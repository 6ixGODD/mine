# ADR-0007: Canonical Plan Lineage and Compensation Worktree Reuse

## Status

Approved by the repository owner on 2026-08-21.

Bootstrap approval evidence: proposed-content SHA-256
`65D048B8AEA7802973C844B1730A076C626C2CE08603C3BC86EB8A8E4A5F574B`.
This approval predates the deterministic ADR registry introduced by this
decision set and must be imported by the one-time registry bootstrap.

## Context

Free-form Plan identifiers (`01`, `02-1`, `08-2`) do not encode hierarchy or
compensation lineage. If every compensation node receives a new linked
worktree, rejected work leaves duplicate checkout directories and repeated
environments under `.mine/`. Parallel execution still requires node-specific
branches, but compensation is serial continuation of one rejected lineage.

## Decision

Adopt the following case-sensitive canonical grammar:

```text
base Plan ID:       planNN(?:-NN)*
compensation ID:   <base>-CNN
NN:                01..99 (00 forbidden)
slug:              [a-z0-9]+(?:-[a-z0-9]+)*
Plan file:          docs/plan/<id>-<slug>.md
Plan branch:        plan/<id>-<slug>
lineage worktree:   .mine/worktrees/<base-id>/
```

Examples:

```text
docs/plan/plan01-02-stage1-for-baseline-arch-impl.md
docs/plan/plan01-02-C01-fix-some-bugs-of-baseline.md
.mine/worktrees/plan01-02/
```

`C` appears at most once and only as the last ID segment. Compensation ordinals
are contiguous within a lineage: the first replacement is `C01`; if it is
rejected, its replacement is sibling `C02`, not `C01-C01`. A compensation node
must equal the rejected node's recorded `compensating_plan`, share its base
lineage, and inherit the rejected node's hard predecessors rather than depend
on a terminal rejected node.

Each node owns a unique branch, but every node in the lineage owns the same
worktree locator. On rejection, Root records the verdict and preserves both the
rejected branch ref and the linked worktree. Before compensation execution,
MINE requires the lineage worktree to be clean, creates the compensation branch
from the rejected branch HEAD, and switches only that linked worktree. It never
switches Root `dev` and never creates a second linked worktree for `C01`, `C02`,
or later compensation.

When a lineage is finally accepted, Review merges the active node into Root
`dev`, validates integration, removes the clean shared worktree, deletes the
merged active branch, and only then records `ACCEPTED`. Earlier rejected branch
refs remain until release closure for evidence and are removed only by bounded
managed-branch cleanup.

## Consequences

- Plan identity, path, branch, report paths, and worktree lineage become
  deterministically derivable and machine-validatable.
- `mine-plan-create`, `mine-plan-exec`, `mine-plan-review`, `mine-arch`, and
  `mine-sync` must use the same grammar and never invent legacy aliases.
- The current pre-implementation bootstrap node `01` and its old worktree name
  require one explicit cutover owned by the implementation Plan. The temporary
  exception is removed before stable release; dual grammar does not survive.
- Real-Git tests must prove base -> C01 -> C02 branch switching in one directory,
  dirty/locked refusal, Root branch stability, rejected-ref preservation, and
  final worktree removal.

## Rejected alternatives

- a new worktree for every compensation node: accumulates redundant directories
  and environments and loses lineage ownership;
- reusing the rejected branch: hides which implementation produced evidence;
- nested compensation IDs such as `C01-C01`: obscures ordinal ordering;
- unconstrained numeric IDs or filenames: cannot safely derive ownership;
- deleting rejected branch refs immediately: weakens review and release evidence.

## Approval questions

1. Approve the exact Plan ID/path/branch/worktree grammar above?
2. Approve sibling compensation numbering (`C01`, then `C02`) and one shared
   lineage worktree?
3. Approve retaining rejected branch refs until release closure while removing
   the linked directory immediately after final acceptance?
