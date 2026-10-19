# ADR-0020: Input-Bound Verification Evidence Reuse

## Status and scope

Repository-owner review candidate. Governs verification/evidence reuse and
resource cleanup; it preserves mandatory gates and Plan-isolated mutable output.

## Context and evidence

`skills/mine-plan-review/SKILL.md:231,254,306,313-316` calls for overlapping
full validation stages. Current graph nodes record reports and implementation
commits, not machine-checkable validation input receipts. Reuse/invalidation
is mainly prose policy. The owner has repeatedly objected to rerunning the
same unchanged full test suite at execution, review, sync and release.

LODE's differential-validation policy and cache-prewarm report demonstrate the
practical cost, but its build measurements are not MINE Git performance data.
Shared mutable output is not a safe substitute for validation reuse.

## Proposed decision

1. A verification receipt records exact command/arguments, exit/result, input
   identity, relevant dependency/config/generator inputs, toolchain, platform,
   selected features, meaningful environment and durable evidence artifacts.
   Redact credentials; identify environment-sensitive checks explicitly.
2. Prefer conservative whole-tree identity first. Dependency-aware narrowing
   is permitted only with an explicit validated input manifest/policy. Equal
   commit names or a green test count alone never proves matching inputs.
   Uncommitted files, external data and dynamic HOME state must be captured or
   make the check non-reusable. Timestamps are not identity.
3. Executor, independent reviewer, sync and release may reuse a successful
   applicable receipt when its complete input/policy/toolchain/platform keys
   still match. An identical product tree may reuse evidence across graph or
   prose-only commits if those files are proven outside that check's inputs.
4. Missing, altered, incomplete or unverifiable receipts fail reuse, not the
   whole lifecycle automatically: run the relevant check and record new evidence.
   Explicitly mandated independent/platform/runtime probes still run. Reviewing
   trust boundaries is not replaced by the implementer's green checks.
5. Conflict resolution, dependency/toolchain changes, generators, packaging and
   stable-candidate changes invalidate affected checks. A platform's result
   never silently substitutes for another required platform. Timeouts/warnings/
   skipped failures are never receipts for a pass.
6. All mutable build/test/generated outputs stay isolated by exact Plan node.
   Share only demonstrably safe immutable/content-addressed downloads. Receipt
   reuse means avoiding redundant execution, not trusting stale shared binaries.
7. Before deleting accepted worktrees/build outputs, retain required logs and
   evidence in Root-visible report/receipt storage. Clean exact owned outputs
   when no active process needs them. Rejection preserves source/ref and
   compensation worktree; large caches may be removed safely without claiming
   rejected evidence was accepted. Exceptional disk costs require an explicit
   owner-approved serialized strategy, never automatic sharing.

## Alternatives and consequences

- Full reruns at every role boundary spend compute without adding evidence.
- Blind reuse of a report, HEAD or shared target risks false-green outcomes.
- A distributed cache service adds deployment/trust concerns not needed here.

The selected local evidence policy adds accountable invalidation and lets
independent review focus on new counterexamples. Receipt authenticity is local
repository trust, not cryptographic proof that an agent really executed a test.
Detailed Design chooses the minimal schema and retention/export contract; no
remote uploads or shared mutable build cache are authorized.

## Verification obligations

Prove unchanged checks reuse without launching a test process; changed source,
features/toolchain/generator/environment/platform invalidates appropriately.
Cover graph-only commits, untracked relevant inputs, forged/missing artifacts,
conflict resolution, genuine independent probes, cleanup with running processes,
and release reuse across an identical curated product tree. Report reused/run/
invalidated/skipped separately, with exact provenance rather than a blanket pass.

## Primary sources

Opened 2026-10-09.

- Bazel project, [Remote Caching](https://bazel.build/remote/caching): cached
  actions depend on inputs and commands, and undeclared environmental inputs
  can invalidate correctness. This informs receipt keys; MINE does not adopt
  Bazel or a remote cache.
- Git project, [git-worktree](https://git-scm.com/docs/git-worktree): checkout
  separation does not itself establish independent build-output ownership.

## Approval question

Approve conservative input-bound evidence reuse with explicit invalidation and
independent review probes, retaining isolated mutable output and owned cleanup?
