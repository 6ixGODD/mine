//! Root-owned journal locations, atomic persistence and borrowed-lock authority.

use crate::domain::error::MineResult;
use crate::domain::graph::RepoPath;
pub use crate::domain::lifecycle_operation::{LifecycleOperation, OperationId};
use crate::infrastructure::graph_snapshot::resolve_contained;
use std::path::{Path, PathBuf};

pub(crate) mod history;
mod journal;
pub use journal::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationLocation {
    root: PathBuf,
    relative: RepoPath,
}
impl OperationLocation {
    pub fn new(root: &Path, id: &OperationId) -> MineResult<Self> {
        let relative = RepoPath::new(format!(".mine/runtime/operations/{}.json", id.as_str()))
            .expect("validated operation ID");
        regular_path(root, &relative)?;
        Ok(Self {
            root: root.canonicalize()?,
            relative,
        })
    }
    pub fn relative_path(&self) -> &RepoPath {
        &self.relative
    }
    pub fn resolve(&self) -> MineResult<PathBuf> {
        regular_path(&self.root, &self.relative)
    }
}

pub(crate) fn regular_path(root: &Path, relative: &RepoPath) -> MineResult<PathBuf> {
    let mut path = root.canonicalize()?;
    for component in Path::new(relative.as_str()).components() {
        path.push(component);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        let linked = metadata.file_type().is_symlink();
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::MetadataExt;
            linked || metadata.file_attributes() & 0x400 != 0
        };
        if linked {
            return Err(crate::domain::error::MineError::GraphInvalid {
                detail: "operation mutation path traverses a link/reparse point".into(),
            });
        }
    }
    resolve_contained(root, relative)
}
