//! Concrete adapters over domain ports.
//!
//! Provides the system sources for repository identifiers and time.
//! Execution-graph persistence infrastructure: atomic
//! writes, exclusive file locking (via the vetted `fs4` crate), and the TOML
//! store. Read-only Git evidence and the safe design-backup
//! adapter. Later plans add the design index, repository locator, embedded
//! skills, and the event log.

pub mod adr_store;
pub mod atomic_write;
pub mod design_backup;
pub mod embedded_skills;
pub mod file_lock;
pub mod git;
pub mod git_evidence;
pub mod governance_git;
pub mod graph_checkpoint;
pub mod graph_snapshot;
pub mod operation_store;
pub(crate) mod strict_json;
pub mod system;
pub mod toml_store;
pub mod verification_store;
