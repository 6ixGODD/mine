# Semantic Ownership and Generated Write Closure

## Authority and implementation boundary

Accepted target under [ADR-0016](../decisions/0016-semantic-ownership-and-generated-write-closure.md).
Candidate source implements typed ownership, effective clarification and shared
compatibility evaluation. Agents must not pretend that installed 0.1.13 enforces
this model or that candidate implementation is independent acceptance.

## Ownership contract

One Plan owns a coherent responsibility, not an exhaustive filename list. Its
registered contract contains these typed facts (see native schemas for field names):

| Fact | Meaning |
|---|---|
| namespace claims | Explicit case-sensitive domain/module names and owned subtrees; declared excluded subtrees are outside authority. |
| responsibility / owned contracts | Bounded behavior and contract identities the owner may implement. |
| excluded responsibilities/contracts | Explicit negative authority, including shared algorithms owned elsewhere. |
| physical mapping | Safe source/test prefixes and composition/export locations needed for those responsibilities. |
| shared declarations | Exact shared entries, permitted declaration class and named integration owner. |
| generator transactions | Named generator, inputs, authoritative machine source, complete outputs and seal/digest refreshes. |
| policy identity | Exact approved Design references/digests defining mapping, partitions and shared-resource rules. |

Namespace hierarchy uses explicit segment boundaries, not substring matching.
Identifiers do not encode a filesystem path. Physical paths use the existing
containment/normalization rules. A binary validates declared structure and
identity, not inferred Rust semantics. Independent review checks that actual
changes satisfy the declared responsibility.

Inside the agreed mapping/responsibility, new files, internal moves/splits,
ordinary tests and necessary parent exports require no per-file reapproval.
An excluded contract, cross-module behavior, authority/security or acceptance
change is outside scope regardless of filename. Architecture remains the source
of responsibility; code layout is impact evidence, not the source of authority.

## Production policy approval bridge

Detailed integration clarification, 2026-10-14: this section implements the
already approved exact-policy authority in ADR-0016 through the existing
[ADR approval lifecycle](adr-lifecycle.md). It does not approve a concrete
repository policy, add an approval actor, or authorize an executor to approve
its own mapping. The accepted pure kernel verifies supplied bytes/provenance;
the application adapter must establish their authoritative source first.

### Policy source and exact approval binding

Keep the versioned `OwnershipPolicyDocument` JSON within the configured Design
root, conventionally `governance/ownership-policy.json`. Do not add another
global policy registry or a mandatory new config field merely to locate it.
A typed semantic registration/clarification request selects a safe policy path
and its approving ADR ID. These are untrusted selectors, not approval facts.
The exact public argument spelling is owned by the CLI/MCP integration and
must be published with real help/schema; this document invents no installed flag.

The selected ADR body contains exactly one fenced block with info string
`mine-ownership-policy`, whose JSON object has only `schema_version` (1),
`policy_path` (normalized repository-relative path), and `policy_sha256`
(SHA-256 of the exact external JSON bytes). Reject missing/duplicate blocks,
duplicate/unknown keys, unsupported versions and invalid paths/digests.
This is a typed content convention inside an ordinary ADR body, not a new ADR
registry schema or a second approval workflow. Its owner reviews the mapping
and proposes/approves the body containing that binding through existing commands.

The adapter, shared by CLI and MCP:

1. resolves Root configuration/Design ownership and contained paths, never a
   caller worktree copy or a path outside Design through symlinks/junctions;
2. loads the authoritative registry and selected ADR; requires APPROVED,
   applicability to the semantic registration scope, valid approval evidence
   and exact ADR body digest parity;
3. parses the unique binding from those verified bytes; requires selected path
   equality and verifies the actual JSON bytes against the bound digest;
4. derives `PolicyIdentity.sha256` from those exact verified JSON bytes and
   `design_references` from the approving ADR's actual registered path; derives
   the approval actor from its registry entry, not from request assertions;
5. passes the verified bytes and derived `PolicyApproval` to the existing pure
   kernel, which validates the complete mapping and requested subset.

No `approved=true`, caller-authored trusted approval object, coincidental hash,
Design link or mere existence of an APPROVED principle ADR is sufficient.
ADR-0016 itself contains no concrete policy binding: its approval cannot bless
arbitrary JSON written later. PROPOSED/REJECTED/SUPERSEDED, missing or tampered
approvals refuse semantic admission; never silently follow a replacement into
a different policy. A changed mapping needs an explicitly approved new binding
through the existing ADR lifecycle, not an edited approved body.

### Provenance and the two revision domains

The versioned ownership provenance records repository/workspace identity,
policy path and byte digest, approving ADR ID/path/body digest, observed ADR
registry revision, authoritative approval actor, and the graph revision at
which that exact binding was first admitted. This is the policy provenance
already required with original/effective ownership, not an independent policy
approval table or a fabricated historical migration field.

`PolicyApproval.observed_revision` is that **graph** observation revision.
The ADR registry has an independent counter: never copy its revision into the
kernel's graph-revision field. Reconstruct an existing binding with its persisted
graph observation, not today's graph revision; otherwise valid older clarification
records would become falsely stale. Read-only show/wave never creates provenance.
Legacy nodes lacking this evidence keep their conservative authority until the
existing explicit semantic activation/baseline path supplies it; no backdating.

Under the Root mutation coordinator, use one consistent registry/body/policy
observation and revalidate before publication. A changed approval/body/policy
invalidates the decision and requires a fresh observation; it is not an implicit
approval refresh. Preserve separately observed registry versus graph revisions
in diagnostics. Rechecking an unchanged valid binding does not create another
approval or alter original Plan bytes. Preserve held resources on invalid policy;
do not drop an invalid owner from scheduling to make another start admissible.

### Integration ownership and verification

The native integration owner may implement this adapter within its assigned
application/graph/store/CLI/MCP boundaries and v7 provenance. The five-Skill
consumer owner teaches authoring/review of the binding and uses actual adapters.
No new Plan or retrospective policy approval is required merely to implement
and test the bridge. Isolated test repositories establish their own explicit
fixture ADR approval; the real bootstrap graph/policy is not activated by tests.

Tests must cover an approved matching pair through real CLI/MCP; unrelated
approved ADR plus attacker-selected JSON; JSON tamper with recomputed caller
hash; ADR body tamper; absent/multiple malformed bindings; policy path escape;
superseded/nonapproved authority; actor spoof; registry revision greater than
graph revision; reconstruction after later graph commits; stale observation and
no-write refusal; and retained ownership on failure. Changing the policy requires
its real owner decision, not an implementation fixture's approval evidence.

Opened 2026-10-14: [OWASP Authorization Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html)
supports deny-by-default and authorization validation at each operation; it does
not supply MINE approval evidence. [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)
supports decision records and supersession instead of rewriting accepted history.

### Registration must produce observable authority

New registration validates a usable authority declaration before committing the
node. DRAFT postpones execution admission, not validation of the declaration
needed by show/wave. A conservative registration requires a nonempty valid
physical mapping; a semantic registration instead requires its complete
validated contract and approved binding. An optional `exclusive_write_paths`
transport field is not permission to persist an empty conservative authority:
omission is valid only when another supported registration form supplies the
required authority. Do not infer Root-wide scope, a Plan-body path or shared
reservations as replacement exclusive authority.

Resolve the new node through the same authority validation used by its public
reader, within registration's existing locked transaction and before graph or
checkpoint persistence. Reject missing/invalid mapping with a precise contract
error and no persisted node/revision/ref change. This validation does not reserve
execution ownership or bypass the later release/prepare/start conflict checks.
Do not weaken `EffectiveAuthority` to make an invalid registration queryable.
Existing malformed records remain explicit diagnostic failures; do not silently
repair them or expand their authority during reads or migration.

Public CLI/MCP tests must cover omitted and empty conservative mapping refusing
without mutation, valid conservative add followed by show, and valid semantic
add/show without a redundant legacy mapping where that typed form supports it.
First replay the exact failing payload and preserve its error before classifying
a fixture defect. Pure DAG fixtures need not masquerade as valid registrations.

### Conservative repositories do not require semantic policy approval

Clarification, 2026-10-14: the existing ADR-0016 migration promise preserves
legacy path authority, not a requirement to manufacture an approved JSON policy
before every old repository can use wave/start. The same evaluator supports an
explicit conservative-only context and a verified semantic-policy context.
These are validated authority variants, not two independent schedulers or a
caller-controlled bypass flag.

- Conservative authority is derived from original persisted path declarations
  and repository/workspace/Plan identities, without semantic activation facts.
  It has no approved policy identity, actor or synthetic approval digest. Empty
  or malformed authority is not expanded into unrestricted ownership.
- The all-conservative context admits only conservative nodes. It still checks
  complete held owners, dependencies, exact physical overlap, registered
  worktree/lineage, mutable outputs, reserved resources, pending operations and
  observer revision; it grants no semantic partition/shared-declaration relief.
- If a node contains a semantic contract, binding or clarification, or a request
  asks to activate semantic authority, require the production policy bridge.
  Missing/tampered/revoked policy is an error, never grounds for reconstructing
  that node as conservative. Migration preserves such facts or refuses; it does
  not erase them to obtain a runnable graph.
- A mixed snapshot uses verified policy for semantic nodes and preserves
  conservative nodes as conservative. A pair involving either conservative node
  receives no semantic overlap relaxation. Complete declared shared/generated
  resources remain occupied; do not drop reservations because the other node
  lacks a policy. A policy visible elsewhere does not silently activate a legacy
  node or grant it semantic scope.

Kernel/DTO types must distinguish absent-by-design conservative policy from
invalid/missing required semantic policy. An explicit tagged context or an
equally strict validated representation is sufficient. Show/wave reports the
mode and actual policy identity when applicable; never return placeholder
approved identities, zero digests or inferred actors. Existing preparation and
receipt input binding records this actual authority variant and declarations,
so changing modes invalidates affected evidence without inventing policy approval.

First semantic activation is the existing typed, owner-authorized baseline /
clarification path, with a real approved policy and a locked conflict check
against all held owners. Absence-to-approved policy is permitted **only** there,
not by ordinary reconstruction, prepare/start, or a missing-field default.
Retain original conservative declarations and any known original body digest;
where the historical digest was absent, attest current bytes as a new baseline
without claiming they are original history. Preserve append-only provenance and
all existing actor/reason/identity/revision checks. The first baseline preserves
the original physical mapping; it is not an opportunity to expand behavior or
acceptance. Do not apply an unconditional
old-policy == new-policy check that makes this specified first activation
impossible; later clarification still cannot silently switch policy identities.

Plan19 may correct this integration mismatch in
`src/domain/ownership/resolver.rs`, `src/domain/scheduling/kernel.rs`, necessary
ownership/scheduling type exports and existing integration call sites, plus
focused `tests/semantic_ownership.rs` / `tests/ownership_scheduling.rs` assertions.
This bounded physical-scope supplement implements the existing conservative
mapping requirement; it authorizes no unrelated kernel redesign or policy
approval and does not change immutable Plan bodies or the live bootstrap graph.

Required tests: a genuine policy-free legacy repository completes show/wave
and a valid start; disjoint and conflicting conservative starts use the same
evaluator; same worktree/output and held/pending conflicts still refuse; mixed
legacy/semantic overlaps remain exclusive; semantic policy loss/tamper cannot
downgrade; explicit first activation succeeds only with approved binding and
correct baseline/conflict evidence; failed activation preserves all original
bytes/provenance; nullable/tagged public output makes no approval claim.

## Physical integration and generators

Different semantic owners may share a directory only under an explicit approved
responsibility partition. Actual shared file edits require an enumerated
declaration class and the same named integration owner. Independent import/export
or registration declarations may qualify; algorithms, schema identity, migrations,
lockfiles and mutable generated sets do not qualify by default.

Each generator transaction has one output owner. The planner records input to
output/seal closure and checks every required gate has authority to produce its
outputs. Mechanical regeneration may use that authority; modifying generator
semantics requires its own owned responsibility. Integration regenerates once
from accepted combined inputs and verifies the whole output set. Merging each
lane's stale generated files is not equivalent to regeneration.

## Original and effective authority

Original immutable Plan bytes and registered declarations remain intact.
Semantic registration persists the original body SHA-256 before handoff.
Clarification compares that stored identity, the request digest and current
bytes; recomputing a digest after tampering does not reauthorize the Plan.
Legacy migration leaves an absent historical digest explicitly absent. Before
semantic activation, the owner must attest the current baseline through the
typed clarification operation; record its actor/reason/revision as a new
baseline, never as proof of the original historical bytes.
Clarification is an append-only owner-authorized record bound to workspace ID,
exact Plan ID/path/body SHA-256, lineage/branch, observed graph revision, actor,
reason, approved policy digest and the bounded clarified mapping/output set.
It may correct a responsibility's file/generator closure; it cannot change
behavior, dependencies, acceptance, public contracts or architecture.

Store original declarations and clarification provenance with the node in the
next versioned graph schema. Mutation is through a typed CLI operation only,
with expected revision; no prompt-only override or direct graph editing. Show
and wave expose original/effective authority and clarification identity together.
A clarification conflicting with another active owner is refused until ownership
is safely coordinated. Invalid/stale provenance fails closed.

Schema migration preserves historical path declarations as conservative mapping
inputs. It never fabricates a semantic partition from directory names. New
semantic authority needs approved mapping; unsupported newer schemas refuse
older writers. Both old conservative mappings and new contracts evaluate through
one compatibility function, not indefinitely divergent policy engines.

## Planning and role obligations

All five Skills use the same ownership interpretation. Planning lists locations
for navigation but states responsibility/exclusions as authority. Execution
records internal layout decisions without permission churn. Review checks
semantic behavior and shared integration intents, not only diff-path inclusion.
Sync distinguishes current enforcement from accepted target. Architecture may
assign foundations through the [architect lane](../integrations/architect-foundation.md).

For cross-boundary work, include a short entry-point -> producer -> consumer
trace with current implementation, owner and verification. A named interface
without a real caller does not prove the requested feature can execute.

## Scheduling, tests and sources

[Active ownership scheduling](../execution-graph/active-ownership-scheduling.md)
defines atomic enforcement. Test ordinary internal file changes, excluded behavior,
wrong/stale clarification identity, shared declarations, unpartitioned algorithms
and complete generation closure. Namespace labels must never silently bypass
physical worktree, output or integration exclusivity.

Opened 2026-10-09: [Fowler: Bounded Context](https://martinfowler.com/bliki/BoundedContext.html)
supports explicit model boundaries; [Git worktree](https://git-scm.com/docs/git-worktree)
supports separate checkout state. Neither proves semantic parallel-write safety.
