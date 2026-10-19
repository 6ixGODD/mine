# Testing, Release, and Recovery

## Quality gates

Accepted extensions are [verification receipts](verification-receipts.md),
[lifecycle receipts](../governance/lifecycle-receipts.md) and
[release delivery](../integrations/release-delivery.md). Candidate consumers are
implemented; focused evidence, complete final-input checks, independent review
and release closure must still be distinguished. Installed 0.1.13 remains the
live self-development authority until the approved release boundary.
Role changes do not require rerunning identical checks; unchanged attributable
evidence may be reused, while required independent/platform checks still run.

Expected repository gates include:

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo doc --no-deps
cargo deny check            # when adopted
```

The 0.2.0 release matrix covers Windows and Linux x86_64. macOS delivery is
owner-deferred under ADR-0034; reusable implementation is not native evidence.

Parallel or sequential Plan verification must attribute mutable outputs to the
exact Plan node. Build products, test binaries, incremental state, coverage,
code-generation intermediates, and packaging staging must not share a writable
directory across Plans. Language adapters select Root-managed
`.mine/build/<plan-id>/...` directories; a compensation node receives a fresh
namespace even though it reuses the rejected lineage's Git worktree. When
measured disk requirements make this infeasible, execution pauses for an
owner-approved serialized or coarser exclusive strategy before any build runs.

## Test layers

Approved ADR-0022 additionally requires real pre-start two-Plan Git sequences,
process contention and every native graph-checkpoint recovery boundary before
0.2.0. See [preparation evidence](../governance/ready-preparation.md#required-evidence-and-primary-sources)
and [checkpoint evidence](../governance/root-graph-checkpoints.md#verification-and-primary-sources).
Pending checkpoint blocks workspace/release closure even after a terminal TOML
write. Earlier candidate evidence invalidated by this correction must be
refreshed; a workflow declaration is not native execution evidence.

### Unit and property tests

- identifiers, markers, and path normalization;
- state transitions and readiness;
- conflict detection and revision rules;
- design-index and anchor validation;
- backup destination validation;
- arbitrary DAG properties;
- serialization round-trips;
- repository escape prevention.

### Integration tests

- `mine init` for absent, valid, foreign, and legacy design roots;
- design backup creation, ignore behavior, link safety, and failure atomicity;
- workspace open/close without user version input;
- CLI lifecycle and JSON snapshots;
- concurrent writers and lock timeout;
- MCP lifecycle calls;
- Skill contracts against real CLI/MCP schemas;
- installer idempotency and backup;
- managed branch and release behavior.
- linked Plan worktree creation, lookup, isolation, conflict recovery, and safe
  cleanup with Root remaining on `dev`.
- start-time `branch_anchor` capture for base and compensation nodes; Root graph
  divergence followed by valid active/reviewable reuse; rewritten, stale,
  detached, wrong-path/branch, and missing-anchor refusals.
- graph provenance repair: the v2 anchor migration for recorded
  implementation chains and active compensation ancestry, including
  ambiguous-base and missing-object no-write refusals, the v3-to-v4
  version-only upgrade, generated-view byte parity, and old-writer rejection
  of newer schemas.
- evidence-bound `IN_PROGRESS -> REJECTED`, canonical report and next sibling
  enforcement, revision conflict, no fabricated implementation evidence, and
  preserved unmerged lineage worktree.
- ADR transition/digest/supersession/readiness behavior, concurrent registry
  writers, deterministic decision-index rendering, and Plan admission refusal.

### Real-client smoke tests

Verify actual discovery on supported current versions of Claude Code, Codex, Pi, and OpenCode. Copied files are not equivalent to discoverable Skills.

## Legacy repository fixtures

Tests include:

1. large repository with no design;
2. MINE scaffold with no meaningful leaves;
3. valid MINE design that is stale relative to code;
4. legacy unmarked `docs/design/`, which must be rejected;
5. foreign repository marker, which must be rejected;
6. repository-internal and external symlinks/junctions;
7. unscoped sync with explicit incomplete-coverage reporting.

## Release model

### Generic repository release gates

Every MINE-managed repository must pass these gates before local stable integration:

1. valid MINE repository configuration (`.mine/config.toml`);
2. valid Design (`mine design validate`);
3. terminal Plan graph on development state (every Plan `ACCEPTED` or `REJECTED` with accepted compensation);
4. accepted compensation chains for every rejected Plan;
5. clean working tree;
6. no pending MINE-owned transactions;
7. stable candidate cleanliness: no configured Plan workspace, tracked managed
   backup/runtime, or old bootstrap Plan tree on the stable branch;
8. configured stable/integration branch correctness (read from `config.branches`, not hardcoded); a configured stable branch that does not exist in git is a decisive, actionable failure (run `mine init` to repair a stale recorded branch), never a silent empty resolution;
9. repository-defined decisive validation evidence (discovered from `AGENTS.md`, Design, and the Plan - never presuming a specific toolchain).

Candidate preflight checks real phase-aware source/candidate evidence through
ADR-0019's typed lifecycle. A fixture result does not establish this project's
actual release operation or permission to publish. Generic
preflight must **not** require `skills/`, `plugins/mine/skills/`, four-client
installation, or MCP tool-count verification.

### MINE source-repository additional gates

The MINE source repository adds project-local gates enforced by `AGENTS.md` quality tables, CI, and MINE-local Design - not by the generic preflight:

- root/generated Skill synchronization (`python scripts/sync-plugin-assets.py --check`);
- embedded payload verification;
- four-client installation smoke tests;
- MCP discovery against the implemented schema (current Design lists 19 tools);
- bootstrap tests;
- release artifact packaging and supported-platform delivery checks. For exactly
  v0.2.0, [ADR-0035](../decisions/0035-local-equivalent-v020-publication.md) replaces
  hosted execution with the independently reviewed local equivalent join;
  hosted CI remains NOT_RUN and macOS remains deferred under ADR-0034.

### Lifecycle

1. develop on managed `dev` and `plan/*` branches;
2. independently accept and integrate all plans;
3. run full `mine-sync` against accepted code (Phase A);
4. `mine-plan-review complete release closure` performs mechanical closure (Phase B);
5. determine next managed repository version;
6. close and purge only the configured owned Plan workspace;
7. verify no plan workspace or design backup enters stable release;
8. integrate through squash or curated commits;
9. tag and publish checksummed binaries (MINE source repository only);
10. delete local managed temporary branches.

## Recovery

- bad sync: restore selected or full design from the timestamped local backup, then rerun with narrower scope or corrected instructions;
- backup failure: do not mutate design;
- namespace conflict: candidate init refuses foreign/active legacy governance;
  preserve it, finish old-binary lifecycle writes and perform reviewed terminal
  cutover, never force-create a competing root;
- revision conflict: reload and re-evaluate;
- lock timeout: report evidence, never break blindly;
- render failure: retain TOML and rerun render;
- failed workspace purge: leave files intact;
- incomplete repository scan: keep explicit warnings and block full-release attestation.
- infeasible immutable active Plan: freeze its write lane, preserve its clean
  branch/worktree, approve the missing Design decision, accept a serialized
  governance-recovery Plan when the current binary lacks the transition, then
  reject the original through `mine plan reject` and resume only its next
  sibling compensation in the preserved lineage worktree;
- schema-v2 graph requiring branch anchors: record any recovery Plan's ordered
  implementation commits through the v2 lifecycle first, then run the sole
  revision-checked `mine graph repair-provenance` migration. Ambiguous evidence
  blocks migration; no manual TOML repair or reflog guess is permitted.
