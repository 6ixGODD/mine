//! Frozen source export for native Release Intent. This module grants neither
//! verification success nor post-integration/Complete/publication authority.
use super::curation::{self, CurationObservation};
use crate::application::{native_evidence, operation_service::*};
use crate::domain::{
    graph::*,
    lifecycle_operation::*,
    release_evidence::{CandidatePolicy, ReleaseManifest, RetainedFile},
    status::PlanStatus,
    verification_receipt::{StoredVerification, VerificationPolicy},
};
use crate::infrastructure::{
    file_lock::FileLock,
    graph_checkpoint::{checkpoint_git, digest},
    operation_store::{OperationStore, read_bounded, regular_path},
    toml_store::TomlStore,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
};

const MAX_FILES: usize = 8192;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

pub(super) mod diagnostic;
mod progress;
pub(super) mod verification;
pub(crate) use diagnostic::diagnose;
pub(crate) use progress::{finalize, stage};
pub(crate) use verification::ReleaseVerification;

fn bundle(id: &OperationId, leaf: &str) -> OperationResult<RepoPath> {
    Ok(RepoPath::new(format!(
        ".mine/runtime/release-evidence/{}/{leaf}",
        id.as_str()
    ))?)
}
fn identity(path: RepoPath, body: &[u8]) -> FileIdentity {
    FileIdentity {
        path,
        bytes: body.len() as u64,
        sha256: digest(body),
    }
}
fn copy_path(id: &OperationId, role: &str, original: &RepoPath) -> OperationResult<RepoPath> {
    bundle(id, &format!("{role}/{}", original.as_str()))
}
fn matches_file(root: &Path, file: &FileIdentity) -> OperationResult<Vec<u8>> {
    let body = read_bounded(&regular_path(root, &file.path)?)?;
    if identity(file.path.clone(), &body) != *file {
        return Err(OperationError::Refused("frozen release evidence changed"));
    }
    Ok(body)
}
fn publish_new(root: &Path, path: &RepoPath, body: &[u8]) -> OperationResult<()> {
    let target = regular_path(root, path)?;
    let parent = target.parent().ok_or(OperationError::Journal)?;
    fs::create_dir_all(parent)?;
    regular_path(root, path)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(body)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(&target)
        .map_err(|error| error.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn policy(observed: &CurationObservation, path: &RepoPath) -> OperationResult<CandidatePolicy> {
    let body = observed
        .candidate_blobs(std::slice::from_ref(path))?
        .remove(0);
    let policy: VerificationPolicy = native_evidence::strict_json(&body)?;
    observed.candidate_blobs(&policy.fields().design_references)?;
    Ok(CandidatePolicy {
        path: path.clone(),
        entry: observed
            .candidate_entry(path)
            .ok_or(OperationError::Journal)?
            .clone(),
        identity: policy.identity(),
    })
}

fn source_paths(
    root: &Path,
    authority: &curation::SourceContract,
    checked_authorizations: &[FileIdentity],
) -> OperationResult<BTreeSet<RepoPath>> {
    let config = &authority.config;
    let mut paths = authority
        .authorizations
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    paths.extend(checked_authorizations.iter().map(|file| file.path.clone()));
    for path in [
        &config.design.root,
        &config.adr.registry,
        &config.adr.rendered_index,
    ] {
        paths.insert(RepoPath::new(path.clone())?);
    }
    let registry: crate::domain::adr::AdrRegistry = toml::from_str(
        std::str::from_utf8(&native_evidence::bytes(
            root,
            &RepoPath::new(config.adr.registry.clone())?,
        )?)
        .map_err(|_| OperationError::Journal)?,
    )
    .map_err(|_| OperationError::Journal)?;
    registry.validate_structure()?;
    for entry in &registry.entries {
        if matches!(
            entry.status,
            crate::domain::adr::AdrStatus::PROPOSED | crate::domain::adr::AdrStatus::APPROVED
        ) {
            let path = RepoPath::new(entry.path.clone())?;
            native_evidence::bytes(root, &path)?;
            entry.verify_digest(root)?;
            paths.insert(path);
        }
    }
    // Only the source's genuine accepted native operations may name extra
    // retained receipts/claims/logs, and only in their exact evidence namespace.
    for node in &authority.workspace.plans {
        if let Some(acceptance) = &node.acceptance {
            let facts = acceptance.fields();
            let review = facts.review.as_ref().ok_or(OperationError::Journal)?;
            let assertion: crate::application::native_lifecycle::ClosureAssertion =
                native_evidence::one_fenced_json(
                    &matches_file(root, &review.report)?,
                    "mine-review-closure",
                )?;
            crate::application::native_lifecycle::validate_closure_assertion(
                &assertion, node, acceptance,
            )?;
            let plan = facts.plan.as_ref().ok_or(OperationError::Journal)?;
            let body = native_evidence::bytes(root, &plan.fields().plan_path)?;
            if plan.fields().body_sha256.as_ref() != Some(&digest(&body))
                || facts.verification_evidence.is_empty()
            {
                return Err(OperationError::Refused(
                    "native accepted source body/gate evidence differs",
                ));
            }
            let prefix = format!("{}/reports/verification/{}/", config.plan.root, node.id);
            for file in &acceptance.fields().verification_evidence {
                if !file.path.as_str().starts_with(&prefix) {
                    return Err(OperationError::Refused(
                        "source receipt evidence is outside its exact Plan namespace",
                    ));
                }
                let body = matches_file(root, file)?;
                paths.insert(file.path.clone());
                if file.path.as_str().ends_with(".json") {
                    let receipt: StoredVerification = native_evidence::strict_json(&body)?;
                    receipt
                        .validate()
                        .map_err(|_| OperationError::Evidence("invalid source receipt"))?;
                    if receipt.receipt.fields().plan
                        != *acceptance
                            .fields()
                            .plan
                            .as_ref()
                            .ok_or(OperationError::Journal)?
                    {
                        return Err(OperationError::Refused(
                            "source receipt attribution differs",
                        ));
                    }
                    for artifact in &receipt.receipt.fields().artifacts {
                        if !artifact.path.as_str().starts_with(&prefix) {
                            return Err(OperationError::Refused(
                                "source receipt artifact is outside its exact Plan namespace",
                            ));
                        }
                        matches_file(root, artifact)?;
                        paths.insert(artifact.path.clone());
                    }
                }
            }
        }
    }
    Ok(paths)
}

fn selected_plans(
    authority: &curation::SourceContract,
    attribution: &[String],
    files: &[RetainedFile],
) -> OperationResult<Vec<PlanIdentity>> {
    if attribution.is_empty()
        || attribution.len() > MAX_FILES
        || attribution.iter().collect::<BTreeSet<_>>().len() != attribution.len()
    {
        return Err(OperationError::Refused(
            "release requires distinct explicit real Plan attribution",
        ));
    }
    attribution
        .iter()
        .map(|id| {
            let node = authority
                .workspace
                .get(id)
                .filter(|node| node.status == PlanStatus::Accepted)
                .ok_or(OperationError::Refused(
                    "release attribution is not an accepted source Plan",
                ))?;
            let body = files
                .iter()
                .find(|file| file.original.path.as_str() == node.path)
                .ok_or(OperationError::Evidence("retained real Plan body missing"))?;
            if node
                .body_sha256
                .as_ref()
                .is_some_and(|original| original != &body.original.sha256)
            {
                return Err(OperationError::Refused("original source Plan body changed"));
            }
            Ok(PlanIdentity::new(PlanIdentityFields {
                repository_id: authority.config.repository_id.clone(),
                workspace_id: authority.workspace.workspace_id.clone(),
                plan_id: node.id.clone(),
                plan_path: body.original.path.clone(),
                body_sha256: Some(body.original.sha256.clone()),
                lineage_id: node.lineage_id.clone(),
                branch: node.branch.clone(),
            })?)
        })
        .collect()
}

/// Current pre-purge source observation only. Later phase adapters must not
/// substitute this live-source proof for validated post-purge retained authority.
fn bound_manifest(
    root: &Path,
    operation: &LifecycleOperation,
    binding: &ClosureBinding,
) -> OperationResult<ReleaseManifest> {
    let fields = operation.fields();
    if fields.kind != OperationKind::Release
        || fields.plan.is_some()
        || fields.registered_worktree.is_some()
        || fields.review.is_some()
        || fields.cleanup.is_some()
        || fields.checkpoint.is_some()
        || fields.verification_evidence.is_empty()
    {
        return Err(OperationError::Refused("release intent facts differ"));
    }
    let file = &fields.verification_evidence[0];
    if file.path != bundle(&fields.operation_id, "manifest.json")? {
        return Err(OperationError::Refused("release manifest locator differs"));
    }
    let manifest: ReleaseManifest = native_evidence::strict_json(&matches_file(root, file)?)?;
    let ClosureBinding::Release {
        baseline,
        candidate,
    } = binding
    else {
        return Err(OperationError::Journal);
    };
    if manifest.schema_version != 1
        || manifest.operation_id != fields.operation_id
        || manifest.repository_id != fields.repository_id
        || manifest.workspace_id != fields.workspace_id
        || manifest.graph_revision != fields.observed_graph_revision
        || manifest.baseline != *baseline
        || manifest.curation.candidate_commit != *candidate
        || manifest.curation.schema_version != 1
        || manifest.curation.source_commit != manifest.source_commit
    {
        return Err(OperationError::Refused(
            "release manifest operation/source binding differs",
        ));
    }
    Ok(manifest)
}

struct ValidatedManifest {
    manifest: ReleaseManifest,
    checked: curation::CheckedCuration,
}
impl std::ops::Deref for ValidatedManifest {
    type Target = ReleaseManifest;
    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

fn validate_manifest(
    root: &Path,
    operation: &LifecycleOperation,
    binding: &ClosureBinding,
) -> OperationResult<ValidatedManifest> {
    let manifest = bound_manifest(root, operation, binding)?;
    let fields = operation.fields();
    let ClosureBinding::Release {
        baseline,
        candidate,
    } = binding
    else {
        return Err(OperationError::Journal);
    };
    let observed = CurationObservation::observe(root, &manifest.source_commit, candidate)?;
    let checked = curation::release_proof(&observed)?;
    if checked.evidence() != &manifest.curation
        || policy(&observed, &manifest.policy.path)? != manifest.policy
    {
        return Err(OperationError::Refused(
            "release candidate/proof/policy changed",
        ));
    }
    let authority = curation::source_contract(&observed)?;
    let mut source_git = checkpoint_git(root, None)?;
    if source_git.text(&[
        "rev-parse",
        "--verify",
        &format!(
            "refs/heads/{}^{{commit}}",
            authority.config.branches.integration
        ),
    ])? != manifest.source_commit.as_str()
    {
        return Err(OperationError::Refused(
            "frozen development source ref advanced or disappeared before closure",
        ));
    }
    if authority.config.repository_id != fields.repository_id
        || authority.workspace.workspace_id != fields.workspace_id
        || authority.workspace.revision != fields.observed_graph_revision
        || authority.workspace.stable_baseline_commit != baseline.as_str()
    {
        return Err(OperationError::Refused(
            "release source repository/workspace/baseline differs",
        ));
    }
    let source_root = regular_path(root, &bundle(&fields.operation_id, "source")?)?;
    let expected = source_paths(
        &source_root,
        &authority,
        &manifest.curation.source_authorizations,
    )?;
    let actual = manifest
        .files
        .iter()
        .map(|file| file.original.path.clone())
        .collect::<BTreeSet<_>>();
    let total = manifest
        .files
        .iter()
        .try_fold(0u64, |total, file| total.checked_add(file.original.bytes));
    if expected != actual
        || actual.len() != manifest.files.len()
        || actual.len() > MAX_FILES
        || total.is_none_or(|bytes| bytes > MAX_BYTES)
    {
        return Err(OperationError::Refused(
            "release source inventory is incomplete or unbounded",
        ));
    }
    for retained in &manifest.files {
        if observed.source_entry(&retained.original.path) != Some(&retained.source_entry) {
            return Err(OperationError::Refused(
                "release source object identity differs",
            ));
        }
        let copied = copy_path(&fields.operation_id, "source", &retained.original.path)?;
        let body = read_bounded(&regular_path(root, &copied)?)?;
        if identity(retained.original.path.clone(), &body) != retained.original {
            return Err(OperationError::Refused("retained source copy changed"));
        }
    }
    let attribution = manifest
        .selected_plans
        .iter()
        .map(|plan| plan.fields().plan_id.clone())
        .collect::<Vec<_>>();
    if selected_plans(&authority, &attribution, &manifest.files)? != manifest.selected_plans {
        return Err(OperationError::Refused("retained Plan attribution differs"));
    }
    for file in &fields.verification_evidence {
        matches_file(root, file)?;
    }
    Ok(ValidatedManifest { manifest, checked })
}

/// Live source is required at admission, but never substituted for the bound
/// retained source by a candidate verification request after Plan removal.
pub(super) fn observe_intent(
    root: &Path,
    operation: &LifecycleOperation,
    binding: &ClosureBinding,
) -> OperationResult<()> {
    let fields = operation.fields();
    if fields.stage != initial_stage(OperationKind::Release)
        || fields.outcome != OperationOutcome::Pending
        || fields.integrated_commit.is_some()
        || fields.verification_evidence.len() != 1
    {
        return Err(OperationError::Refused("release initial stage differs"));
    }
    let manifest = validate_manifest(root, operation, binding)?;
    native_evidence::committed_files(
        root,
        &manifest.source_commit,
        &manifest
            .files
            .iter()
            .map(|file| file.original.clone())
            .collect::<Vec<_>>(),
    )?;
    let mut git = checkpoint_git(root, None)?;
    if git.text(&["rev-parse", "--verify", "HEAD^{commit}"])? != manifest.source_commit.as_str()
        || git.text(&["symbolic-ref", "-q", "HEAD"])?
            != format!(
                "refs/heads/{}",
                crate::application::plan_service::root_configuration(root)?
                    .branches
                    .integration
            )
        || git.text(&[
            "rev-parse",
            "--verify",
            &format!(
                "refs/heads/{}^{{commit}}",
                crate::application::plan_service::root_configuration(root)?
                    .branches
                    .stable
            ),
        ])? != manifest.baseline.as_str()
        || !git
            .run(
                &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
                b"",
            )?
            .is_empty()
    {
        return Err(OperationError::Refused(
            "release source Root changed during admission",
        ));
    }
    Ok(())
}

struct IntentProof<'a> {
    root: &'a Path,
}
impl OperationEvidenceProvider for IntentProof<'_> {
    fn observe(
        &mut self,
        operation: &LifecycleOperation,
        binding: &ClosureBinding,
    ) -> OperationResult<()> {
        observe_intent(self.root, operation, binding)
    }
}

pub(crate) struct Admission<'a> {
    pub(crate) actor: String,
    pub(crate) candidate: CommitId,
    pub(crate) candidate_root: &'a RepoPath,
    pub(crate) attribution: &'a [String],
    pub(crate) policy_path: &'a RepoPath,
}

pub(crate) fn begin_under_lock(
    root: &Path,
    workspace: &PlanWorkspace,
    guard: &FileLock,
    store: &OperationStore<'_>,
    request: Admission<'_>,
) -> OperationResult<LifecycleOperation> {
    let Admission {
        actor,
        candidate,
        candidate_root,
        attribution,
        policy_path,
    } = request;
    let config = crate::application::plan_service::root_configuration(root)?;
    let configured = TomlStore::from_configuration(root, &config)?;
    let preflight = super::preflight_locked(root, &config, &configured, guard)?;
    if !preflight.can_release {
        return Err(OperationError::Refused("source release preflight failed"));
    }
    let source = CommitId::new(preflight.dev_commit)?;
    let observed = CurationObservation::observe(root, &source, &candidate)?;
    let checked = curation::release_proof(&observed)?;
    let authority = curation::source_contract(&observed)?;
    if authority.workspace != *workspace {
        return Err(OperationError::Refused(
            "source snapshot changed before release intent",
        ));
    }
    let policy = policy(&observed, policy_path)?;
    let mut files = Vec::new();
    let mut bodies = BTreeMap::new();
    let mut total = 0u64;
    for path in source_paths(root, &authority, &checked.evidence().source_authorizations)? {
        let body = native_evidence::bytes(root, &path)?;
        total = total
            .checked_add(body.len() as u64)
            .ok_or(OperationError::Journal)?;
        if files.len() >= MAX_FILES || total > MAX_BYTES {
            return Err(OperationError::Evidence(
                "release export exceeds bounded retention",
            ));
        }
        files.push(RetainedFile {
            original: identity(path.clone(), &body),
            source_entry: observed
                .source_entry(&path)
                .ok_or(OperationError::Journal)?
                .clone(),
        });
        bodies.insert(path, body);
    }
    native_evidence::committed_files(
        root,
        &source,
        &files
            .iter()
            .map(|file| file.original.clone())
            .collect::<Vec<_>>(),
    )?;
    let selected_plans = selected_plans(&authority, attribution, &files)?;
    let id = OperationId::generate();
    let baseline = CommitId::new(workspace.stable_baseline_commit.clone())?;
    let manifest = ReleaseManifest {
        schema_version: 1,
        operation_id: id.clone(),
        repository_id: config.repository_id.clone(),
        workspace_id: workspace.workspace_id.clone(),
        graph_revision: workspace.revision,
        source_commit: source,
        candidate_root: candidate_root.clone(),
        baseline: baseline.clone(),
        curation: checked.evidence().clone(),
        policy,
        selected_plans,
        files,
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|_| OperationError::Journal)?;
    let manifest_file = identity(bundle(&id, "manifest.json")?, &manifest_bytes);
    verification::candidate_checkout(root, &manifest, &authority.config, None, &checked)?;
    let directory = regular_path(root, &bundle(&id, "source")?)?;
    if directory
        .parent()
        .ok_or(OperationError::Journal)?
        .try_exists()?
    {
        return Err(OperationError::Refused(
            "release evidence operation directory already exists",
        ));
    }
    if fs4::available_space(root)? < total.saturating_mul(2).saturating_add(4 * 1024 * 1024) {
        return Err(OperationError::Evidence(
            "insufficient space for isolated release evidence",
        ));
    }
    for (path, body) in &bodies {
        publish_new(root, &copy_path(&id, "source", path)?, body)?;
        if path
            .as_str()
            .starts_with(&format!("{}/reports/verification/", config.plan.root))
        {
            publish_new(root, &copy_path(&id, "evidence", path)?, body)?;
        }
    }
    fs::create_dir_all(regular_path(root, &bundle(&id, "evidence")?)?)?;
    publish_new(root, &manifest_file.path, &manifest_bytes)?;
    let operation = LifecycleOperation::new(OperationFields {
        schema_version: 1,
        operation_id: id,
        stage_revision: 0,
        kind: OperationKind::Release,
        stage: initial_stage(OperationKind::Release),
        outcome: OperationOutcome::Pending,
        repository_id: config.repository_id,
        workspace_id: workspace.workspace_id.clone(),
        actor,
        observed_graph_revision: workspace.revision,
        plan: None,
        registered_worktree: None,
        review: None,
        integrated_commit: None,
        verification_evidence: vec![manifest_file],
        cleanup: None,
        checkpoint: None,
        diagnostics: vec![],
    })?;
    OperationService::new(store).begin(
        operation,
        ClosureBinding::Release {
            baseline,
            candidate,
        },
        &mut IntentProof { root },
    )
}
