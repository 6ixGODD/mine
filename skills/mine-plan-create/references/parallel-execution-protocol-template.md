# Parallel Execution Protocol

Create `.mine/plan/parallel-execution-protocol.md` when any plan declares parallel implementation lanes.

## Global rules
- Every lane has one owner, a coherent responsibility/namespace, exclusions and a physical mapping. Internal file changes within that authority do not require permission churn.
- Read the complete native held-owner and selected-owner snapshot, not only READY. IN_PROGRESS and IMPLEMENTED remain held until objective integration/cleanup/checkpoint completion.
- Physical overlap requires its exact approved semantic partition/shared-entry integration rule; conservative overlaps, algorithms, schemas, migrations, lockfiles, whole generators and mutable outputs remain exclusive.
- Shared files are reserved to a named serial integration owner.
- A timeout or silent agent is not permission to launch a duplicate owner; inspect existing work first.
- No lane may edit the execution graph unless explicitly designated scheduler/integrator.
- No lane may stage, unstage, commit, reset, restore, clean, or discard another lane's work.
- Native writes carry the caller-observed revision and explicit actor. Pending graph publication is a global barrier; lost responses query/resume the same operation, never duplicate owners or manual graph-only fast-forward.

## Reserved shared files
List repository-specific files such as manifests, lockfiles, migrations, generated schemas, OpenAPI files, generated clients, central dependency registries, root lint/test configuration, execution graph, and reports.

| File/pattern | Owner | Other lanes may | Join requirement |
|---|---|---|---|
| `<path>` | `<lane>` | read only | `<evidence>` |

## Lane contract
For every lane record:
- plan path and work-package ID;
- start gate and accepted predecessors;
- namespace/responsibility, exclusions, physical mapping and original/effective provenance;
- exact policy/ADR byte binding if semantic; no caller-supplied approval or missing-policy downgrade;
- registered linked lineage checkout; Root stays integration; compensation reuses checkout, not mutable output;
- exact Plan-node mutable output directory for every language and shared immutable environment;
- read-only dependencies;
- forbidden paths;
- expected commits and report fragment;
- integration requests for shared files;
- narrow verification commands;
- completion signal and join artifact.

## Integration
- Merge or integrate in dependency order.
- Resolve shared-file requests only in the integration lane.
- Run schema/code generation once after all contract-defining lanes join.
- Record generator inputs -> complete output transaction -> seal/join gates and ensure every required check may produce its outputs.
- Review merges only the frozen reviewed tip, verifies applicable integrated evidence, checks stopped writer/current ref/clean checkout, removes only exact merged worktree/local branch, then finalizes acceptance and its checkpoint.
- Rejection preserves source/ref and declared compensation lineage; transport failure is same-operation recovery, not a new compensation.
- Run the full join verification matrix and record exact results.
