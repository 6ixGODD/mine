# Project Registry, Lifecycle and Removal — Approved 1.0 Target (ADR-0024)

## Authority and identity

Install/bootstrap creates the user-local `.mine/` namespace with restrictive
user permissions, or lazily initializes it on first use; never in a system-wide
shared directory. Global `control.sqlite3` owns project IDs, canonical locations,
directory operation journals, service settings and shared coordination facts.
`<ProjectRoot>/.mine/state.sqlite3` owns project business state. The stable Project
UUID survives directory moves; paths, display names and Git remotes are not IDs.
Every Plan/principal/work node is qualified by Project. A linked worktree resolves
the same Root/Project, never registers as another Project implicitly.

One project initially binds one canonical Git Root and its managed worktrees;
multi-repository orchestration is not implicitly promised. A separately initialized
repository is another project. A location conflict/duplicate live UUID is refused;
explicit import distinguishes recovery of the same Project from a new fork.
Unavailable/moved/unreadable projects remain visible as such, without recreating
empty state. Relocation verifies marker/DB/Git identity before updating the path.

Global dashboard summaries are projections stamped with source revision/time.
They never authorize an action; each mutation rechecks the authoritative Project
service. Project-local users/agents cannot infer access from the global directory.
The local human owner manages registration and project capability grants.

## Initialization and recovery

Proposed `mine init` semantics: allocate idempotent global REGISTERING intent;
validate canonical target and ownership; create/bind Project store and owned
human documents; verify both sides' Project/operation identity; mark ACTIVE.
Each stage is recoverable using the same operation ID. Existing foreign files,
dirty ambiguous governance and duplicate roots refuse before destructive effects.
SQLite transactions in separate WAL databases are **not one atomic transaction**.
Global recovery can find an interrupted init even if the Project DB was never
created. No success until both authorities agree; retries cannot mint another ID.

CLI, MCP and Web use one user service per control directory. Local on-demand
startup is serialized; no daemon per Plan/session and no hidden startup LLM call.
OS startup registration is a user option, not installation permission inferred
from wanting a dashboard. Service absence is distinct from project corruption.

## Deinitialization

Proposed `mine deinit` removes MINE management, not the user's project. First
show a dry-run manifest of exact owned paths, generated sections, registrations,
managed worktrees/branches and blockers. User execution of deinit confirms that
manifest/revision; changed scope requires a new preview, not silent expansion.

`ACTIVE -> DEINITIALIZING -> REMOVED` is a global durable operation:

1. Stop new admissions, revoke queued wakes and cross-project memberships.
2. Quiesce all managed writers/children, reconcile pending Git effects and held
   resources. Dirty/diverged/rejected work is not automatically disposable.
3. Create a consistent recoverable backup outside the to-be-removed tree, or
   require explicit discard/export choice for human governance and evidence.
4. Remove exact proven-owned artifacts and generated integration sections;
   preserve unrelated user text, environments, secrets and unknown `.mine` files.
   Use native Git worktree cleanup with positive branch/cleanliness proof.
5. Close Project database handles before removal; complete each manifest entry
   once; remove the active global project entry only after confirmed cleanup.

The global operation journal/tombstone survives Project DB deletion and supports
interrupted cleanup. Successful deinit means no MINE-owned project artifacts or
active registration remain, except an explicitly selected external recovery
archive. Unknown files block full cleanup rather than being recursively erased.
Tracked deletion is visible in Git; deinit does not rewrite history or auto-push.
Provider-wide account/session history and global MINE installation are not project
artifacts. Retain a minimal cleanup receipt outside project, separately removable
by the user; never keep hidden full source/transcript copies indefinitely.
An already missing directory can be unregistered after identity-aware confirmation,
but that operation must say local cleanup was impossible, not “cleaned”.

## Acceptance

Crash inject every cross-store stage; repeat init/deinit without duplicate IDs or
deletion; move, duplicate clone, unavailable disk, permission failure, symlink
escape, dirty foreign worktree, in-use database and unknown file cases. Prove
one project's removal never changes another or user's general agent setup.
Primary constraints: [SQLite WAL](https://www.sqlite.org/wal.html),
[ATTACH](https://www.sqlite.org/lang_attach.html),
[backup API](https://www.sqlite.org/backup.html). This lifecycle is MINE policy,
not transactional behavior supplied automatically by SQLite.
