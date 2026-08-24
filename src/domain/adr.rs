//! Pure ADR registry model and approval state machine.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::error::{MineError, MineResult};
use crate::domain::path::normalize_repo_relative;

/// ADR lifecycle status. The serialized spelling is the public registry form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdrStatus {
    DRAFT,
    PROPOSED,
    APPROVED,
    REJECTED,
    SUPERSEDED,
}

impl AdrStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DRAFT => "DRAFT",
            Self::PROPOSED => "PROPOSED",
            Self::APPROVED => "APPROVED",
            Self::REJECTED => "REJECTED",
            Self::SUPERSEDED => "SUPERSEDED",
        }
    }

    pub fn validate_transition(self, target: Self, id: &str) -> MineResult<()> {
        let allowed = matches!(
            (self, target),
            (Self::DRAFT, Self::PROPOSED)
                | (Self::PROPOSED, Self::APPROVED)
                | (Self::PROPOSED, Self::REJECTED)
                | (Self::APPROVED, Self::SUPERSEDED)
        );
        if allowed {
            Ok(())
        } else {
            Err(MineError::InvalidTransition {
                plan_id: id.to_string(),
                from: self.as_str().to_string(),
                to: target.as_str().to_string(),
            })
        }
    }
}

/// One machine registry entry for one ADR body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdrEntry {
    pub id: String,
    pub path: String,
    pub title: String,
    pub status: AdrStatus,
    pub planning_blocking: bool,
    #[serde(default)]
    pub planning_scopes: Vec<String>,
    #[serde(default)]
    pub content_sha256: String,
    #[serde(default)]
    pub proposed_at: String,
    #[serde(default)]
    pub approved_by: String,
    #[serde(default)]
    pub approved_at: String,
    #[serde(default)]
    pub approval_note: String,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default)]
    pub superseded_by: String,
    #[serde(default)]
    pub rejection_reason: String,
    #[serde(default)]
    pub updated_at: String,
}

/// The ADR registry fact source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdrRegistry {
    pub schema_version: u32,
    pub revision: u64,
    #[serde(default, rename = "adrs")]
    pub entries: Vec<AdrEntry>,
}

impl AdrRegistry {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            entries: Vec::new(),
        }
    }

    /// Validates registry structure and cross-entry replacement links.
    pub fn validate_structure(&self) -> MineResult<()> {
        let mut ids = std::collections::HashSet::new();
        for entry in &self.entries {
            validate_adr_id(&entry.id)?;
            normalize_repo_relative(&entry.path)?;
            if !entry.path.starts_with("docs/design/decisions/") || !entry.path.ends_with(".md") {
                return Err(registry_error(
                    &entry.id,
                    "body path must be a decision Markdown file",
                ));
            }
            if !ids.insert(entry.id.clone()) {
                return Err(registry_error(&entry.id, "duplicate ADR id"));
            }
            if entry.superseded_by.is_empty() && entry.status == AdrStatus::SUPERSEDED {
                return Err(registry_error(
                    &entry.id,
                    "SUPERSEDED ADR needs a replacement link",
                ));
            }
            if entry.supersedes.iter().any(String::is_empty) {
                return Err(registry_error(
                    &entry.id,
                    "replacement links must not be empty",
                ));
            }
            if matches!(entry.status, AdrStatus::PROPOSED | AdrStatus::APPROVED)
                && entry.content_sha256.is_empty()
            {
                return Err(registry_error(
                    &entry.id,
                    "proposed/approved ADR needs a digest",
                ));
            }
            if entry.status == AdrStatus::APPROVED
                && (entry.approved_by.is_empty() || entry.approved_at.is_empty())
            {
                return Err(registry_error(
                    &entry.id,
                    "approved ADR needs approval evidence",
                ));
            }
        }
        for entry in &self.entries {
            if !entry.superseded_by.is_empty() && self.get(&entry.superseded_by).is_none() {
                return Err(registry_error(
                    &entry.id,
                    "superseded_by ADR does not exist",
                ));
            }
            for id in &entry.supersedes {
                let old = self
                    .get(id)
                    .ok_or_else(|| registry_error(&entry.id, "superseded ADR does not exist"))?;
                if old.superseded_by != entry.id {
                    return Err(registry_error(
                        &entry.id,
                        "supersession links are not reciprocal",
                    ));
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&AdrEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut AdrEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    pub fn add(&mut self, entry: AdrEntry) -> MineResult<()> {
        if self.get(&entry.id).is_some() {
            return Err(registry_error(&entry.id, "ADR id already exists"));
        }
        if entry.status != AdrStatus::DRAFT {
            return Err(registry_error(&entry.id, "new ADRs must start in DRAFT"));
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Marks a DRAFT ADR as a review candidate and records the exact digest.
    pub fn propose(&mut self, id: &str, digest: String, now: &str) -> MineResult<()> {
        let entry = self.entry_mut(id)?;
        entry.status.validate_transition(AdrStatus::PROPOSED, id)?;
        entry.status = AdrStatus::PROPOSED;
        entry.content_sha256 = digest;
        entry.proposed_at = now.to_string();
        entry.updated_at = now.to_string();
        Ok(())
    }

    /// Records explicit owner approval after digest parity has been checked.
    pub fn approve(
        &mut self,
        id: &str,
        digest: &str,
        by: &str,
        note: &str,
        now: &str,
    ) -> MineResult<()> {
        let entry = self.entry_mut(id)?;
        entry.status.validate_transition(AdrStatus::APPROVED, id)?;
        if entry.content_sha256 != digest {
            return Err(MineError::AdrDigestMismatch {
                adr_id: id.to_string(),
                expected: entry.content_sha256.clone(),
                actual: digest.to_string(),
            });
        }
        if by.trim().is_empty() || note.trim().is_empty() {
            return Err(registry_error(
                id,
                "approval requires explicit actor and note",
            ));
        }
        entry.status = AdrStatus::APPROVED;
        entry.approved_by = by.to_string();
        entry.approved_at = now.to_string();
        entry.approval_note = note.to_string();
        entry.updated_at = now.to_string();
        Ok(())
    }

    pub fn reject(&mut self, id: &str, reason: &str, now: &str) -> MineResult<()> {
        let entry = self.entry_mut(id)?;
        entry.status.validate_transition(AdrStatus::REJECTED, id)?;
        if reason.trim().is_empty() {
            return Err(registry_error(id, "rejection requires a reason"));
        }
        entry.status = AdrStatus::REJECTED;
        entry.rejection_reason = reason.to_string();
        entry.updated_at = now.to_string();
        Ok(())
    }

    /// Atomically links an approved replacement to an approved ADR.
    pub fn supersede(&mut self, old_id: &str, replacement_id: &str, now: &str) -> MineResult<()> {
        if old_id == replacement_id {
            return Err(registry_error(old_id, "an ADR cannot supersede itself"));
        }
        let replacement = self
            .get(replacement_id)
            .ok_or_else(|| MineError::AdrNotFound {
                adr_id: replacement_id.to_string(),
            })?;
        if replacement.status != AdrStatus::APPROVED {
            return Err(registry_error(
                replacement_id,
                "replacement must be APPROVED",
            ));
        }
        let old = self.entry_mut(old_id)?;
        old.status
            .validate_transition(AdrStatus::SUPERSEDED, old_id)?;
        old.status = AdrStatus::SUPERSEDED;
        old.superseded_by = replacement_id.to_string();
        old.updated_at = now.to_string();
        let replacement = self.get_mut(replacement_id).expect("checked above");
        if !replacement.supersedes.iter().any(|id| id == old_id) {
            replacement.supersedes.push(old_id.to_string());
        }
        replacement.updated_at = now.to_string();
        Ok(())
    }

    /// Returns blocking entries applicable to the requested scope.
    #[must_use]
    pub fn applicable_blockers(&self, scope: Option<&str>) -> Vec<&AdrEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.planning_blocking && entry.applies_to(scope))
            .filter(|entry| entry.status != AdrStatus::APPROVED)
            .collect()
    }

    /// Returns whether all applicable blocking ADRs are approved.
    #[must_use]
    pub fn is_ready(&self, scope: Option<&str>) -> bool {
        self.applicable_blockers(scope).is_empty()
    }

    fn entry_mut(&mut self, id: &str) -> MineResult<&mut AdrEntry> {
        self.get_mut(id).ok_or_else(|| MineError::AdrNotFound {
            adr_id: id.to_string(),
        })
    }
}

impl AdrEntry {
    #[must_use]
    pub fn applies_to(&self, scope: Option<&str>) -> bool {
        match scope {
            None => true,
            Some(requested) => {
                self.planning_scopes.is_empty()
                    || self.planning_scopes.iter().any(|candidate| {
                        requested == candidate
                            || requested.starts_with(&format!("{candidate}/"))
                            || candidate.starts_with(&format!("{requested}/"))
                    })
            }
        }
    }

    /// Verifies the body bytes against the independently persisted registry
    /// digest. Approval evidence from the pre-registry bootstrap is never
    /// allowed to substitute for the current body digest.
    pub fn verify_digest(&self, repo_root: &Path) -> MineResult<()> {
        let body_path = repo_root.join(&self.path);
        let bytes = std::fs::read(&body_path)?;
        let actual = sha256_hex(&bytes);
        if actual == self.content_sha256 {
            Ok(())
        } else {
            Err(MineError::AdrDigestMismatch {
                adr_id: self.id.clone(),
                expected: self.content_sha256.clone(),
                actual,
            })
        }
    }
}

/// Computes the public uppercase SHA-256 representation used by approval
/// evidence and registry files.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02X}")).collect()
}

/// Extracts the historical proposed-content digest embedded in ADR-0007..0009.
///
/// This value is bootstrap audit text only. It is not an integrity fact and
/// must never be compared with the current body bytes during validation.
#[must_use]
pub fn historical_bootstrap_digest(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let marker = "Bootstrap approval evidence: proposed-content SHA-256";
    text.get(text.find(marker)?..)
        .and_then(|evidence| evidence.split('`').nth(1))
        .filter(|value| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_string)
}

fn validate_adr_id(id: &str) -> MineResult<()> {
    let number = id
        .strip_prefix("ADR-")
        .filter(|n| n.len() == 4 && n.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|n| n.parse::<u16>().ok())
        .filter(|n| (1..=9999).contains(n));
    if number.is_none() {
        return Err(registry_error(
            id,
            "ADR id must be ADR-NNNN with NNNN in 0001..9999",
        ));
    }
    Ok(())
}

fn registry_error(id: &str, detail: &str) -> MineError {
    MineError::AdrRegistryInvalid {
        path: std::path::PathBuf::from("docs/design/decisions/adr-registry.toml"),
        detail: format!("{id}: {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, status: AdrStatus) -> AdrEntry {
        AdrEntry {
            id: id.to_string(),
            path: format!("docs/design/decisions/{}.md", &id[4..]),
            title: id.to_string(),
            status,
            planning_blocking: true,
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            planning_scopes: vec!["plan01".to_string()],
            content_sha256: String::new(),
            proposed_at: String::new(),
            approved_by: String::new(),
            approved_at: String::new(),
            approval_note: String::new(),
            supersedes: vec![],
            superseded_by: String::new(),
            rejection_reason: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn transition_table_and_explicit_approval() {
        let mut r = AdrRegistry::empty();
        r.add(entry("ADR-0001", AdrStatus::DRAFT)).unwrap();
        r.propose("ADR-0001", "A".to_string(), "now").unwrap();
        r.approve("ADR-0001", "A", "owner", "yes", "now").unwrap();
        assert_eq!(r.get("ADR-0001").unwrap().status, AdrStatus::APPROVED);
        assert!(r.approve("ADR-0001", "A", "owner", "again", "now").is_err());
    }

    #[test]
    fn readiness_filters_scope_and_unresolved_entries() {
        let mut r = AdrRegistry::empty();
        let mut a = entry("ADR-0001", AdrStatus::DRAFT);
        a.planning_scopes = vec!["other".to_string()];
        r.add(a).unwrap();
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert!(r.is_ready(Some("plan01")));
        assert!(!r.is_ready(Some("other")));
    }

    #[test]
    fn digest_verification_refuses_body_drift() {
        let root = tempfile::tempdir().unwrap();
        let body = root.path().join("docs/design/decisions/0001-example.md");
        std::fs::create_dir_all(body.parent().unwrap()).unwrap();
        std::fs::write(&body, b"# ADR-0001: Example\n\nbody\n").unwrap();
        let bytes = std::fs::read(&body).unwrap();
        let entry = AdrEntry {
            path: "docs/design/decisions/0001-example.md".to_string(),
            content_sha256: sha256_hex(&bytes),
            status: AdrStatus::PROPOSED,
            ..entry("ADR-0001", AdrStatus::PROPOSED)
        };
        entry.verify_digest(root.path()).unwrap();
        std::fs::write(&body, b"# ADR-0001: Example\n\nchanged\n").unwrap();
        assert!(matches!(
            entry.verify_digest(root.path()),
            Err(MineError::AdrDigestMismatch { .. })
        ));
    }

    #[test]
    fn forged_historical_bootstrap_digest_never_bypasses_current_digest() {
        let root = tempfile::tempdir().unwrap();
        let body = root.path().join("docs/design/decisions/0007-bootstrap.md");
        std::fs::create_dir_all(body.parent().unwrap()).unwrap();
        let original = b"# ADR-0007: Example\n\nBootstrap approval evidence: SHA-256 `OLD`\n\nDecision: original\n";
        std::fs::write(&body, original).unwrap();
        let entry = AdrEntry {
            id: "ADR-0007".to_string(),
            path: "docs/design/decisions/0007-bootstrap.md".to_string(),
            content_sha256: sha256_hex(original),
            status: AdrStatus::APPROVED,
            ..entry("ADR-0007", AdrStatus::APPROVED)
        };
        let forged = format!(
            "# ADR-0007: Example\n\nBootstrap approval evidence: SHA-256 `{}`\n\nDecision: unauthorized\n",
            entry.content_sha256
        );
        std::fs::write(&body, forged).unwrap();
        assert!(matches!(
            entry.verify_digest(root.path()),
            Err(MineError::AdrDigestMismatch { .. })
        ));
    }

    #[test]
    fn supersession_links_are_reciprocal_and_terminal() {
        let mut registry = AdrRegistry::empty();
        let mut old = entry("ADR-0001", AdrStatus::DRAFT);
        old.content_sha256 = "A".to_string();
        let mut replacement = entry("ADR-0002", AdrStatus::DRAFT);
        replacement.content_sha256 = "B".to_string();
        registry.add(old).unwrap();
        registry.add(replacement).unwrap();
        registry
            .propose("ADR-0001", "A".to_string(), "now")
            .unwrap();
        registry
            .approve("ADR-0001", "A", "owner", "approved", "now")
            .unwrap();
        registry
            .propose("ADR-0002", "B".to_string(), "now")
            .unwrap();
        registry
            .approve("ADR-0002", "B", "owner", "approved", "now")
            .unwrap();
        registry.supersede("ADR-0001", "ADR-0002", "now").unwrap();
        registry.validate_structure().unwrap();
        assert_eq!(
            registry.get("ADR-0001").unwrap().status,
            AdrStatus::SUPERSEDED
        );
        assert_eq!(registry.get("ADR-0001").unwrap().superseded_by, "ADR-0002");
    }
}
