# Ordinary Graph Write Attribution and Recovery

## Authority and boundary

2026-10-16 bounded clarification of ADR-0019/0022's already-required initiating
actor and generated operation identity. This is pending0.2 target Design, not
installed0.1.13 behavior or the proposed1.0 principal/authentication system.
It governs [Root checkpoints](root-graph-checkpoints.md) and the existing typed
[operation service](lifecycle-receipts.md). No new graph schema, Plan status,
journal engine, actor registry or generic command executor is introduced.

## Actor source is explicit request attribution

Validate one non-whitespace initiating actor before creating a journal or making
Git/graph effects. This is an attributable caller declaration, not authentication.
Do not infer it from stored Plan owner, Git author/committer, OS login, report
author, approving ADR owner, command name, system or unknown. Git signing identity
remains an independent prerequisite, not proof of the operation's initiator.

| Entry | Sole actor source / required candidate interface |
|---|---|
| plan start | Explicit nonempty request owner; CLI/MCP must require it rather than synthesize default owner. No duplicate actor argument |
| plan clarify | Validated contained input.actor; no second conflicting source |
| plan add / implemented / reject / release / rewire-compensation | Required CLI --actor, corresponding required MCP field where that tool already exists, and typed application request field |
| graph repair-provenance / repair-compensation-predecessors | Same required explicit actor; reason is not an actor |
| First workspace graph creation | Required actor from its actual typed entry; no bootstrap system default |
| prepare / withdraw | Their existing explicit operation_id + actor requests stay unchanged |
| Acceptance / release closure through operation begin/finalize | Existing validated initiating actor remains frozen; no second checkpoint identity or substituted actor |
| Read-only show/status and standalone graph render restoration | No new actor argument; render remains the separate no-lifecycle-transition contract |

The start owner requirement does not otherwise redesign run_id, grant permissions
or infer1.0 identity. The Skill supplies its explicit actor label; scripts/tests
do likewise. All consuming Skills/templates are updated in the already-assigned
final consumer Plan, using actual released help rather than calling these future
arguments against installed0.1.13. Missing/blank input is a no-effect refusal.

## Internally generated ordinary operation identity

Ordinary typed graph writes keep an internally generated UUID. Do not add a
uniform caller --operation-id to every command. Under the same Root lock, validate
the caller's original expected_revision and complete operation, then freeze the
generated ID, initiating actor, repository/workspace, typed target, original
observer and exact candidate inputs in the existing checkpoint runtime before
the first effect. Preparation/withdrawal's explicit IDs are not replaced.

Once persisted, return that ID and actual progress in success and in any error
after admission. Root status/workspace pending queries must find it even before
the graph revision changes or a newly registered Plan appears. A lost response
does not authorize rerunning the original add/start/implemented command with a
fresh revision. Query the pending record; retain the original identity and inputs.
If journal admission itself never persisted and no effects were attempted, report
that bounded no-effect refusal; never fabricate a recoverable existing record.

## Existing typed recovery entry

Use operation show / Root pending query to locate the original record, then the
existing operation finalize entry's bounded GraphCheckpoint dispatch. Current
caller graph/stage observations protect recovery against stale requests; they do
not replace the original observer, actor, intent, candidate bytes or candidate
commit. Fresh safety evidence is still required at every unfinished stage.
The initial graph case retains its explicit absent-before recovery rules.

Checkpoint recovery reads the persisted native runtime; callers cannot supply a
new graph, candidate tree, path allowlist or arbitrary closure. Public operation
begin/stage cannot manufacture a GraphCheckpoint intent. Finalize resumes the
same kernel and skips objectively completed steps; it never invokes the original
business mutation again or creates another operation/generation/checkpoint.
Missing/corrupt/foreign journal, changed bindings, stale stage or conflicting Git
facts refuse with pending evidence rather than silent rollback. Pure completed
replay returns recorded identity without another graph revision or reachable commit.

## Implementation scope and verification

Plan19 owns existing graph/Plan/workspace request adapters, CLI/MCP DTOs/help,
checkpoint integration and focused native tests; its immutable body is unchanged.
Use existing operation/runtime modules only where directly required. No production
changes in Root, no candidate global install or real bootstrap graph conversion.
Final consumer planning already owns matching Skills; no extra compensation Plan
is needed merely to specify these missing inputs.

Required tests: missing/blank actor no journal/Git/graph effects; explicit start
owner without synthetic default; source actor exactly retained; CLI/MCP parity;
each ordinary entry uses generated-ID checkpoint; failure before/after graph
publication; lost reply discoverable before Plan existence; same-ID finalize
with current observations; stale/current identity distinction; one business
transition and reachable checkpoint; changed actor/intent cannot rewrite history.
Runtime tests are NOT_RUN by this Design clarification.

External semantics unchanged: [Git update-ref2.45](https://git-scm.com/docs/git-update-ref/2.45.0),
reopened2026-10-16, supplies expected-old ref checking, not initiating-agent
attribution or cross-resource transaction identity. Those are MINE's explicit
application contracts; no Git user setting is used to fill missing actors.
