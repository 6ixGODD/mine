//! Record observations and check applicability. Neither API launches commands.

use crate::domain::error::{ContractError, ContractResult, contract_text};
use crate::domain::graph::{FileIdentity, RepoPath, UtcTimestamp};
use crate::domain::graph::{PlanIdentity, PolicyIdentity};
use crate::domain::verification_receipt::*;
pub use crate::domain::verification_receipt::{CheckCommand, ReceiptId, VerificationReceipt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationRequest {
    plan: PlanIdentity,
    actor: String,
    command: CheckCommand,
    policy: PolicyIdentity,
}

/// Real IO boundary: implementations observe files, never execute check argv.
pub trait VerificationBackend {
    fn manifest(
        &self,
        policy: &VerificationPolicy,
        check: &str,
    ) -> VerificationResult<InputManifest>;
    fn file(&self, path: &RepoPath) -> VerificationResult<FileIdentity>;
    /// Exact Plan attribution is distinct from a command's filesystem inputs.
    /// Release composition may use validated retained authority here, never as
    /// a fallback for ordinary file/lock/policy/cwd/manifest observations.
    fn plan_body(&self, plan: &PlanIdentity) -> VerificationResult<FileIdentity>;
    fn artifacts(
        &self,
        receipt: &VerificationReceipt,
        policy: &CheckPolicy,
    ) -> VerificationResult<()>;
    fn load(
        &self,
        plan: &PlanIdentity,
        id: &ReceiptId,
    ) -> VerificationResult<Option<StoredVerification>>;
    fn save(&self, evidence: &StoredVerification) -> VerificationResult<()>;
}

// Gate admission borrows the already observed backend; it must not silently
// construct a second store with different source/evidence roots.
impl<B: VerificationBackend + ?Sized> VerificationBackend for &B {
    fn manifest(
        &self,
        policy: &VerificationPolicy,
        check: &str,
    ) -> VerificationResult<InputManifest> {
        (**self).manifest(policy, check)
    }
    fn file(&self, path: &RepoPath) -> VerificationResult<FileIdentity> {
        (**self).file(path)
    }
    fn plan_body(&self, plan: &PlanIdentity) -> VerificationResult<FileIdentity> {
        (**self).plan_body(plan)
    }
    fn artifacts(
        &self,
        receipt: &VerificationReceipt,
        policy: &CheckPolicy,
    ) -> VerificationResult<()> {
        (**self).artifacts(receipt, policy)
    }
    fn load(
        &self,
        plan: &PlanIdentity,
        id: &ReceiptId,
    ) -> VerificationResult<Option<StoredVerification>> {
        (**self).load(plan, id)
    }
    fn save(&self, evidence: &StoredVerification) -> VerificationResult<()> {
        (**self).save(evidence)
    }
}

/// Must be obtained before the runner starts. Private fields prevent accidental
/// substitution of an after-the-run input key. Endpoint equality cannot detect
/// a hostile edit-and-revert; the runner must hold its source inputs stable.
pub struct VerificationCapture {
    request: VerificationRequest,
    policy: VerificationPolicy,
    check: String,
    manifest: InputManifest,
    context: VerificationContext,
    started_at: UtcTimestamp,
}

/// Portable pre-run observation for a future record --input adapter. Restoring
/// it verifies bindings, not execution authenticity. Like all local receipts,
/// it is trusted caller evidence and must not be invented after the run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureFields {
    pub schema_version: u32,
    pub plan: PlanIdentity,
    pub actor: String,
    pub policy: VerificationPolicy,
    pub check: String,
    pub manifest: InputManifest,
    pub context: VerificationContext,
    pub started_at: UtcTimestamp,
}
impl VerificationCapture {
    pub fn export(&self) -> CaptureFields {
        CaptureFields {
            schema_version: 1,
            plan: self.request.plan.clone(),
            actor: self.request.actor.clone(),
            policy: self.policy.clone(),
            check: self.check.clone(),
            manifest: self.manifest.clone(),
            context: self.context.clone(),
            started_at: self.started_at.clone(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunObservation {
    pub ended_at: UtcTimestamp,
    pub exit_code: Option<i32>,
    pub outcome: CheckOutcome,
    pub warnings_observed: bool,
    /// Already retained, contained evidence. The backend rehashes every file.
    pub artifacts: Vec<ArtifactIdentity>,
}

#[derive(Debug, Clone, Default)]
pub struct ReuseRequirements {
    /// An independently required run cannot be evidence from this actor.
    pub independent_of: Option<String>,
}

pub struct VerificationService<B> {
    backend: B,
    repository_id: String,
    workspace_id: String,
}
impl<B: VerificationBackend> VerificationService<B> {
    /// Identities come from the authoritative Root snapshot, not from a receipt.
    pub fn new(backend: B, repository_id: String, workspace_id: String) -> ContractResult<Self> {
        contract_text("repository_id", &repository_id)?;
        contract_text("workspace_id", &workspace_id)?;
        Ok(Self {
            backend,
            repository_id,
            workspace_id,
        })
    }
    pub fn backend(&self) -> &B {
        &self.backend
    }

    fn validate_request(
        &self,
        request: &VerificationRequest,
        policy: &VerificationPolicy,
        check: &str,
        context: &VerificationContext,
    ) -> VerificationResult<()> {
        if request.plan.fields().repository_id != self.repository_id
            || request.plan.fields().workspace_id != self.workspace_id
        {
            return Err(VerificationError::Refused(
                "foreign repository or workspace",
            ));
        }
        let selected = policy.check(check)?;
        if request.policy != policy.identity() || request.command != selected.command {
            return Err(VerificationError::Refused(
                "command or policy identity mismatch",
            ));
        }
        if context
            .toolchains
            .keys()
            .collect::<std::collections::BTreeSet<_>>()
            != selected.tools.iter().collect()
            || context.features != selected.features
            || context.environment.len() != selected.environment.len()
        {
            return Err(VerificationError::Refused(
                "incomplete tool, feature or environment identities",
            ));
        }
        for expected in &selected.environment {
            let actual = context
                .environment
                .iter()
                .filter(|e| e.name == expected.name)
                .collect::<Vec<_>>();
            if actual.len() != 1
                || actual[0].secret_dependent != expected.secret_dependent
                || actual[0].observable != expected.observable
                || ((expected.secret_dependent || !expected.observable)
                    && actual[0].sha256.is_some())
            {
                return Err(VerificationError::Refused(
                    "unsafe or incomplete environment identity",
                ));
            }
        }
        contract_text("platform", &context.platform)?;
        contract_text("architecture", &context.architecture)?;
        let actual_body = self.backend.plan_body(&request.plan)?;
        if actual_body.path != request.plan.fields().plan_path
            || Some(&actual_body.sha256) != request.plan.fields().body_sha256.as_ref()
        {
            return Err(VerificationError::Refused(
                "immutable Plan body identity mismatch",
            ));
        }
        Ok(())
    }

    pub fn capture(
        &self,
        request: VerificationRequest,
        policy: VerificationPolicy,
        check: &str,
        context: VerificationContext,
        started_at: UtcTimestamp,
    ) -> VerificationResult<VerificationCapture> {
        self.validate_request(&request, &policy, check, &context)?;
        let manifest = self.backend.manifest(&policy, check)?;
        Ok(VerificationCapture {
            request,
            policy,
            check: check.into(),
            manifest,
            context,
            started_at,
        })
    }

    pub fn import_capture(&self, fields: CaptureFields) -> VerificationResult<VerificationCapture> {
        let selected = fields.policy.check(&fields.check)?;
        let request = VerificationRequest::new(
            fields.plan,
            fields.actor,
            selected.command.clone(),
            fields.policy.identity(),
        )?;
        self.validate_request(&request, &fields.policy, &fields.check, &fields.context)?;
        if fields.schema_version != 1
            || fields.manifest.schema_version != 1
            || fields.manifest.policy_sha256 != request.policy.fields().sha256
            || fields.manifest.command != selected.command
            || fields
                .manifest
                .entries
                .windows(2)
                .any(|pair| pair[0].file.path >= pair[1].file.path)
        {
            return Err(VerificationError::Refused(
                "invalid pre-run capture binding",
            ));
        }
        Ok(VerificationCapture {
            request,
            policy: fields.policy,
            check: fields.check,
            manifest: fields.manifest,
            context: fields.context,
            started_at: fields.started_at,
        })
    }

    pub fn record(
        &self,
        capture: VerificationCapture,
        observed: RunObservation,
    ) -> VerificationResult<VerificationDecision> {
        let c = capture;
        self.validate_request(&c.request, &c.policy, &c.check, &c.context)?;
        let selected = c.policy.check(&c.check)?;
        let inputs_stable = c.manifest == self.backend.manifest(&c.policy, &c.check)?;
        let outcome = if observed.warnings_observed && selected.warnings_disqualify {
            CheckOutcome::WarningDisqualified
        } else {
            observed.outcome
        };
        let dependency_locks = selected
            .dependency_locks
            .iter()
            .map(|p| self.backend.file(p).map(|f| (p.clone(), f.sha256)))
            .collect::<VerificationResult<_>>()?;
        let receipt = VerificationReceipt::new(ReceiptFields {
            schema_version: 1,
            receipt_id: ReceiptId::generate(),
            plan: c.request.plan,
            actor: c.request.actor,
            command: c.request.command,
            started_at: c.started_at,
            ended_at: observed.ended_at,
            exit_code: observed.exit_code,
            outcome,
            policy: c.policy.identity(),
            input_manifest_sha256: c.manifest.digest(),
            toolchains: c.context.toolchains,
            dependency_locks,
            platform: c.context.platform,
            architecture: c.context.architecture,
            features: c.context.features,
            environment: c.context.environment,
            artifacts: observed.artifacts,
        })?;
        self.backend.artifacts(&receipt, selected)?;
        let evidence = StoredVerification {
            schema_version: 1,
            check: c.check,
            receipt: receipt.clone(),
            policy: c.policy,
            manifest: c.manifest,
            inputs_stable,
        };
        self.backend.save(&evidence)?;
        Ok(decision(
            VerificationDisposition::Run,
            VerificationReason::ObservedRun,
            Some(receipt),
        ))
    }

    /// Read-only; missing or invalid evidence requests a fresh check from the
    /// caller. It never silently runs that check or repairs stored evidence.
    pub fn check(
        &self,
        request: &VerificationRequest,
        policy: &VerificationPolicy,
        check: &str,
        context: &VerificationContext,
        id: Option<&ReceiptId>,
        requirements: &ReuseRequirements,
    ) -> VerificationDecision {
        use VerificationDisposition as D;
        use VerificationReason as R;
        let Some(id) = id else {
            return decision(D::NotRun, R::NoReceipt, None);
        };
        let stored = match self.backend.load(request.plan(), id) {
            Ok(Some(e)) => e,
            _ => return decision(D::Invalidated, R::EvidenceUnavailable, None),
        };
        let receipt = stored.receipt.clone();
        let result = |d, r| decision(d, r, Some(receipt.clone()));
        let fields = receipt.fields();
        if stored.validate().is_err() {
            return result(D::Invalidated, R::EvidenceUnavailable);
        }
        if stored.schema_version != 1
            || stored.manifest.schema_version != 1
            || fields.receipt_id != *id
            || fields.plan != request.plan
            || fields.command != request.command
            || stored.check != check
        {
            return result(D::Invalidated, R::IdentityChanged);
        }
        if fields.policy != policy.identity()
            || stored.policy != *policy
            || fields.input_manifest_sha256 != stored.manifest.digest()
            || stored.manifest.policy_sha256 != fields.policy.fields().sha256
            || stored.manifest.command != fields.command
        {
            return result(D::Invalidated, R::PolicyChanged);
        }
        if self
            .validate_request(request, policy, check, context)
            .is_err()
        {
            return result(D::Invalidated, R::ContextChanged);
        }
        let selected = policy.check(check).expect("request validated");
        if !stored.inputs_stable {
            return result(D::NonReusable, R::InputsChangedDuringRun);
        }
        if receipt.has_unobservable_inputs()
            || context
                .environment
                .iter()
                .any(|e| e.secret_dependent || !e.observable || e.sha256.is_none())
        {
            return result(D::NonReusable, R::UnobservableInputs);
        }
        if fields.outcome != CheckOutcome::Passed || fields.exit_code != Some(0) {
            return result(D::NonReusable, R::UnsuccessfulRun);
        }
        if selected.independent_required && requirements.independent_of.is_none()
            || requirements
                .independent_of
                .as_ref()
                .is_some_and(|actor| contract_text("independent_of", actor).is_err())
            || requirements
                .independent_of
                .as_ref()
                .is_some_and(|actor| actor == &fields.actor)
        {
            return result(D::Invalidated, R::IndependentRunRequired);
        }
        if selected
            .native_platform
            .as_ref()
            .is_some_and(|p| p != &context.platform)
        {
            return result(D::Invalidated, R::NativePlatformRequired);
        }
        if fields.toolchains != context.toolchains
            || fields.platform != context.platform
            || fields.architecture != context.architecture
            || fields.features != context.features
            || fields.environment != context.environment
        {
            return result(D::Invalidated, R::ContextChanged);
        }
        for path in &selected.dependency_locks {
            if self.backend.file(path).ok().map(|f| f.sha256).as_ref()
                != fields.dependency_locks.get(path)
            {
                return result(D::Invalidated, R::InputsChanged);
            }
        }
        if fields.dependency_locks.len() != selected.dependency_locks.len()
            || self.backend.manifest(policy, check).ok().as_ref() != Some(&stored.manifest)
        {
            return result(D::Invalidated, R::InputsChanged);
        }
        if self.backend.artifacts(&receipt, selected).is_err() {
            return result(D::Invalidated, R::ArtifactsChanged);
        }
        result(D::Reused, R::Applicable)
    }
}

fn decision(
    disposition: VerificationDisposition,
    reason: VerificationReason,
    original: Option<VerificationReceipt>,
) -> VerificationDecision {
    VerificationDecision {
        disposition,
        reason,
        original,
    }
}
impl VerificationRequest {
    pub fn new(
        plan: PlanIdentity,
        actor: String,
        command: CheckCommand,
        policy: PolicyIdentity,
    ) -> ContractResult<Self> {
        contract_text("actor", &actor)?;
        if plan.fields().body_sha256.is_none() {
            return Err(ContractError::new(
                "plan.body_sha256",
                "verification requires an attributable Plan body",
            ));
        }
        Ok(Self {
            plan,
            actor,
            command,
            policy,
        })
    }
    pub fn plan(&self) -> &PlanIdentity {
        &self.plan
    }
    pub fn actor(&self) -> &str {
        &self.actor
    }
    pub fn command(&self) -> &CheckCommand {
        &self.command
    }
    pub fn policy(&self) -> &PolicyIdentity {
        &self.policy
    }
}
