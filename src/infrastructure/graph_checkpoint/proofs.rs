//! Exact native Git object proofs. Framing is bounded by GitRequest; no object
//! database, tag peeling, persistent cache or checkout conversion is invented.
use crate::domain::{
    graph::CommitId,
    lifecycle_operation::{OperationError, OperationResult},
};
use crate::infrastructure::git_evidence::GitRequest;

fn batch_contents<'a>(
    output: &'a [u8],
    purpose: &str,
    expected: &[(&str, &str)],
) -> OperationResult<Vec<&'a [u8]>> {
    let bad = || OperationError::Evidence("exact native object batch framing/type/identity");
    let mut remaining = output;
    let mut contents = Vec::with_capacity(expected.len());
    for (oid, kind) in expected {
        let newline = remaining.iter().position(|b| *b == b'\n').ok_or_else(bad)?;
        let header = std::str::from_utf8(&remaining[..newline]).map_err(|_| bad())?;
        let fields = header.split(' ').collect::<Vec<_>>();
        if fields.len() != 4 || fields[..3] != [purpose, *oid, *kind] {
            return Err(bad());
        }
        let size = fields[3].parse::<usize>().map_err(|_| bad())?;
        if size.to_string() != fields[3] {
            return Err(bad());
        }
        remaining = &remaining[newline + 1..];
        let bytes = remaining.get(..size).ok_or_else(bad)?;
        if remaining.get(size) != Some(&b'\n') {
            return Err(bad());
        }
        contents.push(bytes);
        remaining = &remaining[size + 1..];
    }
    if !remaining.is_empty() {
        return Err(bad());
    }
    Ok(contents)
}

pub(super) fn raw_blobs(git: &mut GitRequest, oids: &[&str]) -> OperationResult<Vec<Vec<u8>>> {
    let expected = oids.iter().map(|oid| (*oid, "blob")).collect::<Vec<_>>();
    let input = format!("{}\n", oids.join("\n"));
    let bytes = git.run(
        &[
            "cat-file",
            "--batch=raw-blob %(objectname) %(objecttype) %(objectsize)",
        ],
        input.as_bytes(),
    )?;
    Ok(batch_contents(&bytes, "raw-blob", &expected)?
        .into_iter()
        .map(<[u8]>::to_vec)
        .collect())
}

fn candidate_headers(content: &[u8], parent: &CommitId, tree: &CommitId) -> OperationResult<()> {
    let bad = || OperationError::Refused("candidate parent/tree evidence mismatch");
    let end = content
        .windows(2)
        .position(|pair| pair == b"\n\n")
        .ok_or_else(bad)?;
    let mut headers = content[..end].split(|b| *b == b'\n');
    let expected_tree = format!("tree {}", tree.as_str());
    if headers.next() != Some(expected_tree.as_bytes()) {
        return Err(bad());
    }
    let expected_parent = format!("parent {}", parent.as_str());
    let mut parents = 0;
    for header in headers {
        if header.starts_with(b"tree ") {
            return Err(bad());
        }
        if header.starts_with(b"parent ") {
            parents += 1;
            if header != expected_parent.as_bytes() {
                return Err(bad());
            }
        }
    }
    if parents != 1 {
        return Err(bad());
    }
    Ok(())
}

pub(super) fn candidate(
    git: &mut GitRequest,
    candidate: &CommitId,
    parent: &CommitId,
    tree: &CommitId,
) -> OperationResult<()> {
    let bytes = git.run(
        &[
            "cat-file",
            "--batch=candidate %(objectname) %(objecttype) %(objectsize)",
        ],
        format!("{}\n{}\n", candidate.as_str(), tree.as_str()).as_bytes(),
    )?;
    // An exact tree header alone does not prove its target's type: ls-tree
    // accepts commit/tag tree-ish values. Prove the native tree in this same
    // bounded child, without peeling or adding a second launch.
    let objects = batch_contents(
        &bytes,
        "candidate",
        &[(candidate.as_str(), "commit"), (tree.as_str(), "tree")],
    )?;
    candidate_headers(objects[0], parent, tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_batch_rejects_wrong_type_identity_size_truncation_and_extra_output() {
        let valid = b"candidate exact commit 3\na\0b\n";
        assert_eq!(
            batch_contents(valid, "candidate", &[("exact", "commit")]).unwrap(),
            [&b"a\0b"[..]]
        );
        for invalid in [
            &b"candidate exact tag 3\na\0b\n"[..],
            &b"candidate foreign commit 3\na\0b\n"[..],
            &b"candidate exact commit 4\na\0b\n"[..],
            &b"candidate exact commit 03\na\0b\n"[..],
            &b"candidate exact commit 3\na\0b"[..],
            &b"candidate exact commit 3\na\0b\nextra"[..],
            &b"exact missing\n"[..],
            &b"candidate exact commit 99999999999999999999999999999999\n"[..],
        ] {
            assert!(batch_contents(invalid, "candidate", &[("exact", "commit")]).is_err());
        }
    }

    #[test]
    fn blob_batch_is_binary_length_framed_ordered_and_exact() {
        let bytes = b"raw-blob first blob 3\na\nb\nraw-blob second blob 0\n\n";
        assert_eq!(
            batch_contents(bytes, "raw-blob", &[("first", "blob"), ("second", "blob")]).unwrap(),
            [&b"a\nb"[..], &b""[..]]
        );
        assert!(
            batch_contents(bytes, "raw-blob", &[("second", "blob"), ("first", "blob")]).is_err()
        );
        assert!(batch_contents(bytes, "raw-blob", &[("first", "blob")]).is_err());
    }

    #[test]
    fn candidate_headers_require_one_exact_tree_and_parent_without_parsing_message_or_signature() {
        let parent = CommitId::new("1".repeat(40)).unwrap();
        let tree = CommitId::new("2".repeat(40)).unwrap();
        let header = format!("tree {}\nparent {}", tree.as_str(), parent.as_str());
        let good = format!(
            "{header}\nauthor x\ncommitter x\ngpgsig signature\n parent signature-text\n\nparent message-text\n"
        );
        candidate_headers(good.as_bytes(), &parent, &tree).unwrap();
        for bad in [
            format!("{header}\nparent {}\n\nmessage", parent.as_str()),
            format!("tree {}\n\nmessage", tree.as_str()),
            format!("{header}\ntree {}\n\nmessage", tree.as_str()),
            format!(
                "tree {}\nparent {}\n\nmessage",
                parent.as_str(),
                tree.as_str()
            ),
            header,
        ] {
            assert!(candidate_headers(bad.as_bytes(), &parent, &tree).is_err());
        }
    }

    #[test]
    fn native_candidate_proof_refuses_real_tags_blobs_and_wrong_parent_or_tree() {
        use std::{
            fs,
            io::Write,
            process::{Command, Stdio},
        };
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().to_owned()
        };
        run(&["init", "-b", "dev"]);
        for (key, value) in [
            ("user.name", "Native Proof"),
            ("user.email", "proof@example.invalid"),
            ("core.autocrlf", "false"),
            ("commit.gpgSign", "false"),
            ("tag.gpgSign", "false"),
        ] {
            run(&["config", key, value]);
        }
        fs::write(root.join("seed"), b"first\n").unwrap();
        run(&["add", "seed"]);
        run(&["commit", "-m", "first"]);
        let parent = CommitId::new(run(&["rev-parse", "HEAD"])).unwrap();
        let old_tree = CommitId::new(run(&["rev-parse", "HEAD^{tree}"])).unwrap();
        fs::write(root.join("seed"), b"second\n").unwrap();
        run(&["add", "seed"]);
        run(&["commit", "-m", "second"]);
        let tip = CommitId::new(run(&["rev-parse", "HEAD"])).unwrap();
        let tree = CommitId::new(run(&["rev-parse", "HEAD^{tree}"])).unwrap();
        let blob = CommitId::new(run(&["rev-parse", "HEAD:seed"])).unwrap();
        run(&["tag", "-a", "proof-tag", "-m", "tag is not exact commit"]);
        let tag = CommitId::new(run(&["rev-parse", "refs/tags/proof-tag"])).unwrap();
        // Git can store a malformed commit whose tree header is a commit OID.
        // ls-tree accepts that OID as tree-ish, so check its actual type too.
        let malformed = format!(
            "tree {}\nparent {}\nauthor Probe <probe@example.invalid> 1700000000 +0000\ncommitter Probe <probe@example.invalid> 1700000000 +0000\n\nwrong tree type\n",
            parent.as_str(),
            parent.as_str()
        );
        let mut child = Command::new("git")
            .current_dir(root)
            .args([
                "hash-object",
                "--literally",
                "-t",
                "commit",
                "-w",
                "--stdin",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(malformed.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let malformed = CommitId::new(String::from_utf8(out.stdout).unwrap().trim()).unwrap();
        let _scope = crate::infrastructure::git_evidence::GitRequestScope::new();
        let mut git = super::super::git::request(root, None).unwrap();
        candidate(&mut git, &tip, &parent, &tree).unwrap();
        assert!(candidate(&mut git, &tag, &parent, &tree).is_err());
        assert!(candidate(&mut git, &blob, &parent, &tree).is_err());
        assert!(candidate(&mut git, &tip, &tip, &tree).is_err());
        assert!(candidate(&mut git, &tip, &parent, &old_tree).is_err());
        assert!(
            candidate(&mut git, &malformed, &parent, &parent).is_err(),
            "commit-valued tree header must not be accepted by tree-ish peeling"
        );
        let before = git.diagnostics().invocations;
        assert_eq!(
            raw_blobs(&mut git, &[blob.as_str(), blob.as_str()]).unwrap(),
            [b"second\n".to_vec(), b"second\n".to_vec()]
        );
        assert_eq!(git.diagnostics().invocations, before + 1);
        assert_eq!(git.diagnostics().invocations, 7);
        assert_eq!(git.diagnostics().peak_children, 1);
    }
}
