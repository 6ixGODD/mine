//! Locked, atomic persistence for the ADR registry and generated index.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::domain::adr::{
    AdrEntry, AdrRegistry, AdrStatus, historical_bootstrap_digest, sha256_hex,
};
use crate::domain::config::AdrConfig;
use crate::domain::error::{MineError, MineResult};

use super::{atomic_write, file_lock};

/// Dedicated ADR registry persistence adapter. Its revision and lock are
/// independent from the execution graph revision and lock.
pub struct AdrStore {
    repo_root: PathBuf,
    registry_path: PathBuf,
    index_path: PathBuf,
    lock_path: PathBuf,
    lock_timeout: Duration,
}

impl AdrStore {
    #[must_use]
    pub fn new(repo_root: &Path, config: &AdrConfig) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            registry_path: repo_root.join(&config.registry),
            index_path: repo_root.join(&config.rendered_index),
            lock_path: repo_root.join(".mine/locks/adr-registry.lock"),
            lock_timeout: Duration::from_millis(u64::from(config.lock_timeout_ms)),
        }
    }

    #[must_use]
    pub fn registry_path(&self) -> &Path {
        &self.registry_path
    }

    #[must_use]
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// Loads and validates the registry and all digests.
    pub fn load(&self) -> MineResult<AdrRegistry> {
        if !self.registry_path.exists() {
            return Err(MineError::AdrRegistryInvalid {
                path: self.registry_path.clone(),
                detail: "registry is absent; run `mine init` for the one-time bootstrap"
                    .to_string(),
            });
        }
        let content = std::fs::read_to_string(&self.registry_path)?;
        let registry: AdrRegistry =
            toml::from_str(&content).map_err(|e| MineError::AdrRegistryInvalid {
                path: self.registry_path.clone(),
                detail: format!("could not parse registry TOML: {e}"),
            })?;
        registry.validate_structure()?;
        for entry in &registry.entries {
            if matches!(entry.status, AdrStatus::PROPOSED | AdrStatus::APPROVED) {
                entry.verify_digest(&self.repo_root)?;
            }
        }
        Ok(registry)
    }

    /// Applies one registry mutation under its dedicated lock and revision.
    pub fn save_with_revision(
        &self,
        expected_revision: u64,
        mutate: impl FnOnce(AdrRegistry) -> MineResult<AdrRegistry>,
    ) -> MineResult<AdrRegistry> {
        let _lock = file_lock::acquire_exclusive(&self.lock_path, self.lock_timeout)?;
        let current = self.load()?;
        if current.revision != expected_revision {
            return Err(MineError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }
        let next = mutate(current)?;
        next.validate_structure()?;
        for entry in &next.entries {
            if matches!(entry.status, AdrStatus::PROPOSED | AdrStatus::APPROVED) {
                entry.verify_digest(&self.repo_root)?;
            }
        }
        let content = toml::to_string(&next).map_err(|e| MineError::AdrRegistryInvalid {
            path: self.registry_path.clone(),
            detail: format!("could not serialize registry: {e}"),
        })?;
        atomic_write::write(&self.registry_path, content.as_bytes())?;
        let index = render_index(&next)?;
        atomic_write::write(&self.index_path, index.as_bytes())?;
        Ok(next)
    }

    /// Imports the nine existing ADR bodies exactly once when the registry is
    /// absent. Repeated calls are idempotent and never alter existing facts.
    pub fn bootstrap_if_absent(&self, now: &str) -> MineResult<AdrRegistry> {
        if self.registry_path.exists() {
            return self.load();
        }
        let _lock = file_lock::acquire_exclusive(&self.lock_path, self.lock_timeout)?;
        if self.registry_path.exists() {
            return self.load();
        }
        let mut registry = AdrRegistry::empty();
        for number in 1..=9 {
            let id = format!("ADR-{number:04}");
            let filename_prefix = format!("{number:04}-");
            let path = std::fs::read_dir(self.repo_root.join("docs/design/decisions"))?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(&filename_prefix) && n.ends_with(".md"))
                })
                .ok_or_else(|| MineError::AdrRegistryInvalid {
                    path: self.registry_path.clone(),
                    detail: format!("bootstrap body missing for {id}"),
                })?;
            let bytes = std::fs::read(&path)?;
            let body = String::from_utf8_lossy(&bytes);
            let title = body
                .lines()
                .find_map(|line| {
                    line.strip_prefix("# ADR-")
                        .and_then(|s| s.split_once(": "))
                        .map(|(_, t)| t.to_string())
                })
                .unwrap_or_else(|| id.clone());
            let path = path
                .strip_prefix(&self.repo_root)
                .map_err(|_| MineError::AdrRegistryInvalid {
                    path: self.registry_path.clone(),
                    detail: "bootstrap body escaped repository root".to_string(),
                })?
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches('/')
                .to_string();
            let historical = historical_bootstrap_digest(&bytes);
            let approval_note = historical.map_or_else(
                || {
                    "Imported from the repository's pre-registry ADR approval evidence."
                        .to_string()
                },
                |digest| {
                    format!(
                        "Imported from the repository's pre-registry ADR approval evidence. Historical proposed-content SHA-256 (non-authoritative audit evidence): `{digest}`."
                    )
                },
            );
            registry.entries.push(AdrEntry {
                id,
                path,
                title,
                status: AdrStatus::APPROVED,
                planning_blocking: true,
                planning_scopes: vec![],
                content_sha256: sha256_hex(&bytes),
                proposed_at: now.to_string(),
                approved_by: "repository-owner".to_string(),
                approved_at: "2026-08-21T00:00:00Z".to_string(),
                approval_note,
                supersedes: vec![],
                superseded_by: String::new(),
                rejection_reason: String::new(),
                updated_at: now.to_string(),
            });
        }
        registry.revision = 1;
        registry.validate_structure()?;
        for entry in &registry.entries {
            entry.verify_digest(&self.repo_root)?;
        }
        atomic_write::write(
            &self.registry_path,
            toml::to_string(&registry)
                .map_err(|e| MineError::AdrRegistryInvalid {
                    path: self.registry_path.clone(),
                    detail: e.to_string(),
                })?
                .as_bytes(),
        )?;
        let index = render_index(&registry)?;
        atomic_write::write(&self.index_path, index.as_bytes())?;
        Ok(registry)
    }
}

/// Deterministic generated decision index.
pub fn render_index(registry: &AdrRegistry) -> MineResult<String> {
    let mut out = String::from("# Architecture Decision Records\n\n");
    out.push_str("> GENERATED VIEW. The registry is the machine fact source.\n\n");
    for entry in &registry.entries {
        out.push_str(&format!(
            "- [{}: {}]({}) — `{}`\n",
            entry.id,
            entry.title,
            entry
                .path
                .strip_prefix("docs/design/decisions/")
                .unwrap_or(&entry.path),
            entry.status.as_str()
        ));
    }
    Ok(out)
}
