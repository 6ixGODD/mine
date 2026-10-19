//! Strict, contained adapter evidence. No caller JSON is an IO grant.
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{CommitId, FileIdentity, RepoPath};
pub(crate) use crate::infrastructure::strict_json::strict_json;
use serde::de::DeserializeOwned;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) fn refused(detail: impl Into<String>) -> MineError {
    MineError::GraphInvalid {
        detail: detail.into(),
    }
}

pub(crate) fn input<T: DeserializeOwned>(root: &Path, path: &RepoPath) -> MineResult<T> {
    strict_json(&bytes(root, path)?)
}

pub(crate) fn bytes(root: &Path, path: &RepoPath) -> MineResult<Vec<u8>> {
    let target = crate::infrastructure::operation_store::regular_path(root, path)?;
    crate::infrastructure::operation_store::read_bounded(&target)
        .map_err(|e| refused(e.to_string()))
}

pub(crate) fn one_fenced_json<T: DeserializeOwned>(body: &[u8], marker: &str) -> MineResult<T> {
    use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
    let text = std::str::from_utf8(body).map_err(|_| refused("report encoding"))?;
    let mut found = None;
    let mut capture = None;
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if info.starts_with(marker) =>
            {
                if info.as_ref() != marker || found.is_some() || capture.is_some() {
                    return Err(refused("duplicate or malformed evidence fence"));
                }
                capture = Some(String::new());
            }
            Event::Text(value) if capture.is_some() => {
                capture.as_mut().expect("capturing").push_str(&value)
            }
            Event::End(TagEnd::CodeBlock) if capture.is_some() => found = capture.take(),
            _ => {}
        }
    }
    strict_json(
        found
            .ok_or_else(|| refused("required evidence fence missing"))?
            .as_bytes(),
    )
}

/// Prove that actual bytes belong to this real commit, not merely a clean
/// status that might hide ignored/skip-worktree input. Git applies only its
/// built-in storage conversions; external clean filters are not run here.
pub(crate) fn committed_bytes(
    root: &Path,
    commit: &CommitId,
    path: &RepoPath,
    body: &[u8],
) -> MineResult<()> {
    let mut git = crate::infrastructure::graph_checkpoint::checkpoint_git(root, None)
        .map_err(|e| refused(e.to_string()))?;
    git.validate_commits(&[commit.as_str().into()])?;
    let attributes = git.run(&["check-attr", "-z", "filter", "--", path.as_str()], b"")?;
    let unset = format!("{}\0filter\0unset\0", path.as_str());
    let unspecified = format!("{}\0filter\0unspecified\0", path.as_str());
    if attributes != unset.as_bytes() && attributes != unspecified.as_bytes() {
        return Err(refused("unsupported Plan-body storage filter"));
    }
    let hash = git.run(
        &[
            "hash-object",
            &format!("--path={}", path.as_str()),
            "--stdin",
        ],
        body,
    )?;
    let hash = std::str::from_utf8(&hash)
        .map_err(|_| refused("object hash encoding"))?
        .trim();
    if hash.len() != commit.as_str().len()
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(refused("invalid native object hash response"));
    }
    let tree = git.run(
        &[
            "ls-tree",
            "-z",
            "--full-tree",
            commit.as_str(),
            "--",
            path.as_str(),
        ],
        b"",
    )?;
    if !["100644", "100755"]
        .iter()
        .any(|mode| tree == format!("{mode} blob {hash}\t{}\0", path.as_str()).as_bytes())
    {
        return Err(refused(
            "actual Plan body is not committed at its observed/reviewed commit",
        ));
    }
    Ok(())
}

/// Batch distinct committed-file proofs. Path argv is bounded per child, and
/// check-attr uses NUL framing. Only built-in Git storage conversions run.
pub(crate) fn committed_files(
    root: &Path,
    commit: &CommitId,
    files: &[crate::domain::graph::FileIdentity],
) -> MineResult<()> {
    observe_committed_files(root, commit, files, None).map(drop)
}

/// Opaque immutable-object evidence, scoped to the current native request.
/// There is deliberately no public constructor, clone or serialization path.
/// Reuse still reobserves contained bytes, attributes and native storage hashes.
pub struct CommittedFilesProof {
    root: PathBuf,
    commit: CommitId,
    files: Vec<FileIdentity>,
    hashes: Vec<String>,
    request: Option<Arc<()>>,
}

pub(crate) fn prove_committed_files(
    root: &Path,
    commit: &CommitId,
    files: &[FileIdentity],
) -> MineResult<CommittedFilesProof> {
    let hashes = observe_committed_files(root, commit, files, None)?;
    Ok(CommittedFilesProof {
        root: root.canonicalize()?,
        commit: commit.clone(),
        files: files.to_vec(),
        hashes,
        request: crate::infrastructure::git_evidence::request_identity(),
    })
}

impl CommittedFilesProof {
    pub(crate) fn reobserve(
        &self,
        root: &Path,
        commit: &CommitId,
        files: &[FileIdentity],
    ) -> MineResult<()> {
        let current = crate::infrastructure::git_evidence::request_identity();
        if self.root != root.canonicalize()?
            || &self.commit != commit
            || self.files != files
            || !self
                .request
                .as_ref()
                .zip(current.as_ref())
                .is_some_and(|(before, now)| Arc::ptr_eq(before, now))
        {
            return Err(refused(
                "committed proof belongs to a different observation",
            ));
        }
        observe_committed_files(root, commit, files, Some(self)).map(drop)
    }
}

fn observe_committed_files(
    root: &Path,
    commit: &CommitId,
    files: &[FileIdentity],
    known: Option<&CommittedFilesProof>,
) -> MineResult<Vec<String>> {
    let mut git = crate::infrastructure::graph_checkpoint::checkpoint_git(root, None)
        .map_err(|e| refused(e.to_string()))?;
    if known.is_none() {
        git.validate_commits(&[commit.as_str().into()])?;
    }
    let mut observed_hashes = Vec::new();
    let mut attribute_input = Vec::new();
    for file in files {
        // Establish regular contained bytes before Git is allowed to open a
        // filename. In particular, never hash an artifact through a symlink.
        let actual = bytes(root, &file.path)?;
        if actual.len() as u64 != file.bytes
            || crate::infrastructure::graph_checkpoint::digest(&actual) != file.sha256
        {
            return Err(refused("unverified committed evidence input"));
        }
        attribute_input.extend_from_slice(file.path.as_str().as_bytes());
        attribute_input.push(0);
    }
    let attributes = git.run(&["check-attr", "-z", "--stdin", "filter"], &attribute_input)?;
    let mut fields = attributes.split(|v| *v == 0);
    for file in files {
        if fields.next() != Some(file.path.as_str().as_bytes())
            || fields.next() != Some(b"filter".as_slice())
            || !fields
                .next()
                .is_some_and(|v| v == b"unset" || v == b"unspecified")
        {
            return Err(refused("unsupported committed evidence storage filter"));
        }
    }
    if fields.next() != Some(b"".as_slice()) || fields.next().is_some() {
        return Err(refused("attribute batch framing"));
    }
    let mut start = 0;
    while start < files.len() {
        let mut end = start;
        let mut units = 0;
        while end < files.len()
            && (end == start || units + files[end].path.as_str().encode_utf16().count() + 3 < 8192)
        {
            units += files[end].path.as_str().encode_utf16().count() + 3;
            end += 1;
        }
        let batch = &files[start..end];
        let mut hash_args = vec!["hash-object", "--"];
        hash_args.extend(batch.iter().map(|v| v.path.as_str()));
        let hashes = git.run(&hash_args, b"")?;
        let hashes = std::str::from_utf8(&hashes).map_err(|_| refused("object batch encoding"))?;
        let hashes = hashes.split_terminator('\n').collect::<Vec<_>>();
        if hashes.len() != batch.len()
            || hashes.iter().any(|v| {
                v.len() != commit.as_str().len()
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return Err(refused("object batch framing"));
        }
        if let Some(known) = known {
            if !hashes
                .iter()
                .copied()
                .eq(known.hashes[start..end].iter().map(String::as_str))
            {
                return Err(refused("committed evidence storage conversion changed"));
            }
        } else {
            let mut tree_args = vec!["ls-tree", "-z", "--full-tree", commit.as_str(), "--"];
            tree_args.extend(batch.iter().map(|v| v.path.as_str()));
            let tree = git.run(&tree_args, b"")?;
            let mut entries = std::collections::BTreeMap::new();
            let framed = tree
                .strip_suffix(&[0])
                .ok_or_else(|| refused("tree batch framing"))?;
            for entry in framed.split(|v| *v == 0) {
                let entry =
                    std::str::from_utf8(entry).map_err(|_| refused("tree batch encoding"))?;
                let (object, path) = entry
                    .split_once('\t')
                    .ok_or_else(|| refused("tree batch framing"))?;
                if entries.insert(path, object).is_some() {
                    return Err(refused("duplicate tree entry"));
                }
            }
            if entries.len() != batch.len() || !tree.ends_with(&[0]) {
                return Err(refused("committed evidence missing"));
            }
            for (file, hash) in batch.iter().zip(&hashes) {
                if !["100644", "100755"].iter().any(|mode| {
                    entries.get(file.path.as_str()) == Some(&format!("{mode} blob {hash}").as_str())
                }) {
                    return Err(refused("evidence is not committed at observed HEAD"));
                }
            }
        }
        observed_hashes.extend(hashes.into_iter().map(String::from));
        for file in batch {
            let actual = bytes(root, &file.path)?;
            if actual.len() as u64 != file.bytes
                || crate::infrastructure::graph_checkpoint::digest(&actual) != file.sha256
            {
                return Err(refused("evidence changed during committed batch proof"));
            }
        }
        start = end;
    }
    Ok(observed_hashes)
}

#[cfg(test)]
mod committed_proof_tests {
    use super::*;
    use crate::infrastructure::{git_evidence::GitRequestScope, graph_checkpoint::digest};
    use std::{fs, process::Command};

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn same_observe_proof_refuses_wrong_group_nonce_body_attributes_and_conversion() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("source");
        let other = dir.path().join("other");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&other).unwrap();
        git(&root, &["init", "--quiet", "-b", "dev"]);
        for (key, value) in [
            ("user.name", "Proof Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
            ("core.safecrlf", "false"),
        ] {
            git(&root, &["config", key, value]);
        }
        fs::write(root.join(".gitattributes"), "*.md -filter -text\n").unwrap();
        let body = b"committed evidence\r\n";
        let files = ["record.md", "subject.md"].map(|path| {
            fs::write(root.join(path), body).unwrap();
            FileIdentity {
                path: RepoPath::new(path).unwrap(),
                bytes: body.len() as u64,
                sha256: digest(body),
            }
        });
        git(
            &root,
            &["add", "--", ".gitattributes", "record.md", "subject.md"],
        );
        git(&root, &["commit", "--quiet", "-m", "proof fixture"]);
        let head = CommitId::new(git(&root, &["rev-parse", "HEAD"])).unwrap();
        let scope = GitRequestScope::new();
        let proof = prove_committed_files(&root, &head, &files).unwrap();
        proof.reobserve(&root, &head, &files).unwrap();
        committed_files(&root, &head, &files).unwrap(); // No-token original path.

        assert!(proof.reobserve(&other, &head, &files).is_err());
        assert!(
            proof
                .reobserve(
                    &root,
                    &CommitId::new("a".repeat(head.as_str().len())).unwrap(),
                    &files
                )
                .is_err()
        );
        assert!(proof.reobserve(&root, &head, &files[..1]).is_err());
        let mut changed_identity = files.clone();
        changed_identity[0].sha256 = digest(b"foreign identity");
        assert!(proof.reobserve(&root, &head, &changed_identity).is_err());
        {
            let _other_request = GitRequestScope::new();
            assert!(proof.reobserve(&root, &head, &files).is_err());
        }
        fs::write(root.join("record.md"), b"changed content\r\n").unwrap();
        assert!(proof.reobserve(&root, &head, &files).is_err());
        fs::write(root.join("record.md"), body).unwrap();

        let attributes = root.join(".git/info/attributes");
        fs::write(&attributes, "record.md filter=foreign\n").unwrap();
        assert!(
            proof
                .reobserve(&root, &head, &files)
                .unwrap_err()
                .to_string()
                .contains("storage filter")
        );
        fs::write(&attributes, "record.md text\n").unwrap();
        assert!(
            proof
                .reobserve(&root, &head, &files)
                .unwrap_err()
                .to_string()
                .contains("storage conversion changed")
        );
        fs::write(&attributes, "record.md !text\n").unwrap();
        git(&root, &["config", "core.autocrlf", "true"]);
        assert!(
            proof
                .reobserve(&root, &head, &files)
                .unwrap_err()
                .to_string()
                .contains("storage conversion changed")
        );
        git(&root, &["config", "core.autocrlf", "false"]);
        fs::write(&attributes, "").unwrap();
        proof.reobserve(&root, &head, &files).unwrap();
        drop(scope);
        assert!(proof.reobserve(&root, &head, &files).is_err());
    }
}
