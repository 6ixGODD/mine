// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Distribution and Skill-contract integration tests.
//!
//! These tests verify the final Skill contract (MCP-first / CLI-fallback
//! against the actual published MCP tools), the distribution structure for
//! Claude Code / Codex / Pi / OpenCode, the deterministic synchronization
//! mechanism, embedded Skill payloads, and drift detection.
//!
//! Sync-algorithm tests run inside **isolated temporary directories** and never
//! modify real user configuration or the live repository. The real sync script
//! is exercised via subprocess only in read-only `--check` mode against the
//! repository itself.

#[path = "distribution/common.rs"]
mod common;
#[path = "distribution/contract.rs"]
mod contract;
#[path = "distribution/embedded.rs"]
mod embedded;
#[path = "distribution/native_contract.rs"]
mod native_contract;
#[path = "distribution/structure.rs"]
mod structure;
#[path = "distribution/sync.rs"]
mod sync;
