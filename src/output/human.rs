//! Concise human-readable output for the default CLI mode.
//!
//! Plain text only: `--no-color` is the default and only mode in this plan.
//! Color could be added later behind a TTY check; the contract requires
//! deterministic plain text for machine/pipeline consumers, and MINE's own
//! CLI never emits ANSI unless explicitly requested in a future plan.

use std::fmt::Write;

use crate::application::init_service::{DesignRootSummary, InitOutcome};
use crate::domain::graph::PlanWorkspace;
use crate::domain::status::PlanStatus;

/// The kind of output line for human mode. All lines are plain strings; the
/// kind is informational for tests that want to assert structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HumanLine {
    Section(String),
    Field { key: String, value: String },
    Note(String),
    Action { kind: String, path: String },
}

impl HumanLine {
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Section(title) => title.clone(),
            Self::Field { key, value } => format!("{key}: {value}"),
            Self::Note(text) => text.clone(),
            Self::Action { kind, path } => format!("  {kind}: {path}"),
        }
    }
}

/// Builds the human report for `mine init`.
#[must_use]
pub fn init_report(outcome: &InitOutcome) -> Vec<HumanLine> {
    let mut lines = vec![HumanLine::Section(
        "mine init: repository initialized".to_string(),
    )];
    lines.push(HumanLine::Field {
        key: "  repository_id".to_string(),
        value: outcome.repository_id.clone(),
    });
    lines.push(HumanLine::Field {
        key: "  mine_code_version".to_string(),
        value: outcome.mine_code_version.clone(),
    });
    lines.push(HumanLine::Field {
        key: "  design_root".to_string(),
        value: match outcome.design_root {
            DesignRootSummary::Absent => "created (was absent)".to_string(),
            DesignRootSummary::Managed => "managed (preserved)".to_string(),
        },
    });
    for a in &outcome.actions {
        match a {
            crate::application::init_service::InitAction::BackedUpDesign { backup_path } => {
                lines.push(HumanLine::Field {
                    key: "  backed up non-MINE design to".to_string(),
                    value: backup_path.display().to_string(),
                });
            }
            other => {
                let kind = match other {
                    crate::application::init_service::InitAction::Created(p)
                        if p.ends_with("AGENTS.md") =>
                    {
                        "created-section"
                    }
                    crate::application::init_service::InitAction::Created(_) => "created",
                    crate::application::init_service::InitAction::Preserved(_) => "preserved",
                    crate::application::init_service::InitAction::CreatedSection(_) => {
                        "created-section"
                    }
                    crate::application::init_service::InitAction::RepairedStableBranch {
                        ..
                    } => "repaired-stable-branch",
                    crate::application::init_service::InitAction::BackedUpDesign { .. } => {
                        unreachable!()
                    }
                };
                let path = match other {
                    crate::application::init_service::InitAction::Created(p)
                    | crate::application::init_service::InitAction::Preserved(p)
                    | crate::application::init_service::InitAction::CreatedSection(p) => p,
                    crate::application::init_service::InitAction::RepairedStableBranch {
                        path,
                        ..
                    } => path,
                    crate::application::init_service::InitAction::BackedUpDesign { .. } => {
                        unreachable!()
                    }
                };
                lines.push(HumanLine::Action {
                    kind: kind.to_string(),
                    path: path.display().to_string(),
                });
            }
        }
    }
    lines
}

/// Builds the human report for `mine status`.
#[must_use]
pub fn status_report(repo_root: &std::path::Path, ws: Option<&PlanWorkspace>) -> Vec<HumanLine> {
    let mut lines = Vec::new();
    lines.push(HumanLine::Section("mine status".to_string()));
    lines.push(HumanLine::Field {
        key: "  repository".to_string(),
        value: repo_root.display().to_string(),
    });
    match ws {
        Some(w) => {
            lines.push(HumanLine::Field {
                key: "  workspace_id".to_string(),
                value: w.workspace_id.clone(),
            });
            lines.push(HumanLine::Field {
                key: "  revision".to_string(),
                value: w.revision.to_string(),
            });
            lines.push(HumanLine::Field {
                key: "  stable_branch".to_string(),
                value: w.stable_branch.clone(),
            });
            lines.push(HumanLine::Field {
                key: "  integration_branch".to_string(),
                value: w.integration_branch.clone(),
            });
            let mut counts: std::collections::BTreeMap<&'static str, usize> =
                std::collections::BTreeMap::new();
            for p in &w.plans {
                let key = match p.status {
                    PlanStatus::Draft => "draft",
                    PlanStatus::Blocked => "blocked",
                    PlanStatus::Ready => "ready",
                    PlanStatus::InProgress => "in_progress",
                    PlanStatus::Implemented => "implemented",
                    PlanStatus::Accepted => "accepted",
                    PlanStatus::Rejected => "rejected",
                    PlanStatus::Withdrawn => "withdrawn",
                };
                *counts.entry(key).or_insert(0) += 1;
            }
            let mut summary = String::new();
            for (k, v) in &counts {
                let _ = write!(summary, "{k}={v} ");
            }
            lines.push(HumanLine::Field {
                key: "  plans".to_string(),
                value: format!("{} total; {}", w.plans.len(), summary.trim_end()),
            });
        }
        None => {
            lines.push(HumanLine::Note(
                "  execution graph: not initialized (run `mine init` first)".to_string(),
            ));
        }
    }
    lines
}

/// Formats the graph table for `mine graph show`/`status`.
#[must_use]
pub fn graph_table(ws: &PlanWorkspace) -> Vec<HumanLine> {
    let mut lines = Vec::new();
    let mut header = String::from("ID   STATUS        HARD-PREDS  TITLE");
    lines.push(HumanLine::Section(header.clone()));
    header.clear();
    for p in &ws.plans {
        let preds = if p.hard_predecessors.is_empty() {
            "—".to_string()
        } else {
            p.hard_predecessors.join(",")
        };
        let _ = write!(
            header,
            "{:<5}|{:<13}|{:<11}| {}",
            p.id,
            p.status.as_str(),
            preds,
            p.title
        );
        lines.push(HumanLine::Section(header.clone()));
        header.clear();
    }
    lines
}
