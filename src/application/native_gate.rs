//! Whole-gate admission is separate from append-only historical evidence.
//! No method in this bridge executes a check or republishes a receipt.
use super::native_evidence::{bytes, committed_bytes, committed_files, input};
use super::verification_service::{
    CaptureFields, RunObservation, VerificationBackend, VerificationRequest, VerificationService,
};
use crate::domain::{graph::*, lifecycle_operation::*, verification_receipt::*};
use crate::infrastructure::{
    graph_checkpoint::checkpoint_git,
    verification_store::{FileVerificationStore, VerificationLocation},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSelection {
    pub check: String,
    pub receipt: FileIdentity,
    pub context: VerificationContext,
    pub fresh_observation: Option<FileIdentity>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateInput {
    pub policy_path: Option<RepoPath>,
    pub checks: Vec<GateSelection>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionKind {
    FreshRun,
    RetainedGateRun,
    Reused,
}
#[derive(Debug, Serialize)]
pub struct GateAdmission {
    check: String,
    receipt: FileIdentity,
    fresh_observation: Option<FileIdentity>,
    admission: AdmissionKind,
}
impl GateAdmission {
    pub(crate) fn retain_admission(&mut self) {
        if matches!(self.admission, AdmissionKind::FreshRun) {
            self.admission = AdmissionKind::RetainedGateRun;
        }
    }
}
/// The exact already-journaled graph transition, not a caller exclusion list.
pub(crate) struct TerminalDelta<'a>(
    pub(crate) &'a crate::infrastructure::graph_checkpoint::CheckpointRuntime,
);
impl TerminalDelta<'_> {
    fn matches(&self, before: &InputManifest, current: &InputManifest) -> bool {
        let Ok((source_before, view_before)) = self.0.before.present() else {
            return false;
        };
        let mut expected = before.clone();
        for (path, original, produced) in [
            (
                &self.0.source_path,
                source_before,
                self.0.source_after.as_str(),
            ),
            (&self.0.view_path, view_before, self.0.view_after.as_str()),
        ] {
            let identity = |body: &str| FileIdentity {
                path: path.clone(),
                bytes: body.len() as u64,
                sha256: sha256(body.as_bytes()),
            };
            if let Some(entry) = expected.entries.iter_mut().find(|v| &v.file.path == path) {
                let Some(actual) = current.entries.iter().find(|v| &v.file.path == path) else {
                    return false;
                };
                if entry.directory
                    || actual.directory
                    || entry.file != identity(original)
                    || (actual.file != identity(original) && actual.file != identity(produced))
                {
                    return false;
                }
                // Modes, directory entries, every non-graph input and all policy
                // fields still compare exactly. No entry is stripped or added.
                entry.file = actual.file.clone();
            }
        }
        expected == *current
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateClaim {
    schema_version: u32,
    operation_id: OperationId,
    target_stage: OperationStage,
    target_stage_revision: u64,
    expected_graph_revision: u64,
    check: String,
    receipt: FileIdentity,
    capture: CaptureFields,
    observation: RunObservation,
    attested_by: String,
    attested_at: UtcTimestamp,
    statement: String,
}

fn refused(message: &'static str) -> OperationError {
    OperationError::Evidence(message)
}
fn verification<T>(result: VerificationResult<T>) -> OperationResult<T> {
    result.map_err(|error| match error {
        VerificationError::Refused(reason) => refused(reason),
        VerificationError::Contract(error) => OperationError::Contract(error),
        VerificationError::Io(error) => OperationError::Io(error),
        VerificationError::Json(_) => OperationError::Journal,
    })
}

pub(crate) struct GateProof<'a, B = FileVerificationStore> {
    root: PathBuf,
    operation: &'a LifecycleOperation,
    head: Option<CommitId>,
    backend: B,
    plan: PlanIdentity,
    evidence_prefix: Option<RepoPath>,
    release_policy: Option<(RepoPath, VerificationPolicy)>,
    pinned: std::collections::BTreeMap<RepoPath, FileIdentity>,
    graph_revision: u64,
}
impl<'a> GateProof<'a> {
    pub(crate) fn new(
        root: &'a Path,
        operation: &'a LifecycleOperation,
        graph_revision: u64,
    ) -> OperationResult<Self> {
        let mut git = checkpoint_git(root, None)?;
        let head = CommitId::new(git.text(&["rev-parse", "--verify", "HEAD^{commit}"])?)?;
        let mut pinned = std::collections::BTreeMap::new();
        if operation.fields().verification_evidence.len() > 128 {
            return Err(refused("operation evidence retention capacity exceeded"));
        }
        for identity in &operation.fields().verification_evidence {
            if pinned
                .insert(identity.path.clone(), identity.clone())
                .is_some()
            {
                return Err(refused("duplicate operation evidence path"));
            }
            if identity.path.as_str().ends_with(".json") {
                let stored: StoredVerification = input(root, &identity.path)?;
                verification(stored.validate())?;
                for artifact in &stored.receipt.fields().artifacts {
                    if pinned
                        .insert(artifact.path.clone(), artifact.clone())
                        .is_some_and(|old| old != *artifact)
                    {
                        return Err(refused("conflicting retained artifact identity"));
                    }
                }
            }
            let size = pinned
                .values()
                .try_fold(0u64, |total, value| total.checked_add(value.bytes));
            if pinned.len() > 128 || size.is_none_or(|size| size > 32 * 1024 * 1024) {
                return Err(refused("operation evidence retention capacity exceeded"));
            }
        }
        if !pinned.is_empty() {
            committed_files(root, &head, &pinned.values().cloned().collect::<Vec<_>>())?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            operation,
            head: Some(head),
            backend: verification(FileVerificationStore::new(root, root))?,
            plan: operation
                .fields()
                .plan
                .clone()
                .ok_or(refused("verification Plan missing"))?,
            evidence_prefix: None,
            release_policy: None,
            pinned,
            graph_revision,
        })
    }
}
impl<B: VerificationBackend> GateProof<'_, B> {
    fn plan(&self) -> OperationResult<&PlanIdentity> {
        Ok(&self.plan)
    }
    fn operation_identity(&self, identity: &FileIdentity) -> OperationResult<FileIdentity> {
        let mut identity = identity.clone();
        if let Some(prefix) = &self.evidence_prefix {
            identity.path =
                RepoPath::new(format!("{}/{}", prefix.as_str(), identity.path.as_str()))?;
        }
        Ok(identity)
    }
    fn retained(
        &self,
        operation: &LifecycleOperation,
        identity: &FileIdentity,
    ) -> OperationResult<bool> {
        Ok(operation
            .fields()
            .verification_evidence
            .contains(&self.operation_identity(identity)?))
    }
    fn bound_bytes(&self, identity: &FileIdentity) -> OperationResult<Vec<u8>> {
        let body = bytes(&self.root, &identity.path)?;
        if sha256(&body) != identity.sha256 || body.len() as u64 != identity.bytes {
            return Err(refused("verification file identity changed"));
        }
        if self.pinned.get(&identity.path) != Some(identity) {
            return Err(refused("unbound historical evidence file"));
        }
        Ok(body)
    }
    fn file(&self, identity: &FileIdentity) -> OperationResult<()> {
        self.bound_bytes(identity).map(|_| ())
    }
    fn stored(&self, identity: &FileIdentity) -> OperationResult<StoredVerification> {
        let stored: StoredVerification =
            super::native_evidence::strict_json(&self.bound_bytes(identity)?)?;
        verification(stored.validate())?;
        let fields = stored.receipt.fields();
        if fields.plan != *self.plan()?
            || VerificationLocation::new(&self.root, self.plan()?, &fields.receipt_id)?
                .relative_path()
                != &identity.path
        {
            return Err(refused("foreign or noncanonical receipt"));
        }
        verification(self.backend.artifacts(
            &stored.receipt,
            verification(stored.policy.check(&stored.check))?,
        ))?;
        for artifact in &fields.artifacts {
            self.file(artifact)?;
        }
        Ok(stored)
    }
    fn claim(&self, identity: &FileIdentity) -> OperationResult<(GateClaim, StoredVerification)> {
        let claim: GateClaim = super::native_evidence::strict_json(&self.bound_bytes(identity)?)?;
        let directory = format!("{}/", self.plan()?.verification_directory().as_str());
        let name = identity
            .path
            .as_str()
            .strip_prefix(&directory)
            .and_then(|v| v.strip_suffix(".gate-claim"))
            .ok_or(refused("noncanonical gate claim path"))?;
        OperationId::parse(name)?;
        if claim.schema_version != 1
            || claim.operation_id != self.operation.fields().operation_id
            || claim.target_stage.kind() != self.operation.fields().kind
            || !matches!(
                claim.target_stage,
                OperationStage::Acceptance(
                    AcceptanceStage::Verified
                        | AcceptanceStage::Cleaned
                        | AcceptanceStage::Accepted
                ) | OperationStage::Release(
                    ReleaseStage::Preflight | ReleaseStage::Integrated | ReleaseStage::Complete
                )
            )
            || stage_number(claim.target_stage) > stage_number(self.operation.fields().stage)
            || claim.target_stage_revision > self.operation.fields().stage_revision
            || (self.release_policy.is_none()
                && (claim.expected_graph_revision
                    < self.operation.fields().observed_graph_revision
                    || claim.expected_graph_revision > self.graph_revision))
            || (self.release_policy.is_some()
                && claim.expected_graph_revision != 0
                && claim.expected_graph_revision != self.operation.fields().observed_graph_revision)
            || claim.statement != "executed_for_this_gate"
            || !self.retained(self.operation, &claim.receipt)?
        {
            return Err(refused("gate claim operation binding differs"));
        }
        let stored = self.stored(&claim.receipt)?;
        let fields = stored.receipt.fields();
        let capture = &claim.capture;
        let observed = &claim.observation;
        let expected_outcome = if observed.warnings_observed
            && verification(stored.policy.check(&stored.check))?.warnings_disqualify
        {
            CheckOutcome::WarningDisqualified
        } else {
            observed.outcome
        };
        if capture.schema_version != 1
            || capture.plan != fields.plan
            || capture.actor != fields.actor
            || capture.policy != stored.policy
            || capture.check != stored.check
            || claim.check != stored.check
            || capture.manifest != stored.manifest
            || capture.context.toolchains != fields.toolchains
            || capture.context.platform != fields.platform
            || capture.context.architecture != fields.architecture
            || capture.context.features != fields.features
            || capture.context.environment != fields.environment
            || capture.started_at != fields.started_at
            || observed.ended_at != fields.ended_at
            || observed.exit_code != fields.exit_code
            || expected_outcome != fields.outcome
            || observed.artifacts != fields.artifacts
            || claim.attested_by != fields.actor
            || fields.ended_at.instant() > claim.attested_at.instant()
            || claim.attested_at.instant() > time::OffsetDateTime::now_utc()
            || fields.artifacts.iter().any(|v| v.path == identity.path)
        {
            return Err(refused(
                "gate claim does not match original capture and run",
            ));
        }
        Ok((claim, stored))
    }
    /// Validate all retained history, without counting it as current coverage.
    pub(crate) fn history(&self, identity: &FileIdentity) -> OperationResult<()> {
        if identity.path.as_str().ends_with(".gate-claim") {
            self.claim(identity)?;
        } else if identity.path.as_str().ends_with(".json") {
            self.stored(identity)?;
        } else {
            return Err(refused("unsupported verification evidence type"));
        }
        Ok(())
    }
    pub(crate) fn admit(
        &self,
        gate: &GateInput,
        old: &LifecycleOperation,
        revision: u64,
        implementer: &str,
        terminal_delta: Option<&TerminalDelta<'_>>,
    ) -> OperationResult<Vec<GateAdmission>> {
        self.admit_checks(gate, old, revision, implementer, terminal_delta, true)
    }
    fn admit_checks(
        &self,
        gate: &GateInput,
        old: &LifecycleOperation,
        revision: u64,
        implementer: &str,
        terminal_delta: Option<&TerminalDelta<'_>>,
        complete: bool,
    ) -> OperationResult<Vec<GateAdmission>> {
        let policy_path = gate
            .policy_path
            .clone()
            .unwrap_or_else(|| RepoPath::new(".mine/verification-policy.json").expect("constant"));
        let policy: VerificationPolicy = if let Some((path, policy)) = &self.release_policy {
            if path != &policy_path {
                return Err(refused("Release gate policy selector differs"));
            }
            policy.clone()
        } else {
            committed_bytes(
                &self.root,
                self.head.as_ref().ok_or(OperationError::Journal)?,
                &policy_path,
                &bytes(&self.root, &policy_path)?,
            )?;
            input(&self.root, &policy_path)?
        };
        let required = policy
            .fields()
            .checks
            .iter()
            .map(|v| v.name.as_str())
            .collect::<BTreeSet<_>>();
        let actual = gate
            .checks
            .iter()
            .map(|v| v.check.as_str())
            .collect::<BTreeSet<_>>();
        if (complete && required != actual)
            || !actual.is_subset(&required)
            || actual.len() != gate.checks.len()
        {
            return Err(refused(
                "incomplete or duplicate current policy check coverage",
            ));
        }
        // All sibling claims count against the same existing Plan retention bound.
        let directory = self
            .root
            .join(self.plan()?.verification_directory().as_str());
        let mut files = 0usize;
        let mut total = 0u64;
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let relative = RepoPath::new(format!(
                "{}/{}",
                self.plan()?.verification_directory().as_str(),
                entry
                    .file_name()
                    .to_str()
                    .ok_or(refused("invalid evidence name"))?
            ))?;
            let observed = bytes(&self.root, &relative)?;
            files += 1;
            total = total
                .checked_add(observed.len() as u64)
                .ok_or(refused("retention length overflow"))?;
            if files > 128 || total > 32 * 1024 * 1024 {
                return Err(refused("Plan evidence retention limit exceeded"));
            }
        }
        let mut admissions = Vec::new();
        for selection in &gate.checks {
            if !self.retained(self.operation, &selection.receipt)? {
                return Err(refused("current receipt is not retained by the operation"));
            }
            let stored = self.stored(&selection.receipt)?;
            let selected = verification(policy.check(&selection.check))?;
            let fields = stored.receipt.fields();
            let request = VerificationRequest::new(
                self.plan()?.clone(),
                self.operation.fields().actor.clone(),
                selected.command.clone(),
                policy.identity(),
            )?;
            let service = VerificationService::new(
                &self.backend,
                self.plan()?.fields().repository_id.clone(),
                self.plan()?.fields().workspace_id.clone(),
            )?;
            // capture validates complete current context/body, without launching the command.
            let current = verification(service.capture(
                request.clone(),
                policy.clone(),
                &selection.check,
                selection.context.clone(),
                fields.started_at.clone(),
            ))?
            .export();
            if stored.policy != policy
                || stored.check != selection.check
                || !stored.inputs_stable
                || fields.plan != *self.plan()?
                || fields.outcome != CheckOutcome::Passed
                || fields.exit_code != Some(0)
                || (stored.manifest != current.manifest
                    && !terminal_delta
                        .is_some_and(|delta| delta.matches(&stored.manifest, &current.manifest)))
                || fields.command != selected.command
                || fields.toolchains != selection.context.toolchains
                || fields.platform != selection.context.platform
                || fields.architecture != selection.context.architecture
                || fields.features != selection.context.features
                || fields.environment != selection.context.environment
                || (selected.independent_required
                    && (implementer.trim().is_empty() || fields.actor == implementer))
                || selected
                    .native_platform
                    .as_ref()
                    .is_some_and(|p| p != &selection.context.platform)
            {
                return Err(refused(
                    "current check is unsuccessful, changed or not independently native",
                ));
            }
            if fields.dependency_locks.len() != selected.dependency_locks.len() {
                return Err(refused("dependency lock coverage differs"));
            }
            for path in &selected.dependency_locks {
                if fields.dependency_locks.get(path)
                    != Some(&verification(self.backend.file(path))?.sha256)
                {
                    return Err(refused("dependency lock changed"));
                }
            }
            let admission = if let Some(identity) = &selection.fresh_observation {
                if !self.retained(self.operation, identity)? {
                    return Err(refused("fresh claim is not retained"));
                }
                let (claim, _) = self.claim(identity)?;
                let admitted = self.retained(old, identity)?
                    && self.retained(old, &selection.receipt)?
                    && matches!(
                        old.fields().stage,
                        OperationStage::Acceptance(
                            AcceptanceStage::Verified
                                | AcceptanceStage::Cleaned
                                | AcceptanceStage::Accepted
                        ) | OperationStage::Release(
                            ReleaseStage::Preflight
                                | ReleaseStage::Integrated
                                | ReleaseStage::Complete
                        )
                    )
                    && stage_number(claim.target_stage) <= stage_number(old.fields().stage)
                    && claim.target_stage_revision <= old.fields().stage_revision;
                if claim.receipt != selection.receipt
                    || claim.check != selection.check
                    || (!admitted
                        && (claim.target_stage != self.operation.fields().stage
                            || claim.target_stage_revision
                                != self.operation.fields().stage_revision
                            || claim.expected_graph_revision
                                != terminal_delta
                                    .and_then(|delta| {
                                        toml::from_str::<PlanWorkspace>(
                                            delta.0.source_before().ok()?,
                                        )
                                        .ok()
                                        .map(|ws| ws.revision)
                                    })
                                    .unwrap_or(revision)))
                {
                    return Err(refused(
                        "fresh run belongs to a different gate or observation",
                    ));
                }
                if admitted {
                    AdmissionKind::RetainedGateRun
                } else {
                    AdmissionKind::FreshRun
                }
            } else {
                let decision = service.check(
                    &request,
                    &policy,
                    &selection.check,
                    &selection.context,
                    Some(&fields.receipt_id),
                    &super::verification_service::ReuseRequirements {
                        independent_of: selected.independent_required.then(|| implementer.into()),
                    },
                );
                if decision.original.as_ref() != Some(&stored.receipt) {
                    return Err(refused(
                        "selected verification changed during gate observation",
                    ));
                }
                if decision.disposition == VerificationDisposition::Reused {
                    AdmissionKind::Reused
                } else if terminal_delta.is_some() && !stored.receipt.has_unobservable_inputs() {
                    // This is the previously admitted run under the exact
                    // terminal graph delta, never standalone post-tree REUSED.
                    AdmissionKind::RetainedGateRun
                } else {
                    return Err(refused("current check needs a real fresh gate run"));
                }
            };
            admissions.push(GateAdmission {
                check: selection.check.clone(),
                receipt: selection.receipt.clone(),
                fresh_observation: selection.fresh_observation.clone(),
                admission,
            });
        }
        Ok(admissions)
    }
}

impl<'a> GateProof<'a, super::release_service::retained::ReleaseVerification> {
    /// Called only after the retained release adapter has validated the full
    /// operation inventory and whole-policy coverage across selected real Plans.
    pub(crate) fn release(
        record: &'a crate::infrastructure::operation_store::JournalRecord,
        graph_revision: u64,
        plan: PlanIdentity,
        backend: super::release_service::retained::ReleaseVerification,
        policy_path: &RepoPath,
    ) -> OperationResult<Self> {
        let policy = verification(backend.load_policy(policy_path))?;
        let evidence_root = backend.evidence_root().to_path_buf();
        let prefix = RepoPath::new(format!(
            ".mine/runtime/release-evidence/{}/evidence",
            record.operation.fields().operation_id.as_str()
        ))?;
        let directory = format!("{}/", plan.verification_directory().as_str());
        let mut pinned = std::collections::BTreeMap::new();
        for file in &record.operation.fields().verification_evidence {
            let Some(logical) = file
                .path
                .as_str()
                .strip_prefix(&format!("{}/", prefix.as_str()))
            else {
                continue;
            };
            if !logical.starts_with(&directory) {
                continue;
            }
            let mut logical_file = file.clone();
            logical_file.path = RepoPath::new(logical)?;
            if pinned
                .insert(logical_file.path.clone(), logical_file.clone())
                .is_some()
            {
                return Err(refused("duplicate retained Release evidence path"));
            }
            if logical.ends_with(".json") {
                let stored: StoredVerification = input(&evidence_root, &logical_file.path)?;
                verification(stored.validate())?;
                for artifact in &stored.receipt.fields().artifacts {
                    if pinned
                        .insert(artifact.path.clone(), artifact.clone())
                        .is_some_and(|old| old != *artifact)
                    {
                        return Err(refused("conflicting retained Release artifact identity"));
                    }
                }
            }
        }
        let total = pinned
            .values()
            .try_fold(0u64, |sum, file| sum.checked_add(file.bytes));
        if pinned.len() > 128 || total.is_none_or(|bytes| bytes > 32 * 1024 * 1024) {
            return Err(refused("Release Plan evidence retention capacity exceeded"));
        }
        Ok(Self {
            root: evidence_root,
            operation: &record.operation,
            head: None,
            backend,
            plan,
            evidence_prefix: Some(prefix),
            release_policy: Some((policy_path.clone(), policy)),
            pinned,
            graph_revision,
        })
    }

    pub(crate) fn admit_release_selection(
        &self,
        gate: &GateInput,
        old: &LifecycleOperation,
        revision: u64,
    ) -> OperationResult<Vec<GateAdmission>> {
        self.admit_checks(gate, old, revision, self.backend.owner(), None, false)
    }
}
