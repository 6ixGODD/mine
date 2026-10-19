//! Canonical Plan identity, filename, report, branch, and lineage rules.
//!
//! The parser is the single source of truth for the grammar approved by
//! ADR-0007. Callers must not recover lineage or compensation ordinals by
//! slicing strings or maintaining a second regular expression.

use std::fmt;

use crate::domain::error::{MineError, MineResult};

const PLAN_PREFIX: &str = "plan";

/// A parsed canonical Plan ID matching `planNN(?:-NN)*(?:-CNN)?`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalPlanId {
    base: Vec<u8>,
    compensation: Option<u8>,
}

impl CanonicalPlanId {
    /// Parses the exact, case-sensitive canonical grammar.
    pub fn parse(value: &str) -> MineResult<Self> {
        if !value.starts_with(PLAN_PREFIX) {
            return Err(invalid(value, "must start with lowercase `plan`"));
        }
        let rest = &value[PLAN_PREFIX.len()..];
        if rest.is_empty() {
            return Err(invalid(value, "must contain at least one numeric segment"));
        }
        let parts: Vec<&str> = rest.split('-').collect();
        let mut base = Vec::new();
        let mut compensation = None;
        for (index, part) in parts.iter().enumerate() {
            if let Some(number) = part.strip_prefix('C') {
                if index != parts.len() - 1 || compensation.is_some() || base.is_empty() {
                    return Err(invalid(
                        value,
                        "`C` may occur only once as the final segment",
                    ));
                }
                compensation = Some(parse_segment(number, value)?);
            } else {
                if part.len() != 2 {
                    return Err(invalid(
                        value,
                        "numeric segments must be exactly two digits",
                    ));
                }
                base.push(parse_segment(part, value)?);
            }
        }
        if base.is_empty() {
            return Err(invalid(value, "must contain at least one numeric segment"));
        }
        Ok(Self { base, compensation })
    }

    /// Returns the canonical textual ID.
    #[must_use]
    pub fn as_str(&self) -> String {
        self.to_string()
    }

    /// Returns the base lineage ID, without a compensation suffix.
    #[must_use]
    pub fn lineage_id(&self) -> String {
        format!(
            "{}{}",
            PLAN_PREFIX,
            self.base
                .iter()
                .map(|n| format!("-{n:02}"))
                .collect::<String>()
                .trim_start_matches('-')
        )
    }

    /// Returns the compensation ordinal, if this is a compensation.
    #[must_use]
    pub fn compensation_ordinal(&self) -> Option<u8> {
        self.compensation
    }

    /// Returns whether this ID names a compensation node.
    #[must_use]
    pub fn is_compensation(&self) -> bool {
        self.compensation.is_some()
    }

    /// Returns the next sibling compensation ID.
    #[must_use]
    pub fn next_compensation(&self) -> Option<String> {
        let next = self.compensation.unwrap_or(0).checked_add(1)?;
        Some(format!("{}-C{next:02}", self.lineage_id()))
    }

    /// Returns the canonical lineage worktree locator.
    #[must_use]
    pub fn worktree_path(&self) -> String {
        self.worktree_path_under(".mine/worktrees")
    }

    /// Returns the canonical lineage worktree locator beneath a configured
    /// repository-relative worktree root.
    #[must_use]
    pub fn worktree_path_under(&self, worktrees_root: &str) -> String {
        format!(
            "{}/{}",
            worktrees_root.trim_end_matches('/'),
            self.lineage_id()
        )
    }
}

/// Derives the registered managed branch for a graph node.
///
/// The one exact bootstrap node keeps its historical `01` identity. Every
/// canonical node is validated through the typed id/path grammar before the
/// branch is returned.
pub fn registered_plan_branch(workspace_id: &str, id: &str, path: &str) -> MineResult<String> {
    if is_exact_bootstrap_node(workspace_id, id, path) {
        let filename = path
            .strip_prefix("docs/plan/")
            .and_then(|value| value.strip_suffix(".md"))
            .ok_or_else(|| invalid(path, "invalid bootstrap plan path"))?;
        return Ok(format!("plan/{filename}"));
    }
    let slug = validate_plan_path(id, path)?;
    Ok(format!("plan/{id}-{slug}"))
}

impl fmt::Display for CanonicalPlanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.lineage_id())?;
        if let Some(c) = self.compensation {
            write!(f, "-C{c:02}")?;
        }
        Ok(())
    }
}

/// Validates a canonical Plan filename and returns its lowercase-kebab slug.
pub fn validate_plan_path(id: &str, path: &str) -> MineResult<String> {
    let parsed = CanonicalPlanId::parse(id)?;
    crate::domain::graph::RepoPath::new(path).map_err(|_| invalid(path, "unsafe Plan path"))?;
    let (_, filename) = path
        .rsplit_once('/')
        .ok_or_else(|| invalid(path, "Plan needs an owned parent directory"))?;
    if filename.contains('/') || filename.contains('\\') || !filename.ends_with(".md") {
        return Err(invalid(
            path,
            "must be one Markdown file directly under the configured Plan root",
        ));
    }
    let expected_prefix = format!("{}-", parsed);
    let slug = filename
        .strip_prefix(&expected_prefix)
        .and_then(|s| s.strip_suffix(".md"))
        .ok_or_else(|| {
            invalid(
                path,
                "filename must be <canonical-id>-<lower-kebab-slug>.md",
            )
        })?;
    validate_slug(slug, path)?;
    Ok(slug.to_string())
}

/// Validates a report path derived from a Plan filename.
pub fn validate_report_path(plan_path: &str, path: &str, kind: &str) -> MineResult<()> {
    let expected = report_path(plan_path, kind)?;
    if path != expected {
        return Err(invalid(path, &format!("expected report path {expected}")));
    }
    Ok(())
}

/// Derive a report from an already registered Plan body, never a second root.
pub fn report_path(plan_path: &str, kind: &str) -> MineResult<String> {
    crate::domain::graph::RepoPath::new(plan_path)
        .map_err(|_| invalid(plan_path, "unsafe Plan path"))?;
    if !matches!(kind, "implementation" | "review") {
        return Err(invalid(kind, "unknown Plan report kind"));
    }
    let (directory, body) = plan_path
        .rsplit_once('/')
        .ok_or_else(|| invalid(plan_path, "Plan needs a parent"))?;
    let stem = body
        .strip_suffix(".md")
        .ok_or_else(|| invalid(plan_path, "Plan needs Markdown extension"))?;
    Ok(format!("{directory}/reports/{stem}-{kind}.md"))
}

/// Validates that a replacement is the next sibling in the rejected lineage.
pub fn validate_compensation_replacement(rejected: &str, replacement: &str) -> MineResult<()> {
    let rejected = CanonicalPlanId::parse(rejected)?;
    let replacement_id = CanonicalPlanId::parse(replacement)?;
    if rejected.lineage_id() != replacement_id.lineage_id() {
        return Err(invalid(
            replacement,
            "compensation must remain in the rejected Plan lineage",
        ));
    }
    let expected = rejected
        .next_compensation()
        .ok_or_else(|| invalid(&rejected.as_str(), "compensation ordinal overflow"))?;
    if replacement != expected {
        return Err(invalid(
            replacement,
            &format!("expected next sibling compensation {expected}"),
        ));
    }
    Ok(())
}

/// Returns whether the exact legacy bootstrap node is allowed during cutover.
/// No other legacy identifier is accepted.
#[must_use]
pub fn is_exact_bootstrap_node(workspace_id: &str, id: &str, path: &str) -> bool {
    workspace_id == "efaa93da-09ca-4888-a411-f896a151ce6d"
        && id == "01"
        && path == "docs/plan/01-isolated-plan-worktrees-and-review-integration.md"
}

fn parse_segment(segment: &str, whole: &str) -> MineResult<u8> {
    if segment.len() != 2 || !segment.as_bytes().iter().all(u8::is_ascii_digit) {
        return Err(invalid(whole, "segments must be exactly two ASCII digits"));
    }
    let number = segment
        .parse::<u8>()
        .map_err(|_| invalid(whole, "invalid numeric segment"))?;
    if !(1..=99).contains(&number) {
        return Err(invalid(
            whole,
            "numeric segments must be in the range 01..99",
        ));
    }
    Ok(number)
}

fn validate_slug(slug: &str, whole: &str) -> MineResult<()> {
    if slug.is_empty()
        || slug.starts_with('-')
        || slug.ends_with('-')
        || slug.contains("--")
        || slug
            .bytes()
            .any(|b| !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
    {
        return Err(invalid(whole, "slug must match [a-z0-9]+(?:-[a-z0-9]+)*"));
    }
    Ok(())
}

fn invalid(value: &str, detail: &str) -> MineError {
    MineError::InvalidPlanIdentity {
        value: value.to_string(),
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_round_trip() {
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        for value in ["plan01", "plan01-02", "plan01-02-C01", "plan99-01-C02"] {
            assert_eq!(CanonicalPlanId::parse(value).unwrap().to_string(), value);
        }
    }

    #[test]
    fn rejects_invalid_segments_and_compensation_forms() {
        for value in [
            "01",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan00",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan1",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan01-c01",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan01-C00",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan01-C01-02",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan01-C01-C02",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan01-00",
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan01-100",
        ] {
            assert!(CanonicalPlanId::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn validates_path_and_lineage() {
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        let slug = validate_plan_path("plan01-02-C01", "docs/plan/plan01-02-C01-fix-some-bugs.md")
            .unwrap();
        assert_eq!(slug, "fix-some-bugs");
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        let id = CanonicalPlanId::parse("plan01-02-C01").unwrap();
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert_eq!(id.lineage_id(), "plan01-02");
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert_eq!(id.worktree_path(), ".mine/worktrees/plan01-02");
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert_eq!(id.next_compensation(), Some("plan01-02-C02".to_string()));
    }

    #[test]
    fn replacement_is_next_sibling_only() {
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        validate_compensation_replacement("plan01-02", "plan01-02-C01").unwrap();
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert!(validate_compensation_replacement("plan01-02-C01", "plan01-03-C02").is_err());
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert!(validate_compensation_replacement("plan01-02-C01", "plan01-02-C01-C01").is_err());
    }

    #[test]
    fn derives_registered_branch_from_typed_identity() {
        assert_eq!(
            registered_plan_branch(
                "workspace",
                // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                "plan01-C02",
                // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                "docs/plan/plan01-C02-protect-and-repair-plan-provenance.md",
            )
            .unwrap(),
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            "plan/plan01-C02-protect-and-repair-plan-provenance"
        );
    }
}
