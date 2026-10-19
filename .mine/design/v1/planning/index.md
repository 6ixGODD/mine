# 1.0 Contract-first Parallel Delivery Blueprint

## Current status, not simulated graph state

Prepared2026-10-16 under the owner's explicit planning/foundation authorization.
ADRs24–31 are APPROVED. These are **11 unregistered Plan blueprints**, not READY
nodes and not permission to execute the current workspace. Provider documents
are work-package attachments to one runtime Plan, not three extra serial Plans.
The current graph remains revision90 with24 nodes; future numbers are provisional.

The subsequent [timing/logging addendum](observability-addendum.md) distributes
the new requirement across these same eleven lanes. ADR-0032 was separately
approved at registry revision 73; shared contract work/review remains a gate.

The installed0.1.13 graph closes only when its entire workspace is terminal.
`planning_scope=v1.0.0` does not isolate release trains. Registering these now
would block0.2; putting them in current `docs/plan` would also expose them to its
terminal purge. This durable Design bundle survives the approved `.mine/design`
cutover. Do not circumvent the gate with DRAFT, withdrawal or candidate self-bootstrap.

## Minimal dependency distance

```text
Current0.2 close + independently reviewed foundation
                       |
    +------------------+------------------------------------+
    | storage | identity | coordination | projects | resources |
    | autonomy | lifecycle | HTTP | dashboard | runtime       |
    |          integration harness starts alongside          |
    +------------------+------------------------------------+
                       |
    all ten leaf Plans ACCEPTED + real integrated/native gates
                       |
              integration Plan final acceptance
                       |
               separate sync / release closure
```

There is no storage -> API -> UI serial chain. Domain handlers use frozen ports;
their own real SQL mapping tests are required, but whole-product joins belong
to integration. Integration starts its harness early, consumes accepted slices
incrementally, and cannot enter final packaging until every leaf is ACCEPTED.
The graph therefore has11 initially independent nodes with **mandatory internal
join gates**, not misleading graph edges forcing the harness to start last.
No node can self-accept or ignore its frozen late gate because graph READY allows
its first package. Maximum useful lanes are resource-limited, not a promise to
spawn11 heavy builds on this host. Runtime additionally has4 disjoint inner lanes.

## Plan allocation and write-domain ledger

| Provisional ID | Blueprint / user-visible responsibility | Exclusive implementation namespace |
|---|---|---|
| plan25 | [Storage](storage.md): durable transactions, import/export | `v1/crates/storage/` |
| plan26 | [Identity](identity.md): roles, assignment, history, handoff | `v1/crates/identity/` |
| plan27 | [Coordination](coordination.md): messages, commitments, waits, escalation | `v1/crates/coordination/` |
| plan28 | [Projects](projects.md): directory, init/deinit/relocate | `v1/crates/projects/` |
| plan29 | [Resources](resources.md): global grants and temporary channels | `v1/crates/resources/` |
| plan30 | [Autonomy](autonomy.md): explicit policy, decisions, budgets | `v1/crates/autonomy/` |
| plan31 | [Lifecycle](lifecycle.md): SQLite graph/ADR/nodes and existing safety | `v1/crates/lifecycle/` |
| plan32 | [HTTP](http.md): local service/auth/projection/SSE | `v1/crates/http/` |
| plan33 | [Dashboard](dashboard.md): actual UI workflows | `v1/dashboard/` |
| plan34 | [Runtime](runtime.md): supervisor plus [Codex](codex.md), [Claude](claude.md), [Pi](pi.md) | `v1/runtime/`; shared foundation stays frozen |
| plan35 | [Integration](integration.md): early harness, production composition, Skills, complete candidate | `v1/integration/`, Root wiring and release assets |

All scope tokens are `v1.0.0`. Before native registration, confirm IDs remain free;
renumber the unstarted bundle consistently if another authorized planning action
has allocated them. Never rewrite an immutable registered/executed node.

## Requirements coverage

| Owner requirement | Responsible lane(s) / final proof |
|---|---|
| One Project aggregate/global directory; safe complete removal | projects+storage; interrupted init/deinit/restore E2E |
| Architect/Executor/Reviewer, short ID and provider identity | identity+runtime; stale/forged/cross-Project refusal |
| Plan ownership and complete A-to-B trace | identity+lifecycle; dirty handoff and old-writer fencing |
| DM/group uniform protocol, offline and low-token promises | coordination; zero inference while waiting, exactly one effective wake |
| Escalate peer -> Architect -> user only for real user decisions | coordination+autonomy; pre-start and mid-work cases |
| Crash/pause/restart at exact work node | lifecycle+runtime; no duplicate effect or unnecessary receipt rerun |
| Actual Project model/effort/fast discovery | runtime provider lanes; native config-root/worktree parity and no silent fallback |
| Unified elegant local dashboard / b2jI | dashboard+HTTP; real decisions, state consistency and accessibility |
| SQLite machine authority, Markdown for humans | storage+lifecycle; quiescent0.2 import, old-writer refusal, no TOML dual authority |
| Experimental autonomous dispatch and technical ADR approval | autonomy; default-off, policy attribution, stop/budget and independent review |
| Cross-Project shared resources/Lead channel | resources; whole-set fairness, uncertain holder, private membership |
| OpenCode removed only in1.0 | integration; runtime/distribution/docs tests, history preserved |
| Session create/resume failure can recover safely | runtime+identity; exact ID and proven quiescence before replacement |
| Ready-to-use installed product rather than unit-only prototype | integration; three native OS/provider routes, real artifacts and install smoke |

## Activation transaction after0.2

1. Confirm current0.2 full sync/release closure completed, old workspace closed,
   exact stable baseline and clean Root; do not infer this from terminal graph alone.
2. Use the **released**0.2 binary/Skills, not the1.0 candidate, to open next dev
   workspace. Check actual installed help/MCP schemas and native Design readiness
   for `v1.0.0`; retain approved ADR raw bytes across layout migration.
3. Independently review this foundation commit and correct shared contract/fixture
   defects before release of downstream Plans. The author's tests are evidence,
   not independent acceptance. Record the reviewed commit/digests in every body.
4. Materialize each blueprint plus the applicable common protocol and required
   runtime attachments into canonical `.mine/plan/planNN-slug.md`. Resolve all
   relative Design links to the active `.mine/design/` paths. Freeze exact owned
   namespaces, report paths, predecessor arrays (empty for these first packages),
   late gates, output paths and current verification commands. Do not substitute
   this index for reading each detailed blueprint.
5. Native add registers DRAFT; native release yields READY only after readiness.
   Serialize Root writes, carry observed revision where the released API requires
   it, validate after writes. Do not hand-edit graph, invent flags or bulk ignore
   failed registration. Record actual before/after revision and statuses.
6. Prepare/start isolated worktrees from accepted dev and acquire single owners.
   No duplicate writer after timeout. Run ready lanes in available CPU/disk/provider
   budget, independent of conceptual DAG width. One shared setup owner provisions
   immutable dependency environments before readers; mutable outputs stay separate.

## Exact next action now

Continue current0.2 execution/review. Do not call `mine-plan-exec plan25` yet.
The1.0 next action is independent foundation/bundle review and, after0.2 closure,
native materialization/registration above. No new product decision is awaiting
approval merely to split these already-approved requirements.
