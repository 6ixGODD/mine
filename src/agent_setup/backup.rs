// Enforce `AGENTS.md`'s "Business code must not use `unsafe`" at compile time.
#![forbid(unsafe_code)]

//! Mandatory configuration backup before any Agent config mutation — Fix 1
//! Transactional agent-setup backup: mandatory exact-byte configuration backup before mutation.
//!
//! Every structured external configuration file is backed up before its first
//! mutation by an install or update:
//!
//! - back up the **exact original bytes** (byte-for-byte copy, not a
//!   parsed/reserialized representation);
//! - create the backup **before** replacing or rewriting the configuration;
//! - never overwrite an existing backup silently (a deterministic MINE-owned
//!   backup location; a repeated install verifies/reuses the existing backup
//!   rather than clobbering it);
//! - verify the backup can be read and matches the original bytes;
//! - if backup creation or verification fails, perform **no** external
//!   mutation (the caller treats `BackupError` as a hard stop).
//!
//! The backup lives at `<root>/.mine/agent-backups/<config_rel>` (with slashes
//! turned to `__` in the filename), a deterministic MINE-owned location
//! recorded in the installation transaction/managed state. No secrets are
//! stored (only a content hash of the original bytes, never the bytes in logs).

use std::path::{Path, PathBuf};

use crate::agent_setup::safety::{SafetyGuard, content_hash};
use crate::domain::error::{MineError, MineResult};
use crate::domain::ports::Clock;

/// The MINE-owned backup directory under the configuration root.
pub const BACKUP_SUBDIR: &str = ".mine/agent-backups";

/// A verified backup: the original config path, the backup path, and a hash of
/// the original bytes (for restore-time verification).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Backup {
    /// Path relative to the config root (forward slashes) of the original.
    pub original_rel: String,
    /// Absolute backup path (MINE-owned, deterministic).
    pub backup_path: PathBuf,
    /// SHA-style hash of the original bytes (drift evidence; never logs bytes).
    pub original_hash: String,
}

/// Creates (or verifies/reuses) an exact-byte backup of `original` before
/// mutation. If `original` does not exist, returns `Ok(None)` (no backup
/// needed for a fresh config). All backup writes go through `guard` so they
/// stay inside the configuration root.
///
/// ### Never clobber
/// If a backup already exists at the deterministic path, its bytes are
/// verified to match the current `original` bytes; if they match, the existing
/// backup is reused (idempotent). If they differ, the existing backup is
/// preserved (never silently overwritten) and the operation records the
/// current bytes to a **new** timestamped backup path so the original backup
/// is not lost.
pub fn backup_before_mutation(
    original: &Path,
    config_root: &Path,
    guard: &SafetyGuard,
) -> MineResult<Option<Backup>> {
    if !original.exists() {
        return Ok(None);
    }
    let original_bytes = std::fs::read(original).map_err(MineError::Io)?;
    let original_rel = rel(original, config_root);
    let backup_dir = config_root.join(BACKUP_SUBDIR);
    let backup_name = backup_name_for(&original_rel);
    let backup_path = guard.ensure_within_root(&backup_dir.join(&backup_name))?;
    std::fs::create_dir_all(backup_path.parent().unwrap()).map_err(MineError::Io)?;

    // Never clobber: verify/reuse if the existing backup matches the original.
    if backup_path.exists() {
        let existing = std::fs::read(&backup_path).map_err(MineError::Io)?;
        if existing == original_bytes {
            return Ok(Some(Backup {
                original_rel,
                backup_path,
                original_hash: content_hash(&original_bytes),
            }));
        }
        // Existing backup differs (a prior install backed up a different
        // original): preserve it by writing to a timestamped sibling instead.
        let ts = crate::infrastructure::system::SystemClock
            .now_utc_rfc3339()
            .replace([':', '.', '-', 'T', 'Z'], "");
        let ts_name = format!("{backup_name}.{ts}");
        let ts_path = guard.ensure_within_root(&backup_dir.join(&ts_name))?;
        std::fs::write(&ts_path, &original_bytes).map_err(MineError::Io)?;
        verify_bytes(&original_bytes, &ts_path)?;
        return Ok(Some(Backup {
            original_rel,
            backup_path: ts_path,
            original_hash: content_hash(&original_bytes),
        }));
    }

    // Fresh backup: exact-byte copy, then verify.
    std::fs::write(&backup_path, &original_bytes).map_err(MineError::Io)?;
    verify_bytes(&original_bytes, &backup_path)?;
    Ok(Some(Backup {
        original_rel,
        backup_path,
        original_hash: content_hash(&original_bytes),
    }))
}

/// Verifies the backup bytes can be read and match the original bytes; raises
/// `MINE_AGENT_BACKUP_FAILED` on any mismatch.
fn verify_bytes(original_bytes: &[u8], backup_path: &Path) -> MineResult<()> {
    let backup_bytes = std::fs::read(backup_path).map_err(|e| MineError::AgentBackupFailed {
        target: backup_path.to_path_buf(),
        detail: format!("backup read failed: {e}"),
    })?;
    if backup_bytes != original_bytes {
        return Err(MineError::AgentBackupFailed {
            target: backup_path.to_path_buf(),
            detail: "backup bytes do not match the original after write".to_string(),
        });
    }
    Ok(())
}

/// Restores `config_root`'s structured config from a verified [`Backup`].
/// Used by the transaction rollback path. The restore is verified to match
/// the backup's recorded original hash.
pub fn restore_from_backup(
    backup: &Backup,
    config_root: &Path,
    guard: &SafetyGuard,
) -> MineResult<()> {
    let backup_bytes =
        std::fs::read(&backup.backup_path).map_err(|e| MineError::AgentBackupFailed {
            target: backup.backup_path.clone(),
            detail: format!("restore: backup read failed: {e}"),
        })?;
    if content_hash(&backup_bytes) != backup.original_hash {
        return Err(MineError::AgentBackupFailed {
            target: backup.backup_path.clone(),
            detail: "restore: backup content hash mismatch (backup drifted)".to_string(),
        });
    }
    let original = guard.ensure_within_root(&config_root.join(&backup.original_rel))?;
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent).map_err(MineError::Io)?;
    }
    std::fs::write(&original, &backup_bytes).map_err(|e| MineError::AgentBackupFailed {
        target: original.clone(),
        detail: format!("restore: write failed: {e}"),
    })?;
    Ok(())
}

/// Maps a relative config path to a backup filename (slashes → `__`).
fn backup_name_for(rel: &str) -> String {
    format!("{}.bak", rel.replace(['/', '\\'], "__"))
}

/// Path relative to `root` as a forward-slash string.
fn rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn guard(tmp: &TempDir) -> SafetyGuard {
        SafetyGuard::new(tmp.path())
    }

    #[test]
    fn backup_is_exact_bytes_and_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let g = guard(&tmp);
        let cfg = tmp.path().join(".codex/config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        let original = b"# a comment\n[mcp_servers]\nexisting = true\n";
        std::fs::write(&cfg, original).unwrap();
        let b = backup_before_mutation(&cfg, tmp.path(), &g)
            .unwrap()
            .unwrap();
        assert_eq!(
            std::fs::read(&b.backup_path).unwrap(),
            original,
            "exact bytes"
        );
        assert_eq!(b.original_hash, content_hash(original));
    }

    #[test]
    fn backup_returns_none_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let g = guard(&tmp);
        let cfg = tmp.path().join(".codex/config.toml");
        assert!(
            backup_before_mutation(&cfg, tmp.path(), &g)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn backup_reuses_matching_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let g = guard(&tmp);
        let cfg = tmp.path().join(".claude.json");
        let original = b"{\"x\":1}";
        std::fs::write(&cfg, original).unwrap();
        let b1 = backup_before_mutation(&cfg, tmp.path(), &g)
            .unwrap()
            .unwrap();
        let b2 = backup_before_mutation(&cfg, tmp.path(), &g)
            .unwrap()
            .unwrap();
        assert_eq!(
            b1.backup_path, b2.backup_path,
            "idempotent reuse, not clobbered"
        );
    }

    #[test]
    fn backup_preserves_existing_when_original_changed() {
        let tmp = tempfile::tempdir().unwrap();
        let g = guard(&tmp);
        let cfg = tmp.path().join(".claude.json");
        std::fs::write(&cfg, b"v1").unwrap();
        let b1 = backup_before_mutation(&cfg, tmp.path(), &g)
            .unwrap()
            .unwrap();
        std::fs::write(&cfg, b"v2-different").unwrap();
        let b2 = backup_before_mutation(&cfg, tmp.path(), &g)
            .unwrap()
            .unwrap();
        assert_ne!(
            b1.backup_path, b2.backup_path,
            "prior backup preserved, new one written"
        );
        assert_eq!(
            std::fs::read(&b1.backup_path).unwrap(),
            b"v1",
            "prior backup intact"
        );
    }

    #[test]
    fn restore_restores_exact_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let g = guard(&tmp);
        let cfg = tmp.path().join(".codex/config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        let original = b"# comment\n[t]\nx = 1\n";
        std::fs::write(&cfg, original).unwrap();
        let b = backup_before_mutation(&cfg, tmp.path(), &g)
            .unwrap()
            .unwrap();
        // Mutate then restore.
        std::fs::write(&cfg, b"# DESTROYED\n").unwrap();
        restore_from_backup(&b, tmp.path(), &g).unwrap();
        assert_eq!(std::fs::read(&cfg).unwrap(), original, "exact-byte restore");
    }
}
