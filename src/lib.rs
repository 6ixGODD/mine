//! MINE core library.
//!
//! Implements the deterministic initialization service, the
//! design-namespace marker validation, and repository identity/version
//! persistence. The execution-graph domain and safe
//! persistence. The CLI, JSON/human output, deterministic
//! rendering, read-only Git evidence, design backup, workspace lifecycle,
//! and the plan-lifecycle commands. The CLI adapter, MCP server, and
//! distribution are wired progressively; later plans add MCP, Skills
//! integration, and distribution.

// `AGENTS.md` mandates "Business code must not use `unsafe`." Enforce it at
// compile time across the whole `mine` library crate so the gate cannot be
// silently regressed by a later plan (the default `cargo clippy -D warnings`
// gate does not include the `unsafe_code` lint). Any platform primitive that
// requires `unsafe` (for example file locking) must live inside a vetted
// external dependency, not in this crate.
#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod agent_setup;
pub mod application;
pub mod cli;
pub mod domain;
pub mod infrastructure;
pub mod mcp;
pub mod output;
pub mod render;
pub mod setup;
