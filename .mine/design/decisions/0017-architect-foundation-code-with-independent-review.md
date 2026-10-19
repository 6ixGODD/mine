# ADR-0017: Architect Foundation Code With Independent Review

## Status and scope

Repository-owner review candidate. This proposes a role-permission change;
current Skills remain authoritative until approved and implemented.

## Context and evidence

`skills/mine-arch/SKILL.md` owns architecture and quality configuration, but
explicitly excludes business implementation. `docs/design/integrations/skills.md`
hands off to planning without a foundation-code lane. The owner requests that
architects can write abstractions, interfaces and minimal foundation code.

LODE feedback also shows the opposite risk: an interface may exist without
real production consumers or execution. Empty interfaces and placeholder success
must not become evidence that architecture or downstream functionality is done.

## Proposed decision

1. Separate the architect's role from acceptance authority. Architect-authored
   source is permitted for narrow consumer-owned interfaces, domain types and
   invariants, composition/bootstrap foundations, and minimal real reference
   implementations justified by an actual variation or consumer.
2. Use the existing canonical Plan lifecycle, not a sixth Skill or special
   unreviewed architecture state. A normal Plan may assign an architecture
   foundation work package to the architect. If no suitable Plan exists, a
   bounded foundation Plan may be planned from approved abstract contracts;
   unrelated detailed architecture need not be completed first.
3. Production edits occur only after the applicable ADR/contracts are approved
   and the work package has registered ownership, dependencies and tests.
   Use its prepared linked worktree and Plan-isolated mutable outputs. Root
   remains dev. The architect reads/follows execution obligations for that
   package and may report IMPLEMENTED, never self-grant ACCEPTED.
4. Independent review verifies real inputs, invariants, failure behavior and
   consumers before integration/cleanup under ADR-0019. Business completion is
   credited only to implemented behavior, not a type declaration or mock.
5. During discovery, read-only probes and non-product discussion examples are
   allowed. Any persistent executable experiment must be explicitly scoped,
   kept outside product integration and identified as disposable evidence;
   it cannot bypass approved production-code authority.
6. Avoid empty ports, speculative factories/plugins, `todo` and fake-success
   implementations. A minimal interface needs a real consumer/test boundary;
   a foundational implementation needs narrowly stated usable behavior and
   honest unsupported cases. Cross-boundary policy stays in Design.

## Alternatives and consequences

- Prose-only architecture prevents useful contract feasibility work.
- Free production edits on Root destroy ownership and independent acceptance.
- A parallel architecture-specific lifecycle duplicates governance and creates
  an exemption to exploit. The selected normal work-package path adds no role
  switching merely to rename the same task, but preserves review separation.

This elaborates the five-Skill arrangement without changing Plan IDs, compensation
lineage or human ADR approval. Approval permits later contract/Skill updates;
this proposal does not authorize source edits during the present architecture run.

## Verification obligations

Test a small interface/invariant foundation delivered by an architect, independent
rejection and acceptance, downstream consumer ownership, and refusal of Root
implementation/self-acceptance. Check all distributed Skill copies describe the
same boundaries and do not imply planning-ready from an empty interface.

## Primary sources

Opened 2026-10-09.

- Michael Nygard / Cognitect, [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions):
  decisions capture context, choice and consequences; they are not implementation
  completion evidence.
- Martin Fowler, [Bounded Context](https://martinfowler.com/bliki/BoundedContext.html):
  foundation ownership follows actual consumer/model boundaries.

## Approval question

Approve architect-authored minimal foundation code through normal Plan work
packages, with applicable decisions approved first and independent acceptance?
