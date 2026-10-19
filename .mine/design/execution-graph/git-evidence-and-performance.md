# Git Evidence and Graph-Operation Performance

## Authority and implementation boundary

Accepted target under [ADR-0015](../decisions/0015-bounded-git-evidence-and-graph-operations.md).
Candidate source implements bounded request-local evidence and snapshot reuse.
The installed 0.1.13 bootstrap retains its older behavior. Process-count and
whole-command performance claims require exact measured evidence; a source
change or focused behavioral pass alone is not a new performance measurement.

## Components and invariants

The domain validates graph structure without launching processes. A request-local
Git evidence adapter validates objects and observes refs/worktrees. Application
services consume one validated workspace/evidence snapshot; CLI/MCP envelopes
and renderers reuse it rather than invoking validation again for metadata.
No daemon, permanent cache or direct object-database implementation is introduced.

The snapshot binds repository common-directory identity, workspace ID/revision,
validated graph bytes, and exact object queries. Immutable-object evidence is
deduplicated inside the request. Ref, worktree, cleanliness and ancestry evidence
is observational and rechecked at the mutation boundary; it is never a durable
grant. Arbitrary Git writers are not protected by the MINE graph lock.

## Object and process protocol

- Use one stdin `git cat-file --batch-check` session for unique exact anchors
  per validation snapshot, requesting object ID and type. Verify every response
  against its request, require a commit, and reject missing/ambiguous/wrong-type
  objects and abbreviated IDs. Check repository-native full-ID equality; do not
  hardcode a SHA-1 length or accept tag peeling in place of an exact commit ID.
- Input values are validated object IDs, not user-provided revision expressions
  or shell fragments. Drain stdout/stderr concurrently with bounds, close stdin,
  check process exit and reap the child. Git replacement behavior must match the
  existing resolve contract, verified by fixtures rather than silently disabled.
- Enumerate refs once with structured `for-each-ref` output and worktrees once
  with porcelain `-z` where applicable. Ancestry checks are deduplicated by exact
  ordered object pair; they remain real Git checks, not invented from branch names.
- Git children run sequentially per request by default. Initial adapter limits:
  10 seconds per child, 30 seconds total Git request budget, and 1 MiB captured
  output per stream. These are target defaults, not current config fields. A
  validated project policy may raise them explicitly; no automatic retry or
  concurrency escalation. Oversized output is an error, not truncated evidence.
- Deadline/cancellation kills and reaps owned Git children and closes pipes.
  Read-only object checks never trigger lazy network fetch. No success can be
  reported from partial output or a still-running child.

## Transaction ordering

Read: parse once -> structural validation -> one deduplicated evidence acquisition
-> domain/service result -> envelope/render from the same snapshot.

Write: acquire Root graph lock -> parse authoritative graph -> check the supplied
revision immediately -> validate old state -> apply typed mutation -> validate
new state -> atomic TOML replacement -> render that validated new state -> unlock.
Within the request, reuse unchanged object evidence between old/new validation;
freshly observe mutable Git evidence immediately before the relevant mutation.
Never reread solely for rendering. TOML is authoritative if Markdown writing fails;
report committed-state/render failure and repair through the existing renderer.

## Algorithms and observability

Build ID/adjacency/index membership once. Kahn traversal processes each node/edge
once; use stable declaration-ranked queueing when needed (at most an additional
logarithmic queue factor), not repeated full scans to preserve order. Wave
compatibility may require pair comparisons but reuses indexed reachability and
the ownership snapshot. Output order stays deterministic.

Diagnostics record operation ID, phase, graph size, total Git invocations, peak
owned children, Git time, validation time and lock-wait/hold time. They omit
credentials and unrelated stdout. Process counts are not described as concurrent
counts. MCP stdout stays protocol-only; telemetry is bounded local diagnostics.

## Verification and failure closure

Count launches for 0/10/100/1000 nodes, duplicate/unique anchors and read/write
commands. One anchor batch per distinct validation snapshot is the structural
gate; after minimum Root-authority discovery, a stale revision launches no
additional object/ref/status/ancestry evidence queries. Count authority discovery
separately rather than claiming the entire command spawns zero Git. Record cold/warm p50/p95 latency without a
machine-specific pass threshold. Preserve deterministic topology and all current
Git refusal fixtures on supported platforms. Inject malformed output, missing
objects, replacement refs, child hangs, lock contention and render interruption.
Timeout before a graph write leaves graph bytes unchanged; a completed write
with lost response follows [lifecycle receipts](../governance/lifecycle-receipts.md).

## Verified sources

### Native checkpoint integration: bounded reuse and request budget

The subsequent approved [checkpoint performance budget](../decisions/0033-checkpoint-aware-whole-command-performance-budget.md)
resolves the old whole-withdraw numeric oracle: the specified fresh CLI/MCP
fixtures must meet the derived 96-launch ceiling and named phase allocation,
with the required index/tree/candidate optimizations and safety evidence. This
does not approve the observed 115-launch implementation. Read/show ceilings
remain unchanged. Graph-anchor batches are counted by exact replacement context,
separately from raw-blob batches and candidate proof; all count toward the whole.

The native checkpoint does not grant permission to reset the public request's
deadline, cancellation or launch counters. Root discovery, graph observation,
private-index Git views, checkpoint creation and completion share one request
budget context. Changing the index/spawner changes the view, not the deadline.
Retain the declared defaults above; hardcoded 60-second/32-MiB child contexts
are not an approved policy override. Check the remaining deadline between
expensive non-child phases too. After any durable mutation, timeout reports the
actual pending/uncertain state and preserves same-operation recovery, never a
false no-effect refusal. Reap owned children with bounded cleanup even when the
normal execution budget has expired.

Within a request, validate exact persisted checkpoint/runtime bytes fully once
and carry a typed validated witness bound to those bytes and identities. Stage
progress checks the successor delta and current mutable facts; journal-only
progress does not justify reparsing, rendering and canonical-serializing the
same before/after graphs at every helper. New/recovered requests and externally
changed evidence still require full validation. Do not bypass store integrity or
reuse a witness after its bound input changes.

Keep fresh mutable checks at actual safety boundaries: initial admission,
immediately before CAS, after CAS immediately before index publication, and final
completion, plus any intervening mutation boundary requiring its own facts.
Share each phase's observation between helpers rather than repeat the same query
without an intervening invalidation. Private-index byte-copy identity can reuse
the validated source-index witness; attributes must still be valid before
staging and publication. Preserve observer-injected drift refusals, external Git
writer detection and full tree/mode/checkout transformation checks.

Bulk tree-entry/blob reads and a request-local checked candidate proof may remove
duplicate immutable queries; they may not remove independent delta, parent,
canonical-byte or CAS checks. Ordinary Git object evidence is NOT immutable under
replacement refs: retain refresh invalidation and the commit-replaced-by-blob
regression. Reuse requires a proven interpretation boundary (including replacement
semantics). The controlled checkpoint's no-replacement execution and explicit
replacement refusal are not a reason to disable replacement behavior globally.

Keep whole-command Trace2 counts, per-phase attribution, peak children and elapsed
time. The observed 122/126 launches and old whole-command <=10 failures remain
failed evidence; the original clarification did not approve a new numeric threshold.
ADR-0033 now supplies the explicit bounded replacement oracle without rewriting
those failures or claiming the mandatory implementation changes have passed.
First implement bounded deduplication and unified budgets, then measure one
representative complete command before repeating the full matrix. Any proposed
revised whole-command budget must derive from enumerated necessary phases and
independent review, not from relabeling snapshot-only counts. Preserve read/show
budgets and the no-per-node-process structural gate. Test cumulative subphases
exceeding one deadline, immutable-witness invalidation, replacement drift and
interruption at every retained mutation boundary.

The committed-report/Plan observation uses an opaque `CommittedFilesProof` in
`native_evidence`: exact canonical Root, commit, ordered file identities, native
storage hashes and request identity bind its reuse. The same observation may
reuse immutable tree membership while rechecking contained raw bytes, effective
filter attributes and storage hashes. Another request, changed group/commit or
conversion refuses reuse. This is not a portable receipt or a mutable-ref cache.
Git policy reads still inspect every occurrence before last-value reduction and
delegate noncanonical boolean coercion to Git. Fresh ref inventory proves exact
direct refs; an empty result retains the symbolic-ref check for dangling refs.

Opened 2026-10-09: [Git cat-file](https://git-scm.com/docs/git-cat-file) provides
stdin batch object information; [Git for-each-ref](https://git-scm.com/docs/git-for-each-ref)
provides structured ref inventory. Use features available in the supported Git
baseline (local evidence: 2.45.1.windows.1), not newer-only flags from latest docs.
