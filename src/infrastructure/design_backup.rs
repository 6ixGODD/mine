//! Safe design-tree backup (`mine design backup`).
//!
//! Implements the backup contract from
//! `docs/design/governance/design-knowledge-base.md` ("Local backup
//! convention") and `docs/design/interfaces/cli-contract.md` ("Design
//! backup"):
//!
//! - validates the design marker and repository ownership;
//! - creates `docs/design-backup-<UTC timestamp>/`;
//! - copies managed design **without following external links** (links must
//!   not dereference outside the repository);
//! - writes `*` to the backup root `.gitignore`;
//! - verifies copy completion, refusing to mutate the source design on any
//!   failure;
//! - emits a structured manifest and the backup path;
//! - performs no design mutation.
//!
//! Safety guarantees:
//! - The deterministic UTC timestamp path is the only backup destination; no
//!   user-controlled path.
//! - Copying refuses to traverse entries that leave the repository root.
//! - The backup root `.gitignore` (`*`) prevents the backup from ever being
//!   tracked or staged for release.
//! - `mine design backup` performs no `git` mutation; it only writes files.

use std::path::{Path, PathBuf};

use super::graph_snapshot::resolve_contained;
use crate::domain::config::{GovernanceLayout, MineConfig};
use crate::domain::design_marker::DesignMarker;
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::RepoPath;
use crate::domain::ports::Clock;

/// Backup directory prefix under the repository root.
pub const BACKUP_PREFIX: &str = ".mine/runtime/design-backups/";

/// The `.gitignore` written into every backup root so backups are never
/// tracked or staged for release.
pub const BACKUP_GITIGNORE: &str = "*\n";

/// A completed backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupOutcome {
    /// Repository root.
    pub repository_root: PathBuf,
    /// Absolute backup directory path.
    pub backup_path: PathBuf,
    /// Repository-relative backup directory path.
    pub backup_path_relative: String,
    /// UTC timestamp used in the directory name (RFC3339-ish `YYYYMMDDTHHMMSSZ`).
    pub timestamp: String,
    /// Number of files copied (excluding the `.gitignore`).
    pub file_count: usize,
    /// Byte size of the managed design tree that was copied.
    pub total_bytes: u64,
}

/// The design-backup service. Constructed with a [`Clock`] for deterministic
/// UTC timestamps (tests inject a fixed clock).
pub struct DesignBackup<'a> {
    clock: &'a dyn Clock,
}

impl<'a> DesignBackup<'a> {
    /// Creates the backup service with a clock source.
    #[must_use]
    pub fn new(clock: &'a dyn Clock) -> Self {
        Self { clock }
    }

    /// Runs a design backup at `repo_root`.
    ///
    /// # Errors
    /// - [`MineError::DesignNamespaceConflict`] / [`MineError::DesignOwnershipMismatch`]
    ///   if the design root is not MINE-managed.
    /// - [`MineError::Io`] on any filesystem/copy failure. On failure the
    ///   source design is left untouched; a partially-created backup
    ///   directory is best-effort removed.
    pub fn backup(&self, repo_root: &Path, config: &MineConfig) -> MineResult<BackupOutcome> {
        // Validate ownership/marker before touching anything.
        let _marker = self.validate_marker(repo_root, config)?;
        let layout = config.layout()?;
        let design_root = resolve_contained(repo_root, layout.design_dir())?;
        let timestamp = self.compact_utc_timestamp();
        let backup_dir_name = format!("{BACKUP_PREFIX}{timestamp}");
        let relative =
            RepoPath::new(backup_dir_name.clone()).map_err(|_| MineError::GraphInvalid {
                detail: "backup clock produced an unsafe destination".into(),
            })?;
        let backup_path = resolve_contained(repo_root, &relative)?;

        // Copy the managed design tree without following external links. The
        // copy happens before writing the .gitignore so an early failure
        // leaves a removable partial directory rather than a stub.
        let backup_root = RepoPath::new(GovernanceLayout::BACKUP_DIR).expect("constant path");
        std::fs::create_dir_all(resolve_contained(repo_root, &backup_root)?)?;
        // A collision is not ours to overwrite or remove on copy failure.
        std::fs::create_dir(&backup_path)?;
        let (file_count, total_bytes) =
            match self.copy_tree_repo_bound(&design_root, &backup_path, repo_root) {
                Ok(stats) => stats,
                Err(e) => {
                    // Best-effort cleanup of the partial backup.
                    let _ = std::fs::remove_dir_all(&backup_path);
                    return Err(e);
                }
            };

        // Write the .gitignore that prevents the backup from being tracked.
        std::fs::write(backup_path.join(".gitignore"), BACKUP_GITIGNORE)?;

        Ok(BackupOutcome {
            repository_root: repo_root.to_path_buf(),
            backup_path: backup_path.clone(),
            backup_path_relative: backup_dir_name,
            timestamp,
            file_count,
            total_bytes,
        })
    }

    /// Loads and validates the design marker against the configured repository
    /// id. Marker must be MINE-managed and belong to this repository.
    fn validate_marker(&self, repo_root: &Path, config: &MineConfig) -> MineResult<DesignMarker> {
        let layout = config.layout()?;
        let marker_path = resolve_contained(repo_root, layout.design_marker())?;
        // An existing design root without a marker is an unmanaged namespace.
        // `mine init` resolves this by backing it up and creating a managed
        // root; this backup/validation path (used by mine-sync and design
        // backup) refuses to operate on a tree `mine init` has not claimed
        // (stable error, not an opaque I/O failure).
        let design_dir = resolve_contained(repo_root, layout.design_dir())?;
        if design_dir.exists() && !marker_path.exists() {
            return Err(MineError::DesignNamespaceConflict {
                path: design_dir.to_path_buf(),
            });
        }
        let content = std::fs::read_to_string(&marker_path)?;
        let marker = DesignMarker::parse(&marker_path, &content)?;
        crate::domain::design_marker::classify(
            &design_dir,
            &marker_path,
            true,
            true,
            Some(&marker),
            Some(&config.repository_id),
        )?;
        Ok(marker)
    }

    /// Compacts the clock's UTC RFC3339 timestamp into
    /// `YYYYMMDDTHHMMSSZ`, the deterministic backup directory suffix.
    fn compact_utc_timestamp(&self) -> String {
        // clock.now_utc_rfc3339() returns e.g. "2026-07-23T19:38:42Z"; compact
        // to "20260723T193842Z". Fall back to the raw value if it does not
        // match the expected shape.
        let raw = self.clock.now_utc_rfc3339();
        let compact: String = raw
            .chars()
            .filter(|c| c.is_ascii_digit() || matches!(c, 'T' | 'Z'))
            .collect();
        if compact.len() >= 16 && compact.contains('T') {
            compact
        } else {
            raw
        }
    }

    /// Copies `src` (the managed design dir) into `dst`, refusing to follow
    /// any link that escapes `repo_root`. Returns `(file_count, total_bytes)`.
    fn copy_tree_repo_bound(
        &self,
        src: &Path,
        dst: &Path,
        repo_root: &Path,
    ) -> MineResult<(usize, u64)> {
        let mut file_count = 0usize;
        let mut total_bytes = 0u64;
        self.copy_entry(src, dst, repo_root, &mut file_count, &mut total_bytes)?;
        Ok((file_count, total_bytes))
    }

    fn copy_entry(
        &self,
        src: &Path,
        dst: &Path,
        repo_root: &Path,
        file_count: &mut usize,
        total_bytes: &mut u64,
    ) -> MineResult<()> {
        let meta = std::fs::symlink_metadata(src)?;
        let canonical = src.canonicalize()?;
        if !canonical.starts_with(repo_root.canonicalize()?) {
            return Err(MineError::Io(std::io::Error::other(
                "design backup link escapes Root",
            )));
        }
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::MetadataExt;
            meta.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let linked = meta.file_type().is_symlink();
        if linked && canonical.is_dir() {
            return Err(MineError::Io(std::io::Error::other(
                "design backup refuses directory links/cycles",
            )));
        }
        if meta.is_dir() {
            std::fs::create_dir_all(dst)?;
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                let child_src = entry.path();
                let child_name = entry.file_name();
                let child_dst = dst.join(&child_name);
                self.copy_entry(&child_src, &child_dst, repo_root, file_count, total_bytes)?;
            }
        } else if meta.is_file() {
            let bytes = std::fs::read(src)?;
            *total_bytes += bytes.len() as u64;
            std::fs::write(dst, &bytes)?;
            if std::fs::read(dst)? != bytes || std::fs::read(src)? != bytes {
                return Err(MineError::Io(std::io::Error::other(
                    "design backup byte verification failed",
                )));
            }
            *file_count += 1;
        } else {
            // Symlink (or other special): refuse to follow if it escapes the
            // repository. Resolve the link target and require it to stay
            // inside repo_root.
            let target = std::fs::read_link(src).map_err(MineError::from)?;
            let resolved = if target.is_absolute() {
                target
            } else {
                src.parent()
                    .ok_or_else(|| MineError::Io(std::io::Error::other("link without parent")))?
                    .join(target)
            };
            let canonical_repo = repo_root
                .canonicalize()
                .unwrap_or_else(|_| repo_root.to_path_buf());
            let canonical_target = resolved.canonicalize().map_err(|e| {
                MineError::Io(std::io::Error::other(format!(
                    "link target {} not resolvable: {e}",
                    resolved.display()
                )))
            })?;
            if !canonical_target.starts_with(&canonical_repo) {
                return Err(MineError::Io(std::io::Error::other(format!(
                    "refusing to follow link {} that escapes the repository root",
                    src.display()
                ))));
            }
            // Copy the link target as a real file (content copy), rather than
            // duplicating the link, so no external reference enters the backup.
            let bytes = std::fs::read(&canonical_target)?;
            *total_bytes += bytes.len() as u64;
            std::fs::write(dst, &bytes)?;
            if std::fs::read(dst)? != bytes || std::fs::read(&canonical_target)? != bytes {
                return Err(MineError::Io(std::io::Error::other(
                    "design backup byte verification failed",
                )));
            }
            *file_count += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::Clock;

    struct FixedClock;
    impl Clock for FixedClock {
        fn now_utc_rfc3339(&self) -> String {
            "2026-07-23T19:38:42Z".to_string()
        }
    }

    fn write_config(_root: &Path, repo_id: &str) -> MineConfig {
        use crate::domain::config::*;

        MineConfig {
            schema_version: 1,
            repository_id: repo_id.to_string(),
            mine_code_version: "0.1.0".to_string(),
            branches: BranchesConfig {
                stable: "master".to_string(),
                integration: "dev".to_string(),
            },
            design: DesignConfig {
                root: ".mine/design/index.md".to_string(),
                marker: ".mine/design/.mine-design.toml".to_string(),
                language: "en".to_string(),
                index_soft_limit_lines: 250,
                leaf_soft_limit_lines: 400,
            },
            plan: PlanConfig {
                root: ".mine/plan".to_string(),
                ephemeral: true,
                purge_before_stable_release: true,
                worktrees_root: ".mine/worktrees".to_string(),
                shared_environment_root: ".".to_string(),
            },
            graph: GraphConfig {
                source: ".mine/plan/execution-graph.toml".to_string(),
                rendered: ".mine/plan/execution-graph.md".to_string(),
                lock_timeout_ms: 5000,
            },
            adr: crate::domain::config::AdrConfig::default(),
        }
    }

    fn seed_managed_design(root: &Path, repo_id: &str) {
        let design = root.join(".mine").join("design");
        std::fs::create_dir_all(&design).unwrap();
        let marker = DesignMarker::new(repo_id.to_string(), "2026-07-23T00:00:00Z".to_string());
        std::fs::write(design.join(".mine-design.toml"), marker.to_toml()).unwrap();
        std::fs::write(design.join("index.md"), "# Design\n").unwrap();
        std::fs::create_dir_all(design.join("area")).unwrap();
        std::fs::write(design.join("area").join("index.md"), "# Area\n").unwrap();
        std::fs::write(design.join("area").join("leaf.md"), "# Leaf\n").unwrap();
    }

    #[test]
    fn backup_copies_managed_design_and_writes_gitignore() {
        let root = tempfile::tempdir().unwrap();
        let cfg = write_config(root.path(), "repo-1");
        seed_managed_design(root.path(), "repo-1");

        let outcome = DesignBackup::new(&FixedClock)
            .backup(root.path(), &cfg)
            .unwrap();
        assert_eq!(outcome.timestamp, "20260723T193842Z");
        assert_eq!(
            outcome.backup_path_relative,
            ".mine/runtime/design-backups/20260723T193842Z"
        );
        let backup = &outcome.backup_path;
        // Copied files exist (nested included): 4 files (marker, index, area/index, area/leaf).
        assert!(backup.join(".mine-design.toml").exists());
        assert!(backup.join("index.md").exists());
        assert!(backup.join("area").join("index.md").exists());
        assert!(backup.join("area").join("leaf.md").exists());
        assert_eq!(outcome.file_count, 4);
        assert!(outcome.total_bytes > 0);
        // .gitignore contains '*'.
        assert_eq!(
            std::fs::read_to_string(backup.join(".gitignore")).unwrap(),
            "*\n"
        );
        // Source design unchanged.
        assert!(root.path().join(".mine/design/index.md").exists());
    }

    #[test]
    fn backup_refuses_foreign_marker() {
        let root = tempfile::tempdir().unwrap();
        let cfg = write_config(root.path(), "repo-1");
        seed_managed_design(root.path(), "other-repo-id");
        let err = DesignBackup::new(&FixedClock)
            .backup(root.path(), &cfg)
            .unwrap_err();
        assert_eq!(err.code(), "MINE_DESIGN_OWNERSHIP_MISMATCH");
    }

    #[test]
    fn backup_refuses_unmanaged_namespace() {
        let root = tempfile::tempdir().unwrap();
        let cfg = write_config(root.path(), "repo-1");
        // Create design dir without marker.
        let design = root.path().join(".mine").join("design");
        std::fs::create_dir_all(&design).unwrap();
        std::fs::write(design.join("index.md"), "x").unwrap();
        let err = DesignBackup::new(&FixedClock)
            .backup(root.path(), &cfg)
            .unwrap_err();
        // No marker => parse fails with DesignMarkerInvalid (missing file) or
        // namespace conflict; the contract requires a stable error either way.
        let code = err.code();
        assert!(
            code == "MINE_DESIGN_MARKER_INVALID" || code == "MINE_DESIGN_NAMESPACE_CONFLICT",
            "got {code}"
        );
    }

    #[test]
    fn backup_refuses_external_symlink() {
        let root = tempfile::tempdir().unwrap();
        let cfg = write_config(root.path(), "repo-1");
        seed_managed_design(root.path(), "repo-1");
        // Drop an external target outside the repo and link to it from the design dir.
        let external = tempfile::tempdir().unwrap();
        std::fs::write(external.path().join("secret.txt"), "external secret").unwrap();
        let link = root.path().join(".mine").join("design").join("escape.link");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(external.path().join("secret.txt"), &link).unwrap();
        }
        #[cfg(windows)]
        {
            // Windows file symlinks require admin or developer mode; create a
            // directory junction to an external dir as a portable escape test.
            // If junction creation is not permitted, skip this assertion
            // rather than fail the build.
            let dst_dir = external.path().join("extdir");
            std::fs::create_dir_all(&dst_dir).unwrap();
            let _ = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&dst_dir)
                .status();
            if !link.exists() {
                return;
            }
        }
        let err = DesignBackup::new(&FixedClock)
            .backup(root.path(), &cfg)
            .unwrap_err();
        assert_eq!(err.code(), "MINE_IO");
        // Partial backup must have been cleaned up on failure.
        assert!(
            !root
                .path()
                .join(".mine/runtime/design-backups/20260723T193842Z")
                .exists(),
            "partial backup must be removed on failure"
        );
    }
}
