//! Request-local validated graph and Git observations. Never a persistent grant.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::domain::error::MineResult;
use crate::domain::graph::PlanWorkspace;
use crate::infrastructure::git::WorktreeInfo;
use crate::infrastructure::git_evidence::{GitDiagnostics, GitRequest, RefInfo};

/// Identity and evidence acquired once for one application request.
pub struct GraphSnapshot {
    operation_id: String,
    workspace: PlanWorkspace,
    pub source_sha256: String,
    pub repository_root: PathBuf,
    pub common_directory: Option<PathBuf>,
    pub validation_time: Duration,
    git: RefCell<GitRequest>,
}

#[derive(Debug, Clone)]
pub struct SnapshotDiagnostics {
    pub operation_id: String,
    pub phase: &'static str,
    pub graph_size: usize,
    pub graph_loads: usize,
    pub validation_time: Duration,
    pub git: GitDiagnostics,
}

/// Resolve existing ancestors (including junctions) and a missing tail under
/// Root. A locator is not an IO grant: consumers recheck before each actual IO.
pub fn resolve_contained(
    root: &Path,
    relative: &crate::domain::graph::RepoPath,
) -> MineResult<PathBuf> {
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(crate::domain::error::MineError::GraphInvalid {
            detail: "locator Root is not a directory".into(),
        });
    }
    let mut candidate = root.clone();
    for segment in relative.as_str().split('/') {
        // A missing tail is safe only beneath directories. In particular,
        // Windows can report NotFound for a child of an existing regular file;
        // do not mistake that for permission to create a branch/worktree first.
        match candidate.metadata() {
            Ok(metadata) if !metadata.is_dir() => {
                return Err(crate::domain::error::MineError::GraphInvalid {
                    detail: "locator ancestor is not a directory".into(),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        candidate.push(segment);
        match candidate.symlink_metadata() {
            Ok(_) => {
                candidate = candidate.canonicalize()?;
                if !candidate.starts_with(&root) {
                    return Err(crate::domain::error::MineError::GraphInvalid {
                        detail: "locator escapes Root through a link".into(),
                    });
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(candidate)
}

impl GraphSnapshot {
    pub(crate) fn new(
        workspace: PlanWorkspace,
        source: &[u8],
        root: &Path,
        mut git: GitRequest,
        validation_time: Duration,
    ) -> MineResult<Self> {
        let common_directory = if root.join(".git").exists() {
            Some(
                PathBuf::from(git.text(&[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-common-dir",
                ])?)
                .canonicalize()?,
            )
        } else {
            None
        };
        Ok(Self {
            operation_id: uuid::Uuid::new_v4().to_string(),
            workspace,
            source_sha256: format!("{:x}", Sha256::digest(source)),
            repository_root: root.to_path_buf(),
            common_directory,
            validation_time,
            git: RefCell::new(git),
        })
    }

    pub fn workspace(&self) -> &PlanWorkspace {
        &self.workspace
    }

    pub fn worktrees(&self) -> MineResult<Vec<WorktreeInfo>> {
        // Non-Git domain/adapter fixtures have no inventory, rather than an
        // ignored child failure. Real managed repositories propagate failures.
        if self.common_directory.is_none() {
            return Ok(Vec::new());
        }
        self.git.borrow_mut().worktrees()
    }

    pub fn refs(&self) -> MineResult<Vec<RefInfo>> {
        self.git.borrow_mut().refs()
    }

    /// Explicit observation boundary; it never extends the request budget.
    pub fn refresh_git(&self) {
        self.git.borrow_mut().refresh_observations();
    }

    pub fn git_diagnostics(&self) -> GitDiagnostics {
        self.git.borrow().diagnostics()
    }

    pub fn diagnostics(&self) -> SnapshotDiagnostics {
        SnapshotDiagnostics {
            operation_id: self.operation_id.clone(),
            phase: "validated_read",
            graph_size: self.workspace.plans.len(),
            graph_loads: 1,
            validation_time: self.validation_time,
            git: self.git_diagnostics(),
        }
    }
}
