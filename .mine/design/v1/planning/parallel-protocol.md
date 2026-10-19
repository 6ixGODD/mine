# Parallel Ownership and Integration Protocol — 1.0 Blueprint

This supplements the current execution protocol only for the next activated
workspace. No current0.2 owner or frozen Plan changes.

## Invariants

One Plan/worktree/owner; one shared-file writer; Root stays dev. Graph/ADR writes
always use Root released MINE and observed revisions. Silent/offline owners are
not proof of relinquishment. Transfer through the current governance path; the
unimplemented1.0 identity service cannot govern its own implementation.

Each leaf owns its complete responsibility and namespace, not a brittle whitelist
of pre-guessed filenames. Private manifests/locks and module-qualified migrations
are lane-owned. No lane edits a peer's schema, body, policy or generated output.
The [allocation ledger](index.md) is authoritative before activation; native
registered scopes become authoritative after materialization.

| Shared artifact | Sole writer | Contract for consumers |
|---|---|---|
| `v1/contracts/**` | Architect before activation; integration owner afterward | Freeze/review changes once; all affected consumers revalidate |
| `v1/runtime/supervisor/contracts/**`, runtime wire/fixtures/bindings | Architect foundation now; runtime supervisor sole writer after activation | Provider siblings and integration read only; WP0 review freezes the common wire |
| Root Cargo/npm manifests/locks, module exports/CLI/MCP registrations | integration | Leaf submits exported API+dependency request, never concurrent edits |
| Static migration catalogue and cross-module schema links | integration, common runner from storage | Domain owns own migration files/digests; no runtime directory scanning |
| Domain table mappings/migration module | owning leaf | No domain acquires authority by writing another table directly |
| UI manifest/lock/generated UI bindings | dashboard | Build shared schema from frozen source; drift check, no private fork |
| Runtime provider manifest/lock | corresponding provider inner lane | No provider updates another SDK lock or global environment |
| Root CI/Skills/generated plugin copies/install/package assets | integration | One complete generated transaction after real native contracts exist |
| Design source/approved ADR | Architect; native ADR lifecycle | No body rewriting or self-approval; implementation drift reported |
| Graph and independent review report | Root governance/reviewer | Native transition only; implementation report belongs to its Plan |

## Isolation and resources

Each registered node writes `<Root>/.mine/build/<node>/<tool-class>`. Runtime
inner lanes further use supervisor/codex/claude/pi subdirectories; no shared Rust
target, JS build/test cache, HOME, snapshots or generated schema staging.
Provision dependencies once in Root-owned lock-digest environments, then read-only
to concurrent lanes. Toolchains/downloads/content-addressed caches may be shared;
mutable dependency setup is serialized. No copies of environments in worktrees.

At planning time D: had about10.4GiB free; this is not enough to promise all native
builds simultaneously. Each start measures disk and memory, declares its projected
outputs, and limits heavy compilation concurrency while allowing independent code
work. Extraordinary shared-output strategies still require user agreement, never
implicit permission from a high-parallelism diagram.

## Integration and receipt rules

Integration starts its own harness in parallel but never edits leaf-owned files.
It consumes reviewed/accepted exports in dev; fixes to a leaf go to its owner,
or are bounded independent-review fixes under existing policy. Early probes can
read candidate artifacts with exact provenance, but cannot relabel them ACCEPTED.
The final gate requires all ten leaf Plans ACCEPTED and exact integrated inputs.

Keep a per-check evidence ledger: command, source/lock/config/toolchain/platform/
fixture/artifact digest, exit, count, log, RUN/REUSED/INVALIDATED/NON_REUSABLE/NOT_RUN.
Reuse valid unchanged evidence; do not rerun complete suites solely because a new
agent is reviewing. Integration/native/security checks still require actual proof.
Preserve first failures. No blanket ignores, “|| true”, mocked native results,
same-file double-hash determinism claims or a Windows run passed off as macOS.

Product completion is distinct from publication. After integration ACCEPTED, run
separate full sync then authorized release closure. No Plan authorizes push, tag,
publish, global install or overwrite of the user's active MINE environment.

Leaf acceptance is component-complete, with its own real mappings/native probes,
not whole-product acceptance. Every blueprint's cross-domain full scenario is
also required at the final integration join; it must not create implicit cyclic
acceptance dependencies among peer Plans. If an owned component genuinely cannot
be tested without a missing contract, fix that shared contract before activation,
not by silently changing the graph or claiming a fake adapter is production.
