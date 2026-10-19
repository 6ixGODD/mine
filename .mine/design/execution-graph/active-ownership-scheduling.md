# Active Ownership Snapshot and Atomic Start

## Authority and implementation boundary

Accepted target under [ADR-0018](../decisions/0018-effective-ownership-snapshot-and-atomic-start.md),
using [semantic ownership](../governance/semantic-ownership.md) for ADR-0016.
Installed 0.1.13 wave omits active owners and start has no active-scope check.
Candidate adapters implement held ownership snapshots and atomic compatibility
checks with observer revision. Installed behavior, candidate implementation,
focused verification and independent acceptance remain distinct evidence.

## Snapshot and common evaluator

Extend existing graph wave/show application DTOs rather than require one process
per active Plan. A scheduling snapshot includes workspace ID/revision, graph and
policy identities, relevant states/dependencies, Plan/body identities, original
and effective ownership, owner/run, lineage/registered worktree, pending closure,
eligible/selected IDs and exact blocking reason/resource/owner for each candidate.
Machine-local absolute locations are runtime evidence, not persisted authority.

One indexed compatibility evaluator serves wave and locked start. It rejects
ancestor relationships, intersecting effective namespace claims, overlapping
exclusive behavior/resource ownership, same lineage/worktree, shared mutable
outputs and whole generator transactions. Physical overlap is conservative
unless an approved semantic partition and shared-entry integration rule allow
that exact overlap. Missing/invalid required semantic policy or scope is an
error, never false. A valid policy-free conservative context is distinct from
a failed semantic lookup; see [conservative authority](../governance/semantic-ownership.md#conservative-repositories-do-not-require-semantic-policy-approval).
Both contexts use this evaluator. A conservative node never receives semantic
overlap relief merely because a neighboring node has an approved policy.

## Repository identity at the scheduling boundary

Detailed integration clarification, 2026-10-14: `MineConfig.repository_id`
(validated against Root ownership markers) is the repository UUID used by
PlanIdentity, lifecycle operations, verification and scheduling. Persisted
`PlanWorkspace.project_id` is a project label/slug, not repository authority.
It is not required to equal that UUID and must not be rewritten for scheduling.

The common pure scheduling evaluator receives the validated repository identity
explicitly from the application adapter, compares every node's repository_id
against it, and emits that identity in OwnershipWave. Keep the graph object,
workspace identity, persisted project label and graph byte digest unchanged.
Do not clone a graph and substitute its project_id merely to satisfy a comparison:
that would sever the supplied graph object's relationship to its byte digest.

Correcting the accepted kernel's mistaken slug comparison and wave assignment
is a bounded integration defect correction, not a new identity model or a graph
migration. The Plan19 integration owner may change
`src/domain/scheduling/kernel.rs`, its required signature/call sites and focused
identity assertions in `tests/ownership_scheduling.rs` for this correction only.
This explicitly supplements physical scope for the already assigned runtime
integration responsibility; it grants no unrelated scheduling/kernel redesign
and does not amend the immutable Plan or any accepted report.

Prove correct UUID plus original non-UUID slug succeeds; same slug with another
repository UUID is rejected; wave reports the UUID; graph bytes/hash/slug are
unchanged; and workspace/Plan/branch/worktree identity checks still fail closed.
Use [the production policy bridge](../governance/semantic-ownership.md#production-policy-approval-bridge)
for approved policy observation; ADR and graph revisions remain distinct.

## Eligibility, occupancy and release

READY means predecessors/Design admission permit attempted execution; it is not
an ownership grant. IN_PROGRESS and IMPLEMENTED hold their effective resources,
including a positive review whose merge/checks/cleanup are not complete. A stopped
worker still holds ownership; process disappearance/time is not authority to free
it. ACCEPTED releases only after verified integration/cleanup and completion of
any required ADR-0022 graph checkpoint. Pending checkpoint is a workspace-wide
mutation/grant barrier, even when TOML already records a terminal transition;
show the pending operation rather than a falsely executable successor wave.

REJECTED releases execution ownership after the evidenced rejection handoff;
its preserved branch/worktree is reserved for the declared next compensation,
not occupied by a still-running rejected executor. Rejection cannot finish that
handoff while a writer is using the checkout. WITHDRAWN never grants execution.
No TTL, distributed lease service or new graph lifecycle status is introduced.

## Wave and start algorithms

Wave reads one validated snapshot, gathers held owners, and selects READY
candidates greedily in stable declaration order against both held and selected
owners. Return exclusions and their reasons. A wave is an advisory result at
one revision, not a durable authorization or a globally maximum schedule.

Start takes the caller-observed expected revision. Under the Root graph lock:

1. reject stale revision before expensive Git queries;
2. validate candidate state, hard predecessors and effective authority: original
   conservative declarations, or the required verified semantic-policy binding;
3. recompute held ownership and run the common compatibility evaluator;
4. revalidate [durable preparation](../governance/ready-preparation.md), its
   complete graph-only history proof and fresh Root/worktree safety;
5. atomically record IN_PROGRESS, owner/run, preparation generation and immutable
   branch anchor, then complete its [graph checkpoint](../governance/root-graph-checkpoints.md)
   before returning start authorization.

Two conflicting starts cannot both acquire authority from the same snapshot.
After a stale result, read/re-evaluate; never internally substitute a newer
revision on behalf of a decision made against the old state. Graph-lock safety
does not license uncoordinated non-MINE Git or file writes.

## Public and persistence boundary

The target extends `mine graph wave`, `mine_graph_wave` and Plan show DTOs with
the complete snapshot/effective scope. Start CLI/MCP gains explicit expected
revision. Fields and versioned schema updates must ship together with their
actual help/JSON schemas/Skill references. Installed 0.1.13 cannot be invoked with
these candidate arguments during self-development; use installed lifecycle
and a serialized integration owner for bootstrap changes.

Typed occupancy is derived from node/closure facts under the existing graph
lock. Clarification mutations use the same active-compatibility rules. They do
not create a second ownership registry or silently edit frozen Plan text.

## Verification and sources

Race incompatible starts from one revision: exactly one proceeds; the other
reports conflict/stale state without mutation. Cover active and implemented holds,
failed integration, rejected handoff, compensation reuse, disjoint starts,
semantic partitions, malformed scopes and same-worktree refusal. Compare CLI/MCP
snapshots and reasons. No READY-only wave test substitutes for start enforcement.

Opened 2026-10-09: [Git worktree](https://git-scm.com/docs/git-worktree) defines
checkout identity constraints; [Fowler: Bounded Context](https://martinfowler.com/bliki/BoundedContext.html)
motivates declared semantic boundaries, not source-level inference.
