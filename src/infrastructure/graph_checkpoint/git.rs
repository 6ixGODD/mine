//! All children use the accepted bounded transport. Only the private-index
//! spawner changes GIT_INDEX_FILE; ambient repository/config redirects refuse.
use crate::domain::lifecycle_operation::{OperationError, OperationResult};
use crate::infrastructure::git_evidence::{GitChild, GitLimits, GitRequest, GitSpawner};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

struct ControlledGit {
    index: Option<PathBuf>,
}
impl GitSpawner for ControlledGit {
    fn spawn(&self, root: &Path, args: &[&str]) -> io::Result<GitChild> {
        let mut cmd = tokio::process::Command::new("git");
        cmd.current_dir(root)
            .arg("--no-pager")
            .arg("-C")
            .arg(".")
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_LITERAL_PATHSPECS", "1")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        if let Some(index) = &self.index {
            // Git for Windows rejects Rust's canonical verbatim (\\?\) path
            // in GIT_INDEX_FILE. This owned artifact is beneath the canonical
            // Root; a relative path is equivalent under the fixed child cwd
            // without rewriting arbitrary caller paths or losing containment.
            let relative = index.strip_prefix(root).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "private index outside Root")
            })?;
            cmd.env("GIT_INDEX_FILE", relative);
        }
        GitChild::spawn(cmd)
    }
}
pub(crate) fn request(root: &Path, index: Option<&Path>) -> OperationResult<GitRequest> {
    check_environment()?;
    Ok(GitRequest::with_spawner(
        root,
        GitLimits::default(),
        Arc::new(ControlledGit {
            index: index.map(Path::to_path_buf),
        }),
    ))
}
pub(super) fn check_environment() -> OperationResult<()> {
    // Do not silently override caller policy or a foreign repository/index.
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy().to_ascii_uppercase();
        if key.starts_with("GIT_")
            && !matches!(
                key.as_str(),
                "GIT_PAGER"
                    | "GIT_CEILING_DIRECTORIES"
                    | "GIT_TERMINAL_PROMPT"
                    | "GIT_TRACE"
                    | "GIT_TRACE2"
                    | "GIT_TRACE2_EVENT"
                    | "GIT_TRACE2_PERF"
                    | "GIT_AUTHOR_NAME"
                    | "GIT_AUTHOR_EMAIL"
                    | "GIT_AUTHOR_DATE"
                    | "GIT_COMMITTER_NAME"
                    | "GIT_COMMITTER_EMAIL"
                    | "GIT_COMMITTER_DATE"
            )
        {
            return Err(OperationError::Refused(
                "ambient Git redirection or policy override",
            ));
        }
    }
    Ok(())
}
pub(super) fn config(git: &mut GitRequest, key: &str) -> OperationResult<Option<String>> {
    let result = git.run_raw(&["config", "--get", key], b"")?;
    match result.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8(result.stdout)
                .map_err(|_| OperationError::Evidence("Git config encoding"))?
                .trim()
                .into(),
        )),
        Some(1) => Ok(None),
        _ => Err(OperationError::Evidence("Git config query failed")),
    }
}
pub(super) fn boolean(git: &mut GitRequest, key: &str) -> OperationResult<bool> {
    let result = git.run_raw(&["config", "--bool", "--get", key], b"")?;
    match result.status.code() {
        Some(0) if result.stdout == b"true\n" => Ok(true),
        Some(0) if result.stdout == b"false\n" => Ok(false),
        Some(1) => Ok(false),
        _ => Err(OperationError::Evidence("Git boolean policy query failed")),
    }
}

// Preserve every NUL-delimited occurrence until boolean coercion has been
// decided. Callers reduce last-value precedence only after validating records.
fn selected_config(
    git: &mut GitRequest,
    pattern: &str,
    boolean: bool,
) -> OperationResult<Vec<(String, Option<String>)>> {
    let mut args = vec!["config", "--null"];
    if boolean {
        args.push("--bool");
    }
    args.extend(["--get-regexp", pattern]);
    let out = git.run_raw(&args, b"")?;
    let mut values = Vec::new();
    match out.status.code() {
        Some(1) if out.stdout.is_empty() => return Ok(values),
        Some(0) => {}
        _ => return Err(OperationError::Evidence("Git policy query failed")),
    }
    let text = std::str::from_utf8(&out.stdout)
        .map_err(|_| OperationError::Evidence("Git policy response encoding"))?;
    if !text.ends_with('\0') {
        return Err(OperationError::Evidence("Git policy response framing"));
    }
    for entry in text.split_terminator('\0') {
        let (key, value) = match entry.split_once('\n') {
            Some((key, value)) => (key, Some(value.into())),
            None => (entry, None), // A valueless variable is a native true boolean.
        };
        values.push((key.into(), value));
    }
    Ok(values)
}
pub(crate) fn index_policy(git: &mut GitRequest) -> OperationResult<()> {
    // Observe all five policies afresh at every safety boundary, but avoid a
    // second Git child for the common canonical-boolean case. Inspect every
    // occurrence before last-value reduction: an invalid earlier boolean must
    // still be diagnosed by Git even if a later value overrides it.
    let entries = selected_config(
        git,
        r"^(core\.(sparsecheckout|splitindex|fsmonitor|autocrlf)|index\.sparse)$",
        false,
    )?;
    let is_boolean = |key: &str| {
        matches!(
            key,
            "core.sparsecheckout" | "core.splitindex" | "index.sparse"
        )
    };
    let native_coercion = entries
        .iter()
        .any(|(key, value)| is_boolean(key) && !matches!(value.as_deref(), Some("true" | "false")));
    let booleans = if native_coercion {
        selected_config(
            git,
            r"^(core\.(sparsecheckout|splitindex)|index\.sparse)$",
            true,
        )?
    } else {
        entries
            .iter()
            .filter(|(key, _)| is_boolean(key))
            .cloned()
            .collect()
    };
    let boolean: std::collections::BTreeMap<_, _> = booleans.into_iter().collect();
    if boolean
        .values()
        .any(|v| !matches!(v.as_deref(), Some("true" | "false")))
    {
        return Err(OperationError::Evidence("Git boolean policy response"));
    }
    if boolean.values().any(|v| v.as_deref() == Some("true")) {
        return Err(OperationError::Refused(
            "sparse/split index not supported by checkpoint",
        ));
    }
    let mut text = std::collections::BTreeMap::new();
    for (key, value) in entries {
        if !is_boolean(&key) {
            text.insert(
                key,
                value.ok_or(OperationError::Evidence("Git policy response framing"))?,
            );
        }
    }
    if text.get("core.fsmonitor").is_some_and(|v| v != "false") {
        return Err(OperationError::Refused(
            "fsmonitor index optimization unsupported",
        ));
    }
    validate_autocrlf(text.get("core.autocrlf").map(String::as_str))
}
pub(super) fn validate_autocrlf(value: Option<&str>) -> OperationResult<()> {
    if value.is_some_and(|v| !matches!(v, "true" | "false" | "input")) {
        return Err(OperationError::Refused("unsupported Git autocrlf value"));
    }
    Ok(())
}
