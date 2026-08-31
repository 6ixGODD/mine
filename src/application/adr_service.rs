//! Shared ADR registry and scoped Design-readiness application service.

use std::path::Path;

use crate::application::design_service::DesignService;
use crate::domain::adr::{AdrBlocker, AdrEntry, AdrRegistry, AdrStatus, sha256_hex};
use crate::domain::config::MineConfig;
use crate::domain::error::{MineError, MineResult};
use crate::infrastructure::adr_store::{self, AdrStore};

/// Combined registry + Design readiness result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DesignReadiness {
    pub ready: bool,
    pub scope: Option<String>,
    pub registry_revision: u64,
    pub design_valid: bool,
    pub blockers: Vec<AdrBlocker>,
    pub warnings: Vec<String>,
}

/// Per-scope readiness entry previewed by `mine adr supersede --dry-run`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SupersedePreviewScope {
    /// Requested scope; `None` means repository-wide.
    pub scope: Option<String>,
    pub ready: bool,
    pub blockers: Vec<AdrBlocker>,
}

/// Result of `mine adr supersede --dry-run`: post-supersession readiness for
/// every affected scope, computed in memory without acquiring the write lock
/// or writing the registry or generated index.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SupersedePreview {
    pub id: String,
    pub replacement: String,
    pub registry_revision: u64,
    pub scopes: Vec<SupersedePreviewScope>,
}

/// Result of `mine design render`: the regenerated generated-index path and
/// whether its bytes changed. The registry and its revision are untouched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DesignIndexRender {
    pub path: String,
    pub changed: bool,
}

/// Request for a DRAFT ADR registry entry.
#[derive(Debug, Clone)]
pub struct AdrAddRequest {
    pub id: String,
    pub path: String,
    pub title: String,
    pub planning_blocking: bool,
    pub planning_scopes: Vec<String>,
}

pub struct AdrService<'a> {
    repo_root: &'a Path,
    config: &'a MineConfig,
}

impl<'a> AdrService<'a> {
    #[must_use]
    pub fn new(repo_root: &'a Path, config: &'a MineConfig) -> Self {
        Self { repo_root, config }
    }

    fn store(&self) -> AdrStore {
        AdrStore::new(self.repo_root, &self.config.adr)
    }

    pub fn bootstrap(&self, now: &str) -> MineResult<AdrRegistry> {
        self.store().bootstrap_if_absent(now)
    }

    pub fn show(&self, id: &str) -> MineResult<AdrEntry> {
        self.store()
            .load()?
            .get(id)
            .cloned()
            .ok_or_else(|| MineError::AdrNotFound {
                adr_id: id.to_string(),
            })
    }

    pub fn list(&self) -> MineResult<Vec<AdrEntry>> {
        Ok(self.store().load()?.entries)
    }

    /// Validates the registry and then checks the generated decision index
    /// against a fresh deterministic render. A stale or missing index is a
    /// validation failure that names `mine design render` as the
    /// non-destructive repair; validation itself never rewrites files.
    pub fn validate(&self) -> MineResult<AdrRegistry> {
        let store = self.store();
        let registry = store.load()?;
        let expected = adr_store::render_index(&registry)?;
        let index_path = store.index_path();
        let stale = match std::fs::read(index_path) {
            Ok(bytes) => bytes != expected.as_bytes(),
            Err(_) => true,
        };
        if stale {
            return Err(MineError::AdrRegistryInvalid {
                path: index_path.to_path_buf(),
                detail: "generated index is stale; run `mine design render` to repair".to_string(),
            });
        }
        Ok(registry)
    }

    /// Previews a supersession without writing: clones the loaded registry in
    /// memory, applies the same validation as a real supersede, and reports
    /// post-change readiness for every scope the superseded entry applies to
    /// (its own scopes, or every registry scope plus the repository-wide check
    /// when the entry is repository-wide).
    pub fn supersede_preview(&self, id: &str, replacement: &str) -> MineResult<SupersedePreview> {
        let store = self.store();
        let registry = store.load()?;
        let old = registry
            .get(id)
            .cloned()
            .ok_or_else(|| MineError::AdrNotFound {
                adr_id: id.to_string(),
            })?;
        let mut projected = registry.clone();
        projected.supersede(id, replacement, "preview")?;
        let scopes: Vec<Option<String>> = if old.planning_scopes.is_empty() {
            let mut all: Vec<String> = registry
                .entries
                .iter()
                .flat_map(|entry| entry.planning_scopes.iter().cloned())
                .collect();
            all.sort();
            all.dedup();
            all.into_iter()
                .map(Some)
                .chain(std::iter::once(None))
                .collect()
        } else {
            old.planning_scopes
                .iter()
                .map(|s| Some(s.clone()))
                .collect()
        };
        let scopes = scopes
            .into_iter()
            .map(|scope| {
                let blockers = projected.readiness_blockers(scope.as_deref());
                SupersedePreviewScope {
                    ready: blockers.is_empty(),
                    blockers,
                    scope,
                }
            })
            .collect();
        Ok(SupersedePreview {
            id: id.to_string(),
            replacement: replacement.to_string(),
            registry_revision: registry.revision,
            scopes,
        })
    }

    /// Deterministically regenerates the generated decision index from the
    /// registry (`mine design render`). The registry and its revision are
    /// never touched; the index is repaired whether it is stale, corrupted,
    /// or missing.
    pub fn render_design_index(&self) -> MineResult<DesignIndexRender> {
        let store = self.store();
        let registry = store.load()?;
        let rendered = adr_store::render_index(&registry)?;
        let index_path = store.index_path();
        let changed = match std::fs::read(index_path) {
            Ok(bytes) => bytes != rendered.as_bytes(),
            Err(_) => true,
        };
        crate::infrastructure::atomic_write::write(index_path, rendered.as_bytes())?;
        Ok(DesignIndexRender {
            path: self.config.adr.rendered_index.clone(),
            changed,
        })
    }

    pub fn add(&self, req: AdrAddRequest) -> MineResult<AdrRegistry> {
        let store = self.store();
        let expected = store.load()?.revision;
        store.save_with_revision(expected, move |mut registry| {
            registry.add(AdrEntry {
                id: req.id,
                path: req.path,
                title: req.title,
                status: AdrStatus::DRAFT,
                planning_blocking: req.planning_blocking,
                planning_scopes: req.planning_scopes,
                content_sha256: String::new(),
                proposed_at: String::new(),
                approved_by: String::new(),
                approved_at: String::new(),
                approval_note: String::new(),
                supersedes: vec![],
                superseded_by: String::new(),
                rejection_reason: String::new(),
                updated_at: String::new(),
            })?;
            registry.revision = expected + 1;
            Ok(registry)
        })
    }

    pub fn propose(&self, id: &str, now: &str) -> MineResult<AdrRegistry> {
        let store = self.store();
        let expected = store.load()?.revision;
        let entry = store
            .load()?
            .get(id)
            .cloned()
            .ok_or_else(|| MineError::AdrNotFound {
                adr_id: id.to_string(),
            })?;
        let digest = sha256_hex(&std::fs::read(self.repo_root.join(&entry.path))?);
        store.save_with_revision(expected, move |mut registry| {
            registry.propose(id, digest, now)?;
            registry.revision = expected + 1;
            Ok(registry)
        })
    }

    pub fn approve(&self, id: &str, by: &str, note: &str, now: &str) -> MineResult<AdrRegistry> {
        let store = self.store();
        let expected = store.load()?.revision;
        let entry = store
            .load()?
            .get(id)
            .cloned()
            .ok_or_else(|| MineError::AdrNotFound {
                adr_id: id.to_string(),
            })?;
        let digest = sha256_hex(&std::fs::read(self.repo_root.join(&entry.path))?);
        store.save_with_revision(expected, move |mut registry| {
            registry.approve(id, &digest, by, note, now)?;
            registry.revision = expected + 1;
            Ok(registry)
        })
    }

    pub fn reject(&self, id: &str, reason: &str, now: &str) -> MineResult<AdrRegistry> {
        let store = self.store();
        let expected = store.load()?.revision;
        store.save_with_revision(expected, move |mut registry| {
            registry.reject(id, reason, now)?;
            registry.revision = expected + 1;
            Ok(registry)
        })
    }

    pub fn supersede(&self, id: &str, replacement: &str, now: &str) -> MineResult<AdrRegistry> {
        let store = self.store();
        let expected = store.load()?.revision;
        store.save_with_revision(expected, move |mut registry| {
            registry.supersede(id, replacement, now)?;
            registry.revision = expected + 1;
            Ok(registry)
        })
    }

    pub fn readiness(&self, scope: Option<&str>) -> MineResult<DesignReadiness> {
        let registry = self.store().load()?;
        let design = DesignService::validate(self.repo_root, self.config)?;
        let blockers = registry.readiness_blockers(scope);
        let warnings = design
            .warnings
            .iter()
            .map(|warning| format!("{}: {}", warning.code, warning.message))
            .collect::<Vec<_>>();
        Ok(DesignReadiness {
            ready: design.valid && blockers.is_empty(),
            scope: scope.map(str::to_string),
            registry_revision: registry.revision,
            design_valid: design.valid,
            blockers,
            warnings,
        })
    }

    /// Enforces planning readiness for an adapter that is registering or
    /// releasing a Plan. Repositories that predate ADR bootstrap are left to
    /// `mine init`; initialized repositories are fail-closed.
    pub fn require_planning_ready(&self, scope: &str) -> MineResult<()> {
        if !self.repo_root.join(&self.config.adr.registry).exists() {
            return Ok(());
        }
        let result = self.readiness(Some(scope))?;
        if result.ready {
            return Ok(());
        }
        let blockers = result
            .blockers
            .iter()
            .map(|blocker| format!("{} ({})", blocker.id, blocker.reason))
            .collect::<Vec<_>>()
            .join(", ");
        Err(MineError::GraphInvalid {
            detail: format!("planning readiness blocked for {scope}: {blockers}"),
        })
    }
}
