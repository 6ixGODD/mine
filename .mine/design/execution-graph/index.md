# Execution Graph Design Index

The execution graph is the deterministic lifecycle model for one active temporary plan workspace.

## Documents

- [Domain model](domain-model.md)
- [Persistence and concurrency](persistence-and-concurrency.md)
- [State machine and algorithms](state-machine-and-algorithms.md)
- [Unstarted Plan withdrawal](plan-withdrawal.md)
- [Git evidence and graph-operation performance](git-evidence-and-performance.md) — candidate request-local implementation; measured budgets remain evidence-bound.
- [Active ownership snapshot and atomic start](active-ownership-scheduling.md) — accepted ADR-0018 target, including the independently deliverable active path-occupancy correction.

## Scope

The graph exists only while managed `dev` work is active. Its source and generated
view live under the configured Plan root (`.mine/plan/` for fresh 0.2.0;
`docs/plan/` during bootstrap) and are purged before stable release integration.

The graph is not a historical project-management database. Stable branches retain released code and durable design, not temporary planning state.
