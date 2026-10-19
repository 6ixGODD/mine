//! Frozen preparation intent; exact effects, not a general Git command queue.
use super::{CheckpointRuntime, digest, git, plan_git::*, preparation_history::*};
use crate::domain::{config::MineConfig, graph::*, lifecycle_operation::*, status::PlanStatus};
use crate::infrastructure::{
    operation_store::{read_bounded, regular_path},
    toml_store::render_markdown,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutChange {
    None,
    Attach,
    Switch,
    FastForward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareIntent {
    pub generation: PreparationGeneration,
    pub mode: PreparationMode,
    pub authority: PreparationAuthority,
    pub decisions: Vec<FileIdentity>,
    pub original_generation: GenerationBefore,
    pub before_ref: RefState,
    pub before_checkout: CheckoutState,
    pub preserved: PreservedRef,
    pub target_tree: CommitId,
    pub checkout_change: CheckoutChange,
}

pub(crate) struct PrepareInput {
    pub generation: PreparationGeneration,
    pub mode: PreparationMode,
    pub decisions: Vec<FileIdentity>,
}

impl PrepareIntent {
    pub fn validate(
        &self,
        runtime: &CheckpointRuntime,
        operation: &LifecycleOperation,
        creation: EffectProgress,
        checkout: EffectProgress,
        after_checkout: &Option<Box<CheckoutProof>>,
    ) -> OperationResult<()> {
        let before: PlanWorkspace =
            toml::from_str(runtime.source_before()?).map_err(|_| OperationError::Journal)?;
        let generation = &self.generation;
        let identity = generation.plan.fields();
        let node = before
            .get(&identity.plan_id)
            .ok_or(OperationError::Journal)?;
        let original = original(node);
        let body = identity
            .body_sha256
            .as_ref()
            .ok_or(OperationError::Journal)?;
        if node.status != PlanStatus::Ready
            || !node.branch_anchor.is_empty()
            || operation.fields().plan.as_ref() != Some(&generation.plan)
            || operation.fields().registered_worktree.as_ref() != Some(&generation.worktree)
            || generation.operation_id != operation.fields().operation_id
            || generation.actor != operation.fields().actor
            || generation.observed_graph_revision != before.revision
            || generation.observed_root_commit
                != operation
                    .fields()
                    .checkpoint
                    .as_ref()
                    .ok_or(OperationError::Journal)?
                    .fields()
                    .original_head
            || generation.ordinal
                != u64::try_from(node.preparations.len())
                    .map_err(|_| OperationError::Journal)?
                    .checked_add(1)
                    .ok_or(OperationError::Journal)?
            || identity.repository_id != operation.fields().repository_id
            || identity.workspace_id != before.workspace_id
            || identity.plan_path.as_str() != node.path
            || identity.branch != node.branch
            || identity.lineage_id != node.lineage_id
            || generation.worktree.as_str() != node.worktree_path
            || node.body_sha256.as_ref().is_some_and(|known| known != body)
            || self.original_generation != original
            || self.authority.repository != operation.fields().repository_id
            || self.authority.integration != before.integration_branch
            || self.authority.stable != before.stable_branch
            || self.authority.source != runtime.source_path
            || self.authority.view != runtime.view_path
            || policy_for(&self.authority, &before, node, body, &self.decisions)?
                != generation.policy_sha256
        {
            return Err(OperationError::Journal);
        }
        crate::domain::validation::validate_with_worktrees_root(
            &before,
            self.authority.worktrees_root.as_str(),
        )?;
        let target = match &self.preserved {
            PreservedRef::None => {
                if crate::domain::plan_id::CanonicalPlanId::parse(&node.id)?.is_compensation() {
                    return Err(OperationError::Journal);
                }
                &generation.observed_root_commit
            }
            PreservedRef::Rejected {
                plan_id,
                branch,
                tip,
            } => {
                let rejected =
                    crate::domain::validation::declaring_rejected_predecessor(&before, &node.id)?;
                if &rejected.id != plan_id || &rejected.branch != branch {
                    return Err(OperationError::Journal);
                }
                tip
            }
        };
        if target != &generation.prepared_branch_commit {
            return Err(OperationError::Journal);
        }
        match self.mode {
            PreparationMode::Initial => {
                if original != GenerationBefore::Absent || self.before_ref != RefState::Absent {
                    return Err(OperationError::Journal);
                }
            }
            PreparationMode::Recover => {
                if original != GenerationBefore::Absent
                    || self.before_ref
                        != (RefState::Present {
                            tip: target.clone(),
                        })
                {
                    return Err(OperationError::Journal);
                }
            }
            PreparationMode::Reprepare => {
                let GenerationBefore::Present { generation: old } = &original else {
                    return Err(OperationError::Journal);
                };
                if self.before_ref
                    != (RefState::Present {
                        tip: old.prepared_branch_commit.clone(),
                    })
                {
                    return Err(OperationError::Journal);
                }
            }
        }
        let old_checkout = match &self.before_checkout {
            CheckoutState::Absent => None,
            CheckoutState::Present { proof } => Some(proof),
        };
        match self.checkout_change {
            CheckoutChange::Attach
                if old_checkout.is_none() && matches!(self.preserved, PreservedRef::None) => {}
            CheckoutChange::Switch => {
                let PreservedRef::Rejected { branch, tip, .. } = &self.preserved else {
                    return Err(OperationError::Journal);
                };
                if old_checkout.is_none_or(|old| &old.branch != branch || &old.head != tip) {
                    return Err(OperationError::Journal);
                }
            }
            CheckoutChange::FastForward
                if self.mode == PreparationMode::Reprepare
                    && matches!(self.preserved, PreservedRef::None) =>
            {
                if old_checkout.is_none_or(|old| {
                    old.branch != identity.branch
                        || self.before_ref
                            != (RefState::Present {
                                tip: old.head.clone(),
                            })
                        || &old.head == target
                }) {
                    return Err(OperationError::Journal);
                }
            }
            CheckoutChange::None => {
                if old_checkout
                    .is_none_or(|old| old.branch != identity.branch || &old.head != target)
                {
                    return Err(OperationError::Journal);
                }
            }
            _ => return Err(OperationError::Journal),
        }
        self.validate_progress(runtime, operation, creation, checkout, after_checkout)?;
        let mut expected = before.clone();
        let subject = expected
            .get_mut(&identity.plan_id)
            .ok_or(OperationError::Journal)?;
        subject.preparations.push(generation.clone());
        subject.updated_at = generation.prepared_at.as_str().into();
        expected.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
        expected.revision = before
            .revision
            .checked_add(1)
            .ok_or(OperationError::Journal)?;
        let actual: PlanWorkspace =
            toml::from_str(&runtime.source_after).map_err(|_| OperationError::Journal)?;
        if actual != expected || render_markdown(&expected)? != runtime.view_after {
            return Err(OperationError::Refused(
                "preparation candidate exceeds exact generation append",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_progress(
        &self,
        runtime: &CheckpointRuntime,
        operation: &LifecycleOperation,
        creation: EffectProgress,
        checkout: EffectProgress,
        after_checkout: &Option<Box<CheckoutProof>>,
    ) -> OperationResult<()> {
        let identity = self.generation.plan.fields();
        let target = &self.generation.prepared_branch_commit;
        let old_checkout = match &self.before_checkout {
            CheckoutState::Absent => None,
            CheckoutState::Present { proof } => Some(proof),
        };
        if matches!(self.before_ref, RefState::Present { .. })
            && creation != EffectProgress::Complete
            || checkout != EffectProgress::NotAttempted && creation != EffectProgress::Complete
            || self.checkout_change == CheckoutChange::None && checkout != EffectProgress::Complete
            || checkout == EffectProgress::Complete && after_checkout.is_none()
            || after_checkout.is_some() && checkout == EffectProgress::NotAttempted
            || checkout != EffectProgress::Complete
                && operation
                    .fields()
                    .checkpoint
                    .as_ref()
                    .is_some_and(|c| c.fields().stage >= CheckpointStage::GitWorkspaceMutated)
        {
            return Err(OperationError::Journal);
        }
        if let Some(after) = after_checkout {
            if after.branch != identity.branch
                || &after.head != target
                || after.tree != self.target_tree
                || !after
                    .admin
                    .starts_with(runtime.common_dir.join("worktrees"))
            {
                return Err(OperationError::Journal);
            }
            if let Some(old) = old_checkout {
                if after.admin != old.admin
                    || after.admin_stamp != old.admin_stamp
                    || after.gitfile_sha256 != old.gitfile_sha256
                    || after.backlink_sha256 != old.backlink_sha256
                    || self.checkout_change == CheckoutChange::None && after.as_ref() != old
                {
                    return Err(OperationError::Journal);
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn original(node: &PlanNode) -> GenerationBefore {
    node.preparations
        .last()
        .cloned()
        .map_or(GenerationBefore::Absent, |generation| {
            GenerationBefore::Present {
                generation: Box::new(generation),
            }
        })
}

pub(crate) fn completed_checkout(
    kernel: &super::CheckpointKernel,
    generation: &PreparationGeneration,
) -> OperationResult<CheckoutProof> {
    let record = kernel.store().query(&generation.operation_id)?;
    let runtime = record.checkpoint.as_ref().ok_or(OperationError::Journal)?;
    let PlanGitEffects::Preparation {
        intent,
        after_checkout,
        ..
    } = &runtime.plan_git
    else {
        return Err(OperationError::Refused(
            "generation does not bind native preparation intent",
        ));
    };
    if intent.generation != *generation
        || !terminal(&record.operation)
        || record.operation.fields().outcome != OperationOutcome::Committed
        || !runtime.plan_git.complete()
    {
        return Err(OperationError::Refused(
            "generation checkpoint is not completed exact preparation",
        ));
    }
    kernel.generation_history(&record)?;
    Ok(*after_checkout.clone().ok_or(OperationError::Journal)?)
}

pub(crate) fn same_admin(expected: &CheckoutProof, actual: &CheckoutProof) -> bool {
    expected.admin == actual.admin
        && expected.admin_stamp == actual.admin_stamp
        && expected.gitfile_sha256 == actual.gitfile_sha256
        && expected.backlink_sha256 == actual.backlink_sha256
}

/// Guard the target tree BEFORE switch/add/fast-forward can execute filters.
pub(crate) fn target_policy(
    git: &mut crate::infrastructure::git_evidence::GitRequest,
    tip: &CommitId,
) -> OperationResult<()> {
    let tree = git.run_raw(&["ls-tree", "-r", "-z", tip.as_str(), "--"], b"")?;
    if !tree.status.success() || !tree.stderr.is_empty() {
        return Err(OperationError::Evidence("target tree query diagnostic"));
    }
    if !tree.stdout.is_empty() && tree.stdout.last() != Some(&0) {
        return Err(OperationError::Journal);
    }
    let mut paths = Vec::new();
    for entry in tree.stdout.split(|b| *b == 0).filter(|v| !v.is_empty()) {
        let tab = entry
            .iter()
            .position(|b| *b == b'\t')
            .ok_or(OperationError::Journal)?;
        let metadata = std::str::from_utf8(&entry[..tab]).map_err(|_| OperationError::Journal)?;
        let fields = metadata.split(' ').collect::<Vec<_>>();
        let path = &entry[tab + 1..];
        if fields.len() != 3
            || fields[1] != "blob"
            || !matches!(fields[0], "100644" | "100755" | "120000")
            || fields[0] == "120000"
                && (path == b".gitattributes" || path.ends_with(b"/.gitattributes"))
        {
            return Err(OperationError::Refused(
                "unsupported target submodule/attribute tree",
            ));
        }
        CommitId::new(fields[2])?;
        if path.is_empty() {
            return Err(OperationError::Journal);
        }
        paths.extend_from_slice(path);
        paths.push(0);
    }
    let attributes = git.run_raw(
        &[
            "check-attr",
            &format!("--source={}", tip.as_str()),
            "-z",
            "--stdin",
            "filter",
            "ident",
            "working-tree-encoding",
        ],
        &paths,
    )?;
    if !attributes.status.success() || !attributes.stderr.is_empty() {
        return Err(OperationError::Evidence(
            "target attribute query diagnostic",
        ));
    }
    let mut result = attributes.stdout.split(|b| *b == 0);
    for path in paths.split(|b| *b == 0).filter(|v| !v.is_empty()) {
        for attribute in [
            b"filter".as_slice(),
            b"ident".as_slice(),
            b"working-tree-encoding".as_slice(),
        ] {
            if result.next() != Some(path)
                || result.next() != Some(attribute)
                || !result
                    .next()
                    .is_some_and(|value| matches!(value, b"unspecified" | b"unset"))
            {
                return Err(OperationError::Refused(
                    "unsupported target checkout transformation",
                ));
            }
        }
    }
    if paths.is_empty() {
        if !attributes.stdout.is_empty() {
            return Err(OperationError::Journal);
        }
    } else if result.next() != Some(b"".as_slice()) || result.next().is_some() {
        return Err(OperationError::Journal);
    }
    Ok(())
}

pub(crate) fn build(
    root: &Path,
    common: &Path,
    config: &MineConfig,
    workspace: &PlanWorkspace,
    generation: PreparationGeneration,
    mode: PreparationMode,
    decisions: Vec<FileIdentity>,
) -> OperationResult<(
    PrepareIntent,
    EffectProgress,
    EffectProgress,
    Option<Box<CheckoutProof>>,
)> {
    let identity = generation.plan.fields();
    let node = workspace
        .get(&identity.plan_id)
        .ok_or(OperationError::Journal)?;
    let body = read_bounded(&regular_path(root, &identity.plan_path)?)?;
    if identity.body_sha256.as_ref() != Some(&digest(&body)) {
        return Err(OperationError::Refused("preparation body changed"));
    }
    let mut git = git::request(root, None)?;
    committed_body(
        &mut git,
        &generation.plan,
        &generation.observed_root_commit,
        &body,
    )?;
    for file in &decisions {
        let bytes = read_bounded(&regular_path(root, &file.path)?)?;
        if file.bytes != bytes.len() as u64 || digest(&bytes) != file.sha256 {
            return Err(OperationError::Refused("preparation Design input changed"));
        }
    }
    let preserved = if crate::domain::plan_id::CanonicalPlanId::parse(&node.id)?.is_compensation() {
        let predecessor =
            crate::domain::validation::declaring_rejected_predecessor(workspace, &node.id)?;
        let RefState::Present { tip } = reference(&mut git, &predecessor.branch)? else {
            return Err(OperationError::Refused("preserved rejected ref missing"));
        };
        PreservedRef::Rejected {
            plan_id: predecessor.id.clone(),
            branch: predecessor.branch.clone(),
            tip,
        }
    } else {
        PreservedRef::None
    };
    let before_ref = reference(&mut git, &node.branch)?;
    let before_checkout = match &preserved {
        PreservedRef::Rejected { branch, .. }
            if !git
                .worktrees()?
                .iter()
                .any(|w| w.branch.as_deref() == Some(node.branch.as_str())) =>
        {
            checkout(root, common, &generation.worktree, branch, &mut git)?
        }
        _ => checkout(root, common, &generation.worktree, &node.branch, &mut git)?,
    };
    let change = match &before_checkout {
        CheckoutState::Absent => CheckoutChange::Attach,
        CheckoutState::Present { proof } if proof.branch != node.branch => CheckoutChange::Switch,
        CheckoutState::Present { proof } if proof.head != generation.prepared_branch_commit => {
            if mode != PreparationMode::Reprepare
                || !matches!(preserved, PreservedRef::None)
                || !git.is_ancestor(
                    proof.head.as_str(),
                    generation.prepared_branch_commit.as_str(),
                )?
            {
                return Err(OperationError::Refused(
                    "preparation refresh is not a safe explicit fast-forward",
                ));
            }
            CheckoutChange::FastForward
        }
        _ => CheckoutChange::None,
    };
    let creation = if matches!(before_ref, RefState::Absent) {
        EffectProgress::NotAttempted
    } else {
        EffectProgress::Complete
    };
    let (checkout_progress, after) = if change == CheckoutChange::None {
        let CheckoutState::Present { proof } = &before_checkout else {
            return Err(OperationError::Journal);
        };
        (EffectProgress::Complete, Some(Box::new(proof.clone())))
    } else {
        (EffectProgress::NotAttempted, None)
    };
    let target_tree = CommitId::new(git.text(&[
        "rev-parse",
        &format!("{}^{{tree}}", generation.prepared_branch_commit.as_str()),
    ])?)?;
    target_policy(&mut git, &generation.prepared_branch_commit)?;
    Ok((
        PrepareIntent {
            generation,
            mode,
            authority: PreparationAuthority::observe(config)?,
            decisions,
            original_generation: original(node),
            before_ref,
            before_checkout,
            preserved,
            target_tree,
            checkout_change: change,
        },
        creation,
        checkout_progress,
        after,
    ))
}
