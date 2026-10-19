//! Candidate inputs, immutable source attribution and retained receipt IO are
//! separate roots. Only a queried, validated native Release can construct this.
use super::*;
use crate::application::verification_service::VerificationBackend;
use crate::domain::verification_receipt::*;
use crate::infrastructure::{
    git_evidence::GitRequest, operation_store::RepositoryJournal,
    verification_store::FileVerificationStore,
};
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) struct ReleaseVerification {
    candidate: FileVerificationStore,
    evidence: FileVerificationStore,
    source: PathBuf,
    plan: PlanIdentity,
    body: FileIdentity,
    policy: CandidatePolicy,
    owner: String,
    owners: std::sync::Arc<BTreeMap<String, String>>,
}

impl ReleaseVerification {
    pub(crate) fn open(
        root: &Path,
        id: &OperationId,
        plan_id: &str,
    ) -> OperationResult<(PlanIdentity, Self)> {
        let config = crate::application::plan_service::root_configuration(root)?;
        let configured = TomlStore::from_configuration(root, &config)?;
        configured
            .with_coordinator_lock(|guard| {
                let result = (|| -> OperationResult<_> {
                    let journal =
                        RepositoryJournal::under_lock(root, &config.repository_id, guard)?;
                    let observed = journal.observations()?;
                    let record = journal.query(id)?;
                    crate::infrastructure::operation_store::history::predecessors(
                        &journal,
                        &observed,
                        Some(&record.operation.fields().workspace_id),
                    )?;
                    if observed.iter().any(|entry| {
                        entry.operation.fields().operation_id != *id && !terminal(&entry.operation)
                    }) {
                        return Err(OperationError::Refused(
                            "foreign pending operation blocks Release verification",
                        ));
                    }
                    Self::from_record(root, &record, plan_id)
                })();
                result.map_err(|error| native_evidence::refused(error.to_string()))
            })
            .map_err(OperationError::from)
    }

    pub(crate) fn from_record(
        root: &Path,
        record: &crate::infrastructure::operation_store::JournalRecord,
        plan_id: &str,
    ) -> OperationResult<(PlanIdentity, Self)> {
        record.validate()?;
        if record.operation.fields().kind != OperationKind::Release || terminal(&record.operation) {
            return Err(OperationError::Refused(
                "new verification requires an active native Release",
            ));
        }
        let manifest = validate_manifest(
            root,
            &record.operation,
            record.closure.as_ref().ok_or(OperationError::Journal)?,
        )?;
        Self::from_manifest(root, &record.operation, &manifest, plan_id)
    }
    pub(super) fn from_manifest(
        root: &Path,
        operation: &LifecycleOperation,
        manifest: &ValidatedManifest,
        plan_id: &str,
    ) -> OperationResult<(PlanIdentity, Self)> {
        let plan = manifest
            .selected_plans
            .iter()
            .find(|plan| plan.fields().plan_id == plan_id)
            .ok_or(OperationError::Refused(
                "Plan is not explicitly selected in this Release",
            ))?
            .clone();
        let source =
            regular_path(root, &bundle(&manifest.operation_id, "source")?)?.canonicalize()?;
        let evidence =
            regular_path(root, &bundle(&manifest.operation_id, "evidence")?)?.canonicalize()?;
        let body = manifest
            .files
            .iter()
            .find(|file| file.original.path == plan.fields().plan_path)
            .ok_or(OperationError::Journal)?
            .original
            .clone();
        let source_config = crate::domain::config::MineConfig::parse(
            Path::new(".mine/config.toml"),
            std::str::from_utf8(&native_evidence::bytes(
                &source,
                &RepoPath::new(".mine/config.toml")?,
            )?)
            .map_err(|_| OperationError::Journal)?,
        )?;
        let workspace: PlanWorkspace = toml::from_str(
            std::str::from_utf8(&native_evidence::bytes(
                &source,
                &RepoPath::new(source_config.graph.source.clone())?,
            )?)
            .map_err(|_| OperationError::Journal)?,
        )
        .map_err(|_| OperationError::Journal)?;
        let owner = workspace
            .get(plan_id)
            .ok_or(OperationError::Journal)?
            .owner
            .clone();
        let after = matches!(
            operation.fields().stage,
            OperationStage::Release(ReleaseStage::Integrated | ReleaseStage::Complete)
        );
        let integrated = if after {
            Some(
                operation
                    .fields()
                    .integrated_commit
                    .as_ref()
                    .ok_or(OperationError::Refused("integrated Release result missing"))?,
            )
        } else {
            None
        };
        let candidate_root = candidate_checkout(
            root,
            manifest,
            &source_config,
            integrated,
            &manifest.checked,
        )?;
        // candidate_checkout has just reproved C's config blob and physical
        // bytes. C's input namespace and S's retained receipt namespace are
        // separate authorities, even after Root has integrated the new layout.
        let candidate_config = crate::domain::config::MineConfig::parse(
            Path::new(".mine/config.toml"),
            std::str::from_utf8(&native_evidence::bytes(
                &candidate_root,
                &RepoPath::new(".mine/config.toml")?,
            )?)
            .map_err(|_| OperationError::Journal)?,
        )?;
        let candidate = FileVerificationStore::for_bound_layout(
            &candidate_root,
            root,
            &candidate_config.layout()?,
        )
        .map_err(|_| OperationError::Evidence("candidate source unavailable"))?;
        let evidence =
            FileVerificationStore::for_bound_layout(&evidence, root, &source_config.layout()?)
                .map_err(|_| OperationError::Evidence("release evidence unavailable"))?;
        let backend = Self {
            candidate,
            evidence,
            source,
            plan: plan.clone(),
            body,
            policy: manifest.policy.clone(),
            owner,
            owners: std::sync::Arc::new(
                workspace
                    .plans
                    .into_iter()
                    .map(|node| (node.id, node.owner))
                    .collect(),
            ),
        };
        backend.load_policy(&backend.policy.path).map_err(|_| {
            OperationError::Evidence("candidate policy differs from frozen Release")
        })?;
        Ok((plan, backend))
    }
    pub(super) fn for_plan(
        &self,
        manifest: &ReleaseManifest,
        plan: &PlanIdentity,
    ) -> OperationResult<Self> {
        if !manifest.selected_plans.contains(plan) {
            return Err(OperationError::Refused("foreign Release attribution"));
        }
        let body = manifest
            .files
            .iter()
            .find(|file| file.original.path == plan.fields().plan_path)
            .ok_or(OperationError::Journal)?
            .original
            .clone();
        let owner = self
            .owners
            .get(&plan.fields().plan_id)
            .ok_or(OperationError::Journal)?
            .clone();
        Ok(Self {
            plan: plan.clone(),
            body,
            owner,
            ..self.clone()
        })
    }
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }
    pub(crate) fn evidence_root(&self) -> &Path {
        self.evidence.root()
    }
    pub(crate) fn load_policy(&self, path: &RepoPath) -> VerificationResult<VerificationPolicy> {
        if path != &self.policy.path {
            return Err(VerificationError::Refused(
                "policy selector differs from frozen Release",
            ));
        }
        let policy = self.candidate.load_policy(path)?;
        if policy.identity() != self.policy.identity {
            return Err(VerificationError::Refused("candidate policy changed"));
        }
        Ok(policy)
    }
    fn subject(&self, plan: &PlanIdentity) -> VerificationResult<()> {
        if plan != &self.plan {
            return Err(VerificationError::Refused(
                "foreign Release Plan attribution",
            ));
        }
        Ok(())
    }
}

impl VerificationBackend for ReleaseVerification {
    fn manifest(
        &self,
        policy: &VerificationPolicy,
        check: &str,
    ) -> VerificationResult<InputManifest> {
        if policy.identity() != self.policy.identity {
            return Err(VerificationError::Refused("foreign Release policy"));
        }
        self.candidate.manifest(policy, check)
    }
    fn file(&self, path: &RepoPath) -> VerificationResult<FileIdentity> {
        self.candidate.file(path)
    }
    fn plan_body(&self, plan: &PlanIdentity) -> VerificationResult<FileIdentity> {
        self.subject(plan)?;
        let current = native_evidence::bytes(&self.source, &self.body.path)
            .map_err(|_| VerificationError::Refused("retained Plan body unavailable"))?;
        if identity(self.body.path.clone(), &current) != self.body {
            return Err(VerificationError::Refused("retained Plan body changed"));
        }
        Ok(self.body.clone())
    }
    fn artifacts(
        &self,
        receipt: &VerificationReceipt,
        policy: &CheckPolicy,
    ) -> VerificationResult<()> {
        self.subject(&receipt.fields().plan)?;
        self.evidence.artifacts(receipt, policy)
    }
    fn load(
        &self,
        plan: &PlanIdentity,
        id: &ReceiptId,
    ) -> VerificationResult<Option<StoredVerification>> {
        self.subject(plan)?;
        self.evidence.load(plan, id)
    }
    fn save(&self, stored: &StoredVerification) -> VerificationResult<()> {
        self.subject(&stored.receipt.fields().plan)?;
        self.evidence.save(stored)
    }
}

pub(super) fn candidate_checkout(
    root: &Path,
    manifest: &ReleaseManifest,
    config: &crate::domain::config::MineConfig,
    integrated: Option<&CommitId>,
    checked: &curation::CheckedCuration,
) -> OperationResult<PathBuf> {
    if checked.evidence() != &manifest.curation {
        return Err(OperationError::Refused(
            "candidate checkout has no matching checked curation",
        ));
    }
    let candidate = &manifest.curation.candidate_commit;
    let mut git = checkpoint_git(root, None)?;
    let identity = git.text(&[
        "rev-parse",
        "--path-format=absolute",
        "--show-toplevel",
        "--absolute-git-dir",
        "--git-common-dir",
    ])?;
    let identity = identity.lines().collect::<Vec<_>>();
    if identity.len() != 3
        || Path::new(identity[0]).canonicalize()? != root.canonicalize()?
        || Path::new(identity[1]).canonicalize()? != Path::new(identity[2]).canonicalize()?
    {
        return Err(OperationError::Refused(
            "candidate verification requires actual Root authority",
        ));
    }
    let common = PathBuf::from(identity[2]).canonicalize()?;
    let target = if integrated.is_some() {
        root.canonicalize()?
    } else {
        let target = regular_path(root, &manifest.candidate_root)?.canonicalize()?;
        let overlap = |left: &str, right: &str| {
            crate::domain::path::is_within(left, right)
                || crate::domain::path::is_within(right, left)
        };
        if target == root.canonicalize()?
            || [
                ".mine/build",
                ".mine/runtime/release-evidence",
                ".mine/runtime/operations",
                ".mine/locks",
                &config.plan.worktrees_root,
                &config.plan.root,
            ]
            .iter()
            .any(|reserved| overlap(reserved, manifest.candidate_root.as_str()))
        {
            return Err(OperationError::Refused(
                "candidate locator overlaps Root, Plan ownership, evidence or outputs",
            ));
        }
        let inventory = git.worktrees()?;
        let matches = inventory
            .iter()
            .filter(|worktree| {
                Path::new(&worktree.path).canonicalize().ok().as_ref() == Some(&target)
            })
            .collect::<Vec<_>>();
        if matches.len() != 1
            || matches[0].head != candidate.as_str()
            || matches[0].bare
            || matches[0].locked
            || matches[0].prunable
            || matches[0]
                .branch
                .as_ref()
                .is_some_and(|branch| branch.starts_with("plan/"))
        {
            return Err(OperationError::Refused(
                "explicit candidate locator is not a dedicated registered C checkout",
            ));
        }
        target
    };
    let mut source = checkpoint_git(&target, None)?;
    let expected_head = integrated.unwrap_or(candidate);
    let actual = source.text(&[
        "rev-parse",
        "--path-format=absolute",
        "--show-toplevel",
        "--absolute-git-dir",
        "--git-common-dir",
    ])?;
    let actual = actual.lines().collect::<Vec<_>>();
    if actual.len() != 3
        || Path::new(actual[0]).canonicalize()? != target
        || Path::new(actual[2]).canonicalize()? != common
        || (integrated.is_none() && Path::new(actual[1]).canonicalize()? == common)
        || source.text(&["rev-parse", "--verify", "HEAD^{commit}"])? != expected_head.as_str()
        || source.text(&["rev-parse", "--verify", "HEAD^{tree}"])?
            != manifest.curation.candidate_tree.as_str()
    {
        return Err(OperationError::Refused(
            "candidate checkout/repository/tree differs",
        ));
    }
    if integrated.is_some() {
        if source.text(&["symbolic-ref", "-q", "HEAD"])?
            != format!("refs/heads/{}", config.branches.stable)
        {
            return Err(OperationError::Refused(
                "integrated candidate requires actual stable Root",
            ));
        }
        source.validate_commits(&[expected_head.as_str().into()])?;
        if !source.is_ancestor(manifest.baseline.as_str(), expected_head.as_str())? {
            return Err(OperationError::Refused(
                "integrated result does not descend from stable baseline",
            ));
        }
    }
    prove_checkout_bytes(&target, &mut source, expected_head)?;
    checked.verify_checkout(&target)?;
    Ok(target)
}

/// Hash actual tracked bytes, not status/stat-cache guesses. All transformations
/// except Git's built-in text/EOL conversion refuse before hash-object can run.
pub(super) fn clean_index_state(git: &mut GitRequest) -> OperationResult<()> {
    crate::infrastructure::graph_checkpoint::observe_index_policy(git)?;
    let state = git.text(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-path",
        "index.lock",
        "--git-path",
        "MERGE_HEAD",
        "--git-path",
        "CHERRY_PICK_HEAD",
        "--git-path",
        "REVERT_HEAD",
        "--git-path",
        "rebase-merge",
        "--git-path",
        "rebase-apply",
    ])?;
    let states = state.lines().collect::<Vec<_>>();
    if states.len() != 6 {
        return Err(OperationError::Evidence(
            "candidate integration-state framing",
        ));
    }
    for state in states {
        if !Path::new(state).is_absolute() {
            return Err(OperationError::Journal);
        }
        match fs::symlink_metadata(state) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => {
                return Err(OperationError::Refused(
                    "candidate has a foreign index/integration operation",
                ));
            }
        }
    }
    if !git.text(&["rev-parse", "--shared-index-path"])?.is_empty() {
        return Err(OperationError::Refused("candidate split index unsupported"));
    }
    Ok(())
}

pub(super) fn untransformed<'a>(
    git: &mut GitRequest,
    paths: impl Iterator<Item = &'a RepoPath> + Clone,
) -> OperationResult<()> {
    let mut input = Vec::new();
    for path in paths.clone() {
        input.extend_from_slice(path.as_str().as_bytes());
        input.push(0);
    }
    let attrs = git.run(
        &[
            "check-attr",
            "-z",
            "--stdin",
            "filter",
            "ident",
            "working-tree-encoding",
        ],
        &input,
    )?;
    let mut fields = attrs.split(|byte| *byte == 0);
    for path in paths {
        for attribute in ["filter", "ident", "working-tree-encoding"] {
            if fields.next() != Some(path.as_str().as_bytes())
                || fields.next() != Some(attribute.as_bytes())
                || !fields
                    .next()
                    .is_some_and(|value| matches!(value, b"unset" | b"unspecified"))
            {
                return Err(OperationError::Refused(
                    "checkout storage transformation unsupported",
                ));
            }
        }
    }
    if fields.next() != Some(b"".as_slice()) || fields.next().is_some() {
        return Err(OperationError::Evidence("checkout attributes framing"));
    }
    Ok(())
}

pub(crate) fn prove_checkout_bytes(
    root: &Path,
    git: &mut GitRequest,
    head: &CommitId,
) -> OperationResult<()> {
    clean_index_state(git)?;
    let entries = git.run(&["ls-files", "--stage", "-v", "-z"], b"")?;
    if !entries.is_empty() && entries.last() != Some(&0) {
        return Err(OperationError::Evidence("candidate index framing"));
    }
    let mut files = Vec::new();
    let mut paths = BTreeSet::new();
    let mut physical = BTreeSet::new();
    for entry in entries
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let tab = entry
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or(OperationError::Journal)?;
        let fields = std::str::from_utf8(&entry[..tab])
            .map_err(|_| OperationError::Journal)?
            .split(' ')
            .collect::<Vec<_>>();
        if fields.len() != 4
            || fields[0] != "H"
            || !matches!(fields[1], "100644" | "100755")
            || fields[3] != "0"
        {
            return Err(OperationError::Refused(
                "candidate hidden/unmerged/sparse/special index unsupported",
            ));
        }
        let path = RepoPath::new(
            std::str::from_utf8(&entry[tab + 1..]).map_err(|_| OperationError::Journal)?,
        )?;
        let object = CommitId::new(fields[2])?;
        if object.as_str().len() != head.as_str().len()
            || !paths.insert(path.clone())
            || paths.len() > 100_000
        {
            return Err(OperationError::Evidence(
                "candidate duplicate/oversized/wrong-width index",
            ));
        }
        let actual = regular_path(root, &path)?;
        if !physical.insert(actual.clone()) || !fs::symlink_metadata(&actual)?.is_file() {
            return Err(OperationError::Refused("candidate source is not regular"));
        }
        files.push((path, object));
    }
    untransformed(git, files.iter().map(|(path, _)| path))?;
    let delta = git.run_raw(
        &["diff-index", "--cached", "--quiet", head.as_str(), "--"],
        b"",
    )?;
    if !delta.status.success() {
        return Err(OperationError::Refused(
            "candidate index differs from frozen commit",
        ));
    }
    let mut start = 0;
    while start < files.len() {
        let mut end = start;
        let mut units = 0;
        while end < files.len()
            && (end == start || units + files[end].0.as_str().encode_utf16().count() + 3 < 8192)
        {
            units += files[end].0.as_str().encode_utf16().count() + 3;
            end += 1;
        }
        let mut args = vec!["hash-object", "--"];
        args.extend(files[start..end].iter().map(|(path, _)| path.as_str()));
        let hashes = git.run(&args, b"")?;
        let hashes = std::str::from_utf8(&hashes).map_err(|_| OperationError::Journal)?;
        let observed = hashes.split_terminator('\n').collect::<Vec<_>>();
        if !hashes.ends_with('\n')
            || observed.len() != end - start
            || files[start..end]
                .iter()
                .zip(observed)
                .any(|((_, object), actual)| actual != object.as_str())
        {
            return Err(OperationError::Refused(
                "candidate tracked bytes differ from frozen commit",
            ));
        }
        start = end;
    }
    if !git
        .run(
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=no",
                "--ignore-submodules=none",
            ],
            b"",
        )?
        .is_empty()
    {
        return Err(OperationError::Refused(
            "candidate tracked checkout is dirty",
        ));
    }
    Ok(())
}
