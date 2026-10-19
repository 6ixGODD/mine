//! Atomic local records. The caller's existing Root graph lock is borrowed,
//! never acquired recursively. Corrupt/missing records are never permission.
use super::*;
use crate::domain::lifecycle_operation::*;
use crate::infrastructure::file_lock::FileLock;
use crate::infrastructure::graph_checkpoint::{RuntimeValidationWitness, digest};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::{cell::RefCell, collections::HashMap, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalRecord {
    pub schema_version: u32,
    pub operation: LifecycleOperation,
    pub closure: Option<ClosureBinding>,
    /// Checkpoint-specific validated state; other operation kinds have none.
    pub checkpoint: Option<crate::infrastructure::graph_checkpoint::CheckpointRuntime>,
}

/// Observation-only projection. The full checkpoint runtime is validated but
/// not retained across history scans; recovery loads one exact record by ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationObservation {
    pub operation: LifecycleOperation,
    pub closure: Option<ClosureBinding>,
    pub plan_git: Option<crate::infrastructure::graph_checkpoint::PlanGitSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_predecessors:
        Option<Vec<crate::infrastructure::graph_checkpoint::WorkspacePredecessor>>,
}
impl JournalRecord {
    pub fn validate(&self) -> OperationResult<()> {
        self.validate_reusing(None).map(|_| ())
    }
    fn validate_reusing(
        &self,
        witness: Option<&RuntimeValidationWitness>,
    ) -> OperationResult<Option<RuntimeValidationWitness>> {
        if self.schema_version != 1 {
            return Err(OperationError::Journal);
        }
        // Mirror validate_progress's existing end-state restrictions on reads:
        // valid JSON alone must not release a contradictory pending operation.
        let f = self.operation.fields();
        if (f.outcome == OperationOutcome::Committed && !terminal(&self.operation))
            || (f.outcome == OperationOutcome::Refused
                && (f.kind == OperationKind::GraphCheckpoint || f.stage != initial_stage(f.kind)))
        {
            return Err(OperationError::Journal);
        }
        if (self.operation.fields().kind == OperationKind::GraphCheckpoint)
            != self.checkpoint.is_some()
        {
            return Err(OperationError::Journal);
        }
        if self.operation.fields().kind == OperationKind::GraphCheckpoint {
            if self.closure.is_some() {
                return Err(OperationError::Journal);
            }
        } else if self.closure.as_ref().map(ClosureBinding::kind)
            != Some(self.operation.fields().kind)
            || self.operation.fields().checkpoint.is_some()
        {
            return Err(OperationError::Journal);
        }
        if let Some(runtime) = &self.checkpoint {
            return Ok(Some(runtime.validate_reusing(&self.operation, witness)?));
        }
        Ok(None)
    }
}

/// Repository-wide read-only history. This type cannot begin, advance or
/// persist an operation; original workspace identities are never relabeled.
pub struct RepositoryJournal<'a> {
    root: PathBuf,
    repository: String,
    _lock: &'a FileLock,
    // Tiny proofs only, not retained graph bodies/history or Git authority.
    // This journal is constructed within one Root coordinator request.
    validation: RefCell<HashMap<OperationId, RuntimeMemo>>,
    validation_request: RefCell<Option<Arc<()>>>,
}

struct RuntimeMemo {
    witness: RuntimeValidationWitness,
    persisted_bytes: Option<crate::domain::graph::Sha256>,
}

/// Mutation authority remains bound to exactly one workspace. A repository
/// history observation is not an authorization to write into another one.
pub struct OperationStore<'a> {
    journal: RepositoryJournal<'a>,
    workspace: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    operations: Vec<OperationId>,
}
impl<'a> RepositoryJournal<'a> {
    pub fn under_lock(root: &Path, repository: &str, lock: &'a FileLock) -> OperationResult<Self> {
        let root = root.canonicalize()?;
        let expected = root
            .join(".mine/locks")
            .join(crate::infrastructure::toml_store::GRAPH_LOCK_NAME);
        if lock.path().canonicalize()? != expected.canonicalize()? {
            return Err(OperationError::Refused(
                "caller does not hold Root graph lock",
            ));
        }
        if repository.is_empty() {
            return Err(OperationError::Journal);
        }
        Ok(Self {
            root,
            repository: repository.into(),
            _lock: lock,
            validation: RefCell::new(HashMap::new()),
            validation_request: RefCell::new(
                crate::infrastructure::git_evidence::request_identity(),
            ),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn directory(&self) -> OperationResult<PathBuf> {
        regular_path(
            &self.root,
            &RepoPath::new(".mine/runtime/operations/.catalog")?,
        )?;
        Ok(regular_path(
            &self.root,
            &RepoPath::new(".mine/runtime/operations")?,
        )?)
    }
    fn bound(&self, record: &JournalRecord) -> OperationResult<()> {
        self.bound_record(record, false)
    }
    fn bound_record(&self, record: &JournalRecord, force_full: bool) -> OperationResult<()> {
        self.refresh_validation_request();
        let mut cache = self.validation.borrow_mut();
        let id = &record.operation.fields().operation_id;
        let witness = if force_full {
            None
        } else {
            cache.get(id).map(|memo| &memo.witness)
        };
        let checked = record.validate_reusing(witness)?;
        let f = record.operation.fields();
        if f.repository_id != self.repository {
            return Err(OperationError::Refused(
                "foreign journal repository/workspace",
            ));
        }
        if let Some(witness) = checked {
            let persisted_bytes = cache.get(id).and_then(|memo| memo.persisted_bytes.clone());
            cache.insert(
                id.clone(),
                RuntimeMemo {
                    witness,
                    persisted_bytes,
                },
            );
        }
        Ok(())
    }
    fn persisted(&self, record: &JournalRecord, bytes: &[u8]) {
        self.refresh_validation_request();
        if let Some(memo) = self
            .validation
            .borrow_mut()
            .get_mut(&record.operation.fields().operation_id)
        {
            memo.persisted_bytes = Some(digest(bytes));
        }
    }
    fn refresh_validation_request(&self) {
        let current = crate::infrastructure::git_evidence::request_identity();
        let mut previous = self.validation_request.borrow_mut();
        let same = match (&*previous, &current) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.validation.borrow_mut().clear();
            *previous = current;
        }
    }
    /// Exact read-only history: validate the catalog identity set and requested
    /// record. A query is not the complete observation required for admission.
    pub fn query(&self, id: &OperationId) -> OperationResult<JournalRecord> {
        if !self.catalog_ids()?.contains(id) {
            return Err(OperationError::Journal);
        }
        self.load_record(id)
    }
    pub(super) fn load_record(&self, id: &OperationId) -> OperationResult<JournalRecord> {
        self.refresh_validation_request();
        let path = OperationLocation::new(&self.root, id)?.resolve()?;
        let bytes = read_bounded(&path)?;
        let record: JournalRecord =
            serde_json::from_slice(&bytes).map_err(|_| OperationError::Journal)?;
        let unchanged = self
            .validation
            .borrow()
            .get(id)
            .is_some_and(|memo| memo.persisted_bytes.as_ref() == Some(&digest(&bytes)));
        // New/recovered requests and any externally changed bytes fully validate.
        // Internally persisted successors have already passed delta validation.
        self.bound_record(&record, !unchanged)?;
        if &record.operation.fields().operation_id != id {
            return Err(OperationError::Journal);
        }
        self.persisted(&record, &bytes);
        Ok(record)
    }
    fn catalog_ids(&self) -> OperationResult<Vec<OperationId>> {
        let dir = self.directory()?;
        if !dir.try_exists()? {
            return Ok(Vec::new());
        }
        let catalog_path = dir_for_catalog(&dir);
        let catalog = if catalog_path.try_exists()? {
            let catalog: Catalog = serde_json::from_slice(&read_bounded(&catalog_path)?)
                .map_err(|_| OperationError::Journal)?;
            if catalog.schema_version != 1
                || catalog
                    .operations
                    .windows(2)
                    .any(|pair| pair[0].as_str() >= pair[1].as_str())
            {
                return Err(OperationError::Journal);
            }
            Some(catalog)
        } else {
            None
        };
        let mut ids = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(OperationError::Journal)?;
            if let Some(id) = name.strip_suffix(".json") {
                ids.push(OperationId::parse(id)?);
                // The bounded catalog limits even a hostile/unclaimed directory
                // set; do not accumulate arbitrary numbers of extra filenames.
                if catalog
                    .as_ref()
                    .is_none_or(|catalog| ids.len() > catalog.operations.len())
                {
                    return Err(OperationError::Journal);
                }
            }
        }
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        if catalog.is_some_and(|catalog| catalog.operations != ids) {
            return Err(OperationError::Journal);
        }
        Ok(ids)
    }
    /// One catalog/directory comparison, then one fully validated record at a
    /// time. Errors at the end still invalidate the entire observation; there
    /// is no visitor that could mutate lifecycle state during a partial scan.
    pub fn observations(&self) -> OperationResult<Vec<OperationObservation>> {
        self.catalog_ids()?
            .iter()
            .map(|id| {
                crate::infrastructure::git_evidence::check_request_budget()?;
                let record = self.load_record(id)?;
                crate::infrastructure::git_evidence::check_request_budget()?;
                Ok(OperationObservation {
                    initial_predecessors: record.checkpoint.as_ref().and_then(|runtime| {
                        match &runtime.before {
                            crate::infrastructure::graph_checkpoint::GraphBefore::Absent {
                                predecessors,
                            } => Some(predecessors.clone()),
                            _ => None,
                        }
                    }),
                    plan_git: record
                        .checkpoint
                        .as_ref()
                        .and_then(|r| r.plan_git.summary()),
                    operation: record.operation,
                    closure: record.closure,
                })
                // The remaining record.checkpoint is dropped before the next load.
            })
            .collect()
    }
}

impl<'a> OperationStore<'a> {
    pub fn under_lock(
        root: &Path,
        repository: &str,
        workspace: &str,
        lock: &'a FileLock,
    ) -> OperationResult<Self> {
        if workspace.is_empty() {
            return Err(OperationError::Journal);
        }
        Ok(Self {
            journal: RepositoryJournal::under_lock(root, repository, lock)?,
            workspace: workspace.into(),
        })
    }
    pub fn root(&self) -> &Path {
        self.journal.root()
    }
    fn directory(&self) -> OperationResult<PathBuf> {
        self.journal.directory()
    }
    fn bound(&self, record: &JournalRecord) -> OperationResult<()> {
        self.journal.bound(record)?;
        self.workspace_bound(&record.operation)
    }
    pub(crate) fn validate_record(&self, record: &JournalRecord) -> OperationResult<()> {
        self.bound(record)
    }
    fn workspace_bound(&self, operation: &LifecycleOperation) -> OperationResult<()> {
        if operation.fields().workspace_id != self.workspace {
            return Err(OperationError::Refused(
                "foreign journal repository/workspace",
            ));
        }
        Ok(())
    }
    fn load_record(&self, id: &OperationId) -> OperationResult<JournalRecord> {
        let record = self.journal.load_record(id)?;
        self.workspace_bound(&record.operation)?;
        Ok(record)
    }
    pub fn query(&self, id: &OperationId) -> OperationResult<JournalRecord> {
        let record = self.journal.query(id)?;
        self.workspace_bound(&record.operation)?;
        Ok(record)
    }
    fn complete_observations(&self) -> OperationResult<Vec<OperationObservation>> {
        let observations = self.journal.observations()?;
        if observations
            .iter()
            .any(|record| record.operation.fields().workspace_id != self.workspace)
        {
            super::history::predecessors(&self.journal, &observations, Some(&self.workspace))?;
        }
        Ok(observations)
    }
    pub fn observations(&self) -> OperationResult<Vec<OperationObservation>> {
        Ok(self
            .complete_observations()?
            .into_iter()
            .filter(|record| record.operation.fields().workspace_id == self.workspace)
            .collect())
    }
    /// Read-only exact terminal-acceptance lookup. Validate the complete catalog
    /// once and drop each large runtime before the next; retain only a matching
    /// ID until the full scan succeeds. No callback can write on a partial scan.
    pub(crate) fn checkpoint_for_acceptance(
        &self,
        operation: &LifecycleOperation,
    ) -> OperationResult<Option<JournalRecord>> {
        let f = operation.fields();
        if f.repository_id != self.journal.repository
            || f.workspace_id != self.workspace
            || f.stage
                != OperationStage::Acceptance(
                    crate::domain::lifecycle_operation::AcceptanceStage::Accepted,
                )
        {
            return Err(OperationError::Refused(
                "foreign terminal checkpoint lookup",
            ));
        }
        let id = &f
            .plan
            .as_ref()
            .ok_or(OperationError::Journal)?
            .fields()
            .plan_id;
        let mut found = None;
        for record_id in self
            .observations()?
            .into_iter()
            .filter(|record| record.operation.fields().kind == OperationKind::GraphCheckpoint)
            .map(|record| record.operation.fields().operation_id.clone())
        {
            let record = self.load_record(&record_id)?;
            if let Some(runtime) = &record.checkpoint {
                let graph: crate::domain::graph::PlanWorkspace =
                    toml::from_str(&runtime.source_after).map_err(|_| OperationError::Journal)?;
                if graph.get(id).and_then(|node| node.acceptance.as_ref()) == Some(operation) {
                    if found.is_some() {
                        return Err(OperationError::Refused("ambiguous terminal checkpoint"));
                    }
                    found = Some(record_id);
                }
            }
        }
        found.map(|id| self.load_record(&id)).transpose()
    }
    pub fn barriers(&self) -> OperationResult<BarrierFacts> {
        let mut facts = BarrierFacts::default();
        for record in self.observations()? {
            facts.observe(&record.operation);
        }
        facts.held_plans.sort();
        facts.held_plans.dedup();
        Ok(facts)
    }
    pub fn begin(&self, record: &JournalRecord) -> OperationResult<JournalRecord> {
        self.bound(record)?;
        let f = record.operation.fields();
        let path = OperationLocation::new(self.root(), &f.operation_id)?.resolve()?;
        if path.try_exists()? {
            let existing = self.query(&f.operation_id)?;
            if existing == *record {
                self.observations()?;
                return Ok(existing);
            }
            return Err(OperationError::Refused("operation identity already exists"));
        }
        if f.stage_revision != 0
            || f.stage != initial_stage(f.kind)
            || f.outcome != OperationOutcome::Pending
        {
            return Err(OperationError::Refused(
                "begin requires pending initial intent",
            ));
        }
        let records = self.complete_observations()?;
        let mut barriers = BarrierFacts::default();
        for existing in &records {
            if existing.operation.fields().workspace_id == self.workspace {
                barriers.observe(&existing.operation);
            }
        }
        if !barriers.global.is_empty() {
            return Err(OperationError::Refused("pending workspace checkpoint"));
        }
        if let Some(plan) = &f.plan {
            if barriers.held_plans.contains(&plan.fields().plan_id) {
                return Err(OperationError::Refused("Plan already has active closure"));
            }
        }
        // Refuse known serialization/size errors before claiming an identity.
        let bytes = record_bytes(record)?;
        // Publish the claim first. A crash/missing journal is an explicit
        // recovery blocker rather than silently releasing a Plan's closure.
        // The filtered current-workspace view is not the repository catalog.
        // Preserve every sealed historical operation ID when publishing a claim.
        let mut ids = records
            .iter()
            .map(|record| record.operation.fields().operation_id.clone())
            .collect::<Vec<_>>();
        ids.push(f.operation_id.clone());
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let catalog = Catalog {
            schema_version: 1,
            operations: ids,
        };
        let catalog_bytes = serde_json::to_vec(&catalog).map_err(|_| OperationError::Journal)?;
        if catalog_bytes.len() > 32 * 1024 * 1024 {
            return Err(OperationError::Evidence("catalog exceeds bounded size"));
        }
        atomic_replace(&dir_for_catalog(&self.directory()?), &catalog_bytes)?;
        atomic_replace(&path, &bytes)?;
        self.journal.persisted(record, &bytes);
        Ok(record.clone())
    }
    pub fn advance(&self, expected: u64, record: &JournalRecord) -> OperationResult<JournalRecord> {
        self.bound(record)?;
        let current = self.query(&record.operation.fields().operation_id)?;
        // A lost successful response is idempotent; never repeat its effect.
        if current == *record {
            self.observations()?;
            return Ok(current);
        }
        if current.operation.fields().stage_revision != expected {
            return Err(OperationError::Revision {
                expected,
                actual: current.operation.fields().stage_revision,
            });
        }
        validate_progress(&current.operation, &record.operation)?;
        if current.closure != record.closure {
            return Err(OperationError::Refused("closure input changed"));
        }
        if let (Some(a), Some(b)) = (&current.checkpoint, &record.checkpoint) {
            b.validate_successor(a)?;
        }
        self.observations()?;
        self.persist(record)?;
        Ok(record.clone())
    }
    fn persist(&self, record: &JournalRecord) -> OperationResult<()> {
        let path = OperationLocation::new(self.root(), &record.operation.fields().operation_id)?
            .resolve()?;
        let bytes = record_bytes(record)?;
        atomic_replace(&path, &bytes)?;
        self.journal.persisted(record, &bytes);
        Ok(())
    }
}
fn record_bytes(record: &JournalRecord) -> OperationResult<Vec<u8>> {
    let bytes = serde_json::to_vec(record).map_err(|_| OperationError::Journal)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(OperationError::Evidence("journal exceeds bounded size"));
    }
    Ok(bytes)
}
fn dir_for_catalog(directory: &Path) -> PathBuf {
    directory.join(".catalog")
}

pub fn read_bounded(path: &Path) -> OperationResult<Vec<u8>> {
    if !fs::symlink_metadata(path)?.file_type().is_file()
        || fs::metadata(path)?.len() > 32 * 1024 * 1024
    {
        return Err(OperationError::Evidence("not a bounded regular file"));
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(OperationError::Evidence("file exceeds bounded size"));
    }
    Ok(bytes)
}
/// Same-directory exclusive temp, checked flush/fsync, atomic replacement.
/// Directory fsync is available on POSIX; Windows uses the native rename API.
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> OperationResult<()> {
    let parent = path.parent().ok_or(OperationError::Journal)?;
    fs::create_dir_all(parent)?;
    if path.try_exists()? && !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(OperationError::Journal);
    }
    let temp = parent.join(format!(".operation-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> OperationResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() && temp.is_file() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::domain::graph::{CommitId, PlanWorkspace};
    use crate::infrastructure::{
        graph_checkpoint::{CheckpointRuntime, GraphBefore, PlanGitEffects},
        toml_store::render_markdown,
    };

    // Pure journal-format fixture, not proof of a real Git checkpoint.
    fn checkpoint_record(root: &Path) -> JournalRecord {
        let before = PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 3,
            project_id: "fixture".into(),
            workspace_id: "workspace".into(),
            stable_branch: "master".into(),
            integration_branch: "dev".into(),
            stable_baseline_commit: "a".repeat(40),
            design_root: "docs/design/index.md".into(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![],
        };
        let mut after = before.clone();
        after.revision += 1;
        let index = vec![1, 2, 3];
        let fields = CheckpointFields {
            schema_version: 1,
            stage: CheckpointStage::Intent,
            revision_before: 3,
            revision_after: None,
            source_sha256: None,
            view_sha256: None,
            original_head: CommitId::new("a".repeat(40)).unwrap(),
            original_index_sha256: digest(&index),
            candidate_tree: None,
            candidate_commit: None,
            generated_index_sha256: None,
            owned_lock_sha256: None,
        };
        JournalRecord {
            schema_version: 1,
            operation: LifecycleOperation::new(OperationFields {
                schema_version: 1,
                operation_id: OperationId::generate(),
                stage_revision: 0,
                kind: OperationKind::GraphCheckpoint,
                stage: OperationStage::GraphCheckpoint(CheckpointStage::Intent),
                outcome: OperationOutcome::Pending,
                repository_id: "repository".into(),
                workspace_id: "workspace".into(),
                actor: "fixture".into(),
                observed_graph_revision: 3,
                plan: None,
                registered_worktree: None,
                review: None,
                integrated_commit: None,
                verification_evidence: vec![],
                cleanup: None,
                checkpoint: Some(CheckpointFacts::new(fields).unwrap()),
                diagnostics: vec![],
            })
            .unwrap(),
            closure: None,
            checkpoint: Some(CheckpointRuntime {
                root: root.into(),
                common_dir: root.join(".git"),
                index_path: root.join(".git/index"),
                integration_ref: "refs/heads/dev".into(),
                config_sha256: digest(b"fixture config"),
                source_path: RepoPath::new("docs/plan/execution-graph.toml").unwrap(),
                view_path: RepoPath::new("docs/plan/execution-graph.md").unwrap(),
                before: GraphBefore::Present {
                    source: toml::to_string(&before).unwrap(),
                    view: render_markdown(&before).unwrap(),
                },
                source_after: toml::to_string(&after).unwrap(),
                view_after: render_markdown(&after).unwrap(),
                original_index: index,
                generated_index: None,
                candidate_tree: None,
                candidate_commit: None,
                cas_attempted: false,
                lock_claimed: false,
                lock_sha256: None,
                plan_git: PlanGitEffects::None,
            }),
        }
    }

    #[test]
    fn checkpoint_validation_witness_reuses_only_exact_immutable_binding() {
        let dir = tempfile::tempdir().unwrap();
        let record = checkpoint_record(dir.path());
        let witness = record.validate_reusing(None).unwrap().unwrap();
        let reused = record.validate_reusing(Some(&witness)).unwrap().unwrap();
        assert_eq!(reused.generation(), witness.generation());
        for corruption in 0..5 {
            let mut changed = record.clone();
            let runtime = changed.checkpoint.as_mut().unwrap();
            match corruption {
                0 => runtime.source_after = "invalid = [".into(),
                1 => runtime.view_after.push('\n'),
                2 => runtime.integration_ref = "refs/heads/foreign".into(),
                3 => runtime.cas_attempted = true,
                _ => {
                    let mut fields = changed.operation.fields().clone();
                    fields.workspace_id = "foreign-workspace".into();
                    changed.operation = LifecycleOperation::new(fields).unwrap();
                }
            }
            assert!(
                changed.validate_reusing(Some(&witness)).is_err(),
                "corruption{corruption}"
            );
        }
        let mut successor = record.clone();
        let mut fields = successor.operation.fields().clone();
        fields.stage_revision += 1;
        fields.stage = OperationStage::GraphCheckpoint(CheckpointStage::GitWorkspaceMutated);
        let mut checkpoint = fields.checkpoint.as_ref().unwrap().fields().clone();
        checkpoint.stage = CheckpointStage::GitWorkspaceMutated;
        fields.checkpoint = Some(CheckpointFacts::new(checkpoint).unwrap());
        successor.operation = LifecycleOperation::new(fields).unwrap();
        assert_eq!(
            successor
                .validate_reusing(Some(&witness))
                .unwrap()
                .unwrap()
                .generation(),
            witness.generation()
        );
        validate_progress(&record.operation, &successor.operation).unwrap();
    }

    #[test]
    fn checkpoint_validation_disk_change_or_new_request_forces_full_check() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let guard = crate::infrastructure::file_lock::acquire_exclusive(
            &root.join(".mine/locks/execution-graph.lock"),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        let store = OperationStore::under_lock(root, "repository", "workspace", &guard).unwrap();
        let record = checkpoint_record(root);
        let id = &record.operation.fields().operation_id;
        store.begin(&record).unwrap();
        let generation = || {
            store
                .journal
                .validation
                .borrow()
                .get(id)
                .unwrap()
                .witness
                .generation()
        };
        let initial = generation();
        assert_eq!(store.query(id).unwrap(), record);
        assert_eq!(generation(), initial);
        let path = OperationLocation::new(root, id).unwrap().resolve().unwrap();
        // Same decoded record, different exact persisted bytes: still revalidate.
        let mut changed = fs::read(&path).unwrap();
        changed.push(b'\n');
        fs::write(&path, &changed).unwrap();
        assert_eq!(store.query(id).unwrap(), record);
        assert_ne!(generation(), initial);
        let after_external_change = generation();
        assert_eq!(store.query(id).unwrap(), record);
        assert_eq!(generation(), after_external_change);
        let fresh = OperationStore::under_lock(root, "repository", "workspace", &guard).unwrap();
        assert_eq!(fresh.query(id).unwrap(), record);
        assert_ne!(
            fresh
                .journal
                .validation
                .borrow()
                .get(id)
                .unwrap()
                .witness
                .generation(),
            after_external_change
        );
        {
            let _new_request = crate::infrastructure::git_evidence::GitRequestScope::new();
            assert_eq!(store.query(id).unwrap(), record);
            assert_ne!(generation(), after_external_change);
            let current_request = generation();
            assert_eq!(store.query(id).unwrap(), record);
            assert_eq!(generation(), current_request);
        }
        let mut corrupt = record.clone();
        corrupt.checkpoint.as_mut().unwrap().source_after = "invalid = [".into();
        fs::write(&path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
        let bad = fs::read(&path).unwrap();
        assert!(store.query(id).is_err());
        assert_eq!(fs::read(&path).unwrap(), bad);
    }
}
