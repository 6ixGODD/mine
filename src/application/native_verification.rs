//! Real-source/Root-authority bridge to the accepted verification kernel.
//! Context comes from the current runner, never from a previous receipt.
use super::graph_service::GraphService;
use super::native_evidence::{input, refused};
use super::verification_service::*;
use crate::domain::error::{MineResult, contract_version};
use crate::domain::graph::{FileIdentity, PlanIdentity, RepoPath, UtcTimestamp};
use crate::domain::lifecycle_operation::OperationId;
use crate::domain::verification_receipt::*;
use crate::infrastructure::verification_store::{FileVerificationStore, VerificationLocation};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

const DEFAULT_POLICY: &str = ".mine/verification-policy.json";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureInput {
    pub schema_version: u32,
    pub plan_id: String,
    pub actor: String,
    pub check: String,
    pub policy_path: Option<RepoPath>,
    pub context: VerificationContext,
    pub started_at: UtcTimestamp,
    pub release_operation_id: Option<OperationId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordInput {
    pub schema_version: u32,
    pub capture: CaptureFields,
    pub observation: RunObservation,
    pub policy_path: Option<RepoPath>,
    pub release_operation_id: Option<OperationId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckInput {
    pub schema_version: u32,
    pub plan_id: String,
    pub actor: String,
    pub check: String,
    pub context: VerificationContext,
    pub independent_of: Option<String>,
    pub release_operation_id: Option<OperationId>,
}

enum ObservedBackend {
    Plan(FileVerificationStore),
    Release(Box<super::release_service::retained::ReleaseVerification>),
}
impl ObservedBackend {
    fn load_policy(&self, path: &RepoPath) -> VerificationResult<VerificationPolicy> {
        match self {
            Self::Plan(backend) => backend.load_policy(path),
            Self::Release(backend) => backend.load_policy(path),
        }
    }
    fn evidence_root(&self) -> &Path {
        match self {
            Self::Plan(backend) => backend.root(),
            Self::Release(backend) => backend.evidence_root(),
        }
    }
    fn implementer(&self, graph: &GraphService<'_>, id: &str) -> MineResult<String> {
        match self {
            Self::Release(backend) => Ok(backend.owner().into()),
            Self::Plan(_) => Ok(graph
                .snapshot()?
                .workspace()
                .get(id)
                .ok_or_else(|| refused("verification subject missing"))?
                .owner
                .clone()),
        }
    }
}
impl VerificationBackend for ObservedBackend {
    fn manifest(
        &self,
        policy: &VerificationPolicy,
        check: &str,
    ) -> VerificationResult<InputManifest> {
        match self {
            Self::Plan(backend) => backend.manifest(policy, check),
            Self::Release(backend) => backend.manifest(policy, check),
        }
    }
    fn file(&self, path: &RepoPath) -> VerificationResult<FileIdentity> {
        match self {
            Self::Plan(backend) => backend.file(path),
            Self::Release(backend) => backend.file(path),
        }
    }
    fn plan_body(&self, plan: &PlanIdentity) -> VerificationResult<FileIdentity> {
        match self {
            Self::Plan(backend) => backend.plan_body(plan),
            Self::Release(backend) => backend.plan_body(plan),
        }
    }
    fn artifacts(
        &self,
        receipt: &VerificationReceipt,
        policy: &CheckPolicy,
    ) -> VerificationResult<()> {
        match self {
            Self::Plan(backend) => backend.artifacts(receipt, policy),
            Self::Release(backend) => backend.artifacts(receipt, policy),
        }
    }
    fn load(
        &self,
        plan: &PlanIdentity,
        id: &ReceiptId,
    ) -> VerificationResult<Option<StoredVerification>> {
        match self {
            Self::Plan(backend) => backend.load(plan, id),
            Self::Release(backend) => backend.load(plan, id),
        }
    }
    fn save(&self, evidence: &StoredVerification) -> VerificationResult<()> {
        match self {
            Self::Plan(backend) => backend.save(evidence),
            Self::Release(backend) => backend.save(evidence),
        }
    }
}

pub struct NativeVerification<'a, 'store> {
    graph: &'a GraphService<'store>,
    source: PathBuf,
}

impl<'a, 'store> NativeVerification<'a, 'store> {
    pub fn new(graph: &'a GraphService<'store>, source: &Path) -> MineResult<Self> {
        Ok(Self {
            graph,
            source: source.canonicalize()?,
        })
    }

    /// Runner requests are Root-owned mutable outputs, not checkout evidence.
    /// Validate their reserved shape before any content read. The shared input
    /// helper deliberately keeps its caller-selected source semantics.
    fn control_input<T: DeserializeOwned>(&self, path: &RepoPath) -> MineResult<(String, T)> {
        let parts = path.as_str().split('/').collect::<Vec<_>>();
        if parts.len() < 5 || parts[0] != ".mine" || parts[1] != "build" {
            return Err(refused(
                "verification request must be a Root per-Plan output-class file",
            ));
        }
        crate::domain::plan_id::CanonicalPlanId::parse(parts[2])
            .map_err(|e| refused(e.to_string()))?;
        Ok((parts[2].into(), input(self.graph.repo_root(), path)?))
    }

    fn request_service(
        &self,
        id: &str,
        namespace: &str,
        release_operation_id: Option<&OperationId>,
    ) -> MineResult<(PlanIdentity, VerificationService<ObservedBackend>)> {
        if id != namespace {
            return Err(refused(
                "verification request namespace differs from its exact Plan ID",
            ));
        }
        let (plan, service) = self.service(id, release_operation_id)?;
        if plan.fields().plan_id != namespace {
            return Err(refused("verification request differs from registered Plan"));
        }
        Ok((plan, service))
    }

    fn service(
        &self,
        id: &str,
        release_operation_id: Option<&OperationId>,
    ) -> MineResult<(PlanIdentity, VerificationService<ObservedBackend>)> {
        if let Some(operation_id) = release_operation_id {
            let (plan, backend) = super::release_service::retained::ReleaseVerification::open(
                self.graph.repo_root(),
                operation_id,
                id,
            )
            .map_err(|error| refused(error.to_string()))?;
            let service = VerificationService::new(
                ObservedBackend::Release(Box::new(backend)),
                plan.fields().repository_id.clone(),
                plan.fields().workspace_id.clone(),
            )
            .map_err(|error| refused(error.to_string()))?;
            return Ok((plan, service));
        }
        let snapshot = self.graph.snapshot()?;
        let workspace = snapshot.workspace();
        let node = workspace
            .get(id)
            .ok_or_else(|| crate::domain::error::MineError::PlanNotFound { plan_id: id.into() })?;
        let root = self.graph.repo_root().canonicalize()?;
        if self.source != root {
            let target = crate::infrastructure::operation_store::regular_path(
                &root,
                &RepoPath::new(node.worktree_path.clone()).map_err(|e| refused(e.to_string()))?,
            )?
            .canonicalize()?;
            if self.source != target
                || !snapshot.worktrees()?.iter().any(|worktree| {
                    !worktree.bare
                        && !worktree.detached
                        && !worktree.prunable
                        && worktree.branch.as_deref() == Some(node.branch.as_str())
                        && Path::new(&worktree.path).canonicalize().ok().as_ref() == Some(&target)
                })
            {
                return Err(refused(
                    "verification source is not this Plan's registered checkout",
                ));
            }
        }
        let authority = self.graph.effective_authority(id)?;
        let plan = authority.plan().clone();
        // This constructor refuses absent historical body attribution. Only a
        // genuine recorded baseline/attestation may supply it, not this adapter.
        let backend =
            FileVerificationStore::new(&self.source, &root).map_err(|e| refused(e.to_string()))?;
        let service = VerificationService::new(
            ObservedBackend::Plan(backend),
            plan.fields().repository_id.clone(),
            workspace.workspace_id.clone(),
        )
        .map_err(|e| refused(e.to_string()))?;
        Ok((plan, service))
    }

    fn policy(
        service: &VerificationService<ObservedBackend>,
        selector: Option<&RepoPath>,
    ) -> MineResult<VerificationPolicy> {
        let default = RepoPath::new(DEFAULT_POLICY).expect("constant policy path");
        service
            .backend()
            .load_policy(selector.unwrap_or(&default))
            .map_err(|e| refused(e.to_string()))
    }

    pub fn capture(&self, path: &RepoPath) -> MineResult<CaptureFields> {
        let (namespace, request): (_, CaptureInput) = self.control_input(path)?;
        contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
        let (plan, service) = self.request_service(
            &request.plan_id,
            &namespace,
            request.release_operation_id.as_ref(),
        )?;
        let policy = Self::policy(&service, request.policy_path.as_ref())?;
        let selected = policy
            .check(&request.check)
            .map_err(|e| refused(e.to_string()))?;
        let check_request = VerificationRequest::new(
            plan,
            request.actor,
            selected.command.clone(),
            policy.identity(),
        )
        .map_err(|e| refused(e.to_string()))?;
        service
            .capture(
                check_request,
                policy,
                &request.check,
                request.context,
                request.started_at,
            )
            .map(|capture| capture.export())
            .map_err(|e| refused(e.to_string()))
    }

    pub fn record(&self, path: &RepoPath) -> MineResult<VerificationDecision> {
        let (namespace, request): (_, RecordInput) = self.control_input(path)?;
        contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
        let (plan, service) = self.request_service(
            &request.capture.plan.fields().plan_id,
            &namespace,
            request.release_operation_id.as_ref(),
        )?;
        let policy = Self::policy(&service, request.policy_path.as_ref())?;
        if request.capture.plan != plan || request.capture.policy != policy {
            return Err(refused(
                "capture differs from current Root Plan or live policy",
            ));
        }
        let capture = service
            .import_capture(request.capture)
            .map_err(|e| refused(e.to_string()))?;
        service
            .record(capture, request.observation)
            .map_err(|e| refused(e.to_string()))
    }

    pub fn check(
        &self,
        receipt: &RepoPath,
        path: &RepoPath,
        policy_path: Option<&RepoPath>,
    ) -> MineResult<VerificationDecision> {
        let (namespace, request): (_, CheckInput) = self.control_input(path)?;
        contract_version(request.schema_version).map_err(|e| refused(e.to_string()))?;
        let (plan, service) = self.request_service(
            &request.plan_id,
            &namespace,
            request.release_operation_id.as_ref(),
        )?;
        let policy = Self::policy(&service, policy_path)?;
        let selected = policy
            .check(&request.check)
            .map_err(|e| refused(e.to_string()))?;
        if selected.independent_required {
            let owner = service
                .backend()
                .implementer(self.graph, &request.plan_id)?;
            crate::domain::error::contract_text("implementer", &owner)
                .map_err(|e| refused(e.to_string()))?;
            if request.independent_of.as_ref() != Some(&owner) {
                return Err(refused(
                    "independent check must bind the actual graph implementer",
                ));
            }
        }
        let id = receipt
            .as_str()
            .rsplit('/')
            .next()
            .and_then(|name| name.strip_suffix(".json"))
            .ok_or_else(|| refused("receipt filename must be its canonical ID.json"))?;
        let id = ReceiptId::parse(id).map_err(|e| refused(e.to_string()))?;
        if VerificationLocation::new(service.backend().evidence_root(), &plan, &id)?.relative_path()
            != receipt
        {
            return Err(refused(
                "receipt path is not this Plan's exact evidence path",
            ));
        }
        let check_request = VerificationRequest::new(
            plan,
            request.actor,
            selected.command.clone(),
            policy.identity(),
        )
        .map_err(|e| refused(e.to_string()))?;
        let mut decision = service.check(
            &check_request,
            &policy,
            &request.check,
            &request.context,
            Some(&id),
            &ReuseRequirements {
                independent_of: request.independent_of.clone(),
            },
        );
        // The accepted generic kernel conservatively stops at unknown inputs.
        // A native current-context check must still expose known observable
        // drift (notably the terminal graph delta), never hide it as a possible
        // fresh-run exception. This adapter only invalidates; it cannot grant
        // reuse or execute/record another check.
        if decision.disposition == VerificationDisposition::NonReusable
            && decision.reason == VerificationReason::UnobservableInputs
        {
            let fields = decision
                .original
                .as_ref()
                .expect("observed receipt")
                .fields();
            let reason = if fields.toolchains != request.context.toolchains
                || fields.platform != request.context.platform
                || fields.architecture != request.context.architecture
                || fields.features != request.context.features
                || fields.environment != request.context.environment
            {
                Some(VerificationReason::ContextChanged)
            } else if selected
                .native_platform
                .as_ref()
                .is_some_and(|p| p != &request.context.platform)
            {
                Some(VerificationReason::NativePlatformRequired)
            } else if request
                .independent_of
                .as_ref()
                .is_some_and(|actor| actor == &fields.actor)
            {
                Some(VerificationReason::IndependentRunRequired)
            } else {
                match input::<StoredVerification>(service.backend().evidence_root(), receipt) {
                    Ok(stored)
                        if stored.validate().is_ok()
                            && stored.receipt == *decision.original.as_ref().expect("receipt")
                            && stored.receipt.fields().plan == *check_request.plan()
                            && stored.receipt.fields().receipt_id == id
                            && stored.policy == policy
                            && stored.check == request.check
                            && stored.inputs_stable
                            && stored.manifest.digest() == fields.input_manifest_sha256 =>
                    {
                        if service
                            .backend()
                            .manifest(&policy, &request.check)
                            .ok()
                            .as_ref()
                            != Some(&stored.manifest)
                            || fields.dependency_locks.len() != selected.dependency_locks.len()
                            || selected.dependency_locks.iter().any(|path| {
                                service.backend().file(path).ok().map(|v| v.sha256).as_ref()
                                    != fields.dependency_locks.get(path)
                            })
                        {
                            Some(VerificationReason::InputsChanged)
                        } else if service
                            .backend()
                            .artifacts(decision.original.as_ref().expect("receipt"), selected)
                            .is_err()
                        {
                            Some(VerificationReason::ArtifactsChanged)
                        } else {
                            None
                        }
                    }
                    _ => Some(VerificationReason::EvidenceUnavailable),
                }
            };
            if let Some(reason) = reason {
                decision.disposition = VerificationDisposition::Invalidated;
                decision.reason = reason;
            }
        }
        Ok(decision)
    }
}
