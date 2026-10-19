//! Pure domain rules and typed errors.
//!
//! Domain modules contain no filesystem, Git, CLI, or MCP concerns. The core
//! introduced the error model, the project configuration model, the
//! design-namespace marker validation, the repository identity/version logic,
//! and the side-effect ports. The domain is extended with the
//! execution-graph aggregate, plan status and transition rules, path safety,
//! design references, and graph validation/algorithms.

pub mod adr;
pub mod config;
pub mod design_marker;
pub mod design_reference;
pub mod error;
pub mod graph;
pub mod lifecycle_operation;
pub mod ownership;
pub mod path;
pub mod plan_id;
pub mod plan_release;
pub mod ports;
pub(crate) mod release_evidence;
pub mod repository_identity;
pub mod rewire;
pub mod scheduling;
pub mod status;
pub mod validation;
pub mod verification_receipt;
