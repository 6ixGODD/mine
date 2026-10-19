# Transaction Ports, Schema Ownership and Parallel Integration

## Authority and implementation boundary

Detailed implementation contract under approved ADR-0024..0031. This leaf
refines [storage](storage.md), [identity](identity.md), [projects](projects.md),
[coordination](coordination.md), [resources](resources.md) and [autonomy](autonomy.md).
It neither migrates this repository nor changes its active0.2 authority. All new
commands/types below are1.0 contracts, not installed0.1.13 interfaces.

The isolated implementation lives under `v1/`. Its shared `v1/contracts` package
owns transport-neutral typed IDs, command/result envelopes, models and errors.
Domain packages depend on those contracts, not HTTP, provider SDKs or SQLite.
Production wiring, top-level manifests/lockfiles and generated exports have one
integration owner. No1.0 module is implicitly enabled in0.2 production paths.

## Rust persistence ports

`ControlUnitOfWork` and `ProjectUnitOfWork` are distinct capabilities. Each admits
one synchronous transaction callback over borrowed, narrow repository views.
The callback returns a typed result; it cannot retain a transaction/connection,
await, perform external IO, spawn a provider or commit independently. The unit
of work owns begin/commit/rollback and common audit/outbox/idempotency writes.
Do not expose a generic SQL endpoint or untyped key-value repository to domains.

The concrete SQLite adapter has one bounded writer queue per open database and
owns its connection on a dedicated thread. Idle Project handles are evictable;
there is no permanent thread for every registered/offline Project. Read connections
and queue sizes are bounded. A request cancelled after writer admission may still
commit: return/query its original request ID, not a claimed rollback. Service
shutdown drains or records unresolved commands before closing handles.

| Domain view | Required read and mutation capabilities inside the same transaction |
|---|---|
| Identity | read authorization/assignment/attempt epoch; allocate principal; consume enrollment; CAS assignment; append handoff/contribution; revoke attempt |
| Work/lifecycle | read Plan/document/ownership gates; CAS Plan/node; register continuation; pause/resume frontier; persist operation/effect stages |
| Coordination | insert message/recipient rows; create obligation and wait; commit/decline; resolve result and dependent waits; insert unique wake; route issue |
| Projects | read global location/identity; reserve registration; advance init/deinit operation and cleanup item; retire active location |
| Resources | read all requested resources/holders; enqueue full resource set; atomically grant/release set; mark uncertainty; channel membership/offers |
| Autonomy | read effective policy/decision digest; record covered approval; reserve/settle allowance; update admission barrier |
| Runtime | reserve attempt; record native binding/model acknowledgment; persist observation/checkpoint; reconcile attempted external effect |

Methods are typed domain operations, not arbitrary table writes. A transaction
facade can lend several views for one legitimate cross-domain atomic command;
each application handler receives only the views it needs. This is not an
all-powerful domain `Repository` object. SQL mapping may share the borrowed
transaction, but cannot create nested independent commits.

Queries return owned typed snapshots with source revision/event cursor. Pagination
uses stable keys and explicit limits, not offsets over a mutable ordering. An SSE
stream holds no SQLite transaction: fetch a finite event page, release connection,
then wait for notification. Notification is an optimization; persisted cursor is
truth. Compacted history returns reset-required plus a fresh snapshot boundary.

## Command, commit and replay contract

Every mutating command supplies typed operation ID as its request idempotency key,
database/Project scope, command
kind, expected entity revisions, relevant assignment epoch and normalized payload.
The service derives authenticated actor; quoting a principal ID is not admission.
The shared `CommandMeta.operation_id` is the request ID below; do not add a second
independently generated public ID for the same invocation. A longer-running saga
uses that operation ID plus typed effect ordinals/attempt IDs. The shared primary
`expected_revision` targets the command's named entity; cross-domain commands put
additional typed CAS predicates in their payload, not a global Project revision.
A saga-advance command has its own idempotency operation ID and a separate typed
`target_operation_id` in its payload pointing to the original saga. Retrying that
advance reuses its request ID; advancing another stage is a new command. Thus
same-saga recovery preserves the target identity without reusing a command key
for changed payload/kind. Read/status calls cannot advance an operation.
Control-store commands use an explicit control-scope envelope; they do not invent
a Project ID merely to satisfy a Project-only transport envelope.
Canonical digest input is the versioned command kind plus typed payload serialized
in declared struct-field order, with maps sorted by key and sets sorted by typed
ID; duplicate JSON keys and unknown fields are rejected before normalization.
Do not hash caller JSON whitespace/order, timestamps generated during handling or
secrets. Exact document bytes have their own SHA-256 binding and are never
normalized through command serialization.

The common writer performs this sequence:

1. Reject invalid shape, expired authentication, wrong Project, excessive input
   and unsupported command version before assigning write authority.
2. Begin an IMMEDIATE transaction; read `(request_scope, request_id)`. If present,
   require same authenticated originating actor, kind and canonical digest. Verify
   current result-read permission, then return its committed result, including a
   previously recorded domain refusal, without invoking the handler again.
3. For a new request, re-read role grants, Project admission barrier, expected
   revisions/epochs and relevant entity facts in this transaction. An absent
   entity is different from expected revision zero. Cross-entity commands list all
   their CAS predicates; an event cursor is not a substitute for these predicates.
4. Run the deterministic domain handler; persist its mutations, result, audit
   event and outbox rows together. A domain refusal can record a bounded command
   outcome/audit without changing target entity revisions. Invalid authentication
   cannot consume another actor's request ID or retrieve its outcome.
5. Commit. Only now publish the result/cursor and notify workers/subscribers.
   Each changed entity increments revision once per command; several events may
   share one transaction. No-op/replay does not increment domain revision.

Request identity is unique within an authority scope, not just per HTTP connection.
Identity reuse with a different payload is a conflict even if the earlier operation
failed. Retries use the same request ID; an intentional new decision uses a new ID.
Failure to observe commit acknowledgment is UNKNOWN, not definite failure: reopen
the correct store and query that request before authorizing a replacement effect.
Retaining an idempotency key without its outcome is invalid; expiry requires a
documented command-retention boundary, never arbitrary timer deletion while an
operation, wait or client recovery can still reference it.

Use the shared ErrorCode variants `InvalidInput`, `Unauthenticated`, `Forbidden`,
`NotFound`, `RevisionConflict`, `StaleEpoch`, `IdempotencyConflict`, `Busy`,
`Pending`, `OutcomeUnknown`, `VersionUnsupported`, `CorruptStore` and `IoFailure`.
An admission barrier is a typed `Pending` reason, not another incompatible enum.
Typed details contain current revision or safe operation
locator where authorized, never secrets, raw SQL or foreign Project contents.
Busy retry is bounded and preserves request identity. Disk-full/commit/rollback
failure is never reported as a completed mutation. A connection with an unproven
transaction outcome is quarantined/reopened before further writes.

## Cross-domain atomic operations

| Command boundary | Must commit together |
|---|---|
| Request and wait | message, recipients, accountable obligation, continuation wait, delivery outbox |
| Resolve obligation | result/evidence binding, obligation state, affected waits, one effective wake per continuation/result |
| Record decision | exact question/body/policy digest, authorized decision, audit, affected issue/wait resolution and outbox |
| Transfer owner | assignment revision/epoch, append-only handoff, old-attempt revocation, audited rebinding of outstanding obligations; no admission of B before external quiescence proof |
| Admit work | current assignment/attempt/Plan/pause checks, node claim, local budget reservation and external-effect intent |
| Pause | durable pause/admission barrier, cancellation intents and wake suppression; PAUSED only after actual quiescence reconciliation |
| Resource set grant | all requested capacities/epochs, grant and holder records, notification outbox in the control DB |

Global resource acquisition and Project admission cannot be one SQLite transaction.
Reserve a provisional global set under a stable operation ID, record its identity
in the Project admission, then confirm globally before external work. A failed or
unknown intermediate step is reconciled; no effect starts on an unconfirmed grant.
Pending reservations have explicit expiry/recovery and do not become active leases
merely because a Project result was lost. No cross-database foreign keys or ATTACH
atomicity assumptions. Init/deinit and cross-Project channels use the same durable
outbox/inbox plus operation-stage pattern, not a distributed transaction fiction.

## SQL schema conventions

All tables are STRICT. UUID IDs use validated canonical lowercase text; revisions,
epochs, sequence numbers and UTC millisecond timestamps use INTEGER with nonnegative
checks, bounded to SQLite signed64-bit range. Booleans are INTEGER checked0/1.
Shared revision/epoch/cursor constructors, parsers and checked increments enforce
`0..=9_223_372_036_854_775_807`; their public JSON wire representation remains a
canonical decimal string so JavaScript cannot round large counters. SQL adapters
do not truncate, wrap or reinterpret values above that bound.
Digests are lowercase64-character hexadecimal text. Monetary amounts, when known,
use integer micros plus currency, not FLOAT; unknown cost is NULL plus explicit
knowledge status. Text lengths and JSON sizes are bounded in typed decoders and
database CHECKs for externally supplied content. Enum columns have CHECK sets
matching versioned contract enums. Do not silently decode unknown enum values.

`P` below means `project_id`. Every Project-owned row includes P, references the
singleton `project_meta(P)`, and uses a composite key/foreign key where needed to
prevent accidental cross-Project joins. The same physical Project DB cannot host
two P values. Control tables reference global `projects(P)` instead. Cross-store
references are validated by typed services/saga evidence, not SQL foreign keys.
Foreign keys use RESTRICT by default; deletion/retention is an explicit operation,
not cascading erasure of audit/history. Index all child foreign-key lookup paths.

Only bounded, versioned payloads with a closed Rust discriminator may be JSON:
command outcomes, event bodies, exact policy/model snapshots, evidence detail and
provider-specific nonsecret acknowledgments. Authorization, ownership, state,
deadlines, recipients, amounts and search/scheduling keys are normalized columns,
not an `entities(key,json)` database. Large artifacts remain digest-bound files.

## Initial common schema catalog — storage owner

Each store contains the following physical tables; Project versions additionally
carry P. The module migration ledger exists before domain migrations run.

| Table | Required key/relationships/constraints | Required access index |
|---|---|---|
| `store_meta` | singleton key1; store UUID, kind CONTROL/PROJECT, schema catalog version, creation time; immutable identity | singleton |
| `schema_migrations` | PK(module,version); digest, applied time; changed applied digest refuses | PK |
| `command_results` | PK(request_scope,request_id); actor identity, kind, payload digest, result version/body, commit cursor/time; immutable completed result | actor+time |
| `audit_events` | monotonic event_seq PK; request/operation IDs, actor, kind, entity ID, entity revision, bounded payload; append-only | entity+seq; request_id |
| `outbox` | effect_id PK; source event FK, destination/type, payload digest/body, state, attempts, next_due, claimed generation; unique(source_event,effect_kind,destination) | state+next_due+effect_id |
| `inbox_receipts` | PK(source_store_id,effect_id); digest, handled request/result reference; conflicting digest refuses | received_at |
| `operations` | operation_id PK; kind, subject, request reference, stage, revision, immutable intent digest, last observation; no unrelated reuse | subject+stage |
| `operation_effects` | PK(operation_id,effect_ordinal); operation FK, typed intent/digest, attempt generation, state, result evidence; order explicit | state+operation_id |
| `effect_attempts` | PK(operation_id,effect_ordinal,attempt_no); effect FK, started/result times, outcome; append-only | parent FK |
| `artifact_refs` | artifact_id PK; kind, digest, owned locator, bytes, retention class; identity immutable | digest+kind |

`operations` references `command_results(request_scope,request_id)` using a
DEFERRABLE INITIALLY DEFERRED foreign key. Insert the operation and its final
accepted-command result in the same transaction; commit requires both. The result
may say an external operation is pending, which is not its eventual completion.
Never leave a temporarily dangling reference visible after commit.

## Initial domain schema catalog — one migration owner per module

The following rows define the minimum normalized schema. Domain-owned migrations
may add private indexes/columns consistent with this leaf; shared keys, state,
authorization or cross-domain contracts require the contract owner's revision.

| Owner / store | Tables and exact relational core |
|---|---|
| Projects / control | `projects`: PK P, unique canonical_root_key while registration is active; status, location, revision, verified identity digest. `directory_operations`: PK operation_id FK operations, P FK projects, init/deinit/relocate stage, manifest digest, archive locator. `cleanup_items`: PK(operation_id,ordinal), parent FK, exact locator/type/ownership digest, observed-before/result state. `project_tombstones`: P PK, completed operation FK, bounded cleanup receipt; never full hidden transcripts. |
| Projects / Project | `project_meta`: singleton key1, unique P, repository identity, initialization operation ID, lifecycle barrier/revision. `project_settings`: P PK/FK, typed branch names, human-doc roots, output/worktree roots, policy version; no secrets and no duplicate TOML authority. |
| Identity / Project | `principals`: PK(P,principal_id), unique(P,short_id), status, created_at. `role_grants`: PK(P,grant_id), principal FK, role, grantor/evidence reference, valid/revoked time, revision. `enrollments`: PK(P,enrollment_id), one-use token digest UNIQUE, intended role/profile, expiry, consumed principal reference. `plan_assignments`: PK(P,plan_id), Plan FK, executor/reviewer principal FKs, revision, epoch. `handoffs`: PK(P,handoff_id), assignment FK, from/to principal, old/new attempt refs, authorizer/evidence, stage/revision, checkpoint/dirty-manifest artifact refs; history is append-only stage events. `contributions`: PK(P,plan_id,principal_id,contribution_id), Plan/principal FKs, commit/evidence reference, kind/time; cannot delete to permit self-review. |
| Work/lifecycle / Project | `plans`: PK(P,plan_id), canonical body artifact/digest, status, revision, branch/anchor/worktree identities. `plan_dependencies`: PK(P,plan_id,predecessor_id,kind), both Plan FKs, CHECK no self edge; service rejects cycles. `work_nodes`: PK(P,node_id), Plan FK, parent node FK when present, state/revision, assignment epoch, checkpoint ref. `node_dependencies`: PK(P,node_id,predecessor_id), two node FKs, no self edge; cycle validation. `node_claims`: PK(P,node_id), node FK, principal/attempt FKs, epoch, state; at most one admitted writer. `document_bindings`: PK(P,document_id,version), path, raw digest, kind, artifact ref; frozen bindings immutable. `adr_decisions`: PK(P,adr_id,body_digest,decision_id), document FK, status, actor/policy evidence, revision; active exact-digest decision unique. `verification_refs`: PK(P,receipt_id), Plan/node/document bindings, exact code/config inputs and artifact refs; never inferred from a successful unrelated run. |
| Runtime / Project | `session_attempts`: PK(P,attempt_id), principal FK, generation, provider/profile, state/revision, create operation FK, native_session_id nullable until observed; UNIQUE(P,principal_id,generation), unique active native binding per provider/profile. `model_selections`: PK(P,selection_id), requested/effective provider/model/effort/speed, capability snapshot artifact, known/unknown field flags. `runtime_observations`: PK(P,attempt_id,observation_seq), attempt FK, observed process/tool identity and state/time. `node_checkpoints`: PK(P,node_id,checkpoint_id), node FK, immutable source/evidence/continuation refs. |
| Coordination / Project | `messages`: PK(P,message_id), sender principal/attempt FKs, kind, correlation/reply ID, bounded body/digest, created time. `message_recipients`: PK(P,message_id,recipient_id), message/principal FKs, delivery cursor/state. `obligations`: PK(P,obligation_id), request message FK, accountable principal FK, criterion digest, status/revision, deadline, result message FK. `continuation_waits`: PK(P,wait_id), node FK, obligation/decision/resource target, continuation ID, status; UNIQUE(P,node_id,target_kind,target_id,continuation_id). `wakeups`: PK(P,wakeup_id), node FK, cause ID, continuation ID, state/epoch; UNIQUE(P,node_id,cause_id,continuation_id). `timers`: PK(P,timer_id), obligation/issue target, kind, due_at, generation, state; unique target+kind+generation. `issues`: PK(P,issue_id), affected node FK, category, responsible principal, requested decision digest, status/revision. `issue_edges`: PK(P,issue_id,dependent_node_id), FKs. |
| Resources / control | `resources`: resource_id PK, typed capacity/mode, physical identity, revision. `resource_requests`: request_id PK, P FK projects, principal/attempt identity, whole-set digest, state/priority/deadline/revision. `resource_request_items`: PK(request_id,resource_id), two FKs, quantity CHECK>0. `resource_grants`: grant_id PK, request FK UNIQUE, generation/status/expiry. `resource_holders`: PK(grant_id,resource_id), grant/resource FKs, quantity, confirmed state; service computes capacity under writer transaction including reserved/uncertain holders. `channels`: channel_id PK, purpose/resources digest, expiry/status/revision. `channel_members`: PK(channel_id,P,principal_id), channel/project FKs, invitation/acceptance/revocation proof. `channel_messages`: PK(channel_id,message_id), channel/member sender binding, typed protocol kind/body, correlation/digest; distinct per-recipient delivery rows. `channel_offers`: PK(channel_id,offer_id,version), terms digest/state and grant reference if awarded. |
| Autonomy / Project | `autonomy_policies`: PK(P,policy_id,version), requirements digest, enabled/paused, explicit role/effect/resource limits and runtime-policy snapshot, owner approval evidence. `budget_accounts`: PK(P,budget_id), policy FK, currency/known status, limits/usage/reserved totals/revision. `budget_reservations`: PK(P,reservation_id), account/node/attempt FKs, requested allowance, state, settlement revision; unique admission operation. `usage_observations`: PK(P,attempt_id,provider_usage_id), attempt FK, measured tokens/cost/knowledge state; deduplicated monotonic settlement. `policy_decisions`: PK(P,decision_id), policy FK, exact question/ADR digest, classification/rationale, outcome; policy actor explicit. |

Ownership follows tables, not directory overlap: no lane changes another domain's
authorization/state rows directly. Cross-domain application commands use borrowed
domain views in a single transaction. Runtime owns attempt bindings; Identity
owns who may use an attempt. Coordination owns wait resolution; Work owns whether
the resolved continuation may run. Autonomy grants no lifecycle acceptance power.

## Index and invariant requirements

Add compound indexes for runnable work `(P,state,node_id)`, live attempts
`(P,principal_id,state)`, obligations `(P,responder_id,status,deadline)`, timers
`(P,state,due_at,timer_id)`, recipient inbox `(P,recipient_id,delivery_state,message_id)`,
pending operations `(subject,stage)`, resource requests `(state,priority,created_at,request_id)`
and channel membership `(P,principal_id,channel_id)`. Column names in migrations
must match the actual contract fields; aliases above must not become duplicate
facts. Store created_at on queued records to support deterministic aging.

Uniqueness expresses at-most-one active binding/admission; partial indexes use
fixed active-state sets from the frozen enum. FKs cannot enforce DAG acyclicity,
role authority, total shared capacity or policy scope: typed handlers prove these
under the same writer transaction. Both SQL and application checks are required.
State transitions retain prior history rather than rewriting original author,
approved digest or contribution identity. Cleanup snapshots needed after Plan
Git purge remain available through immutable artifact references.

## Migration ownership and independent delivery

Each module has one physical migration owner: storage owns common tables;
projects owns directory/project_meta/settings; identity, work, runtime,
coordination, resources and autonomy each own their listed tables. Use module-
qualified migration IDs `(module,version)` and digests, not competing global
`0002.sql` files. A static compiled catalog declares module dependencies and a
deterministic topological install order. The integration owner alone changes
the central catalog/export list; no dynamic arbitrary migration discovery.
Domain packages are isolated `v1/crates/{storage,identity,coordination,projects,
resources,autonomy,lifecycle,http}/`; runtime implementations live under
`v1/runtime/{supervisor,codex,claude,pi}/`. Each executable/package leaf owns its
manifest and private lockfile until an explicitly planned integration changes
that boundary. No shared mutable workspace lockfile is written by parallel lanes.
Work/lifecycle rows above belong to `lifecycle`; runtime schema fragments belong
to `runtime/supervisor`. The frozen shared-contract package is a path dependency.

Foreign-reference cycles are resolved by creating the full baseline schema before
admitting user mutations, and inserting mutually linked command records in one
transaction with narrowly declared deferred constraints. Baseline install is one
SQLite transaction per database; do not permit a server with half its required
modules installed. Domain test fixtures install the dependency schema without
requiring production domain services to be implemented first. Checksums of applied
migrations are immutable; new changes add a new module version. Old/new unsupported
catalogs refuse writes, never automatically downgrade or recover from old TOML.

The initial schema catalog is frozen before parallel execution. Domain lanes own
their mapping code and migration files, so storage does not become the owner of
every domain repository. Shared-contract changes are serialized and versioned;
private indexes may be added by the owning module without rewriting another lane.

## Contract tests and acceptance ownership

The common test kit supplies deterministic clock/IDs, a test-only in-memory
transaction implementation and fixtures for every shared DTO. Execute the same
domain contract suite against memory and SQLite mappings. Memory passes prove
domain rules only; restart, transaction rollback, WAL, corruption, migration,
backup and concurrent-connection cases require real SQLite files in the Plan's
isolated output directory. Never use production fallback to the fake store.

Required storage probes: duplicate/split payload ID, CAS contention, cross-Project
ID refusal, rollback after each domain/audit/outbox write, lost commit response,
event replay/reset, stale outbox claim, full disk, wrong schema/digest, foreign-key
failure, repeated backup/import and actual SQLite version/pragmas. Required joins:
handoff with unresolved obligation, pause racing a result, budget reservation racing
dispatch, result-before-subscription, and global reservation/project admission
failure on each side. HTTP/UI fixtures cannot substitute for these real joins.

## Primary implementation evidence

- [rusqlite0.40.2 Connection](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Connection.html):
  Send but not Sync; transaction_with_behavior and bounded busy configuration.
- [rusqlite Transaction](https://docs.rs/rusqlite/0.40.2/rusqlite/struct.Transaction.html):
  borrowed, not Send, default rollback and explicit commit.
- [Official feature manifest](https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/Cargo.toml)
  and [bundled SQLite header](https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/sqlite3/sqlite3.h):
  bundled/backup options; inspected bundled SQLite3.53.2. Pin a verified dependency
  and test this repository's Rust baseline; research is not compilation evidence.
- [Tokio blocking work](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html):
  persistent workers use dedicated threads; abort is not rollback of started work.
- [SQLite WAL](https://www.sqlite.org/wal.html),
  [transactions](https://www.sqlite.org/lang_transaction.html),
  [STRICT tables](https://www.sqlite.org/stricttables.html),
  [backup](https://www.sqlite.org/backup.html): short single-writer transactions,
  no multi-store atomicity and consistent backup requirements remain authoritative.
