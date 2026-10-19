//! Repository-specific staging, not a Git migration engine. The release owner
//! supplies an existing clean S checkout; this helper never stages, commits,
//! creates/switches a branch, or mutates the authoritative source tree.

use super::layout_cutover::{self, Projection};
use super::*;
use crate::domain::error::MineResult;
use crate::infrastructure::operation_store::{read_bounded, regular_path};
use crate::infrastructure::{atomic_write, toml_store::TomlStore};
use std::fs;
use std::io::Write;

#[derive(Debug, serde::Serialize)]
pub struct LayoutStageResult {
    pub source_commit: CommitId,
    pub candidate_root: RepoPath,
    pub manifest: RepoPath,
    pub writes_verified: usize,
}

#[derive(serde::Serialize)]
struct StagedChange {
    path: RepoPath,
    before: Option<TreeEntry>,
    after: Option<FileIdentity>,
    mode: Option<u32>,
}

/// Recovery evidence only; deliberately not Deserialize or an authority input.
#[derive(serde::Serialize)]
struct StagingManifest {
    schema_version: u32,
    repository_id: String,
    source_commit: CommitId,
    source_index_sha256: crate::domain::graph::Sha256,
    candidate_root: RepoPath,
    authorizations: Vec<FileIdentity>,
    original_files: Vec<FileIdentity>,
    changes: Vec<StagedChange>,
    writes_verified: usize,
    deletions_verified: usize,
    pending: bool,
}

/// Prepare only the reviewed finite layout projection in an existing dedicated
/// `.mine/runtime/layout-candidates/<name>` linked checkout at S. The caller subsequently uses
/// ordinary Git staging/commit and the existing native Release admission.
/// Any interruption retains S and a pending manifest; no silent retry/rollback.
pub fn stage_layout_cutover(
    root: &Path,
    source: &CommitId,
    candidate: &RepoPath,
) -> MineResult<LayoutStageResult> {
    let config = crate::application::plan_service::root_configuration(root)?;
    let store = TomlStore::from_configuration(root, &config)?;
    store.with_coordinator_lock(|_| {
        stage_locked(root, source, candidate, &mut |_| Ok(()))
            .map_err(|error| crate::application::native_evidence::refused(error.to_string()))
    })
}

pub(super) fn stage_locked(
    root: &Path,
    source: &CommitId,
    candidate: &RepoPath,
    after_write: &mut impl FnMut(usize) -> OperationResult<()>,
) -> OperationResult<LayoutStageResult> {
    let root = root.canonicalize()?;
    let tail = candidate
        .as_str()
        .strip_prefix(".mine/runtime/layout-candidates/")
        .filter(|tail| !tail.is_empty() && !tail.contains('/'))
        .ok_or(OperationError::Refused(
            "layout staging requires one dedicated .mine/runtime/layout-candidates child",
        ))?;
    let _ = tail;
    let target = regular_path(&root, candidate)?.canonicalize()?;
    let mut git = checkpoint_git(&root, None)?;
    let roots = git.text(&[
        "rev-parse",
        "--path-format=absolute",
        "--show-toplevel",
        "--absolute-git-dir",
        "--git-common-dir",
    ])?;
    let roots = roots.lines().collect::<Vec<_>>();
    if roots.len() != 3
        || Path::new(roots[0]).canonicalize()? != root
        || Path::new(roots[1]).canonicalize()? != Path::new(roots[2]).canonicalize()?
        || git.text(&["rev-parse", "--verify", "HEAD^{commit}"])? != source.as_str()
    {
        return Err(OperationError::Refused(
            "layout staging requires actual unchanged Root at source S",
        ));
    }
    let observed = CurationObservation::observe(&root, source, source)?;
    let source_contract = source_contract(&observed)?;
    if git.text(&["symbolic-ref", "-q", "HEAD"])?
        != format!("refs/heads/{}", source_contract.config.branches.integration)
    {
        return Err(OperationError::Refused(
            "layout staging Root is not on integration",
        ));
    }
    let projected = layout_cutover::projection(&observed)?;
    let inventory = git.worktrees()?;
    let checkout = inventory
        .iter()
        .filter(|entry| Path::new(&entry.path).canonicalize().ok().as_ref() == Some(&target))
        .collect::<Vec<_>>();
    if checkout.len() != 1
        || checkout[0].head != source.as_str()
        || checkout[0].bare
        || checkout[0].locked
        || checkout[0].prunable
        || target == root
        || checkout[0].branch.is_some()
        || !checkout[0].detached
    {
        return Err(OperationError::Refused(
            "layout staging target is not a dedicated registered S checkout",
        ));
    }
    let mut target_git = checkpoint_git(&target, None)?;
    if Path::new(&target_git.text(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?)
        .canonicalize()?
        != Path::new(roots[2]).canonicalize()?
    {
        return Err(OperationError::Refused(
            "layout staging target belongs to another repository",
        ));
    }
    // Child rules override repository/global defaults, not info/attributes or
    // copied deeper rules. Refuse those foreign policies before any write.
    for request in [&mut git, &mut target_git] {
        let policy = request.text(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/attributes",
        ])?;
        let policy = Path::new(&policy);
        if !policy.is_absolute() {
            return Err(OperationError::Journal);
        }
        if policy.try_exists()? && active_rules(&read_bounded(policy)?) {
            return Err(OperationError::Refused(
                "foreign info/attributes requires review before cutover; no staging writes made",
            ));
        }
    }
    for file in &projected.source_files {
        if file
            .path
            .as_str()
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(".gitattributes"))
            && active_rules(&read_bounded(&regular_path(&root, &file.path)?)?)
        {
            return Err(OperationError::Refused(
                "deeper Design attributes require review before relocation; no staging writes made",
            ));
        }
    }
    super::super::retained::verification::prove_checkout_bytes(&root, &mut git, source)?;
    super::super::retained::verification::prove_checkout_bytes(&target, &mut target_git, source)?;
    if !git
        .run(
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            b"",
        )?
        .is_empty()
        || !target_git
            .run(
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                    "--ignored=matching",
                ],
                b"",
            )?
            .is_empty()
    {
        return Err(OperationError::Refused(
            "layout staging source/target contains foreign dirty data (including ignored target data)",
        ));
    }
    for base in [&root, &target] {
        for namespace in [".mine/design", ".mine/plan"] {
            if regular_path(base, &RepoPath::new(namespace)?)?.try_exists()? {
                return Err(OperationError::Refused(
                    "existing destination namespace must not be claimed by cutover",
                ));
            }
        }
        for file in projected
            .source_files
            .iter()
            .chain(&projected.authorizations)
        {
            let bytes = read_bounded(&regular_path(base, &file.path)?)?;
            if bytes.len() as u64 != file.bytes || digest(&bytes) != file.sha256 {
                return Err(OperationError::Refused(
                    "source/initial-checkout raw bytes differ; do not reconstruct frozen input from normalized blobs",
                ));
            }
        }
    }
    let mut physical_targets = BTreeSet::new();
    for (relative, after) in &projected.changes {
        let physical = regular_path(&target, relative)?;
        let identity = physical.to_string_lossy().to_string();
        #[cfg(windows)]
        let identity = identity.to_ascii_lowercase();
        if !physical_targets.insert(identity) {
            return Err(OperationError::Refused("cutover target paths alias"));
        }
        if after.is_some() && observed.source_entry(relative).is_none() && physical.try_exists()? {
            return Err(OperationError::Refused(
                "cutover destination collision before any write",
            ));
        }
    }
    let required = projected
        .changes
        .values()
        .flatten()
        .try_fold(0u64, |total, entry| {
            total
                .checked_add(entry.bytes.len() as u64)
                .ok_or(OperationError::Journal)
        })?;
    if fs4::available_space(&target)? < required.saturating_mul(2).saturating_add(4 * 1024 * 1024) {
        return Err(OperationError::Refused(
            "insufficient measured disk space for isolated cutover copies",
        ));
    }
    let index = git.text(&["rev-parse", "--path-format=absolute", "--git-path", "index"])?;
    let index_digest = digest(&read_bounded(Path::new(&index))?);
    let key = digest(format!("{}:{}", source.as_str(), candidate.as_str()).as_bytes());
    let manifest_path = RepoPath::new(format!(
        ".mine/runtime/layout-staging/{}.json",
        key.as_str()
    ))?;
    let manifest_file = regular_path(&root, &manifest_path)?;
    // Refuse an existing record even when it looks complete. It is evidence,
    // not permission to start a second operation or overwrite partial work.
    if manifest_file.try_exists()? {
        return Err(OperationError::Refused(
            "existing cutover staging evidence requires inspection, not a new execution",
        ));
    }
    let changes = projected
        .changes
        .iter()
        .map(|(path, after)| StagedChange {
            path: path.clone(),
            before: observed.source_entry(path).cloned(),
            after: after.as_ref().map(|file| FileIdentity {
                path: path.clone(),
                bytes: file.bytes.len() as u64,
                sha256: digest(&file.bytes),
            }),
            mode: after.as_ref().map(|file| file.mode),
        })
        .collect();
    let mut manifest = StagingManifest {
        schema_version: 1,
        repository_id: source_contract.config.repository_id,
        source_commit: source.clone(),
        source_index_sha256: index_digest.clone(),
        candidate_root: candidate.clone(),
        authorizations: projected.authorizations.clone(),
        original_files: projected.source_files.clone(),
        changes,
        writes_verified: 0,
        deletions_verified: 0,
        pending: true,
    };
    publish_manifest(&manifest_file, &manifest)?;
    let result = apply(
        &target,
        &projected,
        &manifest_file,
        &mut manifest,
        after_write,
    );
    if let Err(error) = result {
        return Err(OperationError::Io(std::io::Error::other(format!(
            "layout staging pending at {}; original source S is preserved: {error}",
            manifest_path.as_str()
        ))));
    }
    if git.text(&["rev-parse", "--verify", "HEAD^{commit}"])? != source.as_str()
        || digest(&read_bounded(Path::new(&index))?) != index_digest
    {
        return Err(OperationError::Refused(
            "source changed during staging; candidate/manifest remain pending",
        ));
    }
    super::super::retained::verification::prove_checkout_bytes(&root, &mut git, source)?;
    manifest.pending = false;
    save_manifest(&manifest_file, &manifest)?;
    Ok(LayoutStageResult {
        source_commit: source.clone(),
        candidate_root: candidate.clone(),
        manifest: manifest_path,
        writes_verified: manifest.writes_verified,
    })
}

fn apply(
    target: &Path,
    projected: &Projection,
    manifest_file: &Path,
    manifest: &mut StagingManifest,
    after_write: &mut impl FnMut(usize) -> OperationResult<()>,
) -> OperationResult<()> {
    // Establish exact owned controls first; no ignored output is copied.
    let mut writes = projected
        .changes
        .iter()
        .filter_map(|(path, after)| after.as_ref().map(|file| (path, file)))
        .collect::<Vec<_>>();
    writes.sort_by_key(|(path, _)| {
        (
            !matches!(path.as_str(), ".mine/.gitignore" | ".mine/.gitattributes"),
            path.as_str(),
        )
    });
    for (path, expected) in &writes {
        let path = regular_path(target, path)?;
        atomic_write::write(&path, &expected.bytes)?;
        if fs::read(&path)? != expected.bytes {
            return Err(OperationError::Evidence(
                "staged copy byte verification failed",
            ));
        }
        manifest.writes_verified += 1;
        save_manifest(manifest_file, manifest)?;
        after_write(manifest.writes_verified)?;
    }
    crate::infrastructure::governance_git::inspect(target)?;
    let governed = writes
        .iter()
        .filter(|(path, _)| {
            path.as_str().starts_with(".mine/")
                && (path.as_str().ends_with(".md")
                    || path.as_str().ends_with(".toml")
                    || path.as_str().ends_with(".json"))
        })
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    crate::infrastructure::governance_git::check_attributes(target, &governed, false)?;
    let mut directories = BTreeSet::new();
    for (relative, after) in &projected.changes {
        if after.is_some() {
            continue;
        }
        let path = regular_path(target, relative)?;
        if !fs::symlink_metadata(&path)?.is_file() {
            return Err(OperationError::Refused(
                "legacy staged deletion is not an exact regular source file",
            ));
        }
        fs::remove_file(path)?;
        manifest.deletions_verified += 1;
        save_manifest(manifest_file, manifest)?;
        let mut parent = relative.as_str().rsplit_once('/').map(|pair| pair.0);
        while let Some(directory) = parent {
            if !["docs/design", "docs/plan"]
                .iter()
                .any(|owned| crate::domain::path::is_within(owned, directory))
            {
                break;
            }
            directories.insert(directory.to_string());
            parent = directory.rsplit_once('/').map(|pair| pair.0);
        }
    }
    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.matches('/').count()));
    for directory in directories {
        // Empty-directory removal only; never recursive deletion or a source move.
        fs::remove_dir(regular_path(target, &RepoPath::new(directory)?)?)?;
    }
    for (path, expected) in writes {
        if fs::read(regular_path(target, path)?)? != expected.bytes {
            return Err(OperationError::Evidence(
                "staged destination changed during verified deletion",
            ));
        }
    }
    Ok(())
}

fn active_rules(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).map_or(true, |text| {
        text.lines().any(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#')
        })
    })
}

fn encoded(manifest: &StagingManifest) -> OperationResult<Vec<u8>> {
    serde_json::to_vec_pretty(manifest).map_err(|_| OperationError::Journal)
}
fn publish_manifest(path: &Path, manifest: &StagingManifest) -> OperationResult<()> {
    let parent = path.parent().ok_or(OperationError::Journal)?;
    fs::create_dir_all(parent)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&encoded(manifest)?)?;
    file.sync_all()?;
    Ok(())
}
fn save_manifest(path: &Path, manifest: &StagingManifest) -> OperationResult<()> {
    Ok(atomic_write::write(path, &encoded(manifest)?)?)
}

#[cfg(test)]
mod tests {
    use super::super::layout_cutover::ProjectedFile;
    use super::*;

    #[test]
    fn interrupted_copy_retains_source_old_candidate_files_and_pending_evidence() {
        // This tests the deterministic IO interruption seam, not authorization.
        // Authorization and real registered S/C checkouts have separate tests.
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let old = RepoPath::new("docs/design/decisions/frozen.md").unwrap();
        let new = RepoPath::new(".mine/design/decisions/frozen.md").unwrap();
        let approved = b"# Frozen approval\r\n";
        for root in [source.path(), target.path()] {
            let file = root.join(old.as_str());
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, approved).unwrap();
        }
        let projection = Projection {
            changes: BTreeMap::from([
                (old.clone(), None),
                (
                    new.clone(),
                    Some(ProjectedFile {
                        mode: 0o100644,
                        bytes: approved.to_vec(),
                    }),
                ),
            ]),
            authorizations: vec![],
            source_files: vec![],
        };
        let manifest_file = source
            .path()
            .join(".mine/runtime/layout-staging/interrupted.json");
        let mut manifest = StagingManifest {
            schema_version: 1,
            repository_id: "synthetic-io-seam".into(),
            source_commit: CommitId::new("1".repeat(40)).unwrap(),
            source_index_sha256: digest(b"index"),
            candidate_root: RepoPath::new(".mine/runtime/layout-candidates/interrupted").unwrap(),
            authorizations: vec![],
            original_files: vec![],
            changes: vec![],
            writes_verified: 0,
            deletions_verified: 0,
            pending: true,
        };
        publish_manifest(&manifest_file, &manifest).unwrap();
        let error = apply(
            target.path(),
            &projection,
            &manifest_file,
            &mut manifest,
            &mut |_| {
                Err(OperationError::Refused(
                    "deterministic interruption after verified copy",
                ))
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("deterministic interruption"));
        assert_eq!(
            fs::read(source.path().join(old.as_str())).unwrap(),
            approved
        );
        assert_eq!(
            fs::read(target.path().join(old.as_str())).unwrap(),
            approved
        );
        assert_eq!(
            fs::read(target.path().join(new.as_str())).unwrap(),
            approved
        );
        let retained = fs::read(&manifest_file).unwrap();
        let state: serde_json::Value = serde_json::from_slice(&retained).unwrap();
        assert_eq!(state["pending"], true);
        assert_eq!(state["writes_verified"], 1);
        assert_eq!(state["deletions_verified"], 0);
        assert!(publish_manifest(&manifest_file, &manifest).is_err());
        assert_eq!(fs::read(manifest_file).unwrap(), retained);
    }
}
