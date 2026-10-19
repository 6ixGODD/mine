# Unshipped control-plane foundation

This namespace is deliberately **not wired into the current product**. The owner
authorized a bounded architecture foundation on `dev`; independent review is still
required. No CLI, installed Skill, real database, provider session, global user
configuration or current release attachment uses this code.

`contracts/` contains executable shared invariants and a generated schema.
`dashboard/` is a separately checked fixture-only UI shell, not a working backend.
The detailed target and unregistered implementation blueprints live in the active
Design knowledge base's `v1/` area. They activate only in the next release workspace.

Use Root-managed `.mine/build/v1-foundation/` for this pre-Plan verification, and
each exact node's namespace once execution starts. Do not use Root Cargo build
output, install these packages globally, or create environments in a worktree.

## Contract checks (PowerShell from Root)

```powershell
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) '.mine/build/v1-foundation/cargo-target'
cargo fmt --manifest-path v1/contracts/Cargo.toml -- --check
cargo clippy --manifest-path v1/contracts/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v1/contracts/Cargo.toml
cargo run --manifest-path v1/contracts/Cargo.toml --bin export-schema -- --check
```

`export-schema --write` is the sole schema generator. Generated drift is reviewed
with all consumers; it is not fixed independently in TypeScript. Schema structure
does not replace semantic validation or authorization. No secrets belong in fixtures.
