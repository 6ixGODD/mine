//! A narrow, same-identity Git/graph protocol. No cross-resource atomicity is
//! claimed. All methods require the caller's already-held Root graph lock.
#[path = "prepare_effects.rs"]
mod prepare_effects;
#[path = "restore_view.rs"]
mod restore_view;
use super::{git, *};
use crate::domain::{
    graph::{CommitId, PlanWorkspace},
    lifecycle_operation::*,
};
use crate::infrastructure::{
    file_lock::FileLock,
    git_evidence::GitRequest,
    operation_store::{
        JournalRecord, OperationStore, RepositoryJournal, atomic_replace, read_bounded,
    },
    toml_store::render_markdown,
};
pub use restore_view::GraphRenderOutcome;
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

pub struct CheckpointKernel<'a> {
    paths: GraphCheckpointPaths,
    config: MineConfig,
    config_hash: crate::domain::graph::Sha256,
    store: OperationStore<'a>,
    index_proofs: std::cell::RefCell<Vec<IndexWitness>>,
}

/// Self-contained/flags evidence for exact bytes under one request and native
/// repository interpretation. Config, attributes and live bytes remain fresh.
struct IndexWitness {
    root: PathBuf,
    common: PathBuf,
    bytes: Vec<u8>,
    request: Option<std::sync::Arc<()>>,
}
impl IndexWitness {
    fn same_request(&self) -> bool {
        match (
            &self.request,
            crate::infrastructure::git_evidence::request_identity(),
        ) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, &b),
            _ => false,
        }
    }
}

/// Facts from one fresh safety phase, not reusable across observer/mutation
/// boundaries. Only helpers in that same phase consume these observations.
struct PhaseObservation {
    head: CommitId,
    index_sha256: crate::domain::graph::Sha256,
    clean: bool,
}

/// A checked immutable candidate in this request. A later phase must still
/// freshly refuse replacement refs and check Root/index/worktree facts.
struct CheckedCandidate {
    binding: crate::domain::graph::Sha256,
    commit: CommitId,
    tree: CommitId,
    request: Option<std::sync::Arc<()>>,
}

impl CheckedCandidate {
    fn matches(&self, record: &JournalRecord) -> OperationResult<bool> {
        let current = crate::infrastructure::git_evidence::request_identity();
        let same_request = match (&self.request, &current) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        };
        let runtime = runtime(record)?;
        Ok(same_request
            && runtime.candidate_commit.as_ref() == Some(&self.commit)
            && runtime.candidate_tree.as_ref() == Some(&self.tree)
            && super::validation::RuntimeValidationWitness::binding(runtime, &record.operation)?
                == self.binding)
    }
}
impl<'a> CheckpointKernel<'a> {
    /// First-workspace admission derives its identity only from validated Root
    /// setup. Never materialize a seed graph just to construct the store.
    pub fn initial_under_lock(
        root: &Path,
        lock: &'a FileLock,
        expected_revision: u64,
        actor: &str,
        uuids: &dyn crate::domain::ports::UuidSource,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<(Self, JournalRecord)> {
        let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
        crate::domain::error::contract_text("operation.actor", actor)?;
        if expected_revision != 0 {
            return Err(OperationError::Revision {
                expected: expected_revision,
                actual: 0,
            });
        }
        let root = root.canonicalize()?;
        let config_path = crate::infrastructure::operation_store::regular_path(
            &root,
            &RepoPath::new(".mine/config.toml")?,
        )?;
        let config_bytes = read_bounded(&config_path)?;
        let config = MineConfig::parse(
            &config_path,
            std::str::from_utf8(&config_bytes).map_err(|_| OperationError::Journal)?,
        )?;
        let paths = GraphCheckpointPaths::from_config(&root, &config)?;
        let (source, view) = paths.resolve()?;
        if optional_text(&source)?.is_some() || optional_text(&view)?.is_some() {
            return Err(OperationError::Refused("initial graph pair is not absent"));
        }
        let journal = RepositoryJournal::under_lock(&root, &config.repository_id, lock)?;
        let observed = journal.observations()?;
        let predecessors = crate::infrastructure::operation_store::history::predecessors(
            &journal, &observed, None,
        )?;
        let workspace_id = uuids.new_repository_id();
        if observed
            .iter()
            .any(|record| record.operation.fields().workspace_id == workspace_id)
        {
            return Err(OperationError::Refused(
                "workspace UUID already appeared in repository history",
            ));
        }
        let store = OperationStore::under_lock(&root, &config.repository_id, &workspace_id, lock)?;
        let kernel = Self {
            paths,
            config,
            config_hash: digest(&config_bytes),
            store,
            index_proofs: Default::default(),
        };
        let mut git = git::request(&root, None)?;
        let (common_dir, index_path, original_head) = kernel.identity(&mut git)?;
        kernel.supported_index(&mut git, &index_path, &common_dir)?;
        kernel.attributes(&mut git)?;
        if lock_path(&index_path).try_exists()?
            || !git
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
            || !git
                .run_raw(
                    &[
                        "diff-index",
                        "--cached",
                        "--quiet",
                        original_head.as_str(),
                        "--",
                    ],
                    b"",
                )?
                .status
                .success()
        {
            return Err(OperationError::Refused("initial Root/index is not clean"));
        }
        for path in [&kernel.paths.source, &kernel.paths.view] {
            if !git
                .run(
                    &["ls-tree", "-z", original_head.as_str(), "--", path.as_str()],
                    b"",
                )?
                .is_empty()
                || !git
                    .run(&["ls-files", "-z", "--", path.as_str()], b"")?
                    .is_empty()
            {
                return Err(OperationError::Refused(
                    "initial graph exists in HEAD/index",
                ));
            }
        }
        let marker_path = RepoPath::new(kernel.config.design.marker.clone())?;
        let marker_file =
            crate::infrastructure::operation_store::regular_path(&root, &marker_path)?;
        let marker = crate::domain::design_marker::DesignMarker::parse(
            &marker_file,
            &text_file(&marker_file)?,
        )?;
        if marker.schema_version != crate::domain::design_marker::DesignMarker::SCHEMA_VERSION
            || marker.managed_by != crate::domain::design_marker::DesignMarker::MANAGED_BY
            || marker.repository_id != kernel.config.repository_id
        {
            return Err(OperationError::Refused("initial Design ownership mismatch"));
        }
        for path in [
            RepoPath::new(".mine/config.toml")?,
            marker_path,
            RepoPath::new(kernel.config.design.root.clone())?,
        ] {
            let actual = crate::infrastructure::operation_store::regular_path(&root, &path)?;
            read_bounded(&actual)?;
            let entry = git.text(&["ls-tree", original_head.as_str(), "--", path.as_str()])?;
            if !entry.starts_with("100644 blob ")
                || !entry.ends_with(&format!("\t{}", path.as_str()))
            {
                return Err(OperationError::Refused(
                    "initial setup is not committed regular files",
                ));
            }
        }
        let baseline = oid(&git.text(&[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{}^{{commit}}", kernel.config.branches.stable),
        ])?)?;
        git.validate_commits(&[baseline.as_str().into()])?;
        if !git.is_ancestor(baseline.as_str(), original_head.as_str())? {
            return Err(OperationError::Refused(
                "initial Root is not based on actual stable",
            ));
        }
        let candidate = PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 1,
            project_id: kernel.config.repository_id.clone(),
            workspace_id: workspace_id.clone(),
            stable_branch: kernel.config.branches.stable.clone(),
            integration_branch: kernel.config.branches.integration.clone(),
            stable_baseline_commit: baseline.as_str().into(),
            design_root: kernel.config.design.root.clone(),
            ephemeral_workspace: kernel.config.plan.ephemeral,
            purge_before_stable_release: kernel.config.plan.purge_before_stable_release,
            plans: Vec::new(),
        };
        crate::domain::validation::validate_with_worktrees_root(
            &candidate,
            &kernel.config.plan.worktrees_root,
        )?;
        let original_index = read_bounded(&index_path)?;
        let checkpoint = CheckpointFacts::new(CheckpointFields {
            schema_version: 1,
            stage: CheckpointStage::Intent,
            revision_before: 0,
            revision_after: None,
            source_sha256: None,
            view_sha256: None,
            original_head,
            original_index_sha256: digest(&original_index),
            candidate_tree: None,
            candidate_commit: None,
            generated_index_sha256: None,
            owned_lock_sha256: None,
        })?;
        let record = JournalRecord {
            schema_version: 1,
            operation: LifecycleOperation::new(OperationFields {
                schema_version: 1,
                operation_id: OperationId::generate(),
                stage_revision: 0,
                kind: OperationKind::GraphCheckpoint,
                stage: OperationStage::GraphCheckpoint(CheckpointStage::Intent),
                outcome: OperationOutcome::Pending,
                repository_id: kernel.config.repository_id.clone(),
                workspace_id,
                actor: actor.into(),
                observed_graph_revision: 0,
                plan: None,
                registered_worktree: None,
                review: None,
                integrated_commit: None,
                verification_evidence: Vec::new(),
                cleanup: None,
                checkpoint: Some(checkpoint),
                diagnostics: Vec::new(),
            })?,
            closure: None,
            checkpoint: Some(CheckpointRuntime {
                root,
                common_dir,
                index_path,
                integration_ref: format!("refs/heads/{}", kernel.config.branches.integration),
                config_sha256: kernel.config_hash.clone(),
                source_path: kernel.paths.source.clone(),
                view_path: kernel.paths.view.clone(),
                before: GraphBefore::Absent { predecessors },
                source_after: toml::to_string(&candidate).map_err(|_| OperationError::Journal)?,
                view_after: render_markdown(&candidate)?,
                original_index,
                generated_index: None,
                candidate_tree: None,
                candidate_commit: None,
                cas_attempted: false,
                lock_claimed: false,
                lock_sha256: None,
                plan_git: PlanGitEffects::None,
            }),
        };
        crate::infrastructure::git_evidence::check_request_budget()?;
        let record = kernel.store.begin(&record)?;
        crate::infrastructure::git_evidence::check_request_budget()?;
        observer.reached(Boundary::Intent)?;
        Ok((kernel, record))
    }

    /// Resume an explicit initial intent before either graph file exists. Its
    /// UUID is journal authority, never a caller replacement or a new UUID.
    pub fn initial_intent_under_lock(
        root: &Path,
        lock: &'a FileLock,
        id: &OperationId,
    ) -> OperationResult<Self> {
        let config_path = crate::infrastructure::operation_store::regular_path(
            root,
            &RepoPath::new(".mine/config.toml")?,
        )?;
        let bytes = read_bounded(&config_path)?;
        let config = MineConfig::parse(
            &config_path,
            std::str::from_utf8(&bytes).map_err(|_| OperationError::Journal)?,
        )?;
        let journal = RepositoryJournal::under_lock(root, &config.repository_id, lock)?;
        let observations = journal.observations()?;
        let record = journal.query(id)?;
        let expected = crate::infrastructure::operation_store::history::predecessors(
            &journal,
            &observations,
            Some(&record.operation.fields().workspace_id),
        )?;
        if !matches!(runtime(&record)?.before, GraphBefore::Absent { ref predecessors } if *predecessors == expected)
            || observations.iter().any(|r| {
                r.operation.fields().workspace_id == record.operation.fields().workspace_id
                    && r.operation.fields().operation_id != *id
            })
        {
            return Err(OperationError::Refused(
                "initial intent predecessor authority is not verified",
            ));
        }
        let store = OperationStore::under_lock(
            root,
            &config.repository_id,
            &record.operation.fields().workspace_id,
            lock,
        )?;
        let kernel = Self {
            paths: GraphCheckpointPaths::from_config(root, &config)?,
            config,
            config_hash: digest(&bytes),
            store,
            index_proofs: Default::default(),
        };
        kernel.bound_runtime(&record)?;
        Ok(kernel)
    }
    pub fn under_lock(root: &Path, lock: &'a FileLock) -> OperationResult<Self> {
        let config_path = root.join(".mine/config.toml");
        let bytes = read_bounded(&config_path)?;
        let config = MineConfig::parse(
            &config_path,
            std::str::from_utf8(&bytes).map_err(|_| OperationError::Journal)?,
        )?;
        let paths = GraphCheckpointPaths::from_config(root, &config)?;
        let ws = parse(&read_bounded(&paths.resolve()?.0)?)?;
        let store =
            OperationStore::under_lock(root, &config.repository_id, &ws.workspace_id, lock)?;
        Ok(Self {
            paths,
            config,
            config_hash: digest(&bytes),
            store,
            index_proofs: Default::default(),
        })
    }
    pub fn store(&self) -> &OperationStore<'a> {
        &self.store
    }
    /// Observation for an uncertain outcome, never a new mutation admission.
    /// Available without starting a child after the execution deadline expires.
    pub(crate) fn observed_revision(&self) -> Option<u64> {
        Some(
            parse(&read_bounded(&self.paths.resolve().ok()?.0).ok()?)
                .ok()?
                .revision,
        )
    }
    /// Finish only operation-owned temporary checkpoint artifacts after a
    /// lost completed response. Never replay old Plan cleanup on a new tip.
    pub(crate) fn replay_completed(&self, record: &JournalRecord) -> OperationResult<()> {
        self.historical(record)?;
        self.cleanup(record)
    }
    /// Read-only proof of a completed checkpoint after later legitimate Root
    /// commits. It never replays Plan Git effects or requires old resources.
    pub(crate) fn historical(&self, record: &JournalRecord) -> OperationResult<()> {
        let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
        self.bound_runtime(record)?;
        if !terminal(&record.operation) {
            return Err(OperationError::Refused(
                "checkpoint is not complete history",
            ));
        }
        let mut git = git::request(self.store.root(), None)?;
        let (common, index, head) = self.identity(&mut git)?;
        let r = runtime(record)?;
        if common != r.common_dir || index != r.index_path {
            return Err(OperationError::Refused("historical Root identity changed"));
        }
        self.supported_index(&mut git, &index, &common)?;
        self.prove_candidate(&mut git, record, None)?;
        if !git.is_ancestor(
            r.candidate_commit
                .as_ref()
                .ok_or(OperationError::Journal)?
                .as_str(),
            head.as_str(),
        )? || lock_path(&index).try_exists()?
            || !git
                .run_raw(
                    &["diff-index", "--cached", "--quiet", head.as_str(), "--"],
                    b"",
                )?
                .status
                .success()
            || !git
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
                "historical checkpoint not in clean current Root history",
            ));
        }
        Ok(())
    }
    /// Candidate is a typed, validated graph, never a path whitelist or Git tree.
    /// The central transaction owns its business transition.
    pub fn begin(
        &self,
        id: OperationId,
        expected_revision: u64,
        actor: &str,
        candidate: &PlanWorkspace,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<JournalRecord> {
        self.begin_inner(
            id,
            expected_revision,
            actor,
            candidate,
            (None, None),
            observer,
        )
    }
    pub(crate) fn begin_observed(
        &self,
        id: OperationId,
        expected_revision: u64,
        actor: &str,
        candidate: &PlanWorkspace,
        original_head: &CommitId,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<JournalRecord> {
        self.begin_inner(
            id,
            expected_revision,
            actor,
            candidate,
            (None, Some(original_head)),
            observer,
        )
    }
    pub(crate) fn begin_with_withdrawal(
        &self,
        id: OperationId,
        expected_revision: u64,
        actor: &str,
        candidate: &PlanWorkspace,
        request: plan_git::WithdrawInput,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<JournalRecord> {
        self.begin_inner(
            id,
            expected_revision,
            actor,
            candidate,
            (Some(plan_git::PlanGitRequest::Withdrawal(request)), None),
            observer,
        )
    }
    pub(crate) fn begin_with_preparation(
        &self,
        id: OperationId,
        expected_revision: u64,
        actor: &str,
        candidate: &PlanWorkspace,
        request: super::preparation_intent::PrepareInput,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<JournalRecord> {
        self.begin_inner(
            id,
            expected_revision,
            actor,
            candidate,
            (
                Some(plan_git::PlanGitRequest::Preparation(Box::new(request))),
                None,
            ),
            observer,
        )
    }
    fn begin_inner(
        &self,
        id: OperationId,
        expected_revision: u64,
        actor: &str,
        candidate: &PlanWorkspace,
        binding: (Option<plan_git::PlanGitRequest>, Option<&CommitId>),
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<JournalRecord> {
        let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
        let (requested, expected_root) = binding;
        crate::domain::validation::validate_with_worktrees_root(
            candidate,
            &self.config.plan.worktrees_root,
        )?;
        let source_after = toml::to_string(candidate).map_err(|_| OperationError::Journal)?;
        let view_after = render_markdown(candidate)?;
        let location =
            crate::infrastructure::operation_store::OperationLocation::new(self.store.root(), &id)?
                .resolve()?;
        if location.try_exists()? {
            let current = self.store.query(&id)?;
            let runtime = current.checkpoint.as_ref().ok_or(OperationError::Journal)?;
            if current.operation.fields().actor != actor
                || expected_root.is_some_and(|head| {
                    current
                        .operation
                        .fields()
                        .checkpoint
                        .as_ref()
                        .is_none_or(|c| &c.fields().original_head != head)
                })
                || current.operation.fields().observed_graph_revision != expected_revision
                || runtime.source_after != source_after
                || runtime.view_after != view_after
                || !match (&requested, &runtime.plan_git) {
                    (None, PlanGitEffects::None) => true,
                    (
                        Some(plan_git::PlanGitRequest::Withdrawal(request)),
                        PlanGitEffects::Withdrawal { intent, .. },
                    ) => {
                        intent.plan.fields().plan_id == request.id
                            && intent.reason == request.reason
                            && intent.withdrawn_at == request.at
                    }
                    (
                        Some(plan_git::PlanGitRequest::Preparation(request)),
                        PlanGitEffects::Preparation { intent, .. },
                    ) => {
                        intent.generation == request.generation
                            && intent.mode == request.mode
                            && intent.decisions == request.decisions
                    }
                    _ => false,
                }
            {
                return Err(OperationError::Refused("same operation input changed"));
            }
            return Ok(current);
        }
        let (source, view) = self.paths.resolve()?;
        let source_before = text_file(&source)?;
        let view_before = text_file(&view)?;
        let before = parse(source_before.as_bytes())?;
        if before.revision != expected_revision {
            return Err(OperationError::Revision {
                expected: expected_revision,
                actual: before.revision,
            });
        }
        if expected_revision.checked_add(1) != Some(candidate.revision)
            || candidate.workspace_id != before.workspace_id
            || candidate.integration_branch != self.config.branches.integration
            || candidate.stable_branch != self.config.branches.stable
            || view_before != render_markdown(&before)?
        {
            return Err(OperationError::Refused(
                "graph revision/workspace/render binding",
            ));
        }
        let mut git = git::request(self.store.root(), None)?;
        let (common_dir, index_path, original_head) = self.identity(&mut git)?;
        if expected_root.is_some_and(|head| head != &original_head) {
            return Err(OperationError::Refused(
                "Root moved after typed operation admission evidence",
            ));
        }
        self.supported_index(&mut git, &index_path, &common_dir)?;
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
            return Err(OperationError::Refused("Root is not clean"));
        }
        let clean = git.run_raw(
            &[
                "diff-index",
                "--cached",
                "--quiet",
                original_head.as_str(),
                "--",
            ],
            b"",
        )?;
        if !clean.status.success() {
            return Err(OperationError::Refused("actual index tree differs from H"));
        }
        // Reuse the accepted exact commit verifier; syntax alone is not evidence.
        let anchors = candidate
            .plans
            .iter()
            .filter(|p| !p.branch_anchor.is_empty())
            .map(|p| p.branch_anchor.clone())
            .collect::<Vec<_>>();
        git.validate_commits(&anchors)?;
        if lock_path(&index_path).try_exists()? {
            return Err(OperationError::Refused("unknown Root index lock"));
        }
        let original_index = read_bounded(&index_path)?;
        let plan_git = match requested {
            None => PlanGitEffects::None,
            Some(plan_git::PlanGitRequest::Withdrawal(request)) => plan_git::withdrawal(
                self.store.root(),
                &common_dir,
                &self.config_hash,
                &self.config.repository_id,
                &before,
                request,
                &original_head,
            )?,
            Some(plan_git::PlanGitRequest::Preparation(request)) => {
                let prior = if request.mode == crate::domain::graph::PreparationMode::Reprepare {
                    let old = before
                        .get(&request.generation.plan.fields().plan_id)
                        .and_then(|node| node.preparations.last())
                        .ok_or(OperationError::Refused(
                            "reprepare requires completed prior generation",
                        ))?;
                    Some(super::preparation_intent::completed_checkout(self, old)?)
                } else {
                    None
                };
                let (intent, creation, checkout, after_checkout) =
                    super::preparation_intent::build(
                        self.store.root(),
                        &common_dir,
                        &self.config,
                        &before,
                        request.generation,
                        request.mode,
                        request.decisions,
                    )?;
                if prior.is_some_and(|proof| {
                    intent.before_checkout != (plan_git::CheckoutState::Present { proof })
                }) {
                    return Err(OperationError::Refused(
                        "reprepare checkout is not the original completed generation",
                    ));
                }
                PlanGitEffects::Preparation {
                    intent: Box::new(intent),
                    creation,
                    checkout,
                    after_checkout,
                }
            }
        };
        let checkpoint = CheckpointFacts::new(CheckpointFields {
            schema_version: 1,
            stage: CheckpointStage::Intent,
            revision_before: expected_revision,
            revision_after: None,
            source_sha256: None,
            view_sha256: None,
            original_head,
            original_index_sha256: digest(&original_index),
            candidate_tree: None,
            candidate_commit: None,
            generated_index_sha256: None,
            owned_lock_sha256: None,
        })?;
        let (plan, registered_worktree) = match &plan_git {
            PlanGitEffects::None => (None, None),
            PlanGitEffects::Withdrawal { intent, .. } => {
                (Some(intent.plan.clone()), Some(intent.worktree.clone()))
            }
            PlanGitEffects::Preparation { intent, .. } => (
                Some(intent.generation.plan.clone()),
                Some(intent.generation.worktree.clone()),
            ),
        };
        let operation = LifecycleOperation::new(OperationFields {
            schema_version: 1,
            operation_id: id,
            stage_revision: 0,
            kind: OperationKind::GraphCheckpoint,
            stage: OperationStage::GraphCheckpoint(CheckpointStage::Intent),
            outcome: OperationOutcome::Pending,
            repository_id: self.config.repository_id.clone(),
            workspace_id: before.workspace_id,
            actor: actor.into(),
            observed_graph_revision: expected_revision,
            plan,
            registered_worktree,
            review: None,
            integrated_commit: None,
            verification_evidence: Vec::new(),
            cleanup: None,
            checkpoint: Some(checkpoint),
            diagnostics: Vec::new(),
        })?;
        let runtime = CheckpointRuntime {
            plan_git,
            root: self.store.root().to_path_buf(),
            common_dir,
            index_path,
            integration_ref: format!("refs/heads/{}", self.config.branches.integration),
            config_sha256: self.config_hash.clone(),
            source_path: self.paths.source.clone(),
            view_path: self.paths.view.clone(),
            before: GraphBefore::Present {
                source: source_before,
                view: view_before,
            },
            source_after,
            view_after,
            original_index,
            generated_index: None,
            candidate_tree: None,
            candidate_commit: None,
            cas_attempted: false,
            lock_claimed: false,
            lock_sha256: None,
        };
        crate::infrastructure::git_evidence::check_request_budget()?;
        let record = self.store.begin(&JournalRecord {
            schema_version: 1,
            operation,
            closure: None,
            checkpoint: Some(runtime),
        })?;
        crate::infrastructure::git_evidence::check_request_budget()?;
        observer.reached(Boundary::Intent)?;
        Ok(record)
    }
    /// Reconcile observations before each effect; an error never rolls back a
    /// persisted graph or advanced ref. Query retains exact pending facts.
    pub fn resume(
        &self,
        id: &OperationId,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<JournalRecord> {
        let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
        let mut record = self.store.query(id)?;
        self.bound_runtime(&record)?;
        if terminal(&record.operation) {
            self.completed(&record, None)?;
            self.cleanup(&record)?;
            observer.reached(Boundary::Response)?;
            return Ok(record);
        }
        let mut git = git::request(self.store.root(), None)?;
        // The Plan effect runner freshly observes Root and bound resources
        // before any effect (including the already-complete case). Do not
        // repeat that full mutable observation immediately at this entry.
        if runtime(&record)?.plan_git == PlanGitEffects::None {
            self.observe(&mut git, &record, false)?;
        }
        let (source, view) = self.paths.resolve()?;
        let mut facts = record
            .operation
            .fields()
            .checkpoint
            .as_ref()
            .ok_or(OperationError::Journal)?
            .fields()
            .clone();
        if record
            .checkpoint
            .as_ref()
            .is_some_and(|r| r.plan_git != PlanGitEffects::None)
        {
            self.resume_plan_git(&mut record, &mut git, &facts, observer)?;
            if facts.stage < CheckpointStage::GitWorkspaceMutated {
                facts.stage = CheckpointStage::GitWorkspaceMutated;
                self.progress(&mut record, &facts, OperationOutcome::Pending)?;
            }
        }
        if facts.stage < CheckpointStage::GraphPersisted {
            let r = runtime(&record)?;
            match &r.before {
                GraphBefore::Present { source: before, .. } => {
                    let actual = text_file(&source)?;
                    if actual == *before {
                        atomic_replace(&source, r.source_after.as_bytes())?;
                        observer.reached(Boundary::GraphWritten)?;
                    } else if actual != r.source_after {
                        return Err(OperationError::Refused("graph source changed"));
                    }
                }
                GraphBefore::Absent { .. } => {
                    if self.publish_initial(
                        &mut git,
                        &record,
                        &source,
                        r.source_after.as_bytes(),
                    )? {
                        observer.reached(Boundary::GraphWritten)?;
                    }
                }
            }
            if facts.stage < CheckpointStage::GitWorkspaceMutated {
                facts.stage = CheckpointStage::GitWorkspaceMutated;
                self.progress(&mut record, &facts, OperationOutcome::Pending)?;
            }
            facts.stage = CheckpointStage::GraphPersisted;
            facts.revision_after = facts.revision_before.checked_add(1);
            facts.source_sha256 = Some(digest(runtime(&record)?.source_after.as_bytes()));
            self.progress(&mut record, &facts, OperationOutcome::CommittedRenderRepair)?;
            observer.reached(Boundary::GraphRecorded)?;
        }
        if facts.stage < CheckpointStage::RenderComplete {
            let r = runtime(&record)?;
            match &r.before {
                GraphBefore::Present { view: before, .. } => {
                    let actual = text_file(&view)?;
                    if actual == *before {
                        atomic_replace(&view, r.view_after.as_bytes())?;
                        observer.reached(Boundary::ViewWritten)?;
                    } else if actual != r.view_after {
                        return Err(OperationError::Refused("graph view changed"));
                    }
                }
                GraphBefore::Absent { .. } => {
                    if self.publish_initial(&mut git, &record, &view, r.view_after.as_bytes())? {
                        observer.reached(Boundary::ViewWritten)?;
                    }
                }
            }
            facts.stage = CheckpointStage::RenderComplete;
            facts.view_sha256 = Some(digest(runtime(&record)?.view_after.as_bytes()));
            self.progress(&mut record, &facts, OperationOutcome::Pending)?;
            observer.reached(Boundary::ViewRecorded)?;
        }
        let prepared_phase = self.observe(&mut git, &record, true)?;
        let mut proven_tree = None;
        if facts.stage < CheckpointStage::CandidatePrepared {
            let r = runtime(&record)?;
            if r.cas_attempted || prepared_phase.head != facts.original_head {
                return Err(OperationError::Refused(
                    "cannot regenerate candidate after CAS",
                ));
            }
            let private = self.artifact(id, "index")?;
            if private.try_exists()? {
                let mut old = git::request(self.store.root(), Some(&private))?;
                // A recovered scratch index has no current copy witness. Do
                // not overwrite same-tree foreign flags or unsupported modes.
                self.supported_index(&mut old, &private, &r.common_dir)?;
                let old_tree = old.text(&["write-tree"])?;
                let h_tree = git.text(&[
                    "rev-parse",
                    &format!("{}^{{tree}}", facts.original_head.as_str()),
                ])?;
                if old_tree != h_tree {
                    self.prove_tree(&mut old, &record, &oid(&old_tree)?)?;
                }
            }
            atomic_replace(&private, &r.original_index)?;
            observer.reached(Boundary::PrivateIndexCreated)?;
            let copied = read_bounded(&private)?;
            if copied != r.original_index || digest(&copied) != prepared_phase.index_sha256 {
                return Err(OperationError::Refused(
                    "private index is not exact validated source copy",
                ));
            }
            let mut isolated = git::request(self.store.root(), Some(&private))?;
            if isolated.text(&["write-tree"])?
                != git.text(&[
                    "rev-parse",
                    &format!("{}^{{tree}}", facts.original_head.as_str()),
                ])?
            {
                return Err(OperationError::Refused(
                    "private initial tree differs from H",
                ));
            }
            // Flags/shared-index state belong to the exact validated index
            // bytes above. Git policy and attributes remain fresh before add.
            git::index_policy(&mut isolated)?;
            self.attributes(&mut isolated)?;
            isolated.run(
                &[
                    "add",
                    "--",
                    self.paths.source.as_str(),
                    self.paths.view.as_str(),
                ],
                b"",
            )?;
            let tree = oid(&isolated.text(&["write-tree"])?)?;
            self.prove_tree(&mut isolated, &record, &tree)?;
            proven_tree = Some(tree.clone());
            let mut args = vec![
                "commit-tree",
                tree.as_str(),
                "-p",
                facts.original_head.as_str(),
            ];
            // commit-tree does not implicitly apply commit.gpgSign.
            if git::boolean(&mut isolated, "commit.gpgSign")? {
                args.push("-S");
            }
            let candidate = oid(&String::from_utf8(isolated.run(
                &args,
                format!("chore(graph): checkpoint operation {}\n", id.as_str()).as_bytes(),
            )?)
            .map_err(|_| OperationError::Journal)?)?;
            observer.reached(Boundary::CandidateCreated)?;
            let generated = read_bounded(&private)?;
            // Newly staged bytes need their first complete representation
            // proof before publication. Its two queries move here from the
            // final phase; no safety proof is deleted from the whole budget.
            self.index_representation(&mut isolated, &private, &r.common_dir)?;
            if read_bounded(&private)? != generated {
                return Err(OperationError::Refused("generated private index changed"));
            }
            let r = record.checkpoint.as_mut().ok_or(OperationError::Journal)?;
            r.candidate_tree = Some(tree.clone());
            r.candidate_commit = Some(candidate.clone());
            r.generated_index = Some(generated.clone());
            facts.stage = CheckpointStage::CandidatePrepared;
            facts.candidate_tree = Some(tree);
            facts.candidate_commit = Some(candidate);
            facts.generated_index_sha256 = Some(digest(&generated));
            self.progress(&mut record, &facts, OperationOutcome::Pending)?;
            observer.reached(Boundary::CandidateRecorded)?;
        }
        let checked_candidate = self.prove_candidate(&mut git, &record, proven_tree.as_ref())?;
        let r = runtime(&record)?;
        let index = r.index_path.clone();
        let lock = lock_path(&index);
        let witness = self.artifact(id, "lock-witness")?;
        let generated = r.generated_index.clone().ok_or(OperationError::Journal)?;
        let candidate = r.candidate_commit.clone().ok_or(OperationError::Journal)?;
        let ref_value = self.ref_value(&mut git)?;
        // Publication may have completed before its journal update/response.
        let published =
            ref_value == candidate && read_bounded(&index)? == generated && !lock.try_exists()?;
        if !published {
            if !r.lock_claimed {
                if lock.try_exists()? || witness.try_exists()? {
                    return Err(OperationError::Refused(
                        "unknown index lock; ownership unproved",
                    ));
                }
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock)?;
                file.write_all(format!("MINE-OWNED-LOCK:{}\n", id.as_str()).as_bytes())?;
                file.flush()?;
                file.sync_all()?;
                drop(file);
                observer.reached(Boundary::LockCreated)?;
                // An interruption before the following journal update is an
                // unknown-lock refusal, not permission to infer ownership.
                fs::hard_link(&lock, &witness)?;
                record
                    .checkpoint
                    .as_mut()
                    .ok_or(OperationError::Journal)?
                    .lock_claimed = true;
                self.progress(&mut record, &facts, OperationOutcome::Pending)?;
                observer.reached(Boundary::LockClaimed)?;
            }
            same_file(&lock, &witness)?;
            self.observe(&mut git, &record, true)?;
            let r = runtime(&record)?;
            if let Some(hash) = &r.lock_sha256 {
                if digest(&read_bounded(&lock)?) != *hash {
                    return Err(OperationError::Refused("owned lock bytes changed"));
                }
            } else {
                let mut file = OpenOptions::new().write(true).truncate(true).open(&lock)?;
                file.write_all(&generated)?;
                file.flush()?;
                file.sync_all()?;
                drop(file);
                observer.reached(Boundary::LockWritten)?;
                same_file(&lock, &witness)?;
                record
                    .checkpoint
                    .as_mut()
                    .ok_or(OperationError::Journal)?
                    .lock_sha256 = Some(digest(&generated));
                facts.owned_lock_sha256 = Some(digest(&generated));
                self.progress(&mut record, &facts, OperationOutcome::Pending)?;
                observer.reached(Boundary::LockRecorded)?;
            }
            let pre_cas = self.observe(&mut git, &record, true)?;
            let after_cas = if pre_cas.head == facts.original_head {
                record
                    .checkpoint
                    .as_mut()
                    .ok_or(OperationError::Journal)?
                    .cas_attempted = true;
                self.progress(&mut record, &facts, OperationOutcome::Unknown)?;
                observer.reached(Boundary::CasRecorded)?;
                let result = git.run_raw(
                    &[
                        "update-ref",
                        "--no-deref",
                        &runtime(&record)?.integration_ref,
                        candidate.as_str(),
                        facts.original_head.as_str(),
                    ],
                    b"",
                );
                observer.reached(Boundary::CasReturned)?;
                let observed = self.ref_value(&mut git)?;
                if observed != candidate {
                    result?;
                    return Err(OperationError::Evidence("checkpoint ref CAS not committed"));
                }
                observed
            } else {
                pre_cas.head
            };
            if after_cas != candidate {
                return Err(OperationError::Refused("integration ref drift"));
            }
            facts.stage = CheckpointStage::RefAdvanced;
            self.progress(&mut record, &facts, OperationOutcome::Pending)?;
            observer.reached(Boundary::RefRecorded)?;
            self.observe(&mut git, &record, true)?;
            same_file(&lock, &witness)?;
            if read_bounded(&lock)? != generated {
                return Err(OperationError::Refused("lock is not generated index"));
            }
            observer.reached(Boundary::PostRefObserved)?;
            // Observers/hooks can mutate HEAD/files; recheck AFTER observation.
            self.observe(&mut git, &record, true)?;
            same_file(&lock, &witness)?;
            if read_bounded(&lock)? != generated {
                return Err(OperationError::Refused(
                    "owned lock changed before publication",
                ));
            }
            fs::rename(&lock, &index)?;
            #[cfg(unix)]
            fs::File::open(index.parent().ok_or(OperationError::Journal)?)?.sync_all()?;
            observer.reached(Boundary::IndexPublished)?;
        } else if !r.cas_attempted || !r.lock_claimed || r.lock_sha256.is_none() {
            return Err(OperationError::Refused(
                "publication not bound to recorded CAS/lock",
            ));
        }
        if facts.stage < CheckpointStage::RefAdvanced {
            facts.stage = CheckpointStage::RefAdvanced;
            self.progress(&mut record, &facts, OperationOutcome::Pending)?;
        }
        if facts.stage < CheckpointStage::IndexPublished {
            facts.stage = CheckpointStage::IndexPublished;
            self.progress(&mut record, &facts, OperationOutcome::Pending)?;
            observer.reached(Boundary::IndexRecorded)?;
        }
        // The native candidate/tree were proved in this call. Re-observe all
        // mutable Root/attribute/index/ref facts, not immutable blobs twice.
        self.completed(&record, Some(&checked_candidate))?;
        facts.stage = CheckpointStage::Complete;
        self.progress(&mut record, &facts, OperationOutcome::Committed)?;
        observer.reached(Boundary::Complete)?;
        // Retain only journal and exact generated facts. Never remove unknown
        // locks. These operation-ID artifact paths are checked before deletion.
        self.cleanup(&record)?;
        observer.reached(Boundary::Response)?;
        Ok(record)
    }
    fn cleanup(&self, record: &JournalRecord) -> OperationResult<()> {
        let id = &record.operation.fields().operation_id;
        let bytes = runtime(record)?
            .generated_index
            .as_ref()
            .ok_or(OperationError::Journal)?;
        for path in [
            self.artifact(id, "index")?,
            self.artifact(id, "lock-witness")?,
        ] {
            if path.try_exists()? && read_bounded(&path)? == *bytes {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
    fn resume_plan_git(
        &self,
        record: &mut JournalRecord,
        git: &mut GitRequest,
        facts: &CheckpointFields,
        observer: &mut impl BoundaryObserver,
    ) -> OperationResult<()> {
        use plan_git::{CheckoutState, EffectProgress, PlanGitEffects, RefState};
        if matches!(
            runtime(record)?.plan_git,
            PlanGitEffects::Preparation { .. }
        ) {
            return self.resume_preparation(record, git, facts, observer);
        }
        let PlanGitEffects::Withdrawal {
            intent,
            mut removal,
            mut deletion,
        } = runtime(record)?.plan_git.clone()
        else {
            return Ok(());
        };
        let observe = |git: &mut GitRequest,
                       record: &JournalRecord|
         -> OperationResult<(RefState, CheckoutState)> {
            self.observe(git, record, false)?;
            let r = runtime(record)?;
            if !r.plan_git.complete()
                && (text_file(&self.paths.resolve()?.0)? != r.source_before()?
                    || text_file(&self.paths.resolve()?.1)? != r.view_before()?)
            {
                return Err(OperationError::Refused(
                    "graph published before Plan Git effects completed",
                ));
            }
            let body = read_bounded(&crate::infrastructure::operation_store::regular_path(
                self.store.root(),
                &intent.plan.fields().plan_path,
            )?)?;
            if Some(&digest(&body)) != intent.plan.fields().body_sha256.as_ref() {
                return Err(OperationError::Refused(
                    "frozen withdrawal Plan body changed",
                ));
            }
            if let plan_git::PreservedRef::Rejected { branch, tip, .. } = &intent.preserved {
                if plan_git::reference(git, branch)? != (RefState::Present { tip: tip.clone() }) {
                    return Err(OperationError::Refused("preserved rejected ref changed"));
                }
            }
            let reference = plan_git::reference(git, &intent.plan.fields().branch)?;
            let checkout = plan_git::checkout(
                self.store.root(),
                &r.common_dir,
                &intent.worktree,
                &intent.plan.fields().branch,
                git,
            )?;
            Ok((reference, checkout))
        };
        let removed = |checkout: &CheckoutState| -> OperationResult<bool> {
            if !matches!(checkout, CheckoutState::Absent) {
                return Ok(false);
            }
            if let CheckoutState::Present { proof } = &intent.before_checkout {
                if proof.admin.try_exists()? {
                    return Err(OperationError::Refused(
                        "partial checkout removal/admin remains",
                    ));
                }
            }
            Ok(true)
        };
        if removal != EffectProgress::Complete {
            let (reference, checkout) = observe(git, record)?;
            if reference != intent.before_ref {
                return Err(OperationError::Refused("Plan ref changed before removal"));
            }
            if removal == EffectProgress::NotAttempted {
                if checkout != intent.before_checkout {
                    return Err(OperationError::Refused(
                        "checkout changed before removal intent",
                    ));
                }
                if let PlanGitEffects::Withdrawal { removal, .. } = &mut record
                    .checkpoint
                    .as_mut()
                    .ok_or(OperationError::Journal)?
                    .plan_git
                {
                    *removal = EffectProgress::Attempted;
                }
                self.progress(record, facts, OperationOutcome::Pending)?;
                removal = EffectProgress::Attempted;
                observer.reached(Boundary::PlanGitAttempted)?;
            }
            git.refresh_observations();
            let (reference, checkout) = observe(git, record)?;
            if reference != intent.before_ref {
                return Err(OperationError::Refused(
                    "Plan ref drift after removal intent",
                ));
            }
            if !removed(&checkout)? {
                if checkout != intent.before_checkout {
                    return Err(OperationError::Refused(
                        "foreign checkout/admin/index before removal",
                    ));
                }
                let result = git.run(&["worktree", "remove", "--", intent.worktree.as_str()], b"");
                observer.reached(Boundary::PlanGitReturned)?;
                git.refresh_observations();
                let (reference, checkout) = observe(git, record)?;
                if reference != intent.before_ref || !removed(&checkout)? {
                    result?;
                    return Err(OperationError::Evidence(
                        "checkout removal not observed complete",
                    ));
                }
            }
            if removal != EffectProgress::Attempted {
                return Err(OperationError::Journal);
            }
            if let PlanGitEffects::Withdrawal { removal, .. } = &mut record
                .checkpoint
                .as_mut()
                .ok_or(OperationError::Journal)?
                .plan_git
            {
                *removal = EffectProgress::Complete;
            }
            self.progress(record, facts, OperationOutcome::Pending)?;
            removal = EffectProgress::Complete;
            observer.reached(Boundary::PlanGitRecorded)?;
        }
        git.refresh_observations();
        let (reference, checkout) = observe(git, record)?;
        if removal != EffectProgress::Complete || !removed(&checkout)? {
            return Err(OperationError::Refused(
                "completed removal has foreign/reappeared checkout",
            ));
        }
        if deletion != EffectProgress::Complete {
            if deletion == EffectProgress::NotAttempted {
                if reference != intent.before_ref {
                    return Err(OperationError::Refused(
                        "Plan ref changed before guarded deletion",
                    ));
                }
                if let PlanGitEffects::Withdrawal { deletion, .. } = &mut record
                    .checkpoint
                    .as_mut()
                    .ok_or(OperationError::Journal)?
                    .plan_git
                {
                    *deletion = EffectProgress::Attempted;
                }
                self.progress(record, facts, OperationOutcome::Pending)?;
                deletion = EffectProgress::Attempted;
                observer.reached(Boundary::PlanGitAttempted)?;
            }
            git.refresh_observations();
            let (reference, checkout) = observe(git, record)?;
            if !removed(&checkout)? {
                return Err(OperationError::Refused(
                    "checkout reappeared before guarded deletion",
                ));
            }
            if reference != RefState::Absent {
                if reference != intent.before_ref {
                    return Err(OperationError::Refused(
                        "Plan ref changed after deletion intent",
                    ));
                }
                let RefState::Present { tip } = &intent.before_ref else {
                    return Err(OperationError::Journal);
                };
                let result = git.run(
                    &[
                        "update-ref",
                        "--no-deref",
                        "-d",
                        &format!("refs/heads/{}", intent.plan.fields().branch),
                        tip.as_str(),
                    ],
                    b"",
                );
                observer.reached(Boundary::PlanGitReturned)?;
                git.refresh_observations();
                let (reference, checkout) = observe(git, record)?;
                if reference != RefState::Absent || !removed(&checkout)? {
                    result?;
                    return Err(OperationError::Evidence(
                        "guarded Plan ref deletion not observed complete",
                    ));
                }
            }
            if deletion != EffectProgress::Attempted {
                return Err(OperationError::Journal);
            }
            if let PlanGitEffects::Withdrawal { deletion, .. } = &mut record
                .checkpoint
                .as_mut()
                .ok_or(OperationError::Journal)?
                .plan_git
            {
                *deletion = EffectProgress::Complete;
            }
            self.progress(record, facts, OperationOutcome::Pending)?;
            observer.reached(Boundary::PlanGitRecorded)?;
        } else if reference != RefState::Absent {
            return Err(OperationError::Refused("deleted Plan ref reappeared"));
        }
        Ok(())
    }
    fn progress(
        &self,
        record: &mut JournalRecord,
        facts: &crate::domain::lifecycle_operation::CheckpointFields,
        outcome: OperationOutcome,
    ) -> OperationResult<()> {
        crate::infrastructure::git_evidence::check_request_budget()?;
        let expected = record.operation.fields().stage_revision;
        let mut fields = record.operation.fields().clone();
        fields.stage_revision = expected.checked_add(1).ok_or(OperationError::Journal)?;
        fields.stage = OperationStage::GraphCheckpoint(facts.stage);
        fields.checkpoint = Some(CheckpointFacts::new(facts.clone())?);
        fields.outcome = outcome;
        record.operation = LifecycleOperation::new(fields)?;
        *record = self.store.advance(expected, record)?;
        crate::infrastructure::git_evidence::check_request_budget()?;
        Ok(())
    }
    fn artifact(&self, id: &OperationId, suffix: &str) -> OperationResult<PathBuf> {
        Ok(crate::infrastructure::operation_store::regular_path(
            self.store.root(),
            &RepoPath::new(format!(".mine/runtime/operations/{}.{suffix}", id.as_str()))?,
        )?)
    }
    fn identity(&self, git: &mut GitRequest) -> OperationResult<(PathBuf, PathBuf, CommitId)> {
        // One fresh native observation, not one child per path. Git resolves
        // each path (including relocated indexes); nothing is guessed/cached.
        let paths = git.run(
            &[
                "rev-parse",
                "--path-format=absolute",
                "--show-toplevel",
                "--absolute-git-dir",
                "--git-common-dir",
                "--git-path",
                "MERGE_HEAD",
                "--git-path",
                "CHERRY_PICK_HEAD",
                "--git-path",
                "REVERT_HEAD",
                "--git-path",
                "rebase-apply",
                "--git-path",
                "rebase-merge",
                "--git-path",
                "sequencer",
                "--git-path",
                "index",
                "--git-path",
                "info/grafts",
                "HEAD^{commit}",
            ],
            b"",
        )?;
        let paths = std::str::from_utf8(&paths)
            .map_err(|_| OperationError::Evidence("Git identity path encoding"))?
            .lines()
            .collect::<Vec<_>>();
        if paths.len() != 12 {
            return Err(OperationError::Evidence("Git identity path response"));
        }
        let common = absolute(paths[2])?;
        let head = oid(paths[11])?;
        if absolute(paths[0])? != self.store.root()
            || absolute(paths[1])? != common
            || git.text(&["symbolic-ref", "-q", "HEAD"])?
                != format!("refs/heads/{}", self.config.branches.integration)
        {
            return Err(OperationError::Refused(
                "not symbolic Root integration checkout",
            ));
        }
        let branch = format!("refs/heads/{}", self.config.branches.integration);
        let refs = git.text(&[
            "for-each-ref",
            "--format=%(refname)%00%(symref)%00%(objectname)",
            &branch,
            "refs/replace/",
        ])?;
        // Controlled children inspect native objects. Do not silently bypass
        // configured history replacement/graft policy: explicitly refuse it.
        if refs != format!("{branch}\0\0{}", head.as_str()) || Path::new(paths[10]).try_exists()? {
            return Err(OperationError::Refused(
                "integration ref symbolic/unavailable or history replacement unsupported",
            ));
        }
        for state in &paths[3..9] {
            let path = PathBuf::from(*state);
            if !path.is_absolute() {
                return Err(OperationError::Evidence(
                    "Git returned non-absolute state path",
                ));
            }
            if path.try_exists()? {
                return Err(OperationError::Refused("Git integration in progress"));
            }
        }
        if digest(&read_bounded(&self.store.root().join(".mine/config.toml"))?) != self.config_hash
        {
            return Err(OperationError::Refused("configuration changed"));
        }
        Ok((common.clone(), index_path(paths[9], &common)?, head))
    }
    fn supported_index(
        &self,
        git: &mut GitRequest,
        index: &Path,
        common: &Path,
    ) -> OperationResult<()> {
        git::index_policy(git)?;
        self.index_representation(git, index, common)
    }
    fn index_representation(
        &self,
        git: &mut GitRequest,
        index: &Path,
        common: &Path,
    ) -> OperationResult<()> {
        if !fs::symlink_metadata(index)?.file_type().is_file() {
            return Err(OperationError::Refused(
                "index proof requires regular exact bytes",
            ));
        }
        let bytes = read_bounded(index)?;
        {
            let mut proofs = self.index_proofs.borrow_mut();
            proofs.retain(IndexWitness::same_request);
            if proofs.iter().any(|proof| {
                proof.root == self.store.root() && proof.common == common && proof.bytes == bytes
            }) {
                return Ok(());
            }
        }
        if !git.text(&["rev-parse", "--shared-index-path"])?.is_empty() {
            return Err(OperationError::Refused(
                "sparse/split index not supported by checkpoint",
            ));
        }
        let entries = git.run(&["ls-files", "-v", "-z"], b"")?;
        if entries
            .split(|b| *b == 0)
            .filter(|e| !e.is_empty())
            .any(|e| e.first() != Some(&b'H'))
        {
            return Err(OperationError::Refused(
                "skip/assume-unchanged/unmerged index entries",
            ));
        }
        if read_bounded(index)? != bytes {
            return Err(OperationError::Refused(
                "index bytes changed during representation proof",
            ));
        }
        self.index_proofs.borrow_mut().push(IndexWitness {
            root: self.store.root().into(),
            common: common.into(),
            bytes,
            request: crate::infrastructure::git_evidence::request_identity(),
        });
        Ok(())
    }
    fn attributes(&self, git: &mut GitRequest) -> OperationResult<()> {
        let bytes = git.run(
            &[
                "check-attr",
                "-z",
                "filter",
                "ident",
                "working-tree-encoding",
                "text",
                "eol",
                "--",
                self.paths.source.as_str(),
                self.paths.view.as_str(),
            ],
            b"",
        )?;
        let fields = bytes
            .split(|b| *b == 0)
            .filter(|b| !b.is_empty())
            .collect::<Vec<_>>();
        if fields.len() != 30 {
            return Err(OperationError::Evidence("Git attribute response"));
        }
        for triple in fields.chunks_exact(3) {
            let supported = match triple[1] {
                b"text" => matches!(triple[2], b"unspecified" | b"unset" | b"set" | b"auto"),
                b"eol" => matches!(triple[2], b"unspecified" | b"unset" | b"lf" | b"crlf"),
                _ => matches!(triple[2], b"unspecified" | b"unset"),
            };
            if !supported {
                return Err(OperationError::Refused(
                    "unsupported graph storage transformation",
                ));
            }
        }
        Ok(())
    }
    fn bound_runtime(&self, record: &JournalRecord) -> OperationResult<()> {
        crate::infrastructure::git_evidence::check_request_budget()?;
        self.store.validate_record(record)?;
        crate::infrastructure::git_evidence::check_request_budget()?;
        let r = runtime(record)?;
        if r.root != self.store.root()
            || r.config_sha256 != self.config_hash
            || r.source_path != self.paths.source
            || r.view_path != self.paths.view
            || r.integration_ref != format!("refs/heads/{}", self.config.branches.integration)
        {
            return Err(OperationError::Refused("foreign checkpoint journal"));
        }
        // Store validation carries only an exact-input immutable proof across
        // stage deltas. Mutable produced facts and live observations still run.
        Ok(())
    }
    fn ref_value(&self, git: &mut GitRequest) -> OperationResult<CommitId> {
        oid(&git.text(&[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{}^{{commit}}", self.config.branches.integration),
        ])?)
    }
    fn observe(
        &self,
        git: &mut GitRequest,
        record: &JournalRecord,
        rendered: bool,
    ) -> OperationResult<PhaseObservation> {
        self.bound_runtime(record)?;
        let (common, index, head) = self.identity(git)?;
        self.supported_index(git, &index, &common)?;
        self.attributes(git)?;
        let r = runtime(record)?;
        let c = record
            .operation
            .fields()
            .checkpoint
            .as_ref()
            .ok_or(OperationError::Journal)?
            .fields();
        if index != r.index_path || common != r.common_dir {
            return Err(OperationError::Refused(
                "Root index/common directory changed",
            ));
        }
        if head != c.original_head && Some(&head) != r.candidate_commit.as_ref() {
            return Err(OperationError::Refused("Root HEAD/ref changed"));
        }
        let index = read_bounded(&r.index_path)?;
        if index != r.original_index && r.generated_index.as_ref() != Some(&index) {
            return Err(OperationError::Refused("foreign actual index changed"));
        }
        if index != r.original_index && Some(&head) != r.candidate_commit.as_ref() {
            return Err(OperationError::Refused("index publication before ref"));
        }
        let (source, view) = self.paths.resolve()?;
        let actual_source = optional_text(&source)?;
        let actual_view = optional_text(&view)?;
        if rendered {
            if actual_source.as_deref() != Some(r.source_after.as_str())
                || actual_view.as_deref() != Some(r.view_after.as_str())
            {
                return Err(OperationError::Refused("operation graph bytes changed"));
            }
        } else {
            let (source_before, view_before) = match &r.before {
                GraphBefore::Present { source, view } => {
                    (Some(source.as_str()), Some(view.as_str()))
                }
                GraphBefore::Absent { .. } => (None, None),
            };
            if (actual_source.as_deref() != source_before
                && actual_source.as_deref() != Some(r.source_after.as_str()))
                || (actual_view.as_deref() != view_before
                    && actual_view.as_deref() != Some(r.view_after.as_str()))
            {
                return Err(OperationError::Refused("graph source/view drift"));
            }
        }
        let initial = matches!(r.before, GraphBefore::Absent { .. });
        if initial {
            for path in [&self.paths.source, &self.paths.view] {
                if !git
                    .run(
                        &[
                            "ls-tree",
                            "-z",
                            c.original_head.as_str(),
                            "--",
                            path.as_str(),
                        ],
                        b"",
                    )?
                    .is_empty()
                {
                    return Err(OperationError::Refused(
                        "initial original tree contains graph",
                    ));
                }
            }
            let after = parse(r.source_after.as_bytes())?;
            if git.text(&[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{}^{{commit}}", self.config.branches.stable),
            ])? != after.stable_baseline_commit
            {
                return Err(OperationError::Refused(
                    "initial actual stable baseline changed",
                ));
            }
        }
        let status = git.run(
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
                "--ignore-submodules=none",
            ],
            b"",
        )?;
        for entry in status.split(|b| *b == 0).filter(|e| !e.is_empty()) {
            if entry.len() < 4
                || entry[..2].iter().any(|b| matches!(b, b'R' | b'C' | b'U'))
                || (entry[..2].contains(&b'?') && !(initial && entry[..3] == *b"?? "))
                || (entry[3..] != *self.paths.source.as_str().as_bytes()
                    && entry[3..] != *self.paths.view.as_str().as_bytes())
            {
                return Err(OperationError::Refused(
                    "foreign worktree/index/untracked change",
                ));
            }
        }
        Ok(PhaseObservation {
            head,
            index_sha256: digest(&index),
            clean: status.is_empty(),
        })
    }
    /// Publish complete initial bytes without replacing a concurrently-created
    /// entry. A persisted same-ID after-state alone can reconcile a lost reply.
    fn publish_initial(
        &self,
        git: &mut GitRequest,
        record: &JournalRecord,
        path: &Path,
        bytes: &[u8],
    ) -> OperationResult<bool> {
        if let Some(actual) = optional_text(path)? {
            if actual.as_bytes() == bytes {
                self.observe(git, record, false)?;
                return Ok(false);
            }
            return Err(OperationError::Refused("initial destination changed"));
        }
        fs::create_dir_all(path.parent().ok_or(OperationError::Journal)?)?;
        let directory = self
            .artifact(
                &record.operation.fields().operation_id,
                "initial-publication",
            )?
            .parent()
            .ok_or(OperationError::Journal)?
            .to_path_buf();
        let mut temporary = tempfile::Builder::new()
            .prefix(".initial-graph-")
            .tempfile_in(directory)?;
        temporary.write_all(bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        // Resolve containment again after directory creation, and immediately
        // reobserve Root before the no-clobber publication primitive.
        let (source, view) = self.paths.resolve()?;
        if path != source && path != view {
            return Err(OperationError::Journal);
        }
        self.observe(git, record, false)?;
        temporary
            .persist_noclobber(path)
            .map_err(|error| OperationError::Io(error.error))?;
        Ok(true)
    }
    fn prove_tree(
        &self,
        git: &mut GitRequest,
        record: &JournalRecord,
        tree: &CommitId,
    ) -> OperationResult<()> {
        self.attributes(git)?;
        git::validate_autocrlf(git::config(git, "core.autocrlf")?.as_deref())?;
        let r = runtime(record)?;
        let c = record
            .operation
            .fields()
            .checkpoint
            .as_ref()
            .ok_or(OperationError::Journal)?
            .fields();
        if matches!(r.before, GraphBefore::Absent { .. }) {
            // Completed replay also proves additions, not merely a two-path
            // modification that could disguise a fabricated absent-before.
            if !git
                .run(
                    &[
                        "ls-tree",
                        "-z",
                        c.original_head.as_str(),
                        "--",
                        self.paths.source.as_str(),
                        self.paths.view.as_str(),
                    ],
                    b"",
                )?
                .is_empty()
            {
                return Err(OperationError::Refused(
                    "initial checkpoint parent contains graph paths",
                ));
            }
        }
        let delta = git.run(
            &[
                "diff-tree",
                "--no-commit-id",
                "--no-renames",
                "--name-only",
                "-r",
                "-z",
                c.original_head.as_str(),
                tree.as_str(),
                "--",
            ],
            b"",
        )?;
        let changes = delta
            .split(|b| *b == 0)
            .filter(|e| !e.is_empty())
            .collect::<Vec<_>>();
        if changes.len() != 2
            || !changes.contains(&self.paths.source.as_str().as_bytes())
            || !changes.contains(&self.paths.view.as_str().as_bytes())
        {
            return Err(OperationError::Refused(
                "resulting tree is not exact graph pair delta",
            ));
        }
        let paths = [
            (self.paths.source.as_str(), r.source_after.as_bytes()),
            (self.paths.view.as_str(), r.view_after.as_bytes()),
        ];
        let entries = git.run(
            &["ls-tree", "-z", tree.as_str(), "--", paths[0].0, paths[1].0],
            b"",
        )?;
        let mut blobs = std::collections::BTreeMap::new();
        for entry in entries.split(|b| *b == 0).filter(|entry| !entry.is_empty()) {
            let text = std::str::from_utf8(entry).map_err(|_| OperationError::Journal)?;
            let (meta, observed) = text.split_once('\t').ok_or(OperationError::Journal)?;
            let fields = meta.split(' ').collect::<Vec<_>>();
            if fields.len() != 3
                || fields[0] != "100644"
                || fields[1] != "blob"
                || !paths.iter().any(|(path, _)| *path == observed)
                || blobs.insert(observed, oid(fields[2])?).is_some()
            {
                return Err(OperationError::Refused("graph tree path/mode/type differs"));
            }
        }
        if blobs.len() != 2 || !entries.ends_with(&[0]) {
            return Err(OperationError::Refused(
                "graph tree pair missing or malformed",
            ));
        }
        let oids = paths
            .iter()
            .map(|(path, _)| blobs[path].as_str())
            .collect::<Vec<_>>();
        let raw = super::proofs::raw_blobs(git, &oids)?;
        for ((path, canonical), bytes) in paths.into_iter().zip(raw) {
            if bytes != canonical {
                return Err(OperationError::Refused(
                    "graph stored blob not canonical source/view",
                ));
            }
            let checkout = git.run(
                &[
                    "cat-file",
                    "--filters",
                    &format!("--path={path}"),
                    &format!("{}:{path}", tree.as_str()),
                ],
                b"",
            )?;
            // Only Git's declared built-in text/EOL conversion is supported.
            let checkout = String::from_utf8(checkout)
                .map_err(|_| OperationError::Journal)?
                .replace("\r\n", "\n");
            if checkout.as_bytes() != canonical {
                return Err(OperationError::Refused("graph checkout equivalence failed"));
            }
        }
        Ok(())
    }
    fn prove_candidate(
        &self,
        git: &mut GitRequest,
        record: &JournalRecord,
        proven_tree: Option<&CommitId>,
    ) -> OperationResult<CheckedCandidate> {
        let r = runtime(record)?;
        let c = record
            .operation
            .fields()
            .checkpoint
            .as_ref()
            .ok_or(OperationError::Journal)?
            .fields();
        let candidate = r.candidate_commit.as_ref().ok_or(OperationError::Journal)?;
        let tree = r.candidate_tree.as_ref().ok_or(OperationError::Journal)?;
        super::proofs::candidate(git, candidate, &c.original_head, tree)?;
        if proven_tree != Some(tree) {
            self.prove_tree(git, record, tree)?;
        }
        Ok(CheckedCandidate {
            binding: super::validation::RuntimeValidationWitness::binding(r, &record.operation)?,
            commit: candidate.clone(),
            tree: tree.clone(),
            request: crate::infrastructure::git_evidence::request_identity(),
        })
    }
    fn completed(
        &self,
        record: &JournalRecord,
        checked: Option<&CheckedCandidate>,
    ) -> OperationResult<()> {
        let mut git = git::request(self.store.root(), None)?;
        let observed = self.observe(&mut git, record, true)?;
        // Fresh identity above refuses replacements before immutable proof
        // reuse. New/recovered requests retain the full candidate proof path.
        let (head, clean) = if checked
            .map(|proof| proof.matches(record))
            .transpose()?
            .unwrap_or(false)
        {
            (observed.head, observed.clean)
        } else {
            self.prove_candidate(&mut git, record, None)?;
            let head = self.ref_value(&mut git)?;
            let clean = git
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
                .is_empty();
            (head, clean)
        };
        let r = runtime(record)?;
        if Some(head) != r.candidate_commit
            || read_bounded(&r.index_path)?
                != *r.generated_index.as_ref().ok_or(OperationError::Journal)?
            || lock_path(&r.index_path).try_exists()?
            || !clean
        {
            return Err(OperationError::Refused(
                "checkpoint Root completion not clean/exact",
            ));
        }
        Ok(())
    }
}
fn runtime(record: &JournalRecord) -> OperationResult<&CheckpointRuntime> {
    record.checkpoint.as_ref().ok_or(OperationError::Journal)
}
fn parse(bytes: &[u8]) -> OperationResult<PlanWorkspace> {
    toml::from_str(std::str::from_utf8(bytes).map_err(|_| OperationError::Journal)?)
        .map_err(|_| OperationError::Journal)
}
fn text_file(path: &Path) -> OperationResult<String> {
    String::from_utf8(read_bounded(path)?).map_err(|_| OperationError::Journal)
}
fn optional_text(path: &Path) -> OperationResult<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => text_file(path).map(Some),
        Ok(_) => Err(OperationError::Refused("graph entry is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn oid(text: &str) -> OperationResult<CommitId> {
    Ok(CommitId::new(text.trim())?)
}
fn absolute(text: &str) -> OperationResult<PathBuf> {
    let path = PathBuf::from(text.trim());
    if !path.is_absolute() {
        return Err(OperationError::Evidence("Git returned non-absolute path"));
    }
    Ok(path.canonicalize()?)
}
fn index_path(text: &str, common: &Path) -> OperationResult<PathBuf> {
    let path = PathBuf::from(text);
    if !fs::symlink_metadata(&path)?.file_type().is_file() {
        return Err(OperationError::Refused(
            "actual index is not a regular Git-owned file",
        ));
    }
    let index = absolute(text)?;
    if index.parent() != Some(common) || index.file_name() != Some(std::ffi::OsStr::new("index")) {
        return Err(OperationError::Refused(
            "actual index redirected outside Root Git directory",
        ));
    }
    Ok(index)
}
fn lock_path(index: &Path) -> PathBuf {
    let mut path = index.as_os_str().to_os_string();
    path.push(".lock");
    path.into()
}
fn same_file(a: &Path, b: &Path) -> OperationResult<()> {
    let a_meta = fs::symlink_metadata(a)?;
    let b_meta = fs::symlink_metadata(b)?;
    if !a_meta.is_file() || !b_meta.is_file() {
        return Err(OperationError::Refused("lock witness not regular"));
    }
    #[cfg(unix)]
    let same = {
        use std::os::unix::fs::MetadataExt;
        a_meta.dev() == b_meta.dev() && a_meta.ino() == b_meta.ino() && a_meta.nlink() >= 2
    };
    #[cfg(windows)]
    let same = {
        use std::os::windows::fs::MetadataExt;
        let metadata_match = a_meta.creation_time() == b_meta.creation_time()
            && a_meta.last_write_time() == b_meta.last_write_time()
            && a_meta.file_size() == b_meta.file_size()
            && fs::read(a)? == fs::read(b)?;
        // Safe maintained file-lock primitives distinguish a replacement from
        // our hard-link witness without unstable Windows file-ID APIs/unsafe.
        let witness = OpenOptions::new().read(true).write(true).open(b)?;
        fs4::FileExt::try_lock(&witness)
            .map_err(|_| OperationError::Refused("lock witness already held"))?;
        let probe = OpenOptions::new().read(true).write(true).open(a)?;
        let contention = match fs4::FileExt::try_lock(&probe) {
            Ok(()) => {
                fs4::FileExt::unlock(&probe)?;
                false
            }
            Err(fs4::TryLockError::WouldBlock) => true,
            Err(fs4::TryLockError::Error(e)) => {
                fs4::FileExt::unlock(&witness)?;
                return Err(e.into());
            }
        };
        fs4::FileExt::unlock(&witness)?;
        metadata_match && contention
    };
    #[cfg(not(any(unix, windows)))]
    let same = false;
    if !same {
        return Err(OperationError::Refused(
            "index lock ownership witness differs",
        ));
    }
    Ok(())
}
