//! Restore only an absent, already-checkpointed canonical projection. No graph
//! transition, journal recovery, index update or Git ref mutation happens here.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GraphRenderOutcome {
    pub workspace_id: String,
    pub revision: u64,
    pub action: &'static str,
    pub rendered_path: String,
}

impl CheckpointKernel<'_> {
    pub fn restore_view_under_lock(
        &self,
        expected_revision: u64,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<GraphRenderOutcome> {
        let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
        let (source, view) = self.paths.resolve()?;
        let source_before = read_bounded(&source)?;
        let workspace = parse(&source_before)?;
        if workspace.revision != expected_revision {
            return Err(crate::domain::error::MineError::RevisionConflict {
                expected: expected_revision,
                actual: workspace.revision,
            }
            .into());
        }
        let barriers = self.store.barriers()?;
        if let Some(pending) = barriers.global.first() {
            return Err(crate::domain::error::MineError::GraphInvalid {
                detail: format!(
                    "pending operation {} requires exact recovery, not standalone render",
                    pending.operation_id.as_str()
                ),
            }
            .into());
        }
        crate::domain::validation::validate_with_worktrees_root(
            &workspace,
            &self.config.plan.worktrees_root,
        )?;
        let canonical = render_markdown(&workspace)?.into_bytes();
        let mut git = git::request(self.store.root(), None)?;
        let identity = self.identity(&mut git)?;
        self.supported_index(&mut git, &identity.1, &identity.0)?;
        self.attributes(&mut git)?;
        git::validate_autocrlf(git::config(&mut git, "core.autocrlf")?.as_deref())?;
        let original_index = read_bounded(&identity.1)?;
        self.restore_recorded_pair(&mut git, &identity.2, [&source_before, &canonical])?;
        let original_view = view_bytes(&view)?;
        if original_view
            .as_ref()
            .is_some_and(|bytes| bytes != &canonical)
        {
            return Err(OperationError::Refused(
                "existing noncanonical projection is preserved",
            ));
        }
        let observe = |git: &mut GitRequest,
                       temporary: Option<&Path>,
                       published: bool|
         -> OperationResult<()> {
            git.refresh_observations();
            if self.identity(git)? != identity
                || read_bounded(&identity.1)? != original_index
                || lock_path(&identity.1).try_exists()?
                || read_bounded(&source)? != source_before
            {
                return Err(OperationError::Refused(
                    "projection Root/source/index identity changed",
                ));
            }
            self.supported_index(git, &identity.1, &identity.0)?;
            self.attributes(git)?;
            if !git
                .run_raw(
                    &[
                        "diff-index",
                        "--cached",
                        "--quiet",
                        identity.2.as_str(),
                        "--",
                    ],
                    b"",
                )?
                .status
                .success()
            {
                return Err(OperationError::Refused(
                    "projection actual index differs from HEAD",
                ));
            }
            let current = view_bytes(&view)?;
            if published {
                if current.as_ref() != Some(&canonical) {
                    return Err(OperationError::Refused("published projection changed"));
                }
            } else if current != original_view {
                return Err(OperationError::Refused(
                    "projection destination appeared or changed",
                ));
            }
            let status = git.run(
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                    "--ignore-submodules=none",
                ],
                b"",
            )?;
            let temporary = temporary
                .map(|p| {
                    p.strip_prefix(self.store.root())
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                })
                .transpose()
                .map_err(|_| OperationError::Journal)?;
            for entry in status.split(|b| *b == 0).filter(|e| !e.is_empty()) {
                let missing_view = !published
                    && original_view.is_none()
                    && entry.get(..3) == Some(b" D ")
                    && entry.get(3..) == Some(self.paths.view.as_str().as_bytes());
                let owned_temporary = entry.get(..3) == Some(b"?? ")
                    && temporary
                        .as_ref()
                        .is_some_and(|p| entry.get(3..) == Some(p.as_bytes()));
                if !missing_view && !owned_temporary {
                    return Err(OperationError::Refused(
                        "foreign projection worktree/index change",
                    ));
                }
            }
            Ok(())
        };
        observe(&mut git, None, false)?;
        if original_view.is_some() {
            return Ok(GraphRenderOutcome {
                workspace_id: workspace.workspace_id,
                revision: workspace.revision,
                action: "unchanged",
                rendered_path: view.display().to_string(),
            });
        }
        let mut temporary = tempfile::Builder::new()
            .prefix(".mine-view-")
            .tempfile_in(view.parent().ok_or(OperationError::Journal)?)?;
        temporary.write_all(&canonical)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        observer.reached(Boundary::ViewRestorePrepared)?;
        observe(&mut git, Some(temporary.path()), false)?;
        // Publish complete bytes without replacing a collision. On unsupported
        // filesystems the primitive may retain its original temporary hard link;
        // that is not permission for blind cleanup of unknown files.
        temporary
            .persist_noclobber(&view)
            .map_err(|e| OperationError::Io(e.error))?;
        let after = observer
            .reached(Boundary::ViewRestorePublished)
            .and_then(|()| observe(&mut git, None, true));
        if let Err(error) = after {
            return Err(crate::domain::error::MineError::GraphInvalid {
                detail: format!(
                    "canonical view was published; post-publication verification failed: {error}"
                ),
            }
            .into());
        }
        Ok(GraphRenderOutcome {
            workspace_id: workspace.workspace_id,
            revision: workspace.revision,
            action: "restored",
            rendered_path: view.display().to_string(),
        })
    }

    fn restore_recorded_pair(
        &self,
        git: &mut GitRequest,
        head: &CommitId,
        canonical: [&[u8]; 2],
    ) -> OperationResult<()> {
        for (path, bytes) in [self.paths.source.as_str(), self.paths.view.as_str()]
            .into_iter()
            .zip(canonical)
        {
            let entry = git.run(&["ls-tree", "-z", head.as_str(), "--", path], b"")?;
            let text = std::str::from_utf8(&entry).map_err(|_| OperationError::Journal)?;
            let (meta, observed) = text
                .strip_suffix('\0')
                .and_then(|s| s.split_once('\t'))
                .ok_or(OperationError::Journal)?;
            let fields = meta.split(' ').collect::<Vec<_>>();
            if fields.len() != 3 || fields[0] != "100644" || fields[1] != "blob" || observed != path
            {
                return Err(OperationError::Refused(
                    "projection pair is not regular recorded files",
                ));
            }
            if git.run(&["cat-file", "blob", fields[2]], b"")? != bytes {
                return Err(OperationError::Refused(
                    "live source or recorded view is not checkpointed canonical bytes",
                ));
            }
            let stored = git.run(
                &["hash-object", &format!("--path={path}"), "--stdin"],
                bytes,
            )?;
            let stored = std::str::from_utf8(&stored)
                .map_err(|_| OperationError::Journal)?
                .trim();
            let checkout = git.run(
                &[
                    "cat-file",
                    "--filters",
                    &format!("--path={path}"),
                    &format!("{}:{path}", head.as_str()),
                ],
                b"",
            )?;
            let checkout = String::from_utf8(checkout)
                .map_err(|_| OperationError::Journal)?
                .replace("\r\n", "\n");
            if stored != fields[2] || checkout.as_bytes() != bytes {
                return Err(OperationError::Refused(
                    "projection storage/checkout equivalence unproved",
                ));
            }
        }
        Ok(())
    }
}

fn view_bytes(path: &Path) -> OperationResult<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Ok(Some(read_bounded(path)?))
        }
        Ok(_) => Err(OperationError::Refused(
            "projection target is not a regular file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
