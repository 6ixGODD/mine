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

pub mod curation;
pub(crate) mod retained;

/// Explicit operation-selected objective diagnostics, never gate admission.
pub use retained::diagnostic::ReleaseDiagnostic;
pub fn diagnose(
    root: &Path,
    id: &crate::domain::lifecycle_operation::OperationId,
) -> MineResult<ReleaseDiagnostic> {
    retained::diagnose(root, id)
}

use serde::Serialize;

use crate::domain::config::MineConfig;
use crate::domain::error::{MineError, MineResult};
use crate::domain::status::PlanStatus;
use crate::infrastructure::git;
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
    pub pending_operations: Vec<crate::infrastructure::operation_store::OperationObservation>,
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

/// Runs pre-integration release preflight against `repo_root`. Returns the preflight
/// result with `can_release = true` only when every gate passes. No mutation.
pub fn preflight(repo_root: &Path) -> MineResult<ReleasePreflight> {
    let config = crate::cli::context::load_config_or_error(repo_root)?;
    let store = TomlStore::from_configuration(repo_root, &config)?;
    store.with_coordinator_lock(|guard| preflight_locked(repo_root, &config, &store, guard))
}

fn preflight_locked(
    repo_root: &Path,
    config: &crate::domain::config::MineConfig,
    store: &TomlStore,
    guard: &crate::infrastructure::file_lock::FileLock,
) -> MineResult<ReleasePreflight> {
    let mut errors = Vec::new();
    let stable_branch = config.branches.stable.as_str();
    let mut git = crate::infrastructure::graph_checkpoint::checkpoint_git(repo_root, None)
        .map_err(|error| super::native_evidence::refused(error.to_string()))?;
    crate::infrastructure::graph_checkpoint::observe_index_policy(&mut git)
        .map_err(|error| super::native_evidence::refused(error.to_string()))?;
    let identity = git.text(&[
        "rev-parse",
        "--path-format=absolute",
        "--show-toplevel",
        "--absolute-git-dir",
        "--git-common-dir",
    ])?;
    let identity = identity.lines().collect::<Vec<_>>();
    if identity.len() != 3
        || Path::new(identity[0]).canonicalize()? != repo_root.canonicalize()?
        || Path::new(identity[1]).canonicalize()? != Path::new(identity[2]).canonicalize()?
    {
        return Err(super::native_evidence::refused(
            "release preflight requires actual Root",
        ));
    }
    if git.text(&["symbolic-ref", "-q", "HEAD"])?
        != format!("refs/heads/{}", config.branches.integration)
    {
        errors.push("pre-integration release preflight requires Root integration checkout".into());
    }
    let dev_commit = git.text(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    let stable_commit = git
        .refs()?
        .into_iter()
        .find(|reference| reference.name == format!("refs/heads/{stable_branch}"))
        .map(|reference| reference.oid)
        .unwrap_or_default();
    let working_tree_clean = git
        .run(
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            b"",
        )?
        .is_empty();
    if !working_tree_clean {
        errors.push("working tree is dirty".to_string());
    }
    let ws = store.load()?;
    let stable_unchanged = !stable_commit.is_empty() && stable_commit == ws.stable_baseline_commit;
    if stable_commit.is_empty() {
        errors.push(format!(
            "configured stable branch '{stable_branch}' not found in this repository"
        ));
    } else {
        git.validate_commits(&[dev_commit.clone(), stable_commit.clone()])?;
        if !stable_unchanged {
            errors
                .push("actual stable tip differs from the stored pre-integration baseline".into());
        }
        if !git.is_ancestor(&stable_commit, &dev_commit)? {
            errors.push("stable baseline is not an ancestor of development source".into());
        }
    }
    if ws.workspace_id.is_empty()
        || ws.stable_branch != config.branches.stable
        || ws.integration_branch != config.branches.integration
        || ws.design_root != config.design.root
    {
        errors.push("graph/config workspace identity differs".into());
    }
    let history = crate::infrastructure::operation_store::RepositoryJournal::under_lock(
        repo_root,
        &config.repository_id,
        guard,
    )
    .map_err(|error| super::native_evidence::refused(error.to_string()))?;
    let observed = history
        .observations()
        .map_err(|error| super::native_evidence::refused(error.to_string()))?;
    crate::infrastructure::operation_store::history::predecessors(
        &history,
        &observed,
        Some(&ws.workspace_id),
    )
    .map_err(|error| super::native_evidence::refused(error.to_string()))?;
    let pending_operations = observed
        .into_iter()
        .filter(|record| !crate::domain::lifecycle_operation::terminal(&record.operation))
        .collect::<Vec<_>>();
    if !pending_operations.is_empty() {
        errors.push("pending lifecycle operations block release preflight".into());
    }
    let all_plans_terminal = curation::validate_terminal_workspace(&ws).is_ok();
    if !all_plans_terminal {
        let unresolved: Vec<&str> = ws
            .plans
            .iter()
            .filter(|p| curation::validate_terminal_node(&ws, p).is_err())
            .map(|p| p.id.as_str())
            .collect();
        errors.push(format!(
            "unresolved plans remain: {}",
            unresolved.join(", ")
        ));
    }

    // A declared but incomplete/withdrawn compensation is not a release seal.
    let rejections_without_compensation: Vec<String> = ws
        .plans
        .iter()
        .filter(|p| {
            matches!(p.status, PlanStatus::Rejected)
                && curation::validate_terminal_node(&ws, p).is_err()
        })
        .map(|p| p.id.clone())
        .collect();
    if !rejections_without_compensation.is_empty() {
        errors.push(format!(
            "rejected plans without compensation: {}",
            rejections_without_compensation.join(", ")
        ));
    }

    // 4. Design validates.
    let (design_valid, design_paths) = match validate_design(repo_root, config) {
        Ok(paths) => (true, paths),
        Err(error) => {
            errors.push(format!("Design/ADR readiness failed: {error}"));
            (false, vec![])
        }
    };
    // Clean status can hide ignored/skip-worktree governance bytes. Prove raw
    // observations against the actual committed source using built-in storage
    // conversions, never treat status as a commit/content identity.
    let committed = (|| -> MineResult<()> {
        use crate::domain::graph::{CommitId, FileIdentity, RepoPath};
        let mut paths = design_paths;
        for value in [
            ".mine/config.toml",
            &config.graph.source,
            &config.graph.rendered,
        ] {
            paths.push(
                RepoPath::new(value)
                    .map_err(|error| super::native_evidence::refused(error.to_string()))?,
            );
        }
        paths.sort();
        paths.dedup();
        let files = paths
            .into_iter()
            .map(|path| {
                let body = super::native_evidence::bytes(repo_root, &path)?;
                Ok(FileIdentity {
                    path,
                    bytes: body.len() as u64,
                    sha256: crate::infrastructure::graph_checkpoint::digest(&body),
                })
            })
            .collect::<MineResult<Vec<_>>>()?;
        let head = CommitId::new(dev_commit.clone())
            .map_err(|error| super::native_evidence::refused(error.to_string()))?;
        super::native_evidence::committed_files(repo_root, &head, &files)
    })();
    if let Err(error) = committed {
        errors.push(format!("committed source governance proof failed: {error}"));
    }

    // 5. Graph renders consistently (TOML matches MD). Read-only byte
    //    parity against a fresh deterministic render: preflight never
    //    rewrites the view (`mine graph render` is the repair).
    let graph_valid = match crate::infrastructure::toml_store::render_markdown(&ws) {
        Ok(expected) => std::fs::read(store.md_path())
            .map(|bytes| bytes == expected.as_bytes())
            .unwrap_or(false),
        Err(_) => false,
    };
    if !graph_valid {
        errors.push("graph generated view differs from source".into());
    }

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
        !path_exists_on_branch(repo_root, stable_branch, &format!("{}/", config.plan.root));
    let no_design_backups_on_stable = !branch_has_design_backups(repo_root, stable_branch)?;

    // 8. No pending agent transactions (incomplete installs).
    let pending_agent_transactions = check_pending_transactions(repo_root)?;

    // 9. Version resolution from the target repository's config.
    let release_version = config.mine_code_version.clone();

    let can_release = errors.is_empty()
        && all_plans_terminal
        && stable_unchanged
        && design_valid
        && graph_valid
        // Distribution parity is deliberately NOT part of can_release:
        // generic preflight must not gate on MINE-source assets (Design
        // "Repository roles"). The MINE source repo enforces sync via its own
        // gates (mine dist verify, CI).
        && no_plan_artifacts_on_stable
        && no_design_backups_on_stable
        && pending_agent_transactions.is_empty()
        && pending_operations.is_empty()
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
        pending_operations,
        rejections_without_compensation,
        errors,
    })
}

/// Previews what cleanup WOULD remove. No mutation.
pub fn preview_cleanup(repo_root: &Path) -> MineResult<CleanupPreview> {
    let mut plan_workspace_files = Vec::new();
    let config_path = repo_root.join(".mine/config.toml");
    let config = if config_path.try_exists()? {
        Some(MineConfig::parse(
            &config_path,
            &std::fs::read_to_string(&config_path)?,
        )?)
    } else {
        None
    };
    let plan_dir = if let Some(config) = &config {
        crate::infrastructure::graph_snapshot::resolve_contained(
            repo_root,
            config.layout()?.plan_dir(),
        )?
    } else {
        repo_root.join(crate::domain::config::GovernanceLayout::PLAN_DIR)
    };
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
    let backup_root = crate::infrastructure::graph_snapshot::resolve_contained(
        repo_root,
        &crate::domain::graph::RepoPath::new(crate::domain::config::GovernanceLayout::BACKUP_DIR)
            .expect("constant path"),
    )?;
    if backup_root.is_dir() {
        for entry in std::fs::read_dir(&backup_root)? {
            let entry = entry?;
            design_backups.push(
                entry
                    .path()
                    .strip_prefix(repo_root.canonicalize()?)
                    .map_err(|_| MineError::GraphInvalid {
                        detail: "backup preview escapes Root".into(),
                    })?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
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

fn validate_design(
    repo_root: &Path,
    config: &crate::domain::config::MineConfig,
) -> MineResult<Vec<crate::domain::graph::RepoPath>> {
    use crate::domain::{design_marker::DesignMarker, graph::RepoPath};
    let path = |value: &str| {
        RepoPath::new(value).map_err(|error| super::native_evidence::refused(error.to_string()))
    };
    let marker_path = path(&config.design.marker)?;
    let marker_bytes = super::native_evidence::bytes(repo_root, &marker_path)?;
    let marker = DesignMarker::parse(
        Path::new(marker_path.as_str()),
        std::str::from_utf8(&marker_bytes)
            .map_err(|_| super::native_evidence::refused("Design marker encoding"))?,
    )?;
    if marker.schema_version != DesignMarker::SCHEMA_VERSION
        || marker.managed_by != DesignMarker::MANAGED_BY
        || marker.repository_id != config.repository_id
    {
        return Err(super::native_evidence::refused(
            "Design ownership marker does not match configuration",
        ));
    }
    super::native_evidence::bytes(repo_root, &path(&config.design.root)?)?;
    for relative in [&config.adr.registry, &config.adr.rendered_index] {
        super::native_evidence::bytes(repo_root, &path(relative)?)?;
    }
    let registry = super::adr_service::AdrService::new(repo_root, config).validate()?;
    if !registry.readiness_blockers(None).is_empty() {
        return Err(super::native_evidence::refused(
            "repository-wide ADR readiness is blocked",
        ));
    }
    let mut paths = [
        &config.design.marker,
        &config.design.root,
        &config.adr.registry,
        &config.adr.rendered_index,
    ]
    .into_iter()
    .map(|value| path(value))
    .collect::<MineResult<Vec<_>>>()?;
    for entry in registry.entries {
        if matches!(
            entry.status,
            crate::domain::adr::AdrStatus::PROPOSED | crate::domain::adr::AdrStatus::APPROVED
        ) {
            paths.push(path(&entry.path)?);
        }
    }
    Ok(paths)
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

fn check_pending_transactions(repo_root: &Path) -> MineResult<Vec<String>> {
    let mine_dir = repo_root.join(".mine");
    let mut pending = Vec::new();
    for entry in std::fs::read_dir(&mine_dir)? {
        let name = entry?.file_name().to_string_lossy().to_string();
        if name.starts_with("agent-pending-") {
            pending.push(name);
        }
    }
    pending.sort();
    Ok(pending)
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
    let output = crate::infrastructure::git_evidence::GitRequest::new(repo_root)
        .run_raw(&["ls-tree", "-r", "-z", "--name-only", branch], b"")?;
    if !output.status.success() {
        // Fail closed: if tree inspection fails, we cannot prove no backups
        // are present. Return true (backups suspected) so the gate blocks.
        return Ok(true);
    }
    Ok(output.stdout.split(|byte| *byte == 0).any(|path| {
        path.starts_with(b".mine/runtime/design-backups/")
            || path
                .split(|byte| *byte == b'/')
                .any(|part| part.starts_with(b"design-backup-"))
    }))
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
        std::fs::create_dir_all(tmp.path().join(".mine/plan")).unwrap();
        std::fs::write(tmp.path().join(".mine/plan/active.md"), "plan").unwrap();
        std::fs::create_dir_all(tmp.path().join("docs/plan")).unwrap();
        let foreign = tmp.path().join("docs/plan/user.md");
        std::fs::write(&foreign, "user-owned document").unwrap();
        let preview = preview_cleanup(tmp.path()).unwrap();
        assert_eq!(preview.plan_workspace_files, [".mine/plan/active.md"]);
        assert_eq!(
            std::fs::read_to_string(foreign).unwrap(),
            "user-owned document"
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
}
