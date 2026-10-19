//! Finite native metadata projection for the approved MINE 0.2 cutover.
//! Only parsed string-value spans are replaced; unrelated TOML bytes survive.

use super::*;
use crate::domain::adr::AdrRegistry;
use std::ops::Range;

type Edit = (Range<usize>, String);

pub(super) fn configuration(
    raw: &[u8],
    before: &MineConfig,
) -> OperationResult<(MineConfig, Vec<u8>)> {
    let source = text(raw)?;
    let document = toml_edit::ImDocument::parse(source).map_err(|_| OperationError::Journal)?;
    let canonical = crate::domain::config::GovernanceLayout::canonical();
    let mut target = before.clone();
    target.design.root = canonical.design_index().as_str().into();
    target.design.marker = canonical.design_marker().as_str().into();
    target.adr.registry = canonical.adr_registry().as_str().into();
    target.adr.rendered_index = canonical.adr_index().as_str().into();
    target.plan.root = canonical.plan_dir().as_str().into();
    target.graph.source = canonical.graph_source().as_str().into();
    target.graph.rendered = canonical.graph_view().as_str().into();
    target.layout()?;
    let mut edits = Vec::new();
    for (section, key, old, new) in [
        (
            "design",
            "root",
            before.design.root.as_str(),
            target.design.root.as_str(),
        ),
        (
            "design",
            "marker",
            before.design.marker.as_str(),
            target.design.marker.as_str(),
        ),
        (
            "adr",
            "registry",
            before.adr.registry.as_str(),
            target.adr.registry.as_str(),
        ),
        (
            "adr",
            "rendered_index",
            before.adr.rendered_index.as_str(),
            target.adr.rendered_index.as_str(),
        ),
        (
            "plan",
            "root",
            before.plan.root.as_str(),
            target.plan.root.as_str(),
        ),
        (
            "graph",
            "source",
            before.graph.source.as_str(),
            target.graph.source.as_str(),
        ),
        (
            "graph",
            "rendered",
            before.graph.rendered.as_str(),
            target.graph.rendered.as_str(),
        ),
    ] {
        let value = document
            .as_item()
            .get(section)
            .and_then(|table| table.get(key))
            .and_then(toml_edit::Item::as_value)
            .ok_or(OperationError::Refused(
                "missing explicit source layout field",
            ))?;
        edit_string(value, old, new, &mut edits)?;
    }
    let projected = patch(source, edits)?;
    if MineConfig::parse(Path::new(".mine/config.toml"), text(&projected)?)? != target {
        return Err(OperationError::Refused(
            "layout config projection changed a non-layout field",
        ));
    }
    Ok((target, projected))
}

pub(super) fn registry(
    raw: &[u8],
    before: &AdrRegistry,
    old_design: &str,
) -> OperationResult<(AdrRegistry, Vec<u8>)> {
    let source = text(raw)?;
    let document = toml_edit::ImDocument::parse(source).map_err(|_| OperationError::Journal)?;
    let entries = document
        .as_item()
        .get("adrs")
        .and_then(toml_edit::Item::as_array_of_tables)
        .ok_or(OperationError::Refused(
            "source ADR registry lacks its typed entries",
        ))?;
    if entries.len() != before.entries.len() {
        return Err(OperationError::Journal);
    }
    let mut next = before.clone();
    let mut edits = Vec::new();
    for ((table, old), new) in entries.iter().zip(&before.entries).zip(&mut next.entries) {
        new.path = relocate(&old.path, old_design)?;
        let value = table
            .get("path")
            .and_then(toml_edit::Item::as_value)
            .ok_or(OperationError::Journal)?;
        edit_string(value, &old.path, &new.path, &mut edits)?;
        if let Some(scopes) = table.get("planning_scopes") {
            let scopes = scopes.as_array().ok_or(OperationError::Journal)?;
            if scopes.len() != old.planning_scopes.len() {
                return Err(OperationError::Journal);
            }
            for ((value, old), new) in scopes
                .iter()
                .zip(&old.planning_scopes)
                .zip(&mut new.planning_scopes)
            {
                if crate::domain::path::is_within(old_design, old) {
                    *new = relocate(old, old_design)?;
                    edit_string(value, old, new, &mut edits)?;
                }
            }
        }
    }
    next.validate_structure()?;
    let projected = patch(source, edits)?;
    let decoded: AdrRegistry =
        toml::from_str(text(&projected)?).map_err(|_| OperationError::Journal)?;
    if decoded != next {
        return Err(OperationError::Refused(
            "ADR relocation changed approval/status/revision facts",
        ));
    }
    Ok((next, projected))
}

pub(super) fn relocate(path: &str, old_design: &str) -> OperationResult<String> {
    if path == old_design {
        return Ok(".mine/design".into());
    }
    let tail = path
        .strip_prefix(&format!("{old_design}/"))
        .ok_or(OperationError::Refused(
            "metadata path is outside source-owned Design",
        ))?;
    let result = format!(".mine/design/{tail}");
    RepoPath::new(result.clone())?;
    Ok(result)
}

/// No command, audit, dependency declaration, platform or policy threshold is
/// rewritten. The concrete release policy uses Full/Root checks; only unique
/// top-level Design-reference tokens are relocated, otherwise refuse.
pub(super) fn policy(raw: &[u8], old_design: &str) -> OperationResult<Vec<u8>> {
    use crate::domain::verification_receipt::{CommandDirectory, InputScope, VerificationPolicy};
    let before: VerificationPolicy = crate::application::native_evidence::strict_json(raw)?;
    if before.fields().checks.iter().any(|check| {
        !matches!(check.inputs, InputScope::Full)
            || !matches!(check.command.fields().cwd, CommandDirectory::Root)
            || check
                .dependency_locks
                .iter()
                .any(|path| crate::domain::path::is_within(old_design, path.as_str()))
    }) {
        return Err(OperationError::Refused(
            "cutover cannot rewrite dependency audits, commands or lock identities",
        ));
    }
    let mut projected = text(raw)?.to_string();
    let mut fields = before.fields().clone();
    for (old, new) in before
        .fields()
        .design_references
        .iter()
        .zip(&mut fields.design_references)
    {
        if crate::domain::path::is_within(old_design, old.as_str()) {
            let target = relocate(old.as_str(), old_design)?;
            let old_token =
                serde_json::to_string(old.as_str()).map_err(|_| OperationError::Journal)?;
            let new_token = serde_json::to_string(&target).map_err(|_| OperationError::Journal)?;
            if projected.matches(&old_token).count() != 1 {
                return Err(OperationError::Refused(
                    "ambiguous source policy reference token",
                ));
            }
            projected = projected.replacen(&old_token, &new_token, 1);
            *new = RepoPath::new(target)?;
        }
    }
    let expected = VerificationPolicy::new(fields)?;
    let actual: VerificationPolicy =
        crate::application::native_evidence::strict_json(projected.as_bytes())?;
    if actual != expected {
        return Err(OperationError::Refused(
            "cutover changed verification policy authority",
        ));
    }
    Ok(projected.into_bytes())
}

pub(super) fn agents(raw: &[u8]) -> OperationResult<Vec<u8>> {
    const MARKER: &str = "<!-- mine-managed-agents -->";
    let source = text(raw)?;
    if source.matches(MARKER).count() != 1 {
        return Err(OperationError::Refused(
            "ambiguous or missing MINE AGENTS section",
        ));
    }
    let (foreign, managed) = source.split_once(MARKER).ok_or(OperationError::Journal)?;
    let mut managed = managed.to_string();
    for (before, after) in [
        ("`docs/design/index.md`", "`.mine/design/index.md`"),
        (
            "`docs/design/.mine-design.toml`",
            "`.mine/design/.mine-design.toml`",
        ),
        ("`docs/design/`", "`.mine/design/`"),
        ("`docs/plan/`", "`.mine/plan/`"),
        (
            "`docs/plan/execution-graph.toml`",
            "`.mine/plan/execution-graph.toml`",
        ),
        (
            "`docs/plan/execution-graph.md`",
            "`.mine/plan/execution-graph.md`",
        ),
        ("`docs/plan/reports/`", "`.mine/plan/reports/`"),
        ("`docs/plan/<id>-<slug>.md`", "`.mine/plan/<id>-<slug>.md`"),
    ] {
        managed = managed.replace(before, after);
    }
    Ok(format!("{foreign}{MARKER}{managed}").into_bytes())
}

fn text(bytes: &[u8]) -> OperationResult<&str> {
    std::str::from_utf8(bytes).map_err(|_| OperationError::Refused("layout metadata is not UTF-8"))
}

fn edit_string(
    value: &toml_edit::Value,
    before: &str,
    after: &str,
    edits: &mut Vec<Edit>,
) -> OperationResult<()> {
    if value.as_str() != Some(before) {
        return Err(OperationError::Refused(
            "source typed metadata/string span differs",
        ));
    }
    let span = value.span().ok_or(OperationError::Refused(
        "source metadata has no parser span",
    ))?;
    edits.push((span, toml_edit::Value::from(after).to_string()));
    Ok(())
}

fn patch(source: &str, mut edits: Vec<Edit>) -> OperationResult<Vec<u8>> {
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0.start));
    let mut boundary = source.len();
    let mut result = source.to_string();
    for (span, value) in edits {
        if span.start > span.end
            || span.end > boundary
            || !source.is_char_boundary(span.start)
            || !source.is_char_boundary(span.end)
        {
            return Err(OperationError::Refused(
                "overlapping or invalid metadata spans",
            ));
        }
        boundary = span.start;
        result.replace_range(span, &value);
    }
    Ok(result.into_bytes())
}
