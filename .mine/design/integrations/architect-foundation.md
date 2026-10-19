# Architect-Authored Foundation Work

## Authority and current boundary

Accepted target under [ADR-0017](../decisions/0017-architect-foundation-code-with-independent-review.md).
Candidate Skill sources now assign foundation work through the normal execution
path below. Installed 0.1.13 remains the bootstrap boundary: this does not grant
direct source implementation during ADR approval or allow Root implementation.
The real architect-foundation integration test exercises a compiled invariant
and consumer, Root/self-review refusals and independent acceptance/cleanup.
Its focused run passed; that is not acceptance of the candidate itself.

## One execution path

Architecture may assign consumer-owned interfaces, typed domain invariants,
composition/bootstrap foundations and minimal real reference implementations
to an architect-owned work package in an ordinary canonical Plan. Do not add
a sixth Skill, special graph state or unreviewed architecture branch.

Applicable abstract decisions/contracts must be approved first. A foundation
Plan may be planned from those scoped contracts before unrelated detailed
architecture is mature. If an existing Plan already owns the responsibility,
use its architecture work package rather than create a second ceremonial Plan.
Producing source remains an explicit execution request, not a side effect of ADR
approval. The architect follows full execution obligations for the work package.

## Ownership and source evidence

The work package declares semantic responsibility/exclusions, real consumer,
physical mapping, shared integration owner, intended usable behavior, unsupported
cases and focused invariant/contract tests. Use the prepared linked worktree and
exact Plan-node mutable-output namespace; Root stays on dev. Architecture owns
the contract, not all downstream business behavior.

Interface creation needs a demonstrated consumer/test boundary. Foundation code
must enforce its declared invariants and failure behavior. Empty ports, speculative
factories/plugins, `todo` or fake success cannot satisfy foundation acceptance.
An implementation report explicitly states which downstream chain is still absent.

Discovery probes are read-only by default. Explicitly scoped persistent experiments
are disposable, isolated and unmerged; their output is feasibility evidence,
never approved production implementation or downstream completion.

## Independent acceptance and handoff

An architect may report IMPLEMENTED but cannot accept their own work. An independent
reviewer verifies real inputs/outputs, consumers and invariants, then performs
normal integration/cleanup and receipt-bound acceptance. Rejection preserves the
lineage and uses ordinary compensation when a material defect requires it.

The five Skills' responsibilities remain separate: architecture defines accepted
contracts; planning decomposes them; a role assigned to execution may implement;
review is independent; sync describes current code and preserves explicitly
protected accepted targets rather than erasing pending foundations.

## Verification and sources

Exercise a real small interface/invariant implementation by an architect through
the normal Plan path, independent accept/reject, consumer handoff, and refusal
of Root edits/self-acceptance. Verify distributed Skill copies and executable
capabilities together; installing a newer prompt before its binary contract
exists is not a valid bootstrap strategy.

Opened 2026-10-09: [Nygard: Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)
separates decision context/consequences from execution evidence;
[Fowler: Bounded Context](https://martinfowler.com/bliki/BoundedContext.html)
informs coherent consumer/model boundaries.
