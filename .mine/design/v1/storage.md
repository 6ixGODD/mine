# SQLite Authority and Durable Effects — Approved 1.0 Target (ADR-0026)

## Ownership, not one giant shared database

Global `<User>/.mine/control.sqlite3` owns directory/lifecycle journals, service
settings, host resource grants and cross-project channels. Each Project's Root
`.mine/state.sqlite3` owns its config, roles, assignments/handoffs, ADR status,
Plan graph, work nodes, decisions, operation stages, messages/commitments,
session attempts, receipt metadata, audit/outbox and autonomy policy. No second
global copy of Plan state; summaries are disposable revisioned projections.

Human requirements, Design, ADR and Plan bodies and reports remain Markdown under
`.mine/`; DB records exact digest/path/version relationships. Large build artifacts,
bounded logs and provider-native session transcripts stay in owned files referenced
by digest/identity. “Machine state in SQLite” does not mean stuffing binaries or
another vendor's entire filesystem format into database rows.

Rust application services are the single write path shared by CLI/MCP/Web. There
is one controlled writer queue per database, short transactions, explicit foreign
keys, WAL and FULL synchronous control-state durability, bounded busy timeout,
version/CAS predicates and indexed/paged reads. Verify pragmas actually apply.
Never hold a DB transaction while running Git, SDK/network, LLM or UI subscriptions.
Use a maintained pinned SQLite build containing WAL-reset fixes (3.51.3+ or
verified patched backport); no arbitrary host SQLite assumption. WAL is local
disk only; network-share stores are refused, not treated as cross-host locking.

## Transactions, events and consistency

A successful decision transaction updates problem/ADR state, audit and outbox
atomically. Every mutation includes stable idempotency ID, expected entity revision,
authenticated actor and exact input digest where relevant. Repeated same request
returns the recorded result; same ID with changed payload refuses. Concurrent
UI/session decisions cannot both win a CAS. “Agent says approved” does not mutate
status: its binary call must succeed. UI renders committed source revision only.

After commit, dispatch effects with durable attempt IDs. Delivery is at least once;
consumers deduplicate by event/effect identity. External Git/SDK/filesystem effects
are not SQLite transactions. Persist intention, execute, inspect objective result,
then complete, preserving UNKNOWN/PENDING if response disappears. Replay resumes
stages rather than blindly repeating destructive actions or LLM turns. Keep0.2
exact-tip/cleanup, stored-anchor, receipt and journal guarantees; replace medium,
not safety. Machine updates no longer create graph-only Git commits in1.0.

Read projections use a database revision/event cursor. SSE reconnection asks for
events after a cursor; if history was compacted, send reset+fresh snapshot. A
projection cannot authorize actions based on stale optimistic UI. No live DB/WAL
in Git; no TOML dual writer; no cross-database WAL atomicity claim.

## Migration, backup and portability

An explicit0.2-to1.0 cutover requires a quiescent project, verified backup, no
unknown active operations/writers, exact graph/ADR/Markdown/Git identity checks,
and a staged versioned import. Validate integrity, foreign keys and all bindings
before installing the new authority. Refuse old writers after cutover; no silent
fallback to old TOML or simultaneous writer compatibility. Imported historical
timestamps/approvals remain provenance, not newly fabricated approvals. Migration
is not authorized on current active Plan19/21 by these proposed documents.

Use SQLite Online Backup API with manifest/schema/Project/digest checks, not a
copy of an active main DB alone. Include needed human documents/Git references
in a consistent export boundary and verify recovery. A Git clone alone no longer
contains complete machine history or approvals. Offer explicit portable state
backup/import, separately from Markdown-only Git clone; never infer APPROVED
from prose. No automatic upload or secret-bearing export. A fresh independent
clone can initialize as a new Project; same-identity restore is separately checked.

Deinit cleanup receipts live globally outside the removed Project. Normal source
release retains Project business history locally even when the stable Git tree
omits ephemeral Plan files; immutable document/evidence snapshots referenced by
the database must not dangle after workspace cleanup. Retention is user-visible
and separates operational history, bounded raw logs and large disposable outputs.

## Required evidence

Crash after each write/effect/outbox boundary, duplicate notifications, interrupted
backup/import/deinit, stale UI concurrent approval, WAL checkpoint pressure, long
readers, full disk, corruption/version mismatch and cross-project isolation.
No full-table polling to keep the dashboard alive. Quality gates add schema
migration, integrity and recovery tests to Rust baseline; exact DDL is post-approval.
See primary [WAL](https://www.sqlite.org/wal.html),
[transactions](https://www.sqlite.org/lang_transaction.html),
[backup](https://www.sqlite.org/backup.html),
[foreign keys](https://www.sqlite.org/foreignkeys.html).
