//! Release admission reuses the same whole-check verifier as acceptance, but
//! observes the explicitly frozen candidate and retained real Plan identities.
use super::*;
use crate::application::native_lifecycle::finalize::FinalizeOutcome;
use crate::application::{
    native_gate::{GateAdmission, GateInput, GateProof},
    native_lifecycle::{StageInput, StageOutcome},
};
use crate::domain::error::MineResult;
use crate::infrastructure::operation_store::{JournalRecord, RepositoryJournal};

pub(super) fn evidence_plan<'a>(
    manifest: &'a ReleaseManifest,
    file: &FileIdentity,
) -> OperationResult<&'a PlanIdentity> {
    let prefix = format!("{}/", bundle(&manifest.operation_id, "evidence")?.as_str());
    let logical = file
        .path
        .as_str()
        .strip_prefix(&prefix)
        .ok_or(OperationError::Refused(
            "Release evidence is outside its bound evidence root",
        ))?;
    manifest
        .selected_plans
        .iter()
        .find(|plan| logical.starts_with(&format!("{}/", plan.verification_directory().as_str())))
        .ok_or(OperationError::Refused(
            "Release evidence belongs to an unselected Plan",
        ))
}

fn physical(manifest: &ReleaseManifest, logical: &FileIdentity) -> OperationResult<FileIdentity> {
    let mut file = logical.clone();
    file.path = copy_path(&manifest.operation_id, "evidence", &logical.path)?;
    Ok(file)
}

/// Candidate checks do not authorize unrelated dirty Root state. Only exact
/// source-owned deletions already proven by this frozen curation may remain.
fn source_root(
    root: &Path,
    manifest: &ReleaseManifest,
    git: &mut crate::infrastructure::git_evidence::GitRequest,
) -> OperationResult<()> {
    let config = manifest
        .files
        .iter()
        .find(|file| file.original.path.as_str() == ".mine/config.toml")
        .ok_or(OperationError::Journal)?;
    matches_file(root, &config.original)?;
    verification::clean_index_state(git)?;
    let flags = git.run(&["ls-files", "-v", "-z"], b"")?;
    if (!flags.is_empty() && flags.last() != Some(&0))
        || flags
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .any(|entry| !entry.starts_with(b"H ") || entry.len() <= 2)
    {
        return Err(OperationError::Refused(
            "Root has hidden or unresolved index entries",
        ));
    }
    let mut paths = BTreeSet::new();
    for entry in flags
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let path =
            RepoPath::new(std::str::from_utf8(&entry[2..]).map_err(|_| OperationError::Journal)?)?;
        if !paths.insert(path) || paths.len() > 100_000 {
            return Err(OperationError::Journal);
        }
    }
    // Check attributes before status: status can otherwise launch a configured
    // clean filter while examining a dirty Root file, despite a clean C.
    verification::untransformed(git, paths.iter())?;
    let deleted = manifest
        .curation
        .delta
        .iter()
        .filter(|delta| delta.before.is_some() && delta.after.is_none())
        .map(|delta| delta.path.as_str())
        .collect::<BTreeSet<_>>();
    let changes = git.run(
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
        b"",
    )?;
    if !changes.is_empty() && changes.last() != Some(&0) {
        return Err(OperationError::Evidence("Root status framing"));
    }
    for change in changes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        if change.len() <= 3 || !matches!(&change[..3], b" D " | b"D  " | b"DD ") {
            return Err(OperationError::Refused(
                "Root has unrelated dirty/staged/untracked state",
            ));
        }
        let path = std::str::from_utf8(&change[3..]).map_err(|_| OperationError::Journal)?;
        if !deleted.contains(path) {
            return Err(OperationError::Refused(
                "Root deletion is not authorized by frozen curation",
            ));
        }
    }
    Ok(())
}

pub(super) fn observe_source(
    root: &Path,
    manifest: &ReleaseManifest,
    config: &crate::domain::config::MineConfig,
    git: &mut crate::infrastructure::git_evidence::GitRequest,
) -> OperationResult<()> {
    source_root(root, manifest, git)?;
    if git.text(&["symbolic-ref", "-q", "HEAD"])?
        != format!("refs/heads/{}", config.branches.integration)
        || git.text(&["rev-parse", "--verify", "HEAD^{commit}"])? != manifest.source_commit.as_str()
        || git.text(&[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{}^{{commit}}", config.branches.stable),
        ])? != manifest.baseline.as_str()
    {
        return Err(OperationError::Refused(
            "preflight Root source/baseline differs",
        ));
    }
    Ok(())
}

/// Refusal observes live S, not C. Missing/defective pre-begin candidate
/// preparation never authorizes effects and must not make clean refusal depend
/// on restoring that candidate. The frozen manifest remains required to bind S.
fn no_release_effects(
    root: &Path,
    manifest: &ReleaseManifest,
    config: &crate::domain::config::MineConfig,
    workspace: &PlanWorkspace,
) -> OperationResult<()> {
    if workspace.workspace_id != manifest.workspace_id
        || workspace.revision != manifest.graph_revision
    {
        return Err(OperationError::Refused(
            "refusal requires the unchanged source workspace",
        ));
    }
    if manifest.files.len() > MAX_FILES || manifest.files.is_empty() {
        return Err(OperationError::Journal);
    }
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    for file in &manifest.files {
        total = total
            .checked_add(file.original.bytes)
            .ok_or(OperationError::Journal)?;
        if !paths.insert(&file.original.path) || total > MAX_BYTES {
            return Err(OperationError::Journal);
        }
        matches_file(root, &file.original)?;
    }
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
            "refusal requires actual Root worktree",
        ));
    }
    if git.text(&["symbolic-ref", "-q", "HEAD"])?
        != format!("refs/heads/{}", config.branches.integration)
        || git.text(&["rev-parse", "--verify", "HEAD^{commit}"])? != manifest.source_commit.as_str()
        || git.text(&[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{}^{{commit}}", config.branches.stable),
        ])? != manifest.baseline.as_str()
    {
        return Err(OperationError::Refused(
            "refusal source HEAD or stable baseline changed",
        ));
    }
    verification::prove_checkout_bytes(root, &mut git, &manifest.source_commit)?;
    if !git
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
    {
        return Err(OperationError::Refused(
            "refusal requires clean unchanged Root",
        ));
    }
    Ok(())
}

/// The entire declared policy is covered exactly once. Each selected receipt
/// retains its own genuine attribution; no fake operation.plan is synthesized.
fn admit(
    root: &Path,
    manifest: &ValidatedManifest,
    record: &JournalRecord,
    old: &LifecycleOperation,
    revision: u64,
    gate: &GateInput,
) -> OperationResult<Vec<GateAdmission>> {
    if gate
        .policy_path
        .as_ref()
        .is_some_and(|path| path != &manifest.policy.path)
    {
        return Err(OperationError::Refused("Release policy selector differs"));
    }
    let (_, backend) = ReleaseVerification::from_manifest(
        root,
        &record.operation,
        manifest,
        &manifest
            .selected_plans
            .first()
            .ok_or(OperationError::Journal)?
            .fields()
            .plan_id,
    )?;
    let policy = backend
        .load_policy(&manifest.policy.path)
        .map_err(|_| OperationError::Evidence("Release policy unavailable"))?;
    let expected = policy
        .fields()
        .checks
        .iter()
        .map(|check| check.name.as_str())
        .collect::<BTreeSet<_>>();
    let selected = gate
        .checks
        .iter()
        .map(|check| check.check.as_str())
        .collect::<BTreeSet<_>>();
    if expected != selected || selected.len() != gate.checks.len() {
        return Err(OperationError::Refused(
            "incomplete or duplicate current Release check coverage",
        ));
    }
    if record.operation.fields().verification_evidence.len() > 129 {
        return Err(OperationError::Refused(
            "Release evidence retention capacity exceeded",
        ));
    }
    let mut proofs = BTreeMap::new();
    for plan in &manifest.selected_plans {
        proofs.insert(
            plan.fields().plan_id.clone(),
            GateProof::release(
                record,
                revision,
                plan.clone(),
                backend.for_plan(manifest, plan)?,
                &manifest.policy.path,
            )?,
        );
    }
    // Validate every historical receipt/claim, not only today's chosen checks.
    let mut paths = BTreeSet::new();
    for file in record
        .operation
        .fields()
        .verification_evidence
        .iter()
        .skip(1)
    {
        if !paths.insert(file.path.clone()) {
            return Err(OperationError::Refused("duplicate Release evidence path"));
        }
        let plan = evidence_plan(manifest, file)?;
        let proof = proofs
            .get(&plan.fields().plan_id)
            .ok_or(OperationError::Journal)?;
        let prefix = format!("{}/", bundle(&manifest.operation_id, "evidence")?.as_str());
        let mut logical = file.clone();
        logical.path = RepoPath::new(
            file.path
                .as_str()
                .strip_prefix(&prefix)
                .ok_or(OperationError::Journal)?,
        )?;
        proof.history(&logical)?;
    }
    let mut admissions = Vec::new();
    for selection in &gate.checks {
        let receipt = physical(manifest, &selection.receipt)?;
        let plan = evidence_plan(manifest, &receipt)?;
        let proof = proofs
            .get(&plan.fields().plan_id)
            .ok_or(OperationError::Journal)?;
        // One selected check retains the original strict DTO/bindings.
        let selected_gate = GateInput {
            policy_path: Some(manifest.policy.path.clone()),
            checks: vec![selection.clone()],
        };
        admissions.extend(proof.admit_release_selection(&selected_gate, old, revision)?);
    }
    Ok(admissions)
}

pub(crate) fn stage(
    root: &Path,
    expected_revision: u64,
    request: StageInput,
) -> MineResult<StageOutcome> {
    progress(root, expected_revision, request, false).map(|result| StageOutcome {
        operation: result.operation,
        checks: result.checks,
    })
}

pub(crate) fn finalize(
    root: &Path,
    expected_revision: u64,
    request: StageInput,
) -> MineResult<FinalizeOutcome> {
    progress(root, expected_revision, request, true)
}

fn progress(
    root: &Path,
    expected_revision: u64,
    request: StageInput,
    completing: bool,
) -> MineResult<FinalizeOutcome> {
    let config = crate::application::plan_service::root_configuration(root)?;
    let configured = TomlStore::from_configuration(root, &config)?;
    configured.with_coordinator_lock(|guard| {
        let actual = match configured.observed_revision() {
            Ok(revision) => revision,
            Err(crate::domain::error::MineError::GraphNotInitialized { .. }) => 0,
            Err(error) => return Err(error),
        };
        if actual != expected_revision {
            return Err(crate::domain::error::MineError::RevisionConflict { expected: expected_revision, actual });
        }
        let journal = RepositoryJournal::under_lock(root, &config.repository_id, guard)
            .map_err(|error| native_evidence::refused(error.to_string()))?;
        let observations = journal.observations().map_err(|error| native_evidence::refused(error.to_string()))?;
        let old = journal.query(&request.operation.fields().operation_id).map_err(|error| native_evidence::refused(error.to_string()))?;
        let proposed = JournalRecord { operation: request.operation, ..old.clone() };
        let run = || -> OperationResult<FinalizeOutcome> {
            proposed.validate()?;
            let fields = proposed.operation.fields();
            let refusing = fields.stage == OperationStage::Release(ReleaseStage::Intent)
                && fields.outcome == OperationOutcome::Refused;
            let permitted = match (completing, fields.stage) {
                (true, OperationStage::Release(ReleaseStage::Complete)) => fields.outcome == OperationOutcome::Committed,
                (false, OperationStage::Release(ReleaseStage::Preflight | ReleaseStage::Integrated)) => !terminal(&proposed.operation),
                _ => false,
            };
            if !refusing && !permitted {
                return Err(OperationError::Refused("Release terminal progression requires finalize"));
            }
            if proposed.operation != old.operation {
                if old.operation.fields().stage_revision != request.expected_stage_revision {
                    return Err(OperationError::Revision { expected:request.expected_stage_revision, actual:old.operation.fields().stage_revision });
                }
                validate_progress(&old.operation, &proposed.operation)?;
            } else if request.expected_stage_revision != fields.stage_revision
                && request.expected_stage_revision.checked_add(1) != Some(fields.stage_revision) {
                return Err(OperationError::Refused("Release replay stage revision differs"));
            }
            if observations.iter().any(|record| record.operation.fields().operation_id != fields.operation_id && !terminal(&record.operation)) {
                return Err(OperationError::Refused("foreign pending operation blocks Release progress"));
            }
            if refusing {
                if request.verification.is_some() || fields.verification_evidence != old.operation.fields().verification_evidence
                    || fields.integrated_commit.is_some() || fields.diagnostics.last().is_none_or(|reason| reason.trim().is_empty()) {
                    return Err(OperationError::Refused("Intent refusal requires a reason and unchanged bindings without verification/integration"));
                }
                let workspace = configured.load()?;
                if workspace.workspace_id != fields.workspace_id { return Err(OperationError::Refused("refusal belongs to a different workspace")); }
                let replay = old.operation == proposed.operation;
                if !replay {
                    if old.operation.fields().stage != OperationStage::Release(ReleaseStage::Intent)
                        || old.operation.fields().outcome != OperationOutcome::Pending { return Err(OperationError::Refused("only a known side-effect-free Intent can be refused")); }
                    let manifest = bound_manifest(root, &proposed.operation, proposed.closure.as_ref().ok_or(OperationError::Journal)?)?;
                    no_release_effects(root, &manifest, &config, &workspace)?;
                }
                let store = OperationStore::under_lock(root, &config.repository_id, &fields.workspace_id, guard)?;
                store.observations()?;
                let operation = if replay { old.operation.clone() } else { store.advance(request.expected_stage_revision, &proposed)?.operation };
                return Ok(FinalizeOutcome { workspace:None, operation, checkpoint_operation_id:None,
                    revision_before:actual, revision_after:actual, checks:vec![], replay });
            }
            let manifest = validate_manifest(root, &proposed.operation, proposed.closure.as_ref().ok_or(OperationError::Journal)?)?;
            if actual != 0 {
                let source_graph = manifest.files.iter().find(|file| file.original.path.as_str() == config.graph.source).ok_or(OperationError::Journal)?;
                matches_file(root, &source_graph.original)?;
            }
            match fields.stage {
                OperationStage::Release(ReleaseStage::Preflight) => {
                    if fields.integrated_commit.is_some() { return Err(OperationError::Refused("preflight cannot claim stable integration")); }
                    let mut git = checkpoint_git(root, None)?;
                    observe_source(root, &manifest, &config, &mut git)?;
                }
                OperationStage::Release(ReleaseStage::Integrated | ReleaseStage::Complete) => {
                    if actual != 0 || fields.integrated_commit.is_none() {
                        return Err(OperationError::Refused("integrated Release requires purged live graph and actual stable commit"));
                    }
                }
                _ => return Err(OperationError::Journal),
            }
            let gate = request.verification.as_ref().ok_or(OperationError::Evidence("complete current Release verification selection required"))?;
            let checks = admit(root, &manifest, &proposed, &old.operation, actual, gate)?;
            let store = OperationStore::under_lock(root, &config.repository_id, &fields.workspace_id, guard)?;
            let replay = old.operation == proposed.operation;
            let operation = if replay { old.operation.clone() }
                else { store.advance(request.expected_stage_revision, &proposed)?.operation };
            Ok(FinalizeOutcome { workspace:None, operation, checkpoint_operation_id:None,
                revision_before:actual, revision_after:actual, checks, replay })
        };
        run().map_err(|error| native_evidence::refused(error.to_string()))
    })
}
