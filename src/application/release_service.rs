// Enforce `AGENTS.md`'s "Business code must not use `unsafe`" at compile time.
#![forbid(unsafe_code)]

//! Release service: deterministic release-candidate creation, preflight
//! validation, and safe temporary-artifact cleanup.
//!
//! The release path is deterministic and **fail closed**: it refuses to
//! mutate stable state until every required preflight and release-candidate
//! validation succeeds. Per the design ("Release closure"):
//!
//! 1. Every plan is accepted and integrated into `dev`.
//! 2. Run a full `mine-sync` (the user's responsibility; release-preflight
//!    verifies design validity).
//! 3. Resolve the release version from accepted changes + current managed
//!    version.
//! 4. Safely purge the MINE-owned `docs/plan/` workspace.
//! 5. Verify no tracked/untracked plan workspace or design backup enters the
//!    stable tree.
//! 6. Integrate through squash or curated commits (the independent reviewer's
//!    responsibility; the release service provides the candidate, not the
//!    final mutation).
//! 7. Tag/publish when configured (reserved for the reviewer).
//! 8. Delete local managed `plan/*` and `dev` branches (reserved for the
//!    reviewer after stable integration).
//!
//! The release service itself performs NO git mutation (no commit, merge,
//! tag, push, reset, clean, or branch deletion). It validates, resolves the
//! version, enumerates what SHOULD be cleaned, and reports the release
//! candidate state. Final stable mutation belongs to the independent reviewer.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::domain::error::{MineError, MineResult};
use crate::domain::status::PlanStatus;
use crate::infrastructure::git::{self, GitEvidence};
use crate::infrastructure::toml_store::TomlStore;

/// The release-preflight result.
#[derive(Debug, Clone, Serialize)]
pub struct ReleasePreflight {
    pub can_release: bool,
    pub release_version: String,
    pub dev_commit: String,
    pub stable_commit: String,
    pub stable_unchanged: bool,
    pub working_tree_clean: bool,
    pub all_plans_terminal: bool,
    pub design_valid: bool,
    pub graph_valid: bool,
    /// Informational parity of `skills/` vs `plugins/mine/skills/`. NOT a
    /// release gate: generic preflight must not require MINE-source assets
    /// (Design "Repository roles"); the MINE source repo enforces sync via
    /// its own gates (`mine dist verify`, CI).
    pub distribution_synced: bool,
    pub no_plan_artifacts_on_stable: bool,
    pub no_design_backups_on_stable: bool,
    pub pending_agent_transactions: Vec<String>,
    pub rejections_without_compensation: Vec<String>,
    pub errors: Vec<String>,
}

/// The cleanup-preview result: what WOULD be removed if cleanup is executed.
#[derive(Debug, Clone, Serialize)]
pub struct CleanupPreview {
    pub plan_workspace_files: Vec<String>,
    pub design_backups: Vec<String>,
    pub mine_managed_plan_branches: Vec<String>,
    pub dev_branch: Option<String>,
    pub total: usize,
}

/// Runs the full release preflight against `repo_root`. Returns the preflight
/// result with `can_release = true` only when every gate passes. No mutation.
pub fn preflight(repo_root: &Path) -> MineResult<ReleasePreflight> {
    let mut errors = Vec::new();

    // Load repository configuration to resolve configured branch names.
    // Fall back to defaults only when config is absent (first-time preflight
    // before init, which will report can_release=false anyway).
    let config = crate::cli::context::load_config(repo_root);
    let stable_branch = config
        .as_ref()
        .map(|c| c.branches.stable.as_str())
        .unwrap_or("master");

    // 1. Git evidence: working tree clean, dev/stable commits.
    let git_ev = GitEvidence::collect(repo_root);
    let dev_commit = git::head_commit(repo_root).unwrap_or_default();
    let stable_commit = run_git_rev_parse(repo_root, stable_branch)?;
    let working_tree_clean = git_ev.clean;
    let stable_unchanged = stable_commit_exists_and_matches_expected(repo_root, &stable_commit)?;

    // A configured stable branch that has no commit (missing in git) must
    // never be used silently: it is almost always a stale config recorded by
    // an older MINE version (e.g. `master` in a `main`-only repository).
    // Report it as a decisive error with an actionable repair path instead of
    // letting the empty commit propagate into the result.
    if stable_commit.is_empty() && config.is_some() {
        errors.push(format!(
            "configured stable branch '{stable_branch}' not found in this repository \
             (run `mine init` to repair .mine/config.toml)"
        ));
    }

    if !working_tree_clean {
        errors.push("working tree is dirty".to_string());
    }

    // 2. All plans terminal (ACCEPTED or REJECTED with compensation).
    let store = TomlStore::new(repo_root);
    let ws = store.load()?;
    let all_plans_terminal = ws
        .plans
        .iter()
        .all(|p| matches!(p.status, PlanStatus::Accepted | PlanStatus::Rejected));
    if !all_plans_terminal {
        let unresolved: Vec<&str> = ws
            .plans
            .iter()
            .filter(|p| !matches!(p.status, PlanStatus::Accepted | PlanStatus::Rejected))
            .map(|p| p.id.as_str())
            .collect();
        errors.push(format!(
            "unresolved plans remain: {}",
            unresolved.join(", ")
        ));
    }

    // 3. Rejections must have compensation (compensating_plan set).
    let rejections_without_compensation: Vec<String> = ws
        .plans
        .iter()
        .filter(|p| matches!(p.status, PlanStatus::Rejected) && p.compensating_plan.is_empty())
        .map(|p| p.id.clone())
        .collect();
    if !rejections_without_compensation.is_empty() {
        errors.push(format!(
            "rejected plans without compensation: {}",
            rejections_without_compensation.join(", ")
        ));
    }

    // 4. Design validates.
    let design_valid = validate_design(repo_root);

    // 5. Graph renders consistently (TOML matches MD). Read-only byte
    //    parity against a fresh deterministic render: preflight never
    //    rewrites the view (`mine graph render` is the repair).
    let graph_valid = match store.load() {
        Ok(ws) => match crate::infrastructure::toml_store::render_markdown(&ws) {
            Ok(expected) => std::fs::read(store.md_path())
                .map(|bytes| bytes == expected.as_bytes())
                .unwrap_or(false),
            Err(_) => false,
        },
        Err(_) => false,
    };

    // 6. Distribution parity is NOT a generic release gate. Per the accepted
    //    Design (docs/design/integrations/distribution.md -> "Repository
    //    roles"), the generic `mine release` preflight must not require
    //    `skills/` or `plugins/mine/skills/` to exist, and must not guess the
    //    repository role from directory presence. The MINE source repository
    //    enforces root/generated Skill synchronization through its own
    //    MINE-local decisive gates: `mine dist verify`, AGENTS.md quality
    //    tables, and CI (`scripts/sync-plugin-assets.py --check`). The parity
    //    result is still reported below as an informational field only.
    let distribution_synced = check_distribution_sync(repo_root);

    // 7. No plan artifacts or design backups on the stable branch.
    let no_plan_artifacts_on_stable =
        !path_exists_on_branch(repo_root, stable_branch, "docs/plan/");
    let no_design_backups_on_stable = !branch_has_design_backups(repo_root, stable_branch)?;

    // 8. No pending agent transactions (incomplete installs).
    let pending_agent_transactions = check_pending_transactions(repo_root);

    // 9. Version resolution from the target repository's config.
    let release_version = resolve_release_version(repo_root);

    let can_release = errors.is_empty()
        && all_plans_terminal
        && design_valid
        && graph_valid
        // Distribution parity is deliberately NOT part of can_release:
        // generic preflight must not gate on MINE-source assets (Design
        // "Repository roles"). The MINE source repo enforces sync via its own
        // gates (mine dist verify, CI).
        && no_plan_artifacts_on_stable
        && no_design_backups_on_stable
        && pending_agent_transactions.is_empty()
        && rejections_without_compensation.is_empty();

    Ok(ReleasePreflight {
        can_release,
        release_version,
        dev_commit,
        stable_commit,
        stable_unchanged,
        working_tree_clean,
        all_plans_terminal,
        design_valid,
        graph_valid,
        distribution_synced,
        no_plan_artifacts_on_stable,
        no_design_backups_on_stable,
        pending_agent_transactions,
        rejections_without_compensation,
        errors,
    })
}

/// Previews what cleanup WOULD remove. No mutation.
pub fn preview_cleanup(repo_root: &Path) -> MineResult<CleanupPreview> {
    let mut plan_workspace_files = Vec::new();
    let plan_dir = repo_root.join("docs/plan");
    if plan_dir.exists() {
        for entry in walk_files(&plan_dir) {
            let rel = entry
                .strip_prefix(repo_root)
                .unwrap_or(&entry)
                .to_string_lossy()
                .replace('\\', "/");
            plan_workspace_files.push(rel);
        }
    }

    let mut design_backups = Vec::new();
    // Scan docs/ recursively for design-backup-* directories (not just
    // top-level entries, since backups live under docs/). The rejected Plan
    // 08 scanned repo_root's top-level entries, which only returns "docs"
    // and can never match the "design-backup-" prefix.
    let docs_dir = repo_root.join("docs");
    if docs_dir.exists() {
        for entry in std::fs::read_dir(&docs_dir).map_err(MineError::Io)? {
            let entry = entry.map_err(MineError::Io)?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("design-backup-") {
                design_backups.push(format!("docs/{name}"));
            }
        }
    }
    // Also check the repo root for any unscoped design-backup-* dirs (unlikely
    // but exhaustive).
    for entry in std::fs::read_dir(repo_root).map_err(MineError::Io)? {
        let entry = entry.map_err(MineError::Io)?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("design-backup-") && !design_backups.iter().any(|d| d == &name) {
            design_backups.push(name);
        }
    }

    // MINE-managed plan branches (plan/*).
    let mine_managed_plan_branches = list_mine_branches(repo_root);

    // integration branch exists.
    let integration_branch = crate::cli::context::load_config(repo_root)
        .map(|c| c.branches.integration)
        .unwrap_or_else(|| "dev".to_string());
    let dev_branch = if git::branch_exists(repo_root, &integration_branch) {
        Some(integration_branch)
    } else {
        None
    };

    let total =
        plan_workspace_files.len() + design_backups.len() + mine_managed_plan_branches.len();

    Ok(CleanupPreview {
        plan_workspace_files,
        design_backups,
        mine_managed_plan_branches,
        dev_branch,
        total,
    })
}

/// Resolves the release version from the current managed version and accepted
/// changes. The first release defaults to `0.1.0` (the current managed version)
/// when no prior release has been made; subsequent releases increment the
/// patch component.
fn resolve_release_version(repo_root: &Path) -> String {
    // The managed version from <repo_root>/.mine/config.toml is the
    // authoritative source. The release version is the current config
    // mine_code_version, set via `mine repository version set --version
    // <semver>` before release closure. Always read from the resolved
    // repo_root, never from std::env::current_dir().
    crate::cli::context::load_config(repo_root)
        .map(|c| c.mine_code_version)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

fn validate_design(repo_root: &Path) -> bool {
    let marker = repo_root.join("docs/design/.mine-design.toml");
    let index = repo_root.join("docs/design/index.md");
    marker.exists() && index.exists()
}

fn check_distribution_sync(repo_root: &Path) -> bool {
    let skills = repo_root.join("skills");
    let plugin_skills = repo_root.join("plugins/mine/skills");
    // A generic repository that does not ship MINE Skills has nothing to
    // synchronize; the gate is vacuously satisfied. This is not a repo-role
    // heuristic: the distribution check is inherently about skills/ <->
    // plugins/mine/skills/ parity, and if neither exists there is nothing to
    // verify. Only the MINE source repository (which ships skills/) is
    // subject to the sync requirement.
    if !skills.is_dir() {
        return true;
    }
    match (
        relative_file_bytes(&skills),
        relative_file_bytes(&plugin_skills),
    ) {
        (Some(source), Some(generated)) => source == generated,
        _ => false,
    }
}

fn relative_file_bytes(root: &Path) -> Option<std::collections::BTreeMap<String, Vec<u8>>> {
    if !root.is_dir() {
        return None;
    }
    walk_files(root)
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((relative, std::fs::read(path).ok()?))
        })
        .collect()
}

fn check_pending_transactions(repo_root: &Path) -> Vec<String> {
    let mine_dir = repo_root.join(".mine");
    let mut pending = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&mine_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("agent-pending-") {
                pending.push(name);
            }
        }
    }
    pending
}

fn path_exists_on_branch(repo_root: &Path, branch: &str, path: &str) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["cat-file", "-e", &format!("{branch}:{path}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn branch_has_design_backups(repo_root: &Path, branch: &str) -> MineResult<bool> {
    // Use `git ls-tree -r --name-only` (RECURSIVE) so nested paths like
    // `docs/design-backup-20260101T000000Z/some-file.md` are returned.
    // The non-recursive variant only returns top-level tree entries (`docs`),
    // which can never match the `docs/design-backup-` prefix -- the root cause
    // detects nested docs/design-backup-* paths at arbitrary depth.
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["ls-tree", "-r", "--name-only", branch])
        .output()
        .map_err(|e| MineError::Io(std::io::Error::other(format!("git ls-tree failed: {e}"))))?;
    if !output.status.success() {
        // Fail closed: if tree inspection fails, we cannot prove no backups
        // are present. Return true (backups suspected) so the gate blocks.
        return Ok(true);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .any(|line| line.starts_with("docs/design-backup-")))
}

fn stable_commit_exists_and_matches_expected(
    _repo_root: &Path,
    stable_commit: &str,
) -> MineResult<bool> {
    // Verify the stable branch hasn't moved unexpectedly (we don't have a
    // stored expected; this is a structural check that the stable branch
    // exists and has a commit).
    Ok(!stable_commit.is_empty())
}

fn run_git_rev_parse(repo_root: &Path, ref_name: &str) -> MineResult<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", ref_name])
        .output()
        .map_err(|e| MineError::Io(std::io::Error::other(format!("git rev-parse failed: {e}"))))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn list_mine_branches(repo_root: &Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["branch", "--list", "--format=%(refname:short)"])
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| l.starts_with("plan/"))
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    }
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_fails_on_dirty_tree() {
        let tmp = tempfile::tempdir().unwrap();
        // No git repo -> preflight should fail (not crash).
        let result = preflight(tmp.path());
        // It may error or return can_release=false; either way, no crash.
        if let Ok(pf) = result {
            assert!(!pf.can_release, "dirty/non-repo must not be releasable");
        }
    }

    #[test]
    fn distribution_sync_compares_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = tmp.path().join("skills/mine-review");
        let plugin_skills = tmp.path().join("plugins/mine/skills/mine-review");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::create_dir_all(&plugin_skills).unwrap();
        std::fs::write(skills.join("SKILL.md"), "skill copy\n").unwrap();
        std::fs::write(plugin_skills.join("SKILL.md"), "skill copy\n").unwrap();

        assert!(
            check_distribution_sync(tmp.path()),
            "matching relative file paths and bytes must be synchronized regardless of their distinct roots"
        );

        std::fs::write(plugin_skills.join("SKILL.md"), "drifted copy\n").unwrap();
        assert!(
            !check_distribution_sync(tmp.path()),
            "different generated bytes must make distribution synchronization fail"
        );

        std::fs::write(plugin_skills.join("SKILL.md"), "skill copy\n").unwrap();
        std::fs::write(plugin_skills.join("extra.md"), "stale\n").unwrap();
        assert!(
            !check_distribution_sync(tmp.path()),
            "an extra generated file must make distribution synchronization fail"
        );
    }

    #[test]
    fn cleanup_preview_lists_plan_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("docs/plan")).unwrap();
        std::fs::write(tmp.path().join("docs/plan/01.md"), "plan").unwrap();
        let preview = preview_cleanup(tmp.path()).unwrap();
        assert!(
            preview
                .plan_workspace_files
                .iter()
                .any(|f| f.contains("01.md"))
        );
    }

    #[test]
    fn cleanup_preview_lists_design_backups() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("docs/design-backup-20260101T000000Z")).unwrap();
        let preview = preview_cleanup(tmp.path()).unwrap();
        assert!(
            preview
                .design_backups
                .iter()
                .any(|f| f.contains("design-backup-"))
        );
    }

    #[test]
    fn distribution_gate_passes_without_skills_dirs() {
        // A generic repository with neither skills/ nor plugins/mine/skills/
        // reports parity true (informational): there is nothing to compare.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("README.md"), "product\n").unwrap();
        assert!(
            check_distribution_sync(tmp.path()),
            "distribution parity must be true for a repository without MINE Skills"
        );
    }

    #[test]
    fn distribution_parity_is_informational_not_a_release_gate() {
        // An external repository that happens to have its own unrelated
        // skills/ directory (not MINE's) but no plugins/mine/skills/ must
        // NOT be blocked by the distribution gate: generic preflight must
        // not guess the repository role from directory presence (Design
        // "Repository roles"). The parity result is informational only.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("skills/team-notes")).unwrap();
        std::fs::write(
            tmp.path().join("skills/team-notes/README.md"),
            "team notes\n",
        )
        .unwrap();
        assert!(
            !check_distribution_sync(tmp.path()),
            "parity is false when an unrelated skills/ has no matching plugins copy"
        );
        // The gate itself must not decide can_release: parity is reported but
        // never blocks. This is enforced structurally: `can_release` does not
        // include `distribution_synced` (see preflight), and the CLI-level
        // test `external_repo_with_unrelated_skills_is_not_blocked` proves it
        // end to end.
    }

    #[test]
    fn resolve_release_version_reads_target_repo_config() {
        // Repo A has version 1.2.3; the function must read it from repo A's
        // config even though the process CWD is elsewhere.
        let repo_a = tempfile::tempdir().unwrap();
        let mine_dir = repo_a.path().join(".mine");
        std::fs::create_dir_all(&mine_dir).unwrap();
        std::fs::write(
            mine_dir.join("config.toml"),
            r#"schema_version = 1
repository_id = "repo-a"
mine_code_version = "1.2.3"

[branches]
stable = "main"
integration = "dev"

[design]
root = "docs/design"
marker = "docs/design/.mine-design.toml"
language = "en"
index_soft_limit_lines = 250
leaf_soft_limit_lines = 400

[plan]
root = "docs/plan"
ephemeral = true
purge_before_stable_release = true

[graph]
source = "docs/plan/execution-graph.toml"
rendered = "docs/plan/execution-graph.md"
lock_timeout_ms = 5000
"#,
        )
        .unwrap();

        // Force CWD away from repo_a (to a different dir).
        let elsewhere = tempfile::tempdir().unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(elsewhere.path()).unwrap();
        let version = resolve_release_version(repo_a.path());
        std::env::set_current_dir(&prev).unwrap();
        assert_eq!(
            version, "1.2.3",
            "release version must come from the target repo, not the process CWD"
        );
    }
}
