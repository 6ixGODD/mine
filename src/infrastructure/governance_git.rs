//! Bounded, effective Git observations and MINE-owned child control sections.
//! These rules are setup data, never an alternate governance path registry.

use std::path::Path;

use super::{atomic_write, git_evidence::GitRequest, graph_snapshot::resolve_contained};
use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::RepoPath;

const BEGIN: &str = "# BEGIN MINE governance controls";
const END: &str = "# END MINE governance controls";
pub const IGNORE_RULES: &str = "/runtime/\n/locks/\n/build/\n/worktrees/\n!/.gitignore\n!/.gitattributes\n!/config.toml\n!/verification-policy.json\n!/design/\n!/design/**\n!/plan/\n!/plan/**\n";
pub const ATTRIBUTE_RULES: &str = "design/**/*.md -text -filter -ident -working-tree-encoding\ndesign/**/*.toml -text -filter -ident -working-tree-encoding\ndesign/**/*.json -text -filter -ident -working-tree-encoding\nplan/**/*.md -text -filter -ident -working-tree-encoding\nplan/**/*.toml -text -filter -ident -working-tree-encoding\nplan/**/*.json -text -filter -ident -working-tree-encoding\n/config.toml -text -filter -ident -working-tree-encoding\n/verification-policy.json -text -filter -ident -working-tree-encoding\n/.gitignore -text -filter -ident -working-tree-encoding\n/.gitattributes -text -filter -ident -working-tree-encoding\n";

const PROBES: &[&str] = &[
    ".mine/.gitignore",
    ".mine/.gitattributes",
    ".mine/config.toml",
    ".mine/verification-policy.json",
    ".mine/design/index.md",
    ".mine/design/.mine-design.toml",
    ".mine/design/decisions/new-draft.md",
    ".mine/plan/new-plan.md",
    ".mine/plan/execution-graph.toml",
    ".mine/plan/reports/new-report.json",
];

/// Preserve foreign sections, and roll back only our exact control-file writes
/// on refusal. No governance body, global rule, index, or user document changes.
pub fn establish(root: &Path) -> MineResult<()> {
    let definitions = [
        (".mine/.gitignore", IGNORE_RULES),
        (".mine/.gitattributes", ATTRIBUTE_RULES),
    ];
    let mut controls = Vec::new();
    for (relative, rules) in definitions {
        let path = super::operation_store::regular_path(
            root,
            &RepoPath::new(relative).expect("constant path"),
        )?;
        let before = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let content = before.as_deref().unwrap_or_default();
        let after = merge_section(content, rules)?;
        controls.push((path, before, after));
    }
    let git = root.join(".git").exists();
    if git {
        // A child exception cannot rescue an excluded parent. Observe before
        // creating even the control files, including repository/global rules.
        check_visible(root, &[".mine/"])?;
    }
    let result = (|| {
        for (path, before, after) in &controls {
            if before.as_deref() != Some(after.as_slice()) {
                atomic_write::write(path, after)?;
            }
        }
        if git {
            check_visible(root, PROBES)?;
            check_attributes(root, PROBES, false)?;
        }
        Ok(())
    })();
    if result.is_err() {
        for (path, before, after) in controls.iter().rev() {
            // Never overwrite a concurrent writer during recovery.
            if std::fs::read(path).ok().as_deref() == Some(after.as_slice()) {
                match before {
                    Some(bytes) => atomic_write::write(path, bytes)?,
                    None => std::fs::remove_file(path)?,
                }
            }
        }
    }
    result
}

pub(crate) fn merge_section(bytes: &[u8], rules: &str) -> MineResult<Vec<u8>> {
    let content =
        std::str::from_utf8(bytes).map_err(|_| invalid("Git control file is not UTF-8"))?;
    let block = format!("{BEGIN}\n{rules}{END}\n");
    let begins = content.matches(BEGIN).count();
    let ends = content.matches(END).count();
    let updated = match (begins, ends) {
        (0, 0) => format!(
            "{content}{}{block}",
            if content.is_empty() || content.ends_with('\n') {
                ""
            } else {
                "\n"
            }
        ),
        (1, 1) => {
            let start = content.find(BEGIN).expect("one marker");
            let end = content.find(END).expect("one marker");
            if end < start
                || (start != 0 && content.as_bytes()[start - 1] != b'\n')
                || !content[start + BEGIN.len()..].starts_with(['\r', '\n'])
                || (end != 0 && content.as_bytes()[end - 1] != b'\n')
                || (!content[end + END.len()..].is_empty()
                    && !content[end + END.len()..].starts_with(['\r', '\n']))
            {
                return Err(invalid("malformed managed Git control section"));
            }
            let tail = &content[end + END.len()..];
            let tail = tail
                .strip_prefix("\r\n")
                .or_else(|| tail.strip_prefix('\n'))
                .unwrap_or(tail);
            format!("{}{block}{tail}", &content[..start])
        }
        _ => return Err(invalid("ambiguous managed Git control section")),
    };
    Ok(updated.into_bytes())
}

pub fn inspect(root: &Path) -> MineResult<()> {
    check_visible(root, PROBES)?;
    check_attributes(root, PROBES, false)
}

pub fn check_visible(root: &Path, paths: &[&str]) -> MineResult<()> {
    let input = nul_paths(paths);
    let output = GitRequest::new(root).run_raw(
        &["check-ignore", "--no-index", "-v", "-z", "--stdin"],
        &input,
    )?;
    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(invalid("Git ignore observation failed"));
    }
    let parts = records(&output.stdout, 4)?;
    for entry in parts.chunks_exact(4) {
        if !entry[2].starts_with(b"!") {
            return Err(invalid(&format!(
                "governance path {} is excluded by {}:{} pattern {}; correct that rule before init/doctor (no force-add)",
                String::from_utf8_lossy(entry[3]),
                String::from_utf8_lossy(entry[0]),
                String::from_utf8_lossy(entry[1]),
                String::from_utf8_lossy(entry[2])
            )));
        }
    }
    Ok(())
}

/// All four transformations must be explicitly disabled; unspecified `text`
/// still allows autocrlf and is not byte-stability evidence.
pub fn check_attributes(root: &Path, paths: &[&str], cached: bool) -> MineResult<()> {
    let mut args = vec!["check-attr", "-z", "--stdin"];
    if cached {
        args.push("--cached");
    }
    args.extend(["text", "filter", "ident", "working-tree-encoding"]);
    let output = GitRequest::new(root).run(&args, &nul_paths(paths))?;
    let parts = records(&output, 3)?;
    if parts.len() != paths.len() * 12 {
        return Err(invalid("incomplete Git attribute observation"));
    }
    for entry in parts.chunks_exact(3) {
        if entry[2] != b"unset" {
            return Err(invalid(&format!(
                "{}: effective {}={} ({}) is not byte-preserving; inspect higher-precedence/deeper Git attributes and commit the managed baseline before freezing bodies",
                String::from_utf8_lossy(entry[0]),
                String::from_utf8_lossy(entry[1]),
                String::from_utf8_lossy(entry[2]),
                if cached { "index" } else { "working tree" }
            )));
        }
    }
    Ok(())
}

/// Freeze/consume only under committed byte-preserving baseline rules. This
/// observes Git, never stages files, normalizes bodies or writes Git objects.
pub fn consume_bodies(root: &Path, paths: &[&str]) -> MineResult<()> {
    check_attributes(root, paths, false)?;
    check_attributes(root, paths, true)?;
    let mut git = GitRequest::new(root);
    git.run(&["cat-file", "-e", "HEAD:.mine/.gitattributes"], b"")
        .map_err(|_| {
            invalid("commit .mine/.gitattributes in the baseline before freezing governance bodies")
        })?;
    let unchanged = git.run_raw(
        &[
            "diff",
            "--quiet",
            "HEAD",
            "--",
            ".mine/.gitattributes",
            ".mine/.gitignore",
        ],
        b"",
    )?;
    if !unchanged.status.success() {
        return Err(invalid(
            "managed Git controls differ from committed baseline; commit them before proposal/registration/prepare",
        ));
    }
    Ok(())
}

pub fn freeze_body(root: &Path, relative: &RepoPath, bytes: &[u8]) -> MineResult<()> {
    consume_bodies(root, &[relative.as_str()])?;
    let target = resolve_contained(root, relative)?;
    if std::fs::read(target)? != bytes {
        return Err(invalid("governance body changed during freeze observation"));
    }
    let mut git = GitRequest::new(root);
    let raw = git.run(&["hash-object", "--stdin", "--no-filters"], bytes)?;
    let stored = git.run(
        &["hash-object", "--stdin", "--path", relative.as_str()],
        bytes,
    )?;
    if raw != stored {
        return Err(invalid(&format!(
            "{} bytes change through Git storage filters",
            relative.as_str()
        )));
    }
    Ok(())
}

fn nul_paths(paths: &[&str]) -> Vec<u8> {
    paths
        .iter()
        .flat_map(|path| path.as_bytes().iter().copied().chain(std::iter::once(0)))
        .collect()
}

fn records(bytes: &[u8], width: usize) -> MineResult<Vec<&[u8]>> {
    if bytes.is_empty() {
        return Ok(vec![]);
    }
    let mut parts = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    if parts.pop() != Some(b"".as_slice()) || parts.len() % width != 0 {
        return Err(invalid("malformed Git control observation"));
    }
    Ok(parts)
}

fn invalid(detail: &str) -> MineError {
    MineError::ConfigInvalid {
        path: ".mine/config.toml".into(),
        detail: detail.into(),
    }
}
