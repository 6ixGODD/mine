//! Contained, append-only atomic evidence storage and exact owned-output cleanup.

mod filesystem;
pub use filesystem::{FileVerificationStore, OutputLease};

use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{PlanIdentity, RepoPath};
pub use crate::domain::verification_receipt::{ReceiptId, VerificationReceipt};
use crate::infrastructure::graph_snapshot::resolve_contained;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationLocation {
    root: PathBuf,
    directory: RepoPath,
    receipt: RepoPath,
}
impl VerificationLocation {
    pub fn new(root: &Path, plan: &PlanIdentity, receipt_id: &ReceiptId) -> MineResult<Self> {
        let directory = plan.verification_directory();
        let receipt = RepoPath::new(format!(
            "{}/{}.json",
            directory.as_str(),
            receipt_id.as_str()
        ))
        .expect("validated identities");
        resolve_contained(root, &receipt)?;
        Ok(Self {
            root: root.canonicalize()?,
            directory,
            receipt,
        })
    }
    pub fn relative_path(&self) -> &RepoPath {
        &self.receipt
    }
    pub fn resolve(&self) -> MineResult<PathBuf> {
        resolve_contained(&self.root, &self.receipt)
    }
    pub fn artifact(&self, path: &RepoPath) -> MineResult<PathBuf> {
        if path == &self.directory
            || path == &self.receipt
            || !crate::domain::path::is_within(self.directory.as_str(), path.as_str())
        {
            return Err(MineError::GraphInvalid {
                detail:
                    "artifact must be a distinct file inside the exact Plan verification directory"
                        .into(),
            });
        }
        resolve_contained(&self.root, path)
    }
}
