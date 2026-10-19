# Contract-first implementation boundary

## Authority and release isolation

The owner's explicit 2026-10-16 planning request authorizes this bounded Architect
foundation on Root `dev`. This is a one-operation exception, not a change to the
normal prepared-worktree or independent-review policy. ADR24–31 remain unchanged.
No foundation is self-accepted and no current operation migrates real state.

`v1/` is an independent, unpublished source namespace. Root Cargo/package manifests,
CLI, Skills, CI, embedded assets and release attachment inputs do not reference it.
It may exist in a source checkout; it is not a delivered 0.2 feature. Tests must
prove Root Cargo metadata has no dependency on it. Do not install the prototype,
use it to govern this repository, or claim unchanged whole-tree evidence keys.
Before release, audit exact changed paths and package contents, not just binaries.

Current installed MINE has a single ephemeral graph; `planning_scope` selects ADR
readiness, not a release train. Nonterminal future registrations block closing the
current workspace. Therefore the durable `planning/` attachments are **unregistered
blueprints**, not executable graph nodes. After the current release closes, carry
Design through its approved path migration, open the next workspace using the
released binary, allocate IDs, materialize canonical Plan bodies, then register
and release via native MINE. Never fake withdrawal/acceptance to exclude future work.

## Small executable shared foundation

`v1/contracts` owns Rust serde types, schema export and pure invariant checks.
Actual consumers are its conformance tests, future domain/transport adapters and
the dashboard fixture boundary. It does not implement persistence, authentication,
SDKs, orchestration or lifecycle acceptance. No empty plugin framework is needed.

`v1/runtime/supervisor/contracts` additionally implements the closed runtime
request/response/event wire, selection/identity correlation, failure outcomes,
quiescence observations, bounded frames and schema export. It does not start a
process or authorize replacement from an unverified observation. Its README
records precise implemented versus absent boundaries. Runtime WP0 independently
reviews this existing foundation and adds cross-language corpus/binding checks;
it must not reimplement a second wire contract.

`v1/dashboard` uses the real CLI-generated b2jI theme, shared generated types,
Ajv validation and the same frontier fixture parsed by Rust. Its UI is explicitly
DEMO and has no backend mutations. Shared structure plus semantic counter checks
are verified across both languages; no production server/SDK claim follows.

- Distinct UUID types prevent crossing Project/principal/attempt/operation identity.
- Revisions, epochs and cursors are distinct decimal-string wire values, preserving
  SQLite's full nonnegative signed64 range in JavaScript. Zero is valid initial
  state; negative, noncanonical and overflow values refuse. They are not
  interchangeable or an authentication token.
- `CommandMeta` carries request identity, target Project, expected entity revision,
  optional assignment epoch and input digest. Actor identity comes from a trusted
  service authentication context, never from a self-declared request body.
- `ErrorCode` is stable internal vocabulary; adapters redact diagnostics and map
  errors without collapsing unknown external effects into ordinary retry failures.
- Model selection preserves provider-native model/effort/speed strings and a
  discovery snapshot. `runtime_default` and `exact` are distinct; no common effort
  enum or parsing `model-high-fast` into guessed capabilities.
- Capability checks reject stale snapshots and unsupported/unknown exact choices.
  Actual effective values are observations, not inferred from request success.
- Work-node state is separate from Plan status. The pure wake predicate never
  resumes paused/running/done nodes; persisted deduplication belongs to storage.

The schema generator emits JSON Schema 2020-12 from the same Rust types. Consumers
must reject unknown fields/version and validate boundary input before use. Schema
validation is structural; semantic checks (CAS, authorization, capability matching)
remain explicit. No generic JSON payload is a substitute for a domain contract.

## Parallel implementation structure

Use separate leaf packages, not a shared writable Cargo/npm lockfile. Each lane
owns its package manifest/lock and internal modules. Shared contracts are read-only
after foundation review. The integration lane owns production composition, root
manifests, generated shared schema/client, shipping assets and Skill distribution.
All mutable outputs are Root `.mine/build/<exact-node>/<class>`; foundation uses
the explicit pre-Plan identity `v1-foundation`. Root-managed dependency environments
are reused, never replicated inside linked worktrees.

Storage owns transaction machinery and migration catalogue; domain owners implement
their SQL mappings against frozen tables. No transaction spans SDK/Git/network.
Provider adapters and supervisor develop concurrently against typed observations;
the supervisor alone decides admission. HTTP and UI develop against fixtures but
their final acceptance requires real service behavior. Early vertical integration
runs as soon as a coherent slice is available; the final join is not the first test.

## Quality and evidence

Foundation commands (from Root; set isolated `CARGO_TARGET_DIR` first):

```text
cargo fmt --manifest-path v1/contracts/Cargo.toml -- --check
cargo clippy --manifest-path v1/contracts/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v1/contracts/Cargo.toml
cargo run --manifest-path v1/contracts/Cargo.toml --bin export-schema -- --check
cargo metadata --no-deps --format-version 1
```

Dashboard commands are defined by its actual runner/README, not hypothetical root
npm scripts. Both foundations require independent review before downstream Plan
release; native provider/DB/product gates are not satisfied by these tests.

## Opened primary research (2026-10-16)

| Source | Concrete implication |
|---|---|
| [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html) | Independent manifest/workspace and explicit command selection keep the Root default package unchanged |
| [Cargo manifest](https://doc.rust-lang.org/cargo/reference/manifest.html) | `publish=false` is package-publication control, not a claim that Git archives exclude a directory |
| [Schemars](https://docs.rs/schemars/1.2.1/schemars/) | Derive schema from serde-facing types; retain semantic checks beyond generated schema |
| [JSON Schema 2020-12](https://json-schema.org/draft/2020-12) | One versioned structural interchange schema for Rust/TS fixtures |
| [shadcn preset commands](https://ui.shadcn.com/docs/changelog/2026-04-preset-commands) | Decode/generate exact b2jI, do not substitute approximate theme |
| [Vite build](https://vite.dev/config/build-options) and [shared options](https://vite.dev/config/shared-options) | Explicit output/cache directories separate mutable products from source and dependencies |
| [TypeScript strict](https://www.typescriptlang.org/tsconfig/strict.html) | Strict checking is a blocking frontend gate |

Pinned foundation dependencies reuse the already locked Rust dependency versions;
this does not turn an older product test into proof of the new package.
