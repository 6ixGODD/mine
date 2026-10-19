# ADR-0023: Keep MINE Governance Files Under .mine

## Status and authority

Proposed for 0.2.0; native ADR registry is authoritative.
The owner explicitly requested that architecture and Plan files leave docs/,
live below .mine/, and remain normally Git-trackable through .mine/.gitignore.
This body specifies the directory, ignore and bootstrap consequences for review.
It does not claim approval of its digest or authorize immediate live relocation.

After approval this replaces ADR-0006's namespace choice and ADR-0002's path
spelling, preserving their ownership/retention intent. Original approved bodies
and recorded digests remain untouched. Architecture decisions still require
owner approval; implementation and acceptance remain separate.

## Current evidence

At discovery: Root dev f1754456dd66a543e6541cde8f5db9b63b1ef7ff,
installed mine 0.1.13, graph v4 revision 76, ADR registry revision 41.
plan15 is ACCEPTED; plan16/17/18/21 are IN_PROGRESS in registered worktrees.
These are observed facts, not permanent scheduling assumptions.

- .mine/config.toml points Design/ADR/Plan/graph to docs/.
- src/application/init_service.rs generates those roots, markers and AGENTS text.
- src/domain/adr.rs validates decision bodies against docs/design/decisions/.
- src/domain/config.rs, Design/Plan validators, stores, services, release/scan/
  backup paths and all five source Skills contain the current path contract.
- .mine/.gitignore currently ignores runtime/, locks/ and build/; the root
  .gitignore separately ignores /.mine/worktrees/.
- Local .git/info/exclude ignores docs/plan/, which currently requires forced
  staging for new Plans. The new managed layout must not reproduce that behavior.
- REQUIREMENTS.md is absent; the owner's request is the new requirement.

Changing only .mine/config.toml or moving live files would break old-binary
validation/lifecycle and leave frozen Plan/report references unresolved.

## Decision: one canonical governance namespace

| Artifact | 0.2.0 canonical location |
|---|---|
| Design root and progressive-disclosure knowledge base | .mine/design/index.md and .mine/design/ |
| Design ownership marker | .mine/design/.mine-design.toml |
| ADR bodies, registry and generated index | .mine/design/decisions/ |
| Plans and Plan workspace ownership marker | .mine/plan/ |
| Machine graph and generated view | .mine/plan/execution-graph.toml and .mine/plan/execution-graph.md |
| Implementation/review/retained verification evidence | .mine/plan/reports/ |
| Configuration and verification policy | .mine/config.toml and .mine/verification-policy.json |
| Local backups, runtime journals, locks and outputs | .mine/runtime/, .mine/locks/, .mine/build/ |
| Linked Plan worktrees | .mine/worktrees/ |

Public user documentation may remain in docs/. MINE must not claim, move or
delete arbitrary user docs/ content. Current authoritative Design/Plan trees
have one owner and one location; no permanent docs/ alias, symlink or dual-write.

Resolve repository paths centrally from the validated Root/layout contract.
Initialization, CLI/MCP, validation, stores, backup, scanner, lifecycle,
verification receipts and release checks must consume that same authority.
Moving the graph does not grant linked worktrees independent graph authority.
Do not add a configurable multi-layout framework merely to preserve hardcodes.

## Decision: Git tracking and ignored local output

.mine is not a blanket ignored directory. The following rules belong in tracked
.mine/.gitignore:

```gitignore
# Local mutable data
/runtime/
/locks/
/build/
/worktrees/

# Versioned governance inputs
!/.gitignore
!/config.toml
!/verification-policy.json
!/design/
!/design/**
!/plan/
!/plan/**
```

The explicit exceptions express that new governance files are normally visible
to Git and can be staged without -f. Ignore rules do not themselves stage files.
Project ignore management preserves unrelated user rules and uses one managed
source for these child paths rather than duplicate contradictory declarations.

A parent rule excluding the entire .mine directory prevents Git from discovering
the nested rules. Initialization/doctor must detect that effective condition,
identify the responsible pattern and safely reconcile the MINE-owned/root rule
within its authorized setup operation or return an actionable refusal. Never
silently rewrite user-global excludes or claim success from nested text alone.
Inspect .git/info/exclude/global rules without relying on forced tracked files
to hide a fresh-clone/new-file failure. Generated secret/runtime output stays
ignored even when governance exceptions match other .mine children.

## Decision: retention and cleanup remain narrowly owned

This requirement changes location and tracking, not the existing release
retention policy: .mine/design survives on stable; .mine/plan is tracked during
the active dev/Plan lifecycle and its owned ephemeral workspace is purged only
at the established final release boundary. It is never hidden by gitignore.
If permanent Plan history on stable is desired, that is a separate retention
decision; no such change is inferred here.

Design backups move under .mine/runtime/design-backups/ with the existing
verified backup/manifest discipline. Never purge the entire .mine directory:
configuration, Design, decisions, active/rejected worktrees, output and locks
have different owners and lifetimes. Release/native checkpoint/scanner rules
must use exact configured owned roots, including recursive backup checks.

## Bootstrap and rollout boundary

The running release is still developed by installed 0.1.13. Keep this real
legacy v4 graph, config, Design and frozen Plan/report bodies at their existing
paths while that binary is required for lifecycle writes. Test all new layout
behavior in isolated candidate repositories, using per-Plan output namespaces.
Do not change the global binary, rewrite a frozen Plan, move a running lane's
graph, or hand-edit machine state to make the path change appear complete.

New 0.2.0 workspaces use the canonical layout. For this repository, the release
handoff must finish all old-binary Plan mutations before the final curated
stable-tree cutover. Move the durable Design/ADR tree with verified byte/digest
preservation, update current config/governance references in that reviewed
cutover, and prove the resulting tree with the built candidate. Old ephemeral
Plan/graph content follows normal release purge, not a fabricated new history.
A failed cutover retains its exact original inputs/evidence and reports pending;
do not publish a half-moved layout or leave two writable fact sources.

An existing active legacy workspace is detected and refused by new-layout
initialization with an actionable old-workspace completion path; it is not
silently relocated or assigned invented provenance. Automated migration of
live graphs, revision/path/body identities or active worktrees is outside this
bounded requirement. Moving terminal durable Design must preserve approved
ADR bytes/digests; change current indexes and references without rewriting
historical decision text to make a search count zero.

## Implementation coverage and parallel scheduling

This requirement includes binary, five Skills/templates/generated payloads,
AGENTS/source-of-truth references, fixtures and public documentation. Init,
Design/ADR validation, graph/Plan/report paths, backup/sync, release/purge,
scanner, effective ignore checks and CLI/MCP diagnostics must agree.

Do not edit or withdraw an active Plan merely because a new requirement arrived.
The independent ownership/receipt/journal lanes may finish their frozen work.
Preserve existing hard and work-package gate identities, including plan21's
plan19 late gate, unless a separately reviewed impossibility proves a change
necessary. This ADR itself registers no new Plan and changes no execution edge.
The planner must assign the new work before the final 0.2.0 candidate and use
disjoint scopes where possible; avoid a late post-candidate path rewrite and
avoid unnecessary rejection/compensation cascades.

## Required acceptance evidence

- Fresh init creates governance only at .mine/design and .mine/plan as applicable;
  unrelated docs/ remains byte-for-byte unchanged.
- Real Git new-file probes: Design/ADR/Plan/graph/report/config files are visible
  and ordinary add works; runtime/build/locks/worktrees/backups remain ignored.
  Cover broad root exclusion, local/global patterns and a clean clone.
- Effective ignore inspection uses git check-ignore --no-index -v plus actual
  untracked/index evidence; interpret negated patterns correctly.
- CLI/MCP/doctor/Design/ADR/graph/lifecycle/receipt/scan/backup/release fixtures
  use the same resolved roots, including linked-worktree invocation.
- Stable candidate retains .mine/design and configuration, contains no owned
  ephemeral .mine/plan or tracked runtime/backups, and does not purge .mine/.
- Destination collisions, foreign markers, symlink/junction escapes and cutover
  interruption fail without losing files or approved digests.
- All five source/generated/embedded Skills agree with actual binary help and
  paths; examples and AGENTS use the implemented source of truth.
- Native layout implementation tests are future work, NOT_RUN by this proposal.

## Primary evidence

Opened 2026-10-12:
[Git gitignore](https://git-scm.com/docs/gitignore) defines relative patterns,
negation, precedence, tracked-file behavior and excluded-parent limitations.
[Git check-ignore](https://git-scm.com/docs/git-check-ignore) defines --no-index
and verbose negation diagnostics. These semantics motivate effective-path
probes rather than checking only the contents of .mine/.gitignore.

Alternatives rejected: keep docs/ as another writable root; force-add governance
forever; ignore all .mine and assume nested exceptions work; rewrite active
Plan/ADR bytes; purge all .mine as runtime; introduce broad automatic live-graph
migration solely for this directory change.
