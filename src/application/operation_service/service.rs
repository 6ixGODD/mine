//! Typed closure progress. Skills perform merges/checks/cleanup; this service
//! validates fresh evidence and records stages, not arbitrary effects.
use super::*;
pub use crate::application::native_evidence::CommittedFilesProof;
use crate::domain::{
    graph::{FileIdentity, PlanWorkspace},
    lifecycle_operation::*,
};
use crate::infrastructure::{
    graph_checkpoint::digest,
    graph_snapshot::resolve_contained,
    operation_store::{JournalRecord, OperationStore, read_bounded},
};
use std::path::Path;

/// Whole current-gate coverage/applicability is admitted by the concrete adapter
/// before this per-file hook. This hook revalidates append-only historical file
/// identities; historical claims alone are never current coverage authority.
pub trait VerificationEvidenceProvider {
    fn validate(
        &self,
        evidence: &FileIdentity,
        operation: &LifecycleOperation,
    ) -> OperationResult<()>;
    fn writer_stopped(
        &self,
        plan: &crate::domain::graph::PlanIdentity,
        committed: Option<&CommittedFilesProof>,
    ) -> OperationResult<()>;
}
pub trait OperationEvidenceProvider {
    fn observe(
        &mut self,
        operation: &LifecycleOperation,
        binding: &ClosureBinding,
    ) -> OperationResult<()>;
}
pub struct OperationService<'a, 'lock> {
    store: &'a OperationStore<'lock>,
}
impl<'a, 'lock> OperationService<'a, 'lock> {
    pub fn new(store: &'a OperationStore<'lock>) -> Self {
        Self { store }
    }
    pub fn query(&self, query: &OperationQuery) -> OperationResult<LifecycleOperation> {
        let record = self.store.query(query.operation_id())?;
        if record.operation.fields().repository_id != query.repository_id()
            || record.operation.fields().workspace_id != query.workspace_id()
        {
            return Err(OperationError::Refused("foreign query binding"));
        }
        Ok(record.operation)
    }
    pub fn begin(
        &self,
        operation: LifecycleOperation,
        binding: ClosureBinding,
        provider: &mut impl OperationEvidenceProvider,
    ) -> OperationResult<LifecycleOperation> {
        if operation.fields().kind == OperationKind::GraphCheckpoint {
            return Err(OperationError::Refused(
                "checkpoint requires native adapter",
            ));
        }
        provider.observe(&operation, &binding)?;
        Ok(self
            .store
            .begin(&JournalRecord {
                schema_version: 1,
                operation,
                closure: Some(binding),
                checkpoint: None,
            })?
            .operation)
    }
    pub fn stage(
        &self,
        query: &OperationQuery,
        expected: u64,
        next: LifecycleOperation,
        provider: &mut impl OperationEvidenceProvider,
    ) -> OperationResult<LifecycleOperation> {
        let old = self.query(query)?;
        let binding = self
            .store
            .query(query.operation_id())?
            .closure
            .ok_or(OperationError::Journal)?;
        if old == next {
            provider.observe(&old, &binding)?;
            return Ok(old);
        }
        if old.fields().stage_revision != expected {
            return Err(OperationError::Revision {
                expected,
                actual: old.fields().stage_revision,
            });
        }
        validate_progress(&old, &next)?;
        provider.observe(&next, &binding)?;
        Ok(self
            .store
            .advance(
                expected,
                &JournalRecord {
                    schema_version: 1,
                    operation: next,
                    closure: Some(binding),
                    checkpoint: None,
                },
            )?
            .operation)
    }
    pub fn barriers(&self) -> OperationResult<BarrierFacts> {
        self.store.barriers()
    }
}

/// Real file/Git/graph evidence, plus a required receipt verifier supplied by
/// its owning integration boundary. There is no always-valid production seam.
pub struct FilesystemOperationEvidence<'a, V> {
    root: &'a Path,
    verification: V,
}
impl<'a, V: VerificationEvidenceProvider> FilesystemOperationEvidence<'a, V> {
    pub fn new(root: &'a Path, verification: V) -> Self {
        Self { root, verification }
    }
}
impl<V: VerificationEvidenceProvider> OperationEvidenceProvider
    for FilesystemOperationEvidence<'_, V>
{
    fn observe(
        &mut self,
        operation: &LifecycleOperation,
        binding: &ClosureBinding,
    ) -> OperationResult<()> {
        let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
        let f = operation.fields();
        if f.kind == OperationKind::Release {
            return Err(OperationError::Refused(
                "Release requires the native retained-source and candidate verification composition",
            ));
        }
        if f.kind == OperationKind::RejectedHandoff
            && (f.integrated_commit.is_some()
                || f.cleanup.is_some()
                || f.checkpoint.is_some()
                || !f.verification_evidence.is_empty())
        {
            return Err(OperationError::Refused(
                "rejection handoff cannot substitute acceptance/integration/checkpoint facts",
            ));
        }
        let config_path = self.root.join(".mine/config.toml");
        let config = crate::domain::config::MineConfig::parse(
            &config_path,
            &String::from_utf8(read_bounded(&config_path)?).map_err(|_| OperationError::Journal)?,
        )?;
        let graph_path = resolve_contained(
            self.root,
            &crate::domain::graph::RepoPath::new(config.graph.source)?,
        )?;
        let ws: PlanWorkspace = toml::from_str(
            &String::from_utf8(read_bounded(&graph_path)?).map_err(|_| OperationError::Journal)?,
        )
        .map_err(|_| OperationError::Journal)?;
        if config.repository_id != f.repository_id
            || ws.workspace_id != f.workspace_id
            || ws.revision < f.observed_graph_revision
        {
            return Err(OperationError::Refused(
                "repository/workspace/revision evidence differs",
            ));
        }
        if f.stage == initial_stage(f.kind) && ws.revision != f.observed_graph_revision {
            return Err(OperationError::Refused("stale initial graph revision"));
        }
        let mut git = crate::infrastructure::graph_checkpoint::checkpoint_git(self.root, None)?;
        let identity = git.run(
            &[
                "rev-parse",
                "--path-format=absolute",
                "--show-toplevel",
                "--absolute-git-dir",
                "--git-common-dir",
            ],
            b"",
        )?;
        let identity = std::str::from_utf8(&identity)
            .map_err(|_| OperationError::Evidence("Root identity encoding"))?
            .lines()
            .collect::<Vec<_>>();
        if identity.len() != 3
            || Path::new(identity[0]).canonicalize()? != self.root.canonicalize()?
            || Path::new(identity[1]).canonicalize()? != Path::new(identity[2]).canonicalize()?
        {
            return Err(OperationError::Refused(
                "closure requires actual Root worktree",
            ));
        }
        if git.text(&["symbolic-ref", "-q", "HEAD"])?
            != format!("refs/heads/{}", config.branches.integration)
        {
            return Err(OperationError::Refused(
                "closure requires Root integration checkout",
            ));
        }
        if !git
            .run(
                &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
                b"",
            )?
            .is_empty()
        {
            return Err(OperationError::Refused("closure requires clean Root"));
        }
        if let Some(plan) = &f.plan {
            let p = plan.fields();
            let node = ws
                .get(&p.plan_id)
                .ok_or(OperationError::Evidence("Plan missing from graph"))?;
            if node.path != p.plan_path.as_str()
                || node.branch != p.branch
                || node.lineage_id != p.lineage_id
                || p.body_sha256.as_ref()
                    != Some(&digest(&read_bounded(&resolve_contained(
                        self.root,
                        &p.plan_path,
                    )?)?))
                || f.registered_worktree.as_ref().map(|v| v.as_str())
                    != Some(node.worktree_path.as_str())
            {
                return Err(OperationError::Refused(
                    "Plan/body/worktree binding differs",
                ));
            }
            let mut committed_review = None;
            if let Some(review) = &f.review {
                let report_root = if f.kind == OperationKind::RejectedHandoff {
                    use crate::infrastructure::graph_checkpoint::plan_git::{
                        self, CheckoutState, RefState,
                    };
                    let location = f
                        .registered_worktree
                        .as_ref()
                        .ok_or(OperationError::Journal)?;
                    let absolute = resolve_contained(self.root, location)?;
                    let inventory = git.worktrees()?;
                    let selected = inventory
                        .iter()
                        .find(|w| {
                            Path::new(&w.path).canonicalize().ok().as_ref() == Some(&absolute)
                        })
                        .and_then(|w| w.branch.as_deref())
                        .ok_or(OperationError::Refused(
                            "rejection lineage checkout missing or detached",
                        ))?;
                    let legacy_compensation =
                        if let ClosureBinding::RejectedHandoff { compensation } = binding {
                            ws.get(&compensation.fields().plan_id).is_some_and(|comp| {
                                node.status == crate::domain::status::PlanStatus::Rejected
                                    && node.compensating_plan == comp.id
                                    && comp.status == crate::domain::status::PlanStatus::Ready
                                    && selected == comp.branch
                                    && comp.worktree_path == node.worktree_path
                            })
                        } else {
                            false
                        };
                    if selected != p.branch && !legacy_compensation {
                        return Err(OperationError::Refused(
                            "rejection source is not exact declared lineage branch",
                        ));
                    }
                    let common = Path::new(identity[2]).canonicalize()?;
                    let checkout =
                        plan_git::checkout(self.root, &common, location, selected, &mut git)?;
                    if !matches!(checkout,CheckoutState::Present {proof} if proof.head==review.reviewed_tip)
                        || plan_git::reference(&mut git, &p.branch)?
                            != (RefState::Present {
                                tip: review.reviewed_tip.clone(),
                            })
                    {
                        return Err(OperationError::Refused(
                            "rejection source/ref is not exact stopped reviewed tip",
                        ));
                    }
                    self.root.canonicalize()?
                } else if f.integrated_commit.is_none() {
                    let worktree = f
                        .registered_worktree
                        .as_ref()
                        .ok_or(OperationError::Journal)?;
                    let path = resolve_contained(self.root, worktree)?;
                    let inventory = git.worktrees()?;
                    if !inventory.iter().any(|w| {
                        Path::new(&w.path).canonicalize().ok().as_ref() == Some(&path)
                            && (f.stage != initial_stage(f.kind)
                                || w.branch.as_deref() == Some(p.branch.as_str()))
                    }) {
                        return Err(OperationError::Refused(
                            "review checkout does not match registered inventory",
                        ));
                    }
                    let mut checkout =
                        crate::infrastructure::graph_checkpoint::checkpoint_git(&path, None)?;
                    if !checkout
                        .run(
                            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
                            b"",
                        )?
                        .is_empty()
                    {
                        return Err(OperationError::Refused("review checkout is not clean"));
                    }
                    path
                } else {
                    self.root.canonicalize()?
                };
                verify_file(&report_root, &review.report)?;
                if crate::domain::plan_id::validate_report_path(
                    p.plan_path.as_str(),
                    review.report.path.as_str(),
                    "review",
                )
                .is_err()
                {
                    return Err(OperationError::Refused("foreign review path"));
                }
                let report = String::from_utf8(read_bounded(&resolve_contained(
                    &report_root,
                    &review.report.path,
                )?)?)
                .map_err(|_| OperationError::Journal)?;
                if f.kind == OperationKind::RejectedHandoff {
                    let head = crate::domain::graph::CommitId::new(git.text(&[
                        "rev-parse",
                        "--verify",
                        "HEAD^{commit}",
                    ])?)?;
                    let root_plan = read_bounded(&resolve_contained(self.root, &p.plan_path)?)?;
                    committed_review =
                        Some(crate::application::native_evidence::prove_committed_files(
                            self.root,
                            &head,
                            &[
                                review.report.clone(),
                                FileIdentity {
                                    path: p.plan_path.clone(),
                                    bytes: root_plan.len() as u64,
                                    sha256: p.body_sha256.clone().ok_or(
                                        OperationError::Evidence("Plan body identity missing"),
                                    )?,
                                },
                            ],
                        )?);
                }
                validate_review_verdict(report.as_bytes(), f.kind == OperationKind::Acceptance)?;
                git.validate_commits(&[review.reviewed_tip.as_str().into()])?;
                if let Some(integrated) = &f.integrated_commit {
                    git.validate_commits(&[integrated.as_str().into()])?;
                    let head = git.text(&["rev-parse", "HEAD"])?;
                    if !git.is_ancestor(review.reviewed_tip.as_str(), integrated.as_str())?
                        || !git.is_ancestor(integrated.as_str(), &head)?
                    {
                        return Err(OperationError::Refused(
                            "reviewed tip/integration not Root HEAD ancestry",
                        ));
                    }
                } else if git.text(&[
                    "rev-parse",
                    "--verify",
                    &format!("{}^{{commit}}", p.branch),
                ])? != review.reviewed_tip.as_str()
                {
                    return Err(OperationError::Refused("Plan tip changed since review"));
                }
            } else {
                return Err(OperationError::Evidence("closure review evidence missing"));
            }
            self.verification
                .writer_stopped(plan, committed_review.as_ref())?;
            if let OperationStage::Acceptance(stage) = f.stage {
                use crate::domain::status::PlanStatus;
                if (stage == AcceptanceStage::Accepted && node.status != PlanStatus::Accepted)
                    || (stage != AcceptanceStage::Accepted
                        && node.status != PlanStatus::Implemented)
                {
                    return Err(OperationError::Refused(
                        "acceptance graph status/effective writer differs",
                    ));
                }
                if matches!(
                    stage,
                    AcceptanceStage::Verified
                        | AcceptanceStage::Cleaned
                        | AcceptanceStage::Accepted
                ) && f.verification_evidence.is_empty()
                {
                    return Err(OperationError::Evidence(
                        "no applicable integrated verification evidence",
                    ));
                }
                if matches!(stage, AcceptanceStage::Cleaned | AcceptanceStage::Accepted) {
                    let worktree = f
                        .registered_worktree
                        .as_ref()
                        .ok_or(OperationError::Journal)?;
                    let absolute = self.root.join(worktree.as_str());
                    if absolute.try_exists()?
                        || git
                            .worktrees()?
                            .iter()
                            .any(|w| Path::new(&w.path) == absolute)
                        || git
                            .refs()?
                            .iter()
                            .any(|r| r.name == format!("refs/heads/{}", p.branch))
                    {
                        return Err(OperationError::Refused(
                            "owned worktree/branch cleanup not observed",
                        ));
                    }
                }
            }
            if let ClosureBinding::RejectedHandoff { compensation } = binding {
                let cp = compensation.fields();
                crate::domain::plan_id::validate_compensation_replacement(&p.plan_id, &cp.plan_id)?;
                if cp.repository_id != p.repository_id
                    || cp.workspace_id != p.workspace_id
                    || cp.lineage_id != p.lineage_id
                    || cp.body_sha256.as_ref()
                        != Some(&digest(&read_bounded(&resolve_contained(
                            self.root,
                            &cp.plan_path,
                        )?)?))
                {
                    return Err(OperationError::Refused(
                        "compensation body/lineage binding differs",
                    ));
                }
                if let OperationStage::RejectedHandoff(stage) = f.stage {
                    use crate::domain::status::PlanStatus;
                    if stage != RejectionStage::Intent
                        && (node.status != PlanStatus::Rejected
                            || node.compensating_plan != cp.plan_id)
                    {
                        return Err(OperationError::Refused("rejection not committed as pinned"));
                    }
                    if !matches!(stage, RejectionStage::Intent | RejectionStage::Rejected) {
                        let comp = ws
                            .get(&cp.plan_id)
                            .ok_or(OperationError::Evidence("compensation not registered"))?;
                        if comp.path != cp.plan_path.as_str()
                            || comp.branch != cp.branch
                            || comp.lineage_id != cp.lineage_id
                            || comp.worktree_path != node.worktree_path
                        {
                            return Err(OperationError::Refused(
                                "compensation registration differs",
                            ));
                        }
                        if matches!(stage, RejectionStage::Released | RejectionStage::Complete)
                            && comp.status != PlanStatus::Ready
                        {
                            return Err(OperationError::Refused(
                                "compensation is not released READY",
                            ));
                        }
                        if matches!(
                            stage,
                            RejectionStage::Rewired
                                | RejectionStage::Released
                                | RejectionStage::Complete
                        ) && ws.plans.iter().any(|successor| {
                            successor.id != cp.plan_id
                                && successor.hard_predecessors.contains(&p.plan_id)
                        }) {
                            return Err(OperationError::Refused("successor rewire not observed"));
                        }
                    }
                }
            }
        }
        for evidence in &f.verification_evidence {
            verify_file(self.root, evidence)?;
            self.verification.validate(evidence, operation)?;
        }
        Ok(())
    }
}
fn verify_file(root: &Path, evidence: &FileIdentity) -> OperationResult<()> {
    let bytes = read_bounded(&resolve_contained(root, &evidence.path)?)?;
    if bytes.len() as u64 != evidence.bytes || digest(&bytes) != evidence.sha256 {
        return Err(OperationError::Refused("artifact bytes/digest differs"));
    }
    Ok(())
}

/// The same strict existing verdict rule is used for live and retained source
/// observations. This is report consistency, not authentication or a new review.
pub(crate) fn validate_review_verdict(body: &[u8], accepted: bool) -> OperationResult<()> {
    let report = std::str::from_utf8(body).map_err(|_| OperationError::Journal)?;
    let verdict = if accepted {
        "Verdict: ACCEPTED"
    } else {
        "Verdict: REJECTED"
    };
    let verdicts = report
        .lines()
        .map(|line| line.trim().trim_matches('*'))
        .filter(|line| line.starts_with("Verdict:"))
        .collect::<Vec<_>>();
    if verdicts.as_slice() != [verdict] {
        return Err(OperationError::Refused(
            "review is not independently positive/negative as requested",
        ));
    }
    Ok(())
}
