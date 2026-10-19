# ADR-0016: Semantic Ownership and Generated Write Closure

## Status and scope

Repository-owner review candidate. Registry state is authoritative. This
proposal changes Plan ownership and Skill scope rules; it is not an active
permission amendment for existing Plans.

## Context and evidence

`src/domain/graph.rs:97-105,215-240` models exclusive/read/reserved path lists
and path-prefix overlap. `skills/mine-plan-create/references/plan-template.md`
requires exact files/symbols; `skills/mine-plan-exec/SKILL.md:149-151` defers
even shared import/registration/dependency declarations to final integration.

LODE's owner-approved `AGENTS.md:625-636,672-677` instead authorizes coherent
module responsibilities and limited declaration integration in isolated
worktrees. Its reports document a Compiler/VM path overlap with separable
Standalone and Cell responsibilities, and a generated-output gate whose full
output set was missing from the Plan scope. These are planner/policy gaps;
MINE cannot infer program semantics automatically from namespace strings.

## Proposed decision

1. A Plan declares a bounded ownership contract: domain/module namespace,
   responsibility, owned behavior/contracts, excluded responsibilities, and
   required integration owner. Paths and symbols locate impact; they are not
   an exhaustive file whitelist. No glob/DSL or code-semantic inference engine.
2. The owner may add, split, move and test internal files, and adjust parent
   exports needed for the agreed responsibility, without per-file approval.
   Cross-module behavior, public contracts, security/authority and acceptance
   changes still require explicit architectural authority.
3. Map semantic ownership to physical code areas and shared resources in
   Design. Exclusive responsibility/algorithm collisions block concurrent
   ownership even when file paths differ. A namespace label alone never
   authorizes simultaneous edits to a shared algorithm or mutable artifact.
4. Overlapping directories may be partitioned by explicit responsibilities.
   Isolated-worktree edits to a shared source entry are allowed only for an
   enumerated declaration class (such as independent imports/registrations)
   with one integration owner who preserves both intents. Manifests, lockfiles,
   schema identity, migrations and shared algorithms remain serialized unless
   a specific approved integration contract proves otherwise.
5. Describe a generator's inputs, machine authority and complete output set
   as one owned regeneration transaction, including digest/seal refreshes.
   Distinguish mechanical regeneration from changes to generator semantics.
   Planning checks that required verification outputs are owned; integration
   regenerates from accepted combined inputs rather than merging stale outputs.
6. An owner-authorized scope clarification may be an append-only effective
   ownership supplement bound to the immutable Plan body digest, node,
   lineage and observed revision. It cannot edit the Plan or change behavior,
   dependencies, acceptance or architecture. Broader work still uses a new
   governed Plan. Binary queries expose original and effective ownership plus
   provenance; Skills must not rely on a hidden prompt-only exemption.
7. Provide a short production-chain check where relevant: real entry point,
   producer, consumer, existing implementation, responsible owner and test.
   A registered interface without a functioning caller is not readiness proof.

## Alternatives and consequences

- Exhaustive file lists cause repeated approvals for harmless internal splits.
- Directory-only ownership is too coarse for cross-cutting subsystems.
- Namespace-only isolation hides shared-file and generated-output conflicts.
- Making every missed generated file a compensation changes the workflow
  instead of correcting an objectively bounded authorization omission.

The selected model combines semantic authority with physical integration safety.
It applies to all five Skills and native CLI/MCP scheduling. Historical immutable
Plans retain their original text and conservative path policy until a verified
authorized mapping/supplement exists; do not keep two permanent policy engines.

## Verification obligations

Prove module-internal file creation/refactoring requires no renewed file
approval; reject cross-responsibility/security/contract expansion. Test explicit
shared declaration integration, semantic algorithm conflicts, supplements with
stale/wrong body identity, and a generator whose acceptance gate touches outputs
outside a planner's initial file list. Show original/effective scope consistently
through CLI/MCP. ADR-0018 owns atomic start enforcement.

## Primary sources

Opened 2026-10-09.

- Martin Fowler, [Bounded Context](https://martinfowler.com/bliki/BoundedContext.html):
  explicit model boundaries and their relationships support coherent semantic
  ownership. This is guidance, not proof of safe concurrent file writes.
- Git project, [git-worktree](https://git-scm.com/docs/git-worktree): separate
  linked worktrees isolate checkout state; integration remains a distinct step.

## Approval question

Approve responsibility-first scope, bounded internal autonomy, complete
generator transactions and auditable owner-authorized clarifications, while
retaining explicit physical integration ownership?
