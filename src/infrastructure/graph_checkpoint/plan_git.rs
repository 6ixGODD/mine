//! Closed, operation-owned Plan Git effects; never a shell/recovery recipe.
use super::{CheckpointRuntime, digest, git};
use crate::domain::{graph::*, lifecycle_operation::*, status::PlanStatus};
use crate::infrastructure::{
    git_evidence::GitRequest,
    operation_store::{read_bounded, regular_path},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RefState {
    Absent,
    Present { tip: CommitId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GenerationBefore {
    Absent,
    Present {
        generation: Box<PreparationGeneration>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdminStamp {
    Unix { device: u64, inode: u64 },
    Windows { created: u64 },
}
fn stamp(path: &Path) -> OperationResult<AdminStamp> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(OperationError::Refused(
            "Git admin is not a regular directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(AdminStamp::Unix {
            device: meta.dev(),
            inode: meta.ino(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(OperationError::Refused("Git admin reparse point"));
        }
        Ok(AdminStamp::Windows {
            created: meta.creation_time(),
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(OperationError::Refused(
            "unsupported Git admin identity platform",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutProof {
    pub admin: PathBuf,
    pub admin_stamp: AdminStamp,
    pub gitfile_sha256: Sha256,
    pub backlink_sha256: Sha256,
    pub index_sha256: Sha256,
    pub branch: String,
    pub head: CommitId,
    pub tree: CommitId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckoutState {
    Absent,
    Present { proof: CheckoutProof },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreservedRef {
    None,
    Rejected {
        plan_id: String,
        branch: String,
        tip: CommitId,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WithdrawIntent {
    pub plan: PlanIdentity,
    pub worktree: RepoPath,
    pub original_generation: GenerationBefore,
    pub policy_sha256: Sha256,
    pub before_ref: RefState,
    pub before_checkout: CheckoutState,
    pub preserved: PreservedRef,
    pub reason: String,
    pub withdrawn_at: UtcTimestamp,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectProgress {
    NotAttempted,
    Attempted,
    Complete,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanGitEffects {
    None,
    Withdrawal {
        intent: Box<WithdrawIntent>,
        removal: EffectProgress,
        deletion: EffectProgress,
    },
    Preparation {
        intent: Box<super::preparation_intent::PrepareIntent>,
        creation: EffectProgress,
        checkout: EffectProgress,
        after_checkout: Option<Box<CheckoutProof>>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanGitSummary {
    pub plan_id: String,
    pub step: &'static str,
    pub progress: EffectProgress,
}
impl PlanGitEffects {
    pub fn summary(&self) -> Option<PlanGitSummary> {
        match self {
            Self::None => None,
            Self::Preparation {
                intent,
                creation,
                checkout,
                ..
            } => Some(PlanGitSummary {
                plan_id: intent.generation.plan.fields().plan_id.clone(),
                step: if *creation != EffectProgress::Complete {
                    "create_ref"
                } else if *checkout != EffectProgress::Complete {
                    match intent.checkout_change {
                        super::preparation_intent::CheckoutChange::Attach => "attach_checkout",
                        super::preparation_intent::CheckoutChange::Switch => "switch_compensation",
                        super::preparation_intent::CheckoutChange::FastForward => {
                            "fast_forward_checkout"
                        }
                        super::preparation_intent::CheckoutChange::None => "complete",
                    }
                } else {
                    "complete"
                },
                progress: if *creation != EffectProgress::Complete {
                    *creation
                } else {
                    *checkout
                },
            }),
            Self::Withdrawal {
                intent,
                removal,
                deletion,
            } => Some(PlanGitSummary {
                plan_id: intent.plan.fields().plan_id.clone(),
                step: if *removal != EffectProgress::Complete {
                    "remove_checkout"
                } else if *deletion != EffectProgress::Complete {
                    "delete_ref"
                } else {
                    "complete"
                },
                progress: if *removal != EffectProgress::Complete {
                    *removal
                } else {
                    *deletion
                },
            }),
        }
    }
    pub fn complete(&self) -> bool {
        matches!(
            self,
            Self::None
                | Self::Withdrawal {
                    removal: EffectProgress::Complete,
                    deletion: EffectProgress::Complete,
                    ..
                }
                | Self::Preparation {
                    creation: EffectProgress::Complete,
                    checkout: EffectProgress::Complete,
                    ..
                }
        )
    }
    pub fn validate(
        &self,
        runtime: &CheckpointRuntime,
        operation: &LifecycleOperation,
    ) -> OperationResult<()> {
        self.validate_progress(runtime, operation)?;
        if let Self::Preparation {
            intent,
            creation,
            checkout,
            after_checkout,
        } = self
        {
            return intent.validate(runtime, operation, *creation, *checkout, after_checkout);
        }
        let Self::Withdrawal { intent, .. } = self else {
            if operation.fields().plan.is_some() || operation.fields().registered_worktree.is_some()
            {
                return Err(OperationError::Journal);
            }
            return Ok(());
        };
        let before: PlanWorkspace =
            toml::from_str(runtime.source_before()?).map_err(|_| OperationError::Journal)?;
        let after: PlanWorkspace =
            toml::from_str(&runtime.source_after).map_err(|_| OperationError::Journal)?;
        let id = &intent.plan.fields().plan_id;
        let node = before.get(id).ok_or(OperationError::Journal)?;
        let generation =
            node.preparations
                .last()
                .cloned()
                .map_or(GenerationBefore::Absent, |generation| {
                    GenerationBefore::Present {
                        generation: Box::new(generation),
                    }
                });
        if !matches!(
            node.status,
            PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready
        ) || operation.fields().plan.as_ref() != Some(&intent.plan)
            || operation.fields().registered_worktree.as_ref() != Some(&intent.worktree)
            || intent.plan.fields().repository_id != operation.fields().repository_id
            || intent.plan.fields().workspace_id != before.workspace_id
            || intent.plan.fields().plan_path.as_str() != node.path
            || intent.plan.fields().lineage_id != node.lineage_id
            || intent.plan.fields().branch != node.branch
            || intent.plan.fields().body_sha256.is_none()
            || node
                .body_sha256
                .as_ref()
                .is_some_and(|v| Some(v) != intent.plan.fields().body_sha256.as_ref())
            || intent.worktree.as_str() != node.worktree_path
            || intent.original_generation != generation
            || intent.reason.trim().is_empty()
        {
            return Err(OperationError::Journal);
        }
        // Pure record validation binds the digest to the frozen graph/config
        // hash, not today's files; live config is rechecked before each effect.
        let expected_policy = withdrawal_policy(&runtime.config_sha256, node)?;
        if intent.policy_sha256 != expected_policy {
            return Err(OperationError::Journal);
        }
        let mut expected = before.clone();
        let target = expected.get_mut(id).ok_or(OperationError::Journal)?;
        target.status = PlanStatus::Withdrawn;
        target.withdraw_reason = intent.reason.clone();
        target.withdrawn_at = intent.withdrawn_at.as_str().into();
        target.updated_at = intent.withdrawn_at.as_str().into();
        expected.revision = before
            .revision
            .checked_add(1)
            .ok_or(OperationError::Journal)?;
        expected.schema_version = CURRENT_GRAPH_SCHEMA_VERSION;
        if expected != after
            || super::super::toml_store::render_markdown(&expected)? != runtime.view_after
        {
            return Err(OperationError::Refused(
                "withdrawal candidate exceeds exact typed transition",
            ));
        }
        match (&intent.before_ref, &intent.before_checkout) {
            (RefState::Absent, CheckoutState::Present { .. }) => {
                return Err(OperationError::Journal);
            }
            (_, CheckoutState::Present { proof })
                if proof.branch != node.branch || !proof.admin.is_absolute() =>
            {
                return Err(OperationError::Journal);
            }
            (RefState::Present { tip }, CheckoutState::Present { proof }) if tip != &proof.head => {
                return Err(OperationError::Journal);
            }
            _ => {}
        }
        if matches!(node.status, PlanStatus::Draft | PlanStatus::Blocked)
            && (!matches!(intent.before_ref, RefState::Absent)
                || !matches!(intent.before_checkout, CheckoutState::Absent))
        {
            return Err(OperationError::Journal);
        }
        match &intent.preserved {
            PreservedRef::None
                if crate::domain::plan_id::CanonicalPlanId::parse(id)?.is_compensation() =>
            {
                return Err(OperationError::Journal);
            }
            PreservedRef::Rejected {
                plan_id,
                branch,
                tip,
            } => {
                let rejected =
                    crate::domain::validation::declaring_rejected_predecessor(&before, id)?;
                if &rejected.id != plan_id
                    || &rejected.branch != branch
                    || matches!(&intent.before_ref, RefState::Present { tip:actual } if actual != tip)
                {
                    return Err(OperationError::Journal);
                }
            }
            _ => {}
        }
        Ok(())
    }
    /// Recheck produced/progress facts even when immutable intent and graphs
    /// have a request-local validation witness. No mutable Git fact is cached.
    pub(crate) fn validate_progress(
        &self,
        runtime: &CheckpointRuntime,
        operation: &LifecycleOperation,
    ) -> OperationResult<()> {
        match self {
            Self::None => {
                if operation.fields().plan.is_some()
                    || operation.fields().registered_worktree.is_some()
                {
                    return Err(OperationError::Journal);
                }
            }
            Self::Preparation {
                intent,
                creation,
                checkout,
                after_checkout,
            } => {
                intent.validate_progress(
                    runtime,
                    operation,
                    *creation,
                    *checkout,
                    after_checkout,
                )?;
            }
            Self::Withdrawal {
                intent,
                removal,
                deletion,
            } => {
                if matches!(intent.before_checkout, CheckoutState::Absent)
                    && *removal != EffectProgress::Complete
                    || matches!(intent.before_ref, RefState::Absent)
                        && *deletion != EffectProgress::Complete
                    || *deletion != EffectProgress::NotAttempted
                        && *removal != EffectProgress::Complete
                    || !self.complete()
                        && operation.fields().checkpoint.as_ref().is_some_and(|c| {
                            c.fields().stage >= CheckpointStage::GitWorkspaceMutated
                        })
                {
                    return Err(OperationError::Journal);
                }
            }
        }
        Ok(())
    }
    pub fn validate_successor(&self, before: &Self) -> OperationResult<()> {
        match (before, self) {
            (Self::None, Self::None) => Ok(()),
            (
                Self::Preparation {
                    intent: a,
                    creation: ac,
                    checkout: at,
                    after_checkout: aa,
                },
                Self::Preparation {
                    intent: b,
                    creation: bc,
                    checkout: bt,
                    after_checkout: ba,
                },
            ) if a == b
                && monotonic(*ac, *bc)
                && monotonic(*at, *bt)
                && aa.as_ref().is_none_or(|known| ba.as_ref() == Some(known)) =>
            {
                Ok(())
            }
            (
                Self::Withdrawal {
                    intent: a,
                    removal: ar,
                    deletion: ad,
                },
                Self::Withdrawal {
                    intent: b,
                    removal: br,
                    deletion: bd,
                },
            ) if a == b && monotonic(*ar, *br) && monotonic(*ad, *bd) => Ok(()),
            _ => Err(OperationError::Refused("Plan Git intent/progress changed")),
        }
    }
}
fn monotonic(before: EffectProgress, after: EffectProgress) -> bool {
    before == after
        || matches!(
            (before, after),
            (EffectProgress::NotAttempted, EffectProgress::Attempted)
                | (EffectProgress::Attempted, EffectProgress::Complete)
        )
}
pub(crate) fn withdrawal_policy(config: &Sha256, node: &PlanNode) -> OperationResult<Sha256> {
    Ok(digest(
        &serde_json::to_vec(&(config, node)).map_err(|_| OperationError::Journal)?,
    ))
}

pub(crate) fn reference(git: &mut GitRequest, branch: &str) -> OperationResult<RefState> {
    let name = format!("refs/heads/{branch}");
    let output = git.run_raw(
        &[
            "for-each-ref",
            "--format=%(refname)%00%(symref)%00%(objectname)%00%(objecttype)",
            &name,
        ],
        b"",
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(OperationError::Evidence(
            "Plan ref inventory query diagnostic",
        ));
    }
    let bytes = output.stdout;
    if bytes.is_empty() {
        // Existing exact rows already carry their symbolic/direct identity.
        // An empty inventory alone cannot prove absence: dangling symbolic
        // refs may be omitted, so preserve the native symbolic query here.
        let symbolic = git.run_raw(&["symbolic-ref", "-q", &name], b"")?;
        if !symbolic.stderr.is_empty() {
            return Err(OperationError::Evidence(
                "Plan ref symbolic query diagnostic",
            ));
        }
        match symbolic.status.code() {
            Some(1) => {}
            Some(0) => return Err(OperationError::Refused("Plan ref is symbolic")),
            _ => return Err(OperationError::Evidence("Plan ref symbolic query failed")),
        }
        return Ok(RefState::Absent);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| OperationError::Journal)?;
    let fields = text
        .strip_suffix('\n')
        .ok_or(OperationError::Journal)?
        .split('\0')
        .collect::<Vec<_>>();
    if fields.len() == 4 && fields[0] == name && !fields[1].is_empty() {
        return Err(OperationError::Refused("Plan ref is symbolic"));
    }
    if fields.len() != 4 || fields[0] != name || !fields[1].is_empty() || fields[3] != "commit" {
        return Err(OperationError::Refused(
            "Plan ref is not one exact direct commit ref",
        ));
    }
    let tip = CommitId::new(fields[2])?;
    git.validate_commits(&[tip.as_str().into()])?;
    Ok(RefState::Present { tip })
}

/// All stopped handoffs in one ownership observation share one ref inventory
/// and one deduplicated exact-object batch, never three children per old Plan.
pub(crate) fn preserved_references(
    git: &mut GitRequest,
    branches: &[(String, CommitId)],
) -> OperationResult<()> {
    if branches.is_empty() {
        return Ok(());
    }
    let expected = branches
        .iter()
        .map(|(name, tip)| (format!("refs/heads/{name}"), tip))
        .collect::<std::collections::HashMap<_, _>>();
    if expected.len() != branches.len() {
        return Err(OperationError::Refused(
            "duplicate stopped-handoff ref identity",
        ));
    }
    let width = branches[0].1.as_str().len();
    let bytes = git.run(
        &[
            "for-each-ref",
            "--format=%(refname)%00%(symref)%00%(objectname)%00%(objecttype)",
        ],
        b"",
    )?;
    let text = std::str::from_utf8(&bytes).map_err(|_| OperationError::Journal)?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(OperationError::Journal);
    }
    let mut found = std::collections::HashSet::new();
    for row in text.split_terminator('\n') {
        let fields = row.split('\0').collect::<Vec<_>>();
        if fields.len() != 4
            || !fields[0].starts_with("refs/")
            || fields[2].len() != width
            || !matches!(fields[3], "commit" | "tag" | "tree" | "blob")
        {
            return Err(OperationError::Journal);
        }
        let actual = CommitId::new(fields[2])?;
        if let Some(tip) = expected.get(fields[0]) {
            if !fields[1].is_empty()
                || fields[3] != "commit"
                || actual != **tip
                || !found.insert(fields[0])
            {
                return Err(OperationError::Refused(
                    "preserved stopped-handoff ref changed or became symbolic",
                ));
            }
        }
    }
    if found.len() != expected.len() {
        return Err(OperationError::Refused(
            "preserved stopped-handoff ref missing",
        ));
    }
    git.validate_commits(
        &branches
            .iter()
            .map(|(_, tip)| tip.as_str().to_owned())
            .collect::<Vec<_>>(),
    )?;
    Ok(())
}

pub(crate) fn checkout(
    root: &Path,
    common: &Path,
    locator: &RepoPath,
    branch: &str,
    git: &mut GitRequest,
) -> OperationResult<CheckoutState> {
    let target = regular_path(root, locator)?;
    let inventory = git.worktrees()?;
    let mut at_path = Vec::new();
    for record in &inventory {
        let observed = Path::new(&record.path);
        // Missing registrations/locked worktrees are errors, never absence.
        if record.locked || record.prunable {
            return Err(OperationError::Refused(
                "locked/prunable worktree requires owner recovery",
            ));
        }
        if observed == target || observed.canonicalize().ok().as_ref() == Some(&target) {
            at_path.push(record);
        }
        if record.branch.as_deref() == Some(branch)
            && observed.canonicalize().ok().as_ref() != Some(&target)
        {
            return Err(OperationError::Refused("Plan branch checked out elsewhere"));
        }
    }
    if at_path.is_empty() {
        if target.try_exists()? {
            return Err(OperationError::Refused(
                "unregistered/replacement checkout path",
            ));
        }
        return Ok(CheckoutState::Absent);
    }
    if at_path.len() != 1 {
        return Err(OperationError::Refused("checkout inventory is not unique"));
    }
    let entry = at_path[0];
    if entry.bare || entry.detached || entry.branch.as_deref() != Some(branch) {
        return Err(OperationError::Refused(
            "registered checkout has foreign branch/state",
        ));
    }
    let mut source = git::request(&target, None)?;
    git::index_policy(&mut source)?;
    let mut identity_args = vec![
        "rev-parse",
        "--path-format=absolute",
        "--show-toplevel",
        "--absolute-git-dir",
        "--git-common-dir",
        "--git-path",
        "index",
        "--git-path",
        "gitdir",
        "HEAD^{commit}",
        "HEAD^{tree}",
    ];
    for state in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-apply",
        "rebase-merge",
        "sequencer",
        "index.lock",
    ] {
        identity_args.extend(["--git-path", state]);
    }
    let paths = source.run(&identity_args, b"")?;
    let fields = std::str::from_utf8(&paths)
        .map_err(|_| OperationError::Journal)?
        .lines()
        .collect::<Vec<_>>();
    if fields.len() != 14 {
        return Err(OperationError::Evidence("checkout identity response"));
    }
    let canonical = |value: &str| -> OperationResult<PathBuf> {
        let p = Path::new(value);
        if !p.is_absolute() {
            return Err(OperationError::Journal);
        }
        Ok(p.canonicalize()?)
    };
    let admin = canonical(fields[1])?;
    for file in [fields[3], fields[4]] {
        let meta = fs::symlink_metadata(file)?;
        if !meta.file_type().is_file() {
            return Err(OperationError::Refused(
                "checkout admin file is not regular",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(OperationError::Refused("checkout admin file reparse point"));
            }
        }
    }
    let index = canonical(fields[3])?;
    let backlink = canonical(fields[4])?;
    if canonical(fields[0])? != target
        || canonical(fields[2])? != common
        || admin == common
        || !admin.starts_with(common.join("worktrees"))
        || index.parent() != Some(admin.as_path())
        || backlink.parent() != Some(admin.as_path())
        || source.text(&["symbolic-ref", "-q", "HEAD"])? != format!("refs/heads/{branch}")
        || entry.head != fields[5]
    {
        return Err(OperationError::Refused(
            "checkout Git admin/branch identity differs",
        ));
    }
    for state in &fields[7..] {
        if !Path::new(state).is_absolute() || Path::new(state).try_exists()? {
            return Err(OperationError::Refused(
                "checkout integration/index lock in progress",
            ));
        }
    }
    if !source
        .text(&["rev-parse", "--shared-index-path"])?
        .is_empty()
        || !source
            .run(
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                    "--ignored=matching",
                    "--ignore-submodules=none",
                ],
                b"",
            )?
            .is_empty()
    {
        return Err(OperationError::Refused(
            "checkout dirty/ignored/submodule/index state is not lossless",
        ));
    }
    // One native index enumeration proves flags/stages/modes and supplies
    // exact NUL-delimited paths for the batched attribute check. No per-file
    // children or separate identical index listings are needed.
    let entries = source.run(&["ls-files", "--stage", "-v", "-z"], b"")?;
    let attrs_input = index_paths(&entries)?;
    let attributes = source.run(
        &[
            "check-attr",
            "-z",
            "--stdin",
            "filter",
            "ident",
            "working-tree-encoding",
        ],
        &attrs_input,
    )?;
    let parts = attributes.split(|b| *b == 0).collect::<Vec<_>>();
    if !attributes.is_empty()
        && (parts.last() != Some(&b"".as_slice())
            || (parts.len() - 1) % 3 != 0
            || parts[..parts.len() - 1]
                .chunks_exact(3)
                .any(|p| !matches!(p[2], b"unspecified" | b"unset")))
    {
        return Err(OperationError::Refused(
            "unsupported checkout storage transformation",
        ));
    }
    let head = CommitId::new(fields[5])?;
    let diff = source.run_raw(
        &["diff-index", "--cached", "--quiet", head.as_str(), "--"],
        b"",
    )?;
    if !diff.status.success() {
        return Err(OperationError::Refused("checkout index differs from HEAD"));
    }
    let gitfile = regular_path(&target, &RepoPath::new(".git")?)?;
    let proof = CheckoutProof {
        admin_stamp: stamp(&admin)?,
        admin,
        gitfile_sha256: digest(&read_bounded(&gitfile)?),
        backlink_sha256: digest(&read_bounded(&backlink)?),
        index_sha256: digest(&read_bounded(&index)?),
        branch: branch.into(),
        head,
        tree: CommitId::new(fields[6])?,
    };
    Ok(CheckoutState::Present { proof })
}

fn index_paths(entries: &[u8]) -> OperationResult<Vec<u8>> {
    if !entries.is_empty() && entries.last() != Some(&0) {
        return Err(OperationError::Evidence("checkout index response framing"));
    }
    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in entries.split(|b| *b == 0).filter(|v| !v.is_empty()) {
        let tab = entry
            .iter()
            .position(|b| *b == b'\t')
            .ok_or(OperationError::Evidence("checkout index metadata"))?;
        let metadata = std::str::from_utf8(&entry[..tab])
            .map_err(|_| OperationError::Evidence("checkout index metadata encoding"))?;
        let fields = metadata.split(' ').collect::<Vec<_>>();
        if fields.len() != 4
            || fields[0] != "H"
            || !matches!(fields[1], "100644" | "100755" | "120000")
            || fields[3] != "0"
        {
            return Err(OperationError::Refused(
                "checkout submodule/sparse/skip/assume/unmerged index unsupported",
            ));
        }
        CommitId::new(fields[2])?;
        let path = &entry[tab + 1..];
        if path.is_empty() || !seen.insert(path) {
            return Err(OperationError::Evidence("checkout index path uniqueness"));
        }
        paths.extend_from_slice(path);
        paths.push(0);
    }
    Ok(paths)
}

pub(crate) struct WithdrawInput {
    pub id: String,
    pub reason: String,
    pub at: UtcTimestamp,
}

pub(crate) enum PlanGitRequest {
    Withdrawal(WithdrawInput),
    Preparation(Box<super::preparation_intent::PrepareInput>),
}

// These are pre-intent observations, not failed/recoverable Git effects. Keep
// the public lossless-cleanup refusal distinct from malformed registration.
fn withdrawal_git_error(node: &PlanNode, error: OperationError) -> OperationError {
    match error {
        OperationError::Refused(detail) | OperationError::Evidence(detail)
            if node.status == PlanStatus::Ready =>
        {
            crate::domain::error::MineError::GitEvidence {
                plan_id: node.id.clone(),
                detail: detail.into(),
            }
            .into()
        }
        other => other,
    }
}

pub(crate) fn withdrawal(
    root: &Path,
    common: &Path,
    config: &Sha256,
    repository: &str,
    workspace: &PlanWorkspace,
    request: WithdrawInput,
    head: &CommitId,
) -> OperationResult<PlanGitEffects> {
    let id = request.id.as_str();
    let reason = request.reason.as_str();
    let at = request.at;
    let node = workspace
        .get(id)
        .ok_or(OperationError::Evidence("withdrawal Plan missing"))?;
    if !matches!(
        node.status,
        PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready
    ) {
        return Err(crate::domain::error::MineError::InvalidTransition {
            plan_id: id.into(),
            from: node.status.as_str().into(),
            to: "WITHDRAWN".into(),
        }
        .into());
    }
    if reason.trim().is_empty() {
        return Err(OperationError::Refused(
            "withdrawal requires a non-empty reason",
        ));
    }
    let plan_path = RepoPath::new(node.path.clone())?;
    let body = read_bounded(&regular_path(root, &plan_path)?)?;
    let body_sha256 = digest(&body);
    if node
        .body_sha256
        .as_ref()
        .is_some_and(|known| known != &body_sha256)
    {
        return Err(OperationError::Refused(
            "immutable withdrawal Plan body changed",
        ));
    }
    let plan = PlanIdentity::new(PlanIdentityFields {
        repository_id: repository.into(),
        workspace_id: workspace.workspace_id.clone(),
        plan_id: node.id.clone(),
        plan_path,
        body_sha256: Some(body_sha256),
        lineage_id: node.lineage_id.clone(),
        branch: node.branch.clone(),
    })?;
    let worktree = RepoPath::new(node.worktree_path.clone())?;
    let mut git = git::request(root, None)?;
    committed_body(&mut git, &plan, head, &body)?;
    let before_ref =
        reference(&mut git, &node.branch).map_err(|e| withdrawal_git_error(node, e))?;
    let before_checkout = checkout(root, common, &worktree, &node.branch, &mut git)
        .map_err(|e| withdrawal_git_error(node, e))?;
    let parsed = crate::domain::plan_id::CanonicalPlanId::parse(id)?;
    let preserved = if parsed.is_compensation() {
        let predecessor = crate::domain::validation::declaring_rejected_predecessor(workspace, id)?;
        let RefState::Present { tip } =
            reference(&mut git, &predecessor.branch).map_err(|e| withdrawal_git_error(node, e))?
        else {
            return Err(withdrawal_git_error(
                node,
                OperationError::Evidence("preserved rejected ref missing"),
            ));
        };
        PreservedRef::Rejected {
            plan_id: predecessor.id.clone(),
            branch: predecessor.branch.clone(),
            tip,
        }
    } else {
        PreservedRef::None
    };
    if let RefState::Present { tip } = &before_ref {
        let safe = match &preserved {
            PreservedRef::None => git.is_ancestor(tip.as_str(), head.as_str())?,
            PreservedRef::Rejected { tip: old, .. } => old == tip,
        };
        if node.status != PlanStatus::Ready {
            return Err(OperationError::Refused(
                "unstarted registration has inconsistent branch",
            ));
        }
        if !safe {
            return Err(withdrawal_git_error(
                node,
                OperationError::Refused("withdrawal branch contains unique work"),
            ));
        }
    } else if !matches!(before_checkout, CheckoutState::Absent) {
        return Err(withdrawal_git_error(
            node,
            OperationError::Refused("checkout exists without its exact Plan ref"),
        ));
    }
    if matches!(node.status, PlanStatus::Draft | PlanStatus::Blocked)
        && !matches!(before_checkout, CheckoutState::Absent)
    {
        return Err(OperationError::Refused(
            "unstarted registration has inconsistent checkout",
        ));
    }
    let removal = if matches!(before_checkout, CheckoutState::Absent) {
        EffectProgress::Complete
    } else {
        EffectProgress::NotAttempted
    };
    let deletion = if matches!(before_ref, RefState::Absent) {
        EffectProgress::Complete
    } else {
        EffectProgress::NotAttempted
    };
    let intent = WithdrawIntent {
        plan,
        worktree,
        original_generation: node.preparations.last().cloned().map_or(
            GenerationBefore::Absent,
            |generation| GenerationBefore::Present {
                generation: Box::new(generation),
            },
        ),
        policy_sha256: withdrawal_policy(config, node)?,
        before_ref,
        before_checkout,
        preserved,
        reason: reason.into(),
        withdrawn_at: at,
    };
    Ok(PlanGitEffects::Withdrawal {
        intent: Box::new(intent),
        removal,
        deletion,
    })
}

pub(crate) fn committed_body(
    git: &mut GitRequest,
    plan: &PlanIdentity,
    head: &CommitId,
    body: &[u8],
) -> OperationResult<()> {
    let path = plan.fields().plan_path.as_str();
    let attrs = git.run(
        &[
            "check-attr",
            "-z",
            "filter",
            "ident",
            "working-tree-encoding",
            "--",
            path,
        ],
        b"",
    )?;
    let parts = attrs.split(|b| *b == 0).collect::<Vec<_>>();
    if parts.len() != 10
        || parts[9] != b""
        || parts[..9]
            .chunks_exact(3)
            .any(|p| p[0] != path.as_bytes() || !matches!(p[2], b"unset" | b"unspecified"))
    {
        return Err(OperationError::Refused("unsupported Plan storage filter"));
    }
    let hash = git.run(&["hash-object", &format!("--path={path}"), "--stdin"], body)?;
    let hash = std::str::from_utf8(&hash)
        .map_err(|_| OperationError::Journal)?
        .trim();
    let tree = git.run(&["ls-tree", "-z", head.as_str(), "--", path], b"")?;
    if !["100644", "100755"]
        .iter()
        .any(|mode| tree == format!("{mode} blob {hash}\t{path}\0").as_bytes())
    {
        return Err(OperationError::Refused(
            "Plan body is not committed at frozen Root",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RefState, index_paths, reference};
    use crate::domain::graph::CommitId;
    use std::{fs, path::Path, process::Command};

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
    fn fresh_reference_fast_path_preserves_absent_symbolic_broken_and_exact_name_refusals() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "--quiet", "-b", "dev"]);
        git(root, &["config", "user.name", "Ref Fixture"]);
        git(root, &["config", "user.email", "fixture@example.invalid"]);
        git(root, &["config", "commit.gpgSign", "false"]);
        git(
            root,
            &["commit", "--allow-empty", "--quiet", "-m", "ref fixture"],
        );
        let head = CommitId::new(git(root, &["rev-parse", "HEAD"])).unwrap();
        let mut request = super::super::git::request(root, None).unwrap();
        assert_eq!(
            reference(&mut request, "dev").unwrap(),
            RefState::Present { tip: head.clone() }
        );
        assert_eq!(request.diagnostics().invocations, 3); // inventory, width, object batch.
        let before = request.diagnostics().invocations;
        assert_eq!(
            reference(&mut request, "dev").unwrap(),
            RefState::Present { tip: head.clone() }
        );
        assert_eq!(request.diagnostics().invocations - before, 1); // Fresh inventory, not a ref cache.
        let before = request.diagnostics().invocations;
        assert_eq!(reference(&mut request, "absent").unwrap(), RefState::Absent);
        assert_eq!(request.diagnostics().invocations - before, 2); // Dangling-ref fallback required.

        git(root, &["pack-refs", "--all"]);
        assert_eq!(
            reference(&mut request, "dev").unwrap(),
            RefState::Present { tip: head.clone() }
        );
        for (name, target) in [
            ("alias", "refs/heads/dev"),
            ("dangling", "refs/heads/missing"),
        ] {
            let name = format!("refs/heads/{name}");
            git(root, &["symbolic-ref", &name, target]);
            let path = root.join(".git").join(&name);
            let original = fs::read(&path).unwrap();
            assert!(reference(&mut request, name.strip_prefix("refs/heads/").unwrap()).is_err());
            assert_eq!(fs::read(path).unwrap(), original);
        }
        git(root, &["branch", "prefix/child"]);
        assert!(reference(&mut request, "prefix").is_err());
        fs::write(root.join("blob"), b"not a commit\n").unwrap();
        let blob = git(root, &["hash-object", "-w", "--", "blob"]);
        let noncommit = root.join(".git/refs/heads/noncommit");
        fs::write(&noncommit, format!("{blob}\n")).unwrap();
        assert!(reference(&mut request, "noncommit").is_err());
        assert_eq!(fs::read_to_string(noncommit).unwrap(), format!("{blob}\n"));
        let broken = root.join(".git/refs/heads/broken");
        fs::write(&broken, b"not-an-object-id\n").unwrap();
        assert!(reference(&mut request, "broken").is_err());
        assert_eq!(fs::read(broken).unwrap(), b"not-an-object-id\n");
        assert_eq!(git(root, &["rev-parse", "HEAD"]), head.as_str());
    }

    #[test]
    fn one_index_listing_preserves_literal_paths_and_refuses_unsafe_index_states() {
        let oid = "a".repeat(40);
        let input = format!("H 100644 {oid} 0\tspace\tand\nnewline\0H 100755 {oid} 0\tnext\0");
        assert_eq!(
            index_paths(input.as_bytes()).unwrap(),
            b"space\tand\nnewline\0next\0"
        );
        assert!(index_paths(b"").unwrap().is_empty());
        for (tag, mode, stage) in [
            ("h", "100644", "0"),
            ("S", "100644", "0"),
            ("M", "100644", "1"),
            ("H", "160000", "0"),
            ("H", "040000", "0"),
            ("H", "100644", "2"),
        ] {
            assert!(index_paths(format!("{tag} {mode} {oid} {stage}\tfile\0").as_bytes()).is_err());
        }
        assert!(index_paths(format!("H 100644 {oid} 0\tfile").as_bytes()).is_err());
        assert!(
            index_paths(format!("H 100644 {oid} 0\tfile\0H 100644 {oid} 0\tfile\0").as_bytes())
                .is_err()
        );
    }
}
