//! Controlled Git evidence and linked-worktree preparation.
//!
//! `docs/design/interfaces/cli-contract.md` and the branch-and-plan-lifecycle
//! design require MINE to inspect repository Git state. MINE never commits,
//! merges, resets, cleans, stashes, rebases, pushes, or force-pushes. The only
//! mutations here are fixed-argument branch/worktree preparation calls and the
//! provably-lossless cleanup of a never-started `plan/*` lane performed by
//! `mine plan withdraw` (ADR-0012 owner-authorized retirement): a clean
//! linked worktree removal (`git worktree remove`, never `--force`) and a
//! branch delete (`git branch -d`, or `-D` only after an exact-tip proof for a
//! compensation lane).
//!
//! To adhere to "no arbitrary shell execution" (no `sh -c`), every query
//! invokes `git` directly as a controlled subprocess with an explicit,
//! fixed argument vector — never a shell string. A failed or absent `git`
//! binary degrades gracefully (queries return `None`/`false` or a
//! [`MineError::Io`]); callers decide whether Git evidence is required.
//!
//! All functions are `&self`-free so they can be reused from tests with a
//! temporary repository.

use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;

use crate::domain::error::{MineError, MineResult};
use crate::infrastructure::git_evidence::GitRequest;

/// One stable `git worktree list --porcelain -z` record.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorktreeInfo {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// The git executable. Resolved from `PATH` (never via a shell).
#[cfg(test)]
const GIT: &str = "git";

/// Runs a read-only `git` command in `repo_root` with the given args and
/// returns the trimmed stdout on success.
fn run_git(repo_root: &Path, args: &[&str]) -> MineResult<String> {
    GitRequest::new(repo_root).text(args)
}

fn run_git_bytes(repo_root: &Path, args: &[&str]) -> MineResult<Vec<u8>> {
    GitRequest::new(repo_root).run(args, &[])
}

fn run_git_mutation(repo_root: &Path, args: &[&str]) -> MineResult<()> {
    run_git(repo_root, args).map(|_| ())
}

/// Creates a local branch at `start` without switching Root.
pub fn create_branch(repo_root: &Path, branch: &str, start: &str) -> MineResult<()> {
    run_git_mutation(repo_root, &["branch", branch, start])
}

/// Adds a linked worktree for an existing branch, without force.
pub fn add_worktree(repo_root: &Path, path: &Path, branch: &str) -> MineResult<()> {
    let path = path.to_str().ok_or_else(|| MineError::GraphInvalid {
        detail: "worktree path is not valid UTF-8".to_string(),
    })?;
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    run_git_mutation(repo_root, &["worktree", "add", path, branch])
}

/// Switches a clean linked checkout to an existing branch.
pub fn switch_branch(worktree: &Path, branch: &str) -> MineResult<()> {
    run_git_mutation(worktree, &["switch", branch])
}

/// Removes a linked worktree without force. Refuses a dirty worktree (Git
/// itself fails), so unique uncommitted work can never be discarded here.
pub fn remove_worktree(repo_root: &Path, path: &Path) -> MineResult<()> {
    let path = path.to_str().ok_or_else(|| MineError::GraphInvalid {
        detail: "worktree path is not valid UTF-8".to_string(),
    })?;
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    run_git_mutation(repo_root, &["worktree", "remove", path])
}

/// Deletes a local branch. `force = false` uses `git branch -d`, which
/// succeeds only when the branch is fully merged; `force = true` uses `-D`
/// and must be preceded by an exact-tip proof by the caller.
pub fn delete_branch(repo_root: &Path, branch: &str, force: bool) -> MineResult<()> {
    let flag = if force { "-D" } else { "-d" };
    run_git_mutation(repo_root, &["branch", flag, branch])
}

/// Parses Git's stable NUL-delimited worktree inventory. The first record is
/// the main worktree and therefore the authoritative MINE integration root.
pub fn list_worktrees(repo_root: &Path) -> MineResult<Vec<WorktreeInfo>> {
    GitRequest::new(repo_root).worktrees()
}

pub(crate) fn parse_worktrees(bytes: &[u8]) -> MineResult<Vec<WorktreeInfo>> {
    let mut records = Vec::new();
    let mut current: Option<WorktreeInfo> = None;
    for token in bytes.split(|byte| *byte == 0) {
        if token.is_empty() {
            if let Some(record) = current.take() {
                records.push(record);
            }
            continue;
        }
        let line =
            std::str::from_utf8(token).map_err(|_| crate::domain::error::GitFailure::Protocol)?;
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(record) = current.take() {
                records.push(record);
            }
            current = Some(WorktreeInfo {
                path: path.to_string(),
                head: String::new(),
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            });
        } else if let Some(record) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                record.head = head.to_string();
            } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                record.branch = Some(branch.to_string());
            } else if line == "bare" {
                record.bare = true;
            } else if line == "detached" {
                record.detached = true;
            } else if line == "locked" || line.starts_with("locked ") {
                record.locked = true;
            } else if line == "prunable" || line.starts_with("prunable ") {
                record.prunable = true;
            }
        }
    }
    if let Some(record) = current {
        records.push(record);
    }
    if records.is_empty() {
        return Err(MineError::Io(std::io::Error::other(
            "git worktree list returned no records",
        )));
    }
    Ok(records)
}

/// Resolves `(authoritative main worktree, caller worktree)` from any checkout
/// belonging to the repository.
pub fn authority_and_caller_root(start: &Path) -> MineResult<(PathBuf, PathBuf)> {
    let caller = PathBuf::from(run_git(
        start,
        &["rev-parse", "--path-format=absolute", "--show-toplevel"],
    )?)
    .canonicalize()?;
    let authority = PathBuf::from(&list_worktrees(&caller)?[0].path).canonicalize()?;
    Ok((authority, caller))
}

/// Returns the current branch name (the symbolic-ref of `HEAD`), or `None` if
/// `HEAD` is detached or git evidence is unavailable.
///
/// # Errors
/// Returns [`MineError::Io`] only on a filesystem/invocation failure; a
/// detached HEAD returns `Ok(None)`.
pub fn current_branch(repo_root: &Path) -> MineResult<Option<String>> {
    match run_git(repo_root, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Ok(name) => Ok(if name.is_empty() { None } else { Some(name) }),
        Err(MineError::Io(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Returns the full `HEAD` commit hash, or `None` if unavailable (e.g. an
/// unborn branch with no commits).
pub fn head_commit(repo_root: &Path) -> Option<String> {
    let mut request = GitRequest::new(repo_root);
    let head = request.text(&["rev-parse", "HEAD"]).ok()?;
    request.validate_commits(std::slice::from_ref(&head)).ok()?;
    Some(head)
}

/// Returns `true` if the working tree has no uncommitted or untracked changes.
pub fn is_clean(repo_root: &Path) -> bool {
    run_git(repo_root, &["status", "--porcelain"])
        .ok()
        .is_some_and(|s| s.is_empty())
}

/// Lists tracked files (repository-relative, forward slashes) via
/// `git ls-files -z`. Returns an error when Git cannot be invoked (fail
/// closed for release gates).
pub fn list_tracked_files(repo_root: &Path) -> MineResult<Vec<String>> {
    let output = run_git_bytes(repo_root, &["ls-files", "-z"])?;
    let mut files = Vec::new();
    for chunk in output.split(|b| *b == 0) {
        if chunk.is_empty() {
            continue;
        }
        let s = String::from_utf8_lossy(chunk).into_owned();
        files.push(s.replace('\\', "/"));
    }
    Ok(files)
}

/// Returns `true` if a local branch named `name` exists.
pub fn branch_exists(repo_root: &Path, name: &str) -> bool {
    run_git(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
    )
    .ok()
    .is_some_and(|code| !code.is_empty())
}

/// Resolves the commit at a local branch.
pub fn head_of_branch(repo_root: &Path, name: &str) -> MineResult<String> {
    resolve_commit(repo_root, &format!("refs/heads/{name}"))
}

/// Resolves a revision to its full commit object ID and rejects non-commits.
pub fn resolve_commit(repo_root: &Path, revision: &str) -> MineResult<String> {
    run_git(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
    )
}

/// Resolves the first parent of a commit to its full object ID.
pub fn first_parent(repo_root: &Path, commit: &str) -> MineResult<String> {
    resolve_commit(repo_root, &format!("{commit}^"))
}

/// Returns `true` if commit `ancestor` is an ancestor of commit `descendant`
/// (i.e. `descendant` contains `ancestor` in its history).
pub fn is_ancestor(repo_root: &Path, ancestor: &str, descendant: &str) -> bool {
    is_ancestor_checked(repo_root, ancestor, descendant).unwrap_or(false)
}

/// Checks ancestry while preserving the distinction between a false relation
/// and a Git invocation/object failure.
pub fn is_ancestor_checked(repo_root: &Path, ancestor: &str, descendant: &str) -> MineResult<bool> {
    let output = GitRequest::new(repo_root)
        .run_raw(&["merge-base", "--is-ancestor", ancestor, descendant], &[])?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(crate::domain::error::GitFailure::ChildExit.into()),
    }
}

/// Detects the stable branch for `repo_root`. Uses the configured default if
/// git evidence is unavailable, the HEAD is detached, or the detected branch
/// is the integration/plan branch. Returns the configured default otherwise.
///
/// This is used by `mine init` to record the stable branch without forcing a
/// specific name when the repository already has a conventional default.
#[must_use]
pub fn detect_stable_branch(repo_root: &Path, configured_default: &str) -> String {
    if let Ok(Some(branch)) = current_branch(repo_root) {
        // Never record the integration or a plan branch as the stable branch.
        if branch == "dev" || branch.starts_with("plan/") || branch.starts_with("hotfix/") {
            return configured_default.to_string();
        }
        // If the detected branch exists and is not the integration branch,
        // record it as the stable branch.
        if branch_exists(repo_root, &branch) {
            return branch;
        }
    }
    configured_default.to_string()
}

/// Summarizes Git evidence for the JSON `data` object of commands that report
/// repository state, without performing any mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitEvidence {
    pub current_branch: Option<String>,
    pub head_commit: Option<String>,
    pub clean: bool,
}

impl GitEvidence {
    /// Collects read-only Git evidence for `repo_root`. Never mutates Git.
    pub fn collect(repo_root: &Path) -> Self {
        Self {
            current_branch: current_branch(repo_root).ok().flatten(),
            head_commit: head_commit(repo_root),
            clean: is_clean(repo_root),
        }
    }

    /// Returns the repository root if it contains a `.git` entry (worktree or
    /// directory), else `None`. This is the cheapest "is this a git repo"
    /// check that does not invoke git at all.
    #[must_use]
    pub fn repository_root(start: &Path) -> Option<PathBuf> {
        let mut current: &Path = start;
        loop {
            if current.join(".git").exists() {
                return Some(current.to_path_buf());
            }
            {
                let parent = current.parent()?;
                current = parent
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &Path) {
        Command::new(GIT)
            .arg("-C")
            .arg(dir)
            .args(["init", "--quiet"])
            .status()
            .unwrap();
        // Configure a local user to allow commits.
        Command::new(GIT)
            .arg("-C")
            .arg(dir)
            .args(["config", "user.email", "test@example.com"])
            .status()
            .unwrap();
        Command::new(GIT)
            .arg("-C")
            .arg(dir)
            .args(["config", "user.name", "test"])
            .status()
            .unwrap();
        // These evidence/branch fixtures do not test EOL conversion or signing.
        // Own their local policy instead of inheriting the host's CRLF/signers.
        for (key, value) in [("core.autocrlf", "false"), ("commit.gpgSign", "false")] {
            assert!(
                Command::new(GIT)
                    .arg("-C")
                    .arg(dir)
                    .args(["config", key, value])
                    .status()
                    .unwrap()
                    .success(),
                "private fixture policy {key}"
            );
        }
        std::fs::write(dir.join("README.md"), "hi\n").unwrap();
        Command::new(GIT)
            .arg("-C")
            .arg(dir)
            .args(["add", "README.md"])
            .status()
            .unwrap();
        Command::new(GIT)
            .arg("-C")
            .arg(dir)
            .args(["commit", "--quiet", "-m", "init"])
            .status()
            .unwrap();
    }

    #[test]
    fn collects_evidence_from_a_real_repo() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // The default branch name varies across git versions; just assert we
        // detect *a* branch and a commit.
        let ev = GitEvidence::collect(dir.path());
        assert!(ev.current_branch.is_some(), "current branch detected");
        assert!(ev.head_commit.is_some(), "HEAD commit detected");
        assert!(ev.clean, "working tree clean after a clean commit");
    }

    #[test]
    fn detects_unclean_working_tree() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("untracked.txt"), "x").unwrap();
        let ev = GitEvidence::collect(dir.path());
        assert!(!ev.clean);
    }

    #[test]
    fn detects_repository_root_walks_up() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let sub = dir.path().join("nested").join("deeper");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(
            GitEvidence::repository_root(&sub),
            Some(dir.path().to_path_buf())
        );
    }

    #[test]
    fn detect_stable_branch_avoids_integration_and_plan_branches() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // Create and switch to a plan branch.
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            .args(["checkout", "--quiet", "-b", "plan/99-demo"])
            .status()
            .unwrap();
        assert_eq!(
            detect_stable_branch(dir.path(), "master"),
            "master",
            "must not record a plan branch as stable"
        );
    }

    #[test]
    fn is_ancestor_true_for_chained_commits() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let first = head_commit(dir.path()).unwrap();
        std::fs::write(dir.path().join("two.txt"), "2\n").unwrap();
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            .args(["add", "two.txt"])
            .status()
            .unwrap();
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            .args(["commit", "--quiet", "-m", "two"])
            .status()
            .unwrap();
        let second = head_commit(dir.path()).unwrap();
        assert!(is_ancestor(dir.path(), &first, &second));
        assert!(!is_ancestor(dir.path(), &second, &first));
    }

    #[test]
    fn porcelain_inventory_resolves_main_and_linked_roots() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let linked = dir.path().join("linked plan");
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            .args(["worktree", "add", "--quiet", "-b", "plan/plan01-test"])
            .arg(&linked)
            .status()
            .unwrap();
        let records = list_worktrees(&linked).unwrap();
        assert_eq!(records.len(), 2);
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert_eq!(records[1].branch.as_deref(), Some("plan/plan01-test"));
        let (authority, caller) = authority_and_caller_root(&linked).unwrap();
        assert_eq!(authority, dir.path().canonicalize().unwrap());
        assert_eq!(caller, linked.canonicalize().unwrap());
    }

    #[test]
    fn remove_worktree_and_delete_branch_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let linked = dir.path().join("lane");
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            .args(["worktree", "add", "--quiet", "-b", "plan/plan09-lane"])
            .arg(&linked)
            .status()
            .unwrap();
        assert!(linked.exists());

        // Removing a clean linked worktree succeeds and unregisters it.
        remove_worktree(dir.path(), &linked).unwrap();
        assert!(!linked.exists());
        assert!(
            list_worktrees(dir.path())
                .unwrap()
                .iter()
                .all(|w| !w.path.contains("lane"))
        );

        // `-d` deletes a branch merged into HEAD.
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        assert!(branch_exists(dir.path(), "plan/plan09-lane"));
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        delete_branch(dir.path(), "plan/plan09-lane", false).unwrap();
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        assert!(!branch_exists(dir.path(), "plan/plan09-lane"));
    }

    #[test]
    fn remove_worktree_refuses_dirty_worktree() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let linked = dir.path().join("dirty lane");
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            .args(["worktree", "add", "--quiet", "-b", "plan/plan08-dirty"])
            .arg(&linked)
            .status()
            .unwrap();
        std::fs::write(linked.join("scratch.txt"), "uncommitted\n").unwrap();
        assert!(remove_worktree(dir.path(), &linked).is_err());
        assert!(linked.exists(), "dirty worktree is preserved");
    }

    #[test]
    fn delete_branch_minus_d_refuses_unmerged_branch() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // Unique commit on a side branch: `-d` must refuse, `-D` deletes.
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            .args(["switch", "--quiet", "-c", "plan/plan07-unique"])
            .status()
            .unwrap();
        std::fs::write(dir.path().join("unique.txt"), "unique\n").unwrap();
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            .args(["add", "unique.txt"])
            .status()
            .unwrap();
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            .args(["commit", "--quiet", "-m", "unique"])
            .status()
            .unwrap();
        Command::new(GIT)
            .arg("-C")
            .arg(dir.path())
            .args(["switch", "--quiet", "master"])
            .status()
            .unwrap();
        if !branch_exists(dir.path(), "master") {
            // The default branch name varies across git versions.
            Command::new(GIT)
                .arg("-C")
                .arg(dir.path())
                .args(["switch", "--quiet", "main"])
                .status()
                .unwrap();
        }
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        assert!(delete_branch(dir.path(), "plan/plan07-unique", false).is_err());
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        assert!(branch_exists(dir.path(), "plan/plan07-unique"));
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        delete_branch(dir.path(), "plan/plan07-unique", true).unwrap();
        // mine-release-allow-plan-reference: plan-id lifecycle fixture
        assert!(!branch_exists(dir.path(), "plan/plan07-unique"));
    }
}
