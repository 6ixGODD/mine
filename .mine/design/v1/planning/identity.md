# Principal Identity, Assignments and Handoff — Unregistered Execution Blueprint

## Activation status

Provisional allocation: `plan26`; canonical materialized filename
`.mine/plan/plan26-identity.md`, own report
`.mine/plan/reports/plan26-identity-implementation.md`. See [activation](index.md)
and [common protocol](parallel-protocol.md); allocation is not graph registration.

Not a registered/READY Plan and not execution authority. Root will allocate the
canonical ID/worktree/owner and frozen contract commit. Scope `v1.0.0`, governed
by approved ADR-0025 plus24/26/27/28/30 where their boundaries intersect. This
blueprint does not change0.2 free-text owner behavior or active Plan assignments.

## Outcome

Deliver authenticated Project-qualified principals, short display IDs, role grants,
one-use enrollment, Plan executor/reviewer assignments, contribution-aware review
independence and recoverable A-to-B handoff. Session replacement is distinct from
owner transfer. A short ID is never a credential. Provider creation/model discovery
and HTTP authentication transport are separate lanes, not hidden implementation
work added here.

## Inputs and parallel start gate

Read [identity](../identity.md), [transaction contracts](../transaction-contracts.md),
[runtime](../runtime.md), [coordination](../coordination.md) and the immutable ADRs.
Start after shared `v1/contracts` IDs/envelopes/errors and the table catalog are
frozen. Do not wait for the complete storage, runtime, HTTP or coordination product.
Use declared capability ports and a test-only fake transaction/proof source while
their real adapters run in parallel. Final integration requires real SQLite and
real supervised-writer quiescence; fake proof is never accepted in production.

## Owned implementation surface

Own `v1/crates/identity/` (package `mine-v1-identity`), its `src/`, tests, private
fixtures and `migrations/project/`. Own the identity tables and only their SQL
mapping: principals, role_grants, enrollments, plan_assignments, handoffs and
contributions. Runtime owns session_attempts; Work owns plans/nodes; Coordination
owns obligations/waits. Borrow their typed views for a shared transaction; never
patch their tables through private SQL. Integration owns top-level manifests,
shared contract generation and the migration catalog. This lane owns its private
Cargo.lock and never updates another lane's lockfile during parallel execution.

The package may be standalone during parallel work; do not edit root0.2 Cargo
files. Read `v1/contracts` without introducing parallel ID/error enums. Required
interface corrections go through the contract owner with a versioned fixture.

## Work packages

### I1 — Enrollment, principal and authorization

Allocate immutable non-nil UUID and collision-checked short ID, with role label
for presentation and explicit stored grants for authority. Short IDs are never
reused. Consume an owner/delegated-policy-issued enrollment once under transaction;
store only its token digest, scope, expiry, intended role/profile and consumption
result. Generate credentials using a supported secure source through an injected
credential port; no predictable ID/timestamp-derived secret. Secret storage and
HTTP transport remain with the host/security owner, not arbitrary SQLite plaintext.

Authorization evaluates current Project, principal/grant status, exact requested
capability, target assignment/epoch and authenticated attempt. Revocation affects
new admissions; no token retains authority solely because it was once valid.
Architect is allowed arch/plan-create/sync/technical escalation; Executor only its
assigned execution; Reviewer only assigned independent review. Human owner and
delegated Project Lead powers are explicit, not inferred from the Architect label.

### I2 — Assignment and independent review

Create/read executor/reviewer assignments under CAS. Enforce contributor history
across principal/session changes: new role, short ID or runtime cannot erase prior
implementation. Record contributions and previous ownership append-only. Deny
unassigned/stale attempts; diagnostics identify required authorized handoff without
revealing another Project's details. New assignment is not permission to skip Plan
predecessors, frozen scope, review/report gates or mutable-output isolation.

### I3 — Recoverable handoff

Implement requested -> old-writer-quiescing -> checkpoint-verified -> ownership-CAS-
committed -> new-admission -> acknowledged, with stable operation ID and durable
attempt/result records. Explicit owner instruction “B continues” supplies authority
without another ceremonial approval; absent that, require recorded valid delegation.

Quiescence is an externally observed runtime/tool capability bound to exact old
attempt and assignment epoch. Never create it from timeout, PID absence alone or
a mock flag. No SQLite transaction waits while stopping a process. Persist intent,
perform runtime action, collect evidence, then re-read/commit the guarded transition.
If writer cessation cannot be proven, retain UNCERTAIN and refuse B's admission.

Checkpoint exact branch/head/worktree, dirty-file manifest and pending operations,
commitments, evidence and next continuation. Do not stash/reset/clean or force a
checkpoint commit. Ownership CAS increases revision/epoch and revokes old attempt;
append immutable A-to-B provenance and use Coordination's borrowed view to rebind
outstanding accountable obligations atomically, retaining original IDs/senders.
Only then may Runtime admit B under the new epoch. Missing A's session does not
fabricate its acknowledgment. A lost B acknowledgment does not repeat owner transfer.

Same-principal session replacement changes attempt generation and epoch fencing
as defined by the shared contract, not Plan principal ownership. It still requires
old-writer reconciliation and retained work-node state. Do not restart the Plan,
rewrite history or create compensation solely because provider resume failed.

### I4 — Native conformance and integrated handoff proof

Lane acceptance proves its real SQL mapping and authorization/transfer contract
against narrow test ports, including a real controlled child-process stop probe.
The complete Runtime/Coordination/Work production scenario below is a mandatory
final integration gate, not a circular requirement that those peer Plans first
be accepted before this lane can be accepted. Preserve that distinction in receipts.

Run pure authorization/assignment tests against memory, then identical contract
cases against the actual SQLite mapping. Join Runtime/Coordination/Work views in
one real scenario: A owns a dirty Plan with a pending obligation; approved handoff
stops A, preserves dirty hashes, atomically reassigns with fencing, B continues,
and the obligation result reaches B once. A stale command must be rejected even
when its provider session still has cached identity. A same-OS user with direct
filesystem access remains outside MINE API isolation; report that limit honestly.

## Mandatory acceptance cases

- Cross-Project token/ID reuse, forged caller actor, expired/reused enrollment,
  short-ID collision, revoked grant and wrong role fail without ownership mutation.
- Two assignment contenders: one CAS wins; same request replays; altered request
  under same ID fails; independent Plan updates do not globally serialize by CAS.
- Self-review under new principal/session/role refuses with retained contribution
  evidence; legitimate independent Reviewer admission succeeds.
- Crash before/after each handoff stage, old writer still alive, missing native
  session, orphan child process, lost stop/commit/admission/ack response, changed
  branch/dirty manifest and duplicate handoff message.
- Outstanding waits follow the audited receiver; sender/criterion/history stay
  unchanged. Pause during handoff prevents new admission without erasing the
  already committed ownership history. Retry uses the original operation ID.
- Authorized human handoff proceeds without repeated approval; unauthorized B
  cannot self-transfer. Unknown quiescence preserves dirty state and blocks only
  the affected node, not unrelated work unless a global safety barrier exists.

## Post-implementation checks and evidence

The activated lane creates these targets; the following commands are planned,
not already run. Use the registered worktree and Root-managed per-Plan outputs:
`CARGO_TARGET_DIR=<Root>/.mine/build/<plan-id>/cargo-target`, fixture databases/logs
under `<Root>/.mine/build/<plan-id>/identity-tests/`. Root shared toolchains and
safe downloads are reused; do not create a worktree target/environment tree.

```text
cargo fmt --manifest-path v1/crates/identity/Cargo.toml -- --check
cargo clippy --manifest-path v1/crates/identity/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v1/crates/identity/Cargo.toml --test authorization
cargo test --manifest-path v1/crates/identity/Cargo.toml --test handoff_contract
cargo test --manifest-path v1/crates/identity/Cargo.toml --test handoff_sqlite
cargo test --manifest-path v1/crates/identity/Cargo.toml --all-targets
cargo build --manifest-path v1/crates/identity/Cargo.toml --all-targets
```

Record exact commands/exits, test names/counts, first failures, real/fake adapter
distinction, operation/epoch before/after, file hashes and retained evidence. The
cross-lane real writer scenario is a required integration acceptance gate, not
covered by an always-successful quiescence mock. Never claim role enforcement is
an OS sandbox. Do not rerun old full-suite results merely to fill the report.

## External semantics and escalation

Open [SQLite transactions](https://www.sqlite.org/lang_transaction.html),
[foreign keys](https://www.sqlite.org/foreignkeys.html), and the exact pinned
provider's official cancellation/session contract registered by Runtime before
implementing the real proof adapter. This lane consumes that proof contract; it
does not invent SDK stop guarantees. New identity security, takeover authority,
contributor-history erasure or uncertain writer disposal is a material change:
stop that subchain and route evidence to Architect. Do not silently expand scope.

The executor concludes only IMPLEMENTED after the activated Plan's gates and
reports; independent reviewer owns ACCEPTED. No global installation, push, release,
live handoff of active0.2 agents or production DB migration is authorized here.
