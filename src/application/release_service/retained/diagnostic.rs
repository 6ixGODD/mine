//! Read-only phase facts do not confer fresh check applicability or permission.
use super::*;
use crate::domain::error::{MineError, MineResult};
use crate::infrastructure::operation_store::{JournalRecord, RepositoryJournal, history};

#[derive(Debug, serde::Serialize)]
pub struct ReleaseDiagnostic {
    pub operation: LifecycleOperation,
    pub can_release: bool,
    pub objective_phase_facts_valid: bool,
    pub current_gate_admission: &'static str,
    pub explanation: &'static str,
    pub historical_verification_evidence: Vec<FileIdentity>,
}

pub(crate) fn diagnose(root: &Path, id: &OperationId) -> MineResult<ReleaseDiagnostic> {
    let config = crate::application::plan_service::root_configuration(root)?;
    let store = TomlStore::from_configuration(root, &config)?;
    store.with_coordinator_lock(|guard| {
        let run = || -> OperationResult<ReleaseDiagnostic> {
            let journal = RepositoryJournal::under_lock(root, &config.repository_id, guard)?;
            let observed = journal.observations()?;
            let record = journal.query(id)?;
            if record.operation.fields().kind != OperationKind::Release {
                return Err(OperationError::Refused("selected operation is not a Release"));
            }
            let current = match store.load() {
                Ok(workspace) => Some(workspace),
                Err(MineError::GraphNotInitialized { .. }) => None,
                Err(error) => return Err(error.into()),
            };
            history::predecessors(&journal, &observed, current.as_ref().map(|workspace| workspace.workspace_id.as_str()).or(Some(record.operation.fields().workspace_id.as_str())))?;
            if observed.iter().any(|entry| entry.operation.fields().operation_id != *id
                && !terminal(&entry.operation)) {
                return Err(OperationError::Refused("foreign pending operation prevents phase observation"));
            }
            let binding = record.closure.as_ref().ok_or(OperationError::Journal)?;
            let manifest = bound_manifest(root, &record.operation, binding)?;
            let mut git = checkpoint_git(root, None)?;
            let identity = git.text(&["rev-parse", "--path-format=absolute", "--show-toplevel", "--absolute-git-dir", "--git-common-dir"])?;
            let identity = identity.lines().collect::<Vec<_>>();
            if identity.len() != 3 || Path::new(identity[0]).canonicalize()? != root.canonicalize()?
                || Path::new(identity[1]).canonicalize()? != Path::new(identity[2]).canonicalize()? {
                return Err(OperationError::Refused("phase diagnostics require actual Root authority"));
            }
            match record.operation.fields().stage {
                OperationStage::Release(ReleaseStage::Intent | ReleaseStage::Preflight) => {
                    // An explicitly bound Release may already have performed
                    // its authorized Plan purge while Root still observes S.
                    // This is never the no-ID command's archive fallback.
                    if current.as_ref().is_some_and(|current| current.workspace_id != manifest.workspace_id || current.revision != manifest.graph_revision) {
                        return Err(OperationError::Refused("selected source workspace/revision changed"));
                    }
                    let manifest = validate_manifest(root, &record.operation, binding)?;
                    progress::observe_source(root, &manifest, &config, &mut git)?;
                    observe_history(root, &manifest, &record)?;
                }
                OperationStage::Release(ReleaseStage::Integrated) => {
                    if current.is_some() {
                        return Err(OperationError::Refused("integrated diagnostics require purged live graph"));
                    }
                    let manifest = validate_manifest(root, &record.operation, binding)?;
                    observe_history(root, &manifest, &record)?;
                }
                OperationStage::Release(ReleaseStage::Complete) => {
                    history::seal(root, &record)?;
                }
                _ => return Err(OperationError::Refused("unsupported selected release phase")),
            }
            Ok(ReleaseDiagnostic {
                historical_verification_evidence: record.operation.fields().verification_evidence.iter().skip(1).cloned().collect(),
                operation: record.operation,
                can_release: false,
                objective_phase_facts_valid: true,
                current_gate_admission: "not_performed",
                explanation: "Current gate admission not performed by this read-only command; use typed operation stage/finalize with current verification context. Historical evidence grants no new release or publication permission.",
            })
        };
        run().map_err(|error| native_evidence::refused(error.to_string()))
    })
}

fn observe_history(
    root: &Path,
    manifest: &ValidatedManifest,
    record: &JournalRecord,
) -> OperationResult<()> {
    // Construction independently observes the correct phase checkout and policy.
    // History validation never calls current-selection admission or executes checks.
    let first = manifest
        .selected_plans
        .first()
        .ok_or(OperationError::Journal)?;
    let (_, backend) = ReleaseVerification::from_manifest(
        root,
        &record.operation,
        manifest,
        &first.fields().plan_id,
    )?;
    for plan in &manifest.selected_plans {
        let proof = crate::application::native_gate::GateProof::release(
            record,
            manifest.graph_revision,
            plan.clone(),
            backend.for_plan(manifest, plan)?,
            &manifest.policy.path,
        )?;
        let prefix = format!("{}/", bundle(&manifest.operation_id, "evidence")?.as_str());
        for file in record
            .operation
            .fields()
            .verification_evidence
            .iter()
            .skip(1)
        {
            let logical = file
                .path
                .as_str()
                .strip_prefix(&prefix)
                .ok_or(OperationError::Journal)?;
            if progress::evidence_plan(manifest, file)? == plan {
                let mut file = file.clone();
                file.path = RepoPath::new(logical)?;
                proof.history(&file)?;
            }
        }
    }
    Ok(())
}
