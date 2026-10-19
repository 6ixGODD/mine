//! Closed preparation effects, implemented inside the checkpoint kernel module.
use super::*;
use crate::infrastructure::graph_checkpoint::{plan_git::*, preparation_intent::*};

impl CheckpointKernel<'_> {
    /// Read-only completed-generation provenance, not same-operation recovery.
    /// Current configuration may have changed for an explicit refresh; never
    /// rewrite its old hash or weaken bound_runtime for an unfinished operation.
    pub(crate) fn generation_history(&self, record: &JournalRecord) -> OperationResult<()> {
        let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
        let r = runtime(record)?;
        let facts = record
            .operation
            .fields()
            .checkpoint
            .as_ref()
            .ok_or(OperationError::Journal)?
            .fields();
        if !terminal(&record.operation)
            || record.operation.fields().outcome != OperationOutcome::Committed
            || !matches!(r.plan_git, PlanGitEffects::Preparation { .. })
            || !r.plan_git.complete()
            || r.root != self.store.root()
            || r.source_path != self.paths.source
            || r.view_path != self.paths.view
            || r.integration_ref != format!("refs/heads/{}", self.config.branches.integration)
        {
            return Err(OperationError::Refused(
                "completed generation repository/path binding differs",
            ));
        }
        let mut git = git::request(self.store.root(), None)?;
        let (common, index, head) = self.identity(&mut git)?;
        if common != r.common_dir
            || index != r.index_path
            || !git.is_ancestor(
                r.candidate_commit
                    .as_ref()
                    .ok_or(OperationError::Journal)?
                    .as_str(),
                head.as_str(),
            )?
        {
            return Err(OperationError::Refused(
                "completed generation checkpoint is not current Root history",
            ));
        }
        self.prove_candidate(&mut git, record, None)?;
        // Both actual parent blobs must match its frozen before-state. Only
        // the already-supported built-in CRLF clean conversion is admissible;
        // this does not normalize an immutable Plan digest or change any bytes.
        for (path, before) in [
            (&r.source_path, r.source_before()?),
            (&r.view_path, r.view_before()?),
        ] {
            let blob = git.run(
                &[
                    "cat-file",
                    "blob",
                    &format!("{}:{}", facts.original_head.as_str(), path.as_str()),
                ],
                b"",
            )?;
            if blob != before.as_bytes() && blob != before.replace("\r\n", "\n").as_bytes() {
                return Err(OperationError::Refused(
                    "generation parent graph bytes differ",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn preparation_root(&self) -> OperationResult<(PathBuf, CommitId)> {
        let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
        let mut git = git::request(self.store.root(), None)?;
        let (common, index, head) = self.identity(&mut git)?;
        self.supported_index(&mut git, &index, &common)?;
        self.attributes(&mut git)?;
        let (source, view) = self.paths.resolve()?;
        let before = text_file(&source)?;
        if text_file(&view)? != render_markdown(&parse(before.as_bytes())?)?
            || lock_path(&index).try_exists()?
            || git.text(&["rev-parse", "--is-shallow-repository"])? != "false"
            || !git
                .run(
                    &[
                        "status",
                        "--porcelain=v1",
                        "-z",
                        "--untracked-files=all",
                        "--ignore-submodules=none",
                    ],
                    b"",
                )?
                .is_empty()
            || !git
                .run_raw(
                    &["diff-index", "--cached", "--quiet", head.as_str(), "--"],
                    b"",
                )?
                .status
                .success()
        {
            return Err(OperationError::Refused(
                "preparation requires clean complete checkpointed Root",
            ));
        }
        Ok((common, head))
    }

    pub(super) fn resume_preparation(
        &self,
        record: &mut JournalRecord,
        git: &mut GitRequest,
        facts: &CheckpointFields,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<()> {
        let PlanGitEffects::Preparation {
            intent,
            mut creation,
            checkout,
            mut after_checkout,
        } = runtime(record)?.plan_git.clone()
        else {
            return Err(OperationError::Journal);
        };
        let generation = &intent.generation;
        let branch = &generation.plan.fields().branch;
        let target_ref = RefState::Present {
            tip: generation.prepared_branch_commit.clone(),
        };
        let observe = |git: &mut GitRequest,
                       record: &JournalRecord|
         -> OperationResult<(RefState, CheckoutState)> {
            self.observe(git, record, false)?;
            let r = runtime(record)?;
            if !r.plan_git.complete()
                && (text_file(&self.paths.resolve()?.0)? != r.source_before()?
                    || text_file(&self.paths.resolve()?.1)? != r.view_before()?)
            {
                return Err(OperationError::Refused(
                    "graph published before preparation effects complete",
                ));
            }
            if crate::infrastructure::graph_checkpoint::preparation_history::PreparationAuthority::observe(&self.config)? != intent.authority {return Err(OperationError::Refused("preparation authority changed"));}
            let body = read_bounded(&crate::infrastructure::operation_store::regular_path(
                self.store.root(),
                &generation.plan.fields().plan_path,
            )?)?;
            if generation.plan.fields().body_sha256.as_ref() != Some(&digest(&body)) {
                return Err(OperationError::Refused(
                    "frozen preparation Plan body changed",
                ));
            }
            for file in &intent.decisions {
                let actual = read_bounded(&crate::infrastructure::operation_store::regular_path(
                    self.store.root(),
                    &file.path,
                )?)?;
                if actual.len() as u64 != file.bytes || digest(&actual) != file.sha256 {
                    return Err(OperationError::Refused(
                        "frozen preparation decision changed",
                    ));
                }
            }
            if let PreservedRef::Rejected { branch, tip, .. } = &intent.preserved {
                if reference(git, branch)? != (RefState::Present { tip: tip.clone() }) {
                    return Err(OperationError::Refused("preserved rejected ref changed"));
                }
            }
            let current_ref = reference(git, branch)?;
            let path = crate::infrastructure::operation_store::regular_path(
                self.store.root(),
                &generation.worktree,
            )?;
            let inventory = git.worktrees()?;
            for item in &inventory {
                if item.branch.as_deref() == Some(branch.as_str())
                    && Path::new(&item.path).canonicalize().ok().as_ref() != Some(&path)
                {
                    return Err(OperationError::Refused(
                        "preparation branch checked out elsewhere",
                    ));
                }
            }
            let desired_is_here = inventory.iter().any(|w| {
                w.branch.as_deref() == Some(branch.as_str())
                    && Path::new(&w.path).canonicalize().ok().as_ref() == Some(&path)
            });
            let observed_branch =
                if !desired_is_here && intent.checkout_change == CheckoutChange::Switch {
                    let CheckoutState::Present { proof } = &intent.before_checkout else {
                        return Err(OperationError::Journal);
                    };
                    &proof.branch
                } else {
                    branch
                };
            let current_checkout = crate::infrastructure::graph_checkpoint::plan_git::checkout(
                self.store.root(),
                &r.common_dir,
                &generation.worktree,
                observed_branch,
                git,
            )?;
            Ok((current_ref, current_checkout))
        };
        if creation != EffectProgress::Complete {
            let (current_ref, current_checkout) = observe(git, record)?;
            if current_checkout != intent.before_checkout {
                return Err(OperationError::Refused(
                    "checkout changed before preparation ref creation",
                ));
            }
            if creation == EffectProgress::NotAttempted {
                if current_ref != RefState::Absent {
                    return Err(OperationError::Refused(
                        "preparation ref appeared before attempted creation",
                    ));
                }
                if let PlanGitEffects::Preparation { creation, .. } = &mut record
                    .checkpoint
                    .as_mut()
                    .ok_or(OperationError::Journal)?
                    .plan_git
                {
                    *creation = EffectProgress::Attempted;
                }
                self.progress(record, facts, OperationOutcome::Pending)?;
                observer.reached(Boundary::PlanGitAttempted)?;
            }
            git.refresh_observations();
            let (current_ref, current_checkout) = observe(git, record)?;
            if current_checkout != intent.before_checkout {
                return Err(OperationError::Refused(
                    "checkout drift after preparation ref intent",
                ));
            }
            if current_ref == RefState::Absent {
                let null = "0".repeat(generation.prepared_branch_commit.as_str().len());
                let result = git.run(
                    &[
                        "update-ref",
                        "--no-deref",
                        &format!("refs/heads/{branch}"),
                        generation.prepared_branch_commit.as_str(),
                        &null,
                    ],
                    b"",
                );
                observer.reached(Boundary::PlanGitReturned)?;
                git.refresh_observations();
                let (current_ref, current_checkout) = observe(git, record)?;
                if current_ref != target_ref || current_checkout != intent.before_checkout {
                    result?;
                    return Err(OperationError::Evidence(
                        "created preparation ref not observed exact",
                    ));
                }
            } else if current_ref != target_ref {
                return Err(OperationError::Refused(
                    "foreign ref after preparation creation intent",
                ));
            }
            if let PlanGitEffects::Preparation { creation, .. } = &mut record
                .checkpoint
                .as_mut()
                .ok_or(OperationError::Journal)?
                .plan_git
            {
                *creation = EffectProgress::Complete;
            }
            self.progress(record, facts, OperationOutcome::Pending)?;
            creation = EffectProgress::Complete;
            observer.reached(Boundary::PlanGitRecorded)?;
        }
        if creation != EffectProgress::Complete {
            return Err(OperationError::Journal);
        }
        git.refresh_observations();
        let (current_ref, current_checkout) = observe(git, record)?;
        if checkout != EffectProgress::Complete {
            let expected_ref = if intent.checkout_change == CheckoutChange::FastForward {
                &intent.before_ref
            } else {
                &target_ref
            };
            if checkout == EffectProgress::NotAttempted {
                if &current_ref != expected_ref || current_checkout != intent.before_checkout {
                    return Err(OperationError::Refused(
                        "preparation checkout changed before attempted effect",
                    ));
                }
                if let PlanGitEffects::Preparation { checkout, .. } = &mut record
                    .checkpoint
                    .as_mut()
                    .ok_or(OperationError::Journal)?
                    .plan_git
                {
                    *checkout = EffectProgress::Attempted;
                }
                self.progress(record, facts, OperationOutcome::Pending)?;
                observer.reached(Boundary::PlanGitAttempted)?;
            }
            git.refresh_observations();
            let (current_ref, current_checkout) = observe(git, record)?;
            if current_checkout == intent.before_checkout
                && &current_ref == expected_ref
                && after_checkout.is_none()
            {
                let path = crate::infrastructure::operation_store::regular_path(
                    self.store.root(),
                    &generation.worktree,
                )?;
                let result = match intent.checkout_change {
                    CheckoutChange::Attach => {
                        target_policy(git, &generation.prepared_branch_commit)?;
                        git.run(
                            &[
                                "worktree",
                                "add",
                                "--",
                                generation.worktree.as_str(),
                                branch,
                            ],
                            b"",
                        )
                    }
                    CheckoutChange::Switch => {
                        let mut source = git::request(&path, None)?;
                        target_policy(&mut source, &generation.prepared_branch_commit)?;
                        source.run(
                            &["switch", "--no-guess", "--no-overwrite-ignore", branch],
                            b"",
                        )
                    }
                    CheckoutChange::FastForward => {
                        let mut source = git::request(&path, None)?;
                        target_policy(&mut source, &generation.prepared_branch_commit)?;
                        source.run(
                            &[
                                "merge",
                                "--ff-only",
                                "--no-edit",
                                "--no-autostash",
                                generation.prepared_branch_commit.as_str(),
                            ],
                            b"",
                        )
                    }
                    CheckoutChange::None => return Err(OperationError::Journal),
                };
                observer.reached(Boundary::PlanGitReturned)?;
                git.refresh_observations();
                let (current_ref, current_checkout) = observe(git, record)?;
                let CheckoutState::Present { proof } = current_checkout else {
                    result?;
                    return Err(OperationError::Evidence(
                        "preparation checkout not observed complete",
                    ));
                };
                check_after(&intent, &proof)?;
                if current_ref != target_ref {
                    result?;
                    return Err(OperationError::Refused(
                        "preparation target ref changed during checkout",
                    ));
                }
                after_checkout = Some(Box::new(proof));
            } else {
                let CheckoutState::Present { proof } = current_checkout else {
                    return Err(OperationError::Refused(
                        "unknown partial preparation checkout",
                    ));
                };
                check_after(&intent, &proof)?;
                if current_ref != target_ref
                    || after_checkout
                        .as_ref()
                        .is_some_and(|known| known.as_ref() != &proof)
                    || intent.checkout_change == CheckoutChange::Attach && after_checkout.is_none()
                {
                    return Err(OperationError::Refused(
                        "unrecorded or foreign preparation checkout identity",
                    ));
                }
                after_checkout = Some(Box::new(proof));
            }
            // Observed admin/index identity is durable before completion. An
            // unrecorded attach cannot be adopted after an uncertain result.
            if let PlanGitEffects::Preparation {
                after_checkout: after,
                ..
            } = &mut record
                .checkpoint
                .as_mut()
                .ok_or(OperationError::Journal)?
                .plan_git
            {
                *after = after_checkout.clone();
            }
            self.progress(record, facts, OperationOutcome::Pending)?;
            observer.reached(Boundary::PlanCheckoutObserved)?;
            git.refresh_observations();
            let (current_ref, current_checkout) = observe(git, record)?;
            if current_ref != target_ref
                || current_checkout
                    != (CheckoutState::Present {
                        proof: *after_checkout.clone().ok_or(OperationError::Journal)?,
                    })
            {
                return Err(OperationError::Refused(
                    "observed preparation checkout changed before completion",
                ));
            }
            if let PlanGitEffects::Preparation { checkout, .. } = &mut record
                .checkpoint
                .as_mut()
                .ok_or(OperationError::Journal)?
                .plan_git
            {
                *checkout = EffectProgress::Complete;
            }
            self.progress(record, facts, OperationOutcome::Pending)?;
            observer.reached(Boundary::PlanGitRecorded)?;
        } else if current_ref != target_ref
            || current_checkout
                != (CheckoutState::Present {
                    proof: *after_checkout.ok_or(OperationError::Journal)?,
                })
        {
            return Err(OperationError::Refused(
                "completed preparation resources changed",
            ));
        }
        Ok(())
    }
}

fn check_after(intent: &PrepareIntent, proof: &CheckoutProof) -> OperationResult<()> {
    let generation = &intent.generation;
    if proof.branch != generation.plan.fields().branch
        || proof.head != generation.prepared_branch_commit
        || proof.tree != intent.target_tree
    {
        return Err(OperationError::Refused(
            "preparation checkout is not exact target",
        ));
    }
    if let CheckoutState::Present { proof: old } = &intent.before_checkout {
        if proof.admin != old.admin
            || proof.admin_stamp != old.admin_stamp
            || proof.gitfile_sha256 != old.gitfile_sha256
            || proof.backlink_sha256 != old.backlink_sha256
        {
            return Err(OperationError::Refused(
                "preparation replaced the known lineage admin identity",
            ));
        }
    }
    Ok(())
}
