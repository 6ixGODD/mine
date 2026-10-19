# Runtime wire foundation (NOT ACCEPTED)

This standalone Rust crate is an isolated 1.0 foundation, not linked into the
current MINE product. It imports `mine-v1-contracts` through `../../../contracts`
and owns its private manifest/lock. Governing target:
[`runtime-contracts.md`](../../../../docs/design/v1/runtime-contracts.md).

Delivered: finite request calls and method results, correlated responses,
closed events, scoped IDs/counters, common selection/discovery composition,
requested versus observed values, runtime failure/outcome axes, inspection and
quiescence observations, handshake limits, pure validation, LF framing checks,
and a generated consumable JSON Schema. The request shape is
`{protocol,request_id,operation_id,project_id,scope,deadline_ms,call:{method,payload}}`.
Response uses `outcome:{status,result|error}`; result uses `{method,payload}`.
Event uses `event:{kind,payload}`. No arbitrary provider-call passthrough exists.

The schema is structural, not authorization: consumers must also apply semantic
validation. In particular canonical UUID/counter/digest parsers, revision and
generation equality, expiry, supported method/selection, exact native resume,
and correlation constraints are Rust checks, not necessarily fully expressible
in the generated JSON Schema. A TS worker must consume the frozen schema and
shared conformance fixtures, not treat successful JSON validation as permission.

## Explicit limits

- No SDK/process supervisor, network/authentication, SQL, retry loop, callback
  approval bridge, production event ingestion, native provider or LLM execution.
- Path checks only recognize absolute syntax. They do not prove canonical paths,
  symlink containment, registered worktree identity or Project trust.
- Evidence references are identifiers, not independently verified evidence.
  `confidence:confirmed` cannot itself grant writer replacement: the controller
  must validate its process/resource inventory and authority before acting.
- Adapters still must bind runtime profile, policy revision, trusted config root,
  executable/version/schema and exact process ownership. DTO validation cannot
  inspect external state or prove provider capability claims.
- Response/request correlation is not replay protection or idempotency; host
  must enforce connection lifecycle, pending-ID uniqueness and durable intent.
- An `inspect` response carrying a binding must match the requested binding
  exactly; its selection must match the response Project and returned provider.
  For `create_operation_id` inspection, wire validation proves only internal
  consistency: the controller must additionally establish that the returned
  native ID belongs to that durable create intent before using its observations
  to authorize replacement or any state transition.
- Stream sequence handling, control-event backpressure and queue capacities are
  protocol obligations; only constants and individual-frame checks exist here.
- Native SDK mapping, cross-platform behavior, generated TS bindings and a
  shared file-based Rust/TS fixture corpus are NOT RUN / NOT DELIVERED.
- This artifact requires independent foundation review and the runtime WP0
  review/binding gate before four parallel implementation lanes may consume it.

## Focused verification, 2026-10-16

Windows; Root-managed stable toolchain. All commands set:
`CARGO_TARGET_DIR=<Root>/.mine/build/v1-foundation/runtime-wire/cargo-target`.
There is no crate-local target, node_modules or virtual environment.

| Exact command | Result |
|---|---|
| `cargo fmt --manifest-path v1/runtime/supervisor/contracts/Cargo.toml -- --check` | exit 0 |
| `cargo clippy --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code` | final exit 0 |
| `cargo test --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --all-targets --all-features` | exit 0; 20 passed, 0 failed, 0 ignored |
| `cargo run --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --bin export-schema -- --write` | exit 0; generated schema |
| `cargo run --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --bin export-schema -- --check` | exit 0; exact generated equality |

Initial `cargo check` exited 0 with an unused doc-comment warning (fixed, not
treated as the final strict gate). Initial strict clippy exited 1 for a large
`ResponseOutcome` variant; boxed its result without changing JSON representation.
Final strict lint and tests passed after that correction. No ignored warning,
SDK fixture, Linux container or prior test result is claimed as native evidence.

Generated schema SHA-256:
`9993728eb8240ad0b60bb54268c3f030369de49772338ab6614493d2fa1be8a5`.
Root-wide tests were not rerun for this disconnected package. No Plan lifecycle,
branch, commit, acceptance, release or push was performed by this work.

### Independent P1 quick-review correction

The reviewer found that `inspect(native A)` could accept native B within the
same attempt/generation, including foreign-Project selection. Fixed response
binding equality and selection Project/provider correlation; selection without
a returned binding now fails. `RuntimeCapabilitySnapshot.validate` now delegates
model/version/choice validation to common `validate_discovery` instead of keeping
a weaker duplicate implementation. No DTO/schema shape changed.

Focused re-verification on the corrected code (not a rerun of the earlier
20-test suite):

| Exact command | Result |
|---|---|
| `cargo test --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --test contract review_p1_` | exit 0; 6 passed, 0 failed, 20 filtered out |
| `cargo clippy --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --all-targets --all-features -- -D warnings -W unsafe-code` | exit 0 |
| `cargo run --manifest-path v1/runtime/supervisor/contracts/Cargo.toml --bin export-schema -- --check` | exit 0; schema unchanged |

The six regressions cover original-binding success, different native session,
different provider even with matching selection, foreign Project selection,
create-intent selection binding and common-discovery duplicate/empty choices.
No full suite, root integration or native provider smoke was rerun for this fix.
