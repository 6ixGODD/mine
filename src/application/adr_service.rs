//! Shared ADR registry and scoped Design-readiness application service.

use std::path::Path;

use crate::application::design_service::DesignService;
use crate::domain::adr::{AdrEntry, AdrRegistry, AdrStatus, sha256_hex};
use crate::domain::config::MineConfig;
use crate::domain::error::{MineError, MineResult};
use crate::infrastructure::adr_store::AdrStore;

/// Readiness blocker returned by CLI and MCP.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AdrBlocker {
    pub id: String,
    pub status: String,
    pub path: String,
    pub reason: String,
}

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

    pub fn validate(&self) -> MineResult<AdrRegistry> {
        self.store().load()
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
        let blockers = registry
            .entries
            .iter()
            .filter(|entry| entry.planning_blocking && entry.applies_to(scope))
            .filter_map(|entry| {
                if entry.status == AdrStatus::APPROVED {
                    None
                } else {
                    Some(AdrBlocker {
                        id: entry.id.clone(),
                        status: entry.status.as_str().to_string(),
                        path: entry.path.clone(),
                        reason: format!("status is {}", entry.status.as_str()),
                    })
                }
            })
            .collect::<Vec<_>>();
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
