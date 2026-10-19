#![forbid(unsafe_code)]
#[path = "support/config.rs"]
mod fixture_config;
use mine::application::operation_service::*;
use mine::domain::{graph::*, lifecycle_operation::*};
use mine::infrastructure::{
    file_lock, graph_checkpoint::digest, operation_store::*, toml_store::GRAPH_LOCK_NAME,
};
use std::{fs, path::Path, time::Duration};

fn operation(plan_id: &str) -> LifecycleOperation {
    let plan = PlanIdentity::new(PlanIdentityFields {
        repository_id: "repo".into(),
        workspace_id: "ws".into(),
        plan_id: plan_id.into(),
        plan_path: RepoPath::new(format!(".mine/plan/{plan_id}-fixture.md")).unwrap(),
        body_sha256: Some(digest(b"immutable plan")),
        lineage_id: plan_id.into(),
        branch: format!("plan/{plan_id}-fixture"),
    })
    .unwrap();
    LifecycleOperation::new(OperationFields {
        schema_version: 1,
        operation_id: OperationId::generate(),
        stage_revision: 0,
        kind: OperationKind::Acceptance,
        stage: OperationStage::Acceptance(AcceptanceStage::Reviewed),
        outcome: OperationOutcome::Pending,
        repository_id: "repo".into(),
        workspace_id: "ws".into(),
        actor: "reviewer".into(),
        observed_graph_revision: 7,
        plan: Some(plan),
        registered_worktree: Some(RepoPath::new(format!(".mine/worktrees/{plan_id}")).unwrap()),
        review: Some(ReviewEvidence {
            reviewed_tip: CommitId::new("a".repeat(40)).unwrap(),
            report: FileIdentity {
                path: RepoPath::new(format!(".mine/plan/reports/{plan_id}-fixture-review.md"))
                    .unwrap(),
                bytes: 17,
                sha256: digest(b"Verdict: ACCEPTED"),
            },
        }),
        integrated_commit: None,
        verification_evidence: vec![],
        cleanup: None,
        checkpoint: None,
        diagnostics: vec![],
    })
    .unwrap()
}
fn record(op: LifecycleOperation) -> JournalRecord {
    JournalRecord {
        schema_version: 1,
        operation: op,
        closure: Some(ClosureBinding::Acceptance),
        checkpoint: None,
    }
}
fn lock(root: &Path) -> file_lock::FileLock {
    file_lock::acquire_exclusive(
        &root.join(".mine/locks").join(GRAPH_LOCK_NAME),
        Duration::from_secs(1),
    )
    .unwrap()
}

/// Imported storage-only history, not evidence that a release was completed.
/// Reusing a Plan ID across workspaces must not relabel either operation.
fn repository_history_fixture(root: &Path) -> [JournalRecord; 2] {
    // mine-release-allow-plan-reference: repository-history identity fixture
    let first = operation("plan98");
    let mut first_fields = first.fields().clone();
    first_fields.operation_id = OperationId::parse("00000000-0000-4000-8000-000000000001").unwrap();
    let first = record(LifecycleOperation::new(first_fields).unwrap());
    let mut second_fields = first.operation.fields().clone();
    second_fields.operation_id =
        OperationId::parse("00000000-0000-4000-8000-000000000002").unwrap();
    second_fields.workspace_id = "another-workspace".into();
    let mut plan = second_fields.plan.as_ref().unwrap().fields().clone();
    plan.workspace_id = second_fields.workspace_id.clone();
    second_fields.plan = Some(PlanIdentity::new(plan).unwrap());
    let second = record(LifecycleOperation::new(second_fields).unwrap());
    let directory = root.join(".mine/runtime/operations");
    fs::create_dir_all(&directory).unwrap();
    let records = [first, second];
    for record in &records {
        record.validate().unwrap();
        fs::write(
            directory.join(format!(
                "{}.json",
                record.operation.fields().operation_id.as_str()
            )),
            serde_json::to_vec(record).unwrap(),
        )
        .unwrap();
    }
    fs::write(directory.join(".catalog"), serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"operations":records.iter().map(|record|record.operation.fields().operation_id.as_str()).collect::<Vec<_>>()
    })).unwrap()).unwrap();
    records
}

#[test]
fn repository_history_reads_original_workspaces_without_granting_cross_workspace_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let records = repository_history_fixture(root);
    let guard = lock(root);
    let journal = RepositoryJournal::under_lock(root, "repo", &guard).unwrap();
    let directory = root.join(".mine/runtime/operations");
    let catalog = fs::read(directory.join(".catalog")).unwrap();
    let before = records
        .iter()
        .map(|record| {
            fs::read(directory.join(format!(
                "{}.json",
                record.operation.fields().operation_id.as_str()
            )))
            .unwrap()
        })
        .collect::<Vec<_>>();
    let observed = journal.observations().unwrap();
    assert_eq!(observed.len(), 2);
    for (observation, record) in observed.iter().zip(&records) {
        assert_eq!(observation.operation, record.operation);
        assert_eq!(
            journal
                .query(&record.operation.fields().operation_id)
                .unwrap(),
            *record
        );
    }
    assert_ne!(
        observed[0].operation.fields().workspace_id,
        observed[1].operation.fields().workspace_id
    );
    assert_eq!(
        observed[0]
            .operation
            .fields()
            .plan
            .as_ref()
            .unwrap()
            .fields()
            .plan_id,
        observed[1]
            .operation
            .fields()
            .plan
            .as_ref()
            .unwrap()
            .fields()
            .plan_id
    );
    let writer = OperationStore::under_lock(root, "repo", "ws", &guard).unwrap();
    let foreign = &records[1];
    assert!(
        writer
            .query(&foreign.operation.fields().operation_id)
            .is_err()
    );
    assert!(writer.begin(foreign).is_err());
    assert!(writer.advance(0, foreign).is_err());
    assert!(
        writer.observations().is_err(),
        "unsealed prior workspaces are not silently admitted"
    );
    // mine-release-allow-plan-reference: same-workspace attempted new-write fixture
    assert!(writer.begin(&record(operation("plan99"))).is_err());
    assert!(
        RepositoryJournal::under_lock(root, "foreign-repository", &guard)
            .unwrap()
            .observations()
            .is_err()
    );
    assert_eq!(fs::read(directory.join(".catalog")).unwrap(), catalog);
    for (record, bytes) in records.iter().zip(before) {
        assert_eq!(
            fs::read(directory.join(format!(
                "{}.json",
                record.operation.fields().operation_id.as_str()
            )))
            .unwrap(),
            bytes
        );
    }
    assert_eq!(fs::read_dir(directory).unwrap().count(), 3);
}

#[test]
fn repository_history_refuses_late_corruption_missing_records_and_terminal_foreign_identity() {
    for failure in 0..3 {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let records = repository_history_fixture(root);
        let guard = lock(root);
        let journal = RepositoryJournal::under_lock(root, "repo", &guard).unwrap();
        let directory = root.join(".mine/runtime/operations");
        let path = directory.join(format!(
            "{}.json",
            records[1].operation.fields().operation_id.as_str()
        ));
        match failure {
            0 => fs::write(&path, b"malformed late record").unwrap(),
            1 => fs::remove_file(&path).unwrap(),
            2 => {
                let mut fields = records[1].operation.fields().clone();
                fields.repository_id = "foreign-repository".into();
                fields.outcome = OperationOutcome::Refused;
                let mut plan = fields.plan.as_ref().unwrap().fields().clone();
                plan.repository_id = fields.repository_id.clone();
                fields.plan = Some(PlanIdentity::new(plan).unwrap());
                let foreign = record(LifecycleOperation::new(fields).unwrap());
                foreign.validate().unwrap();
                assert!(terminal(&foreign.operation));
                fs::write(&path, serde_json::to_vec(&foreign).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        let catalog = fs::read(directory.join(".catalog")).unwrap();
        let after = fs::read(&path).ok();
        assert!(journal.observations().is_err(), "failure={failure}");
        assert!(
            journal
                .query(&records[1].operation.fields().operation_id)
                .is_err()
        );
        assert_eq!(fs::read(directory.join(".catalog")).unwrap(), catalog);
        assert_eq!(fs::read(&path).ok(), after);
    }
}

/// Deliberately large, structurally valid checkpoint records. This is storage
/// observation evidence, not a claim that Git checkpoint effects ran here.
fn large_history(root: &Path, count: usize) -> Vec<OperationId> {
    use mine::infrastructure::{graph_checkpoint::CheckpointRuntime, toml_store::render_markdown};
    let directory = root.join(".mine/runtime/operations");
    fs::create_dir_all(&directory).unwrap();
    // mine-release-allow-plan-reference: bounded-history checkpoint fixture
    let plan = "plan97";
    let node = serde_json::from_value(serde_json::json!({
        "id":plan,"path":format!(".mine/plan/{plan}-fixture.md"),"title":"x".repeat(512*1024),
        "status":"READY","branch":format!("plan/{plan}-fixture"),"lineage_id":plan,
        "worktree_path":format!(".mine/worktrees/{plan}"),"design_references":[".mine/design/index.md"],"exclusive_write_paths":["src/fixture"]
    })).unwrap();
    let before = PlanWorkspace {
        schema_version: 7,
        revision: 7,
        project_id: "fixture".into(),
        workspace_id: "ws".into(),
        stable_branch: "master".into(),
        integration_branch: "dev".into(),
        stable_baseline_commit: "a".repeat(40),
        design_root: ".mine/design/index.md".into(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans: vec![node],
    };
    mine::domain::validation::validate(&before).unwrap();
    let mut after = before.clone();
    after.revision = 8;
    let original_index = vec![42u8; 128 * 1024];
    let mut ids = Vec::new();
    for ordinal in 0..count {
        let id = OperationId::parse(format!("00000000-0000-4000-8000-{ordinal:012x}")).unwrap();
        let pending = ordinal == 0;
        let stage = if pending {
            CheckpointStage::Intent
        } else {
            CheckpointStage::Complete
        };
        let source_after = toml::to_string(&after).unwrap();
        let view_after = render_markdown(&after).unwrap();
        let tree = CommitId::new("b".repeat(40)).unwrap();
        let commit = CommitId::new("c".repeat(40)).unwrap();
        let runtime = CheckpointRuntime {
            plan_git: mine::infrastructure::graph_checkpoint::PlanGitEffects::None,
            root: root.canonicalize().unwrap(),
            common_dir: root.join(".git"),
            index_path: root.join(".git/index"),
            integration_ref: "refs/heads/dev".into(),
            config_sha256: digest(b"fixture configuration"),
            source_path: RepoPath::new(".mine/plan/execution-graph.toml").unwrap(),
            view_path: RepoPath::new(".mine/plan/execution-graph.md").unwrap(),
            before: mine::infrastructure::graph_checkpoint::GraphBefore::Present {
                source: toml::to_string(&before).unwrap(),
                view: render_markdown(&before).unwrap(),
            },
            source_after,
            view_after,
            original_index: original_index.clone(),
            generated_index: (!pending).then(|| original_index.clone()),
            candidate_tree: (!pending).then(|| tree.clone()),
            candidate_commit: (!pending).then(|| commit.clone()),
            cas_attempted: !pending,
            lock_claimed: !pending,
            lock_sha256: (!pending).then(|| digest(&original_index)),
        };
        let checkpoint = CheckpointFacts::new(CheckpointFields {
            schema_version: 1,
            stage,
            revision_before: 7,
            revision_after: (!pending).then_some(8),
            source_sha256: (!pending).then(|| digest(runtime.source_after.as_bytes())),
            view_sha256: (!pending).then(|| digest(runtime.view_after.as_bytes())),
            original_head: CommitId::new("a".repeat(40)).unwrap(),
            original_index_sha256: digest(&original_index),
            candidate_tree: runtime.candidate_tree.clone(),
            candidate_commit: runtime.candidate_commit.clone(),
            generated_index_sha256: runtime.generated_index.as_ref().map(|bytes| digest(bytes)),
            owned_lock_sha256: runtime.lock_sha256.clone(),
        })
        .unwrap();
        let operation = LifecycleOperation::new(OperationFields {
            schema_version: 1,
            operation_id: id.clone(),
            stage_revision: if pending { 0 } else { 9 },
            kind: OperationKind::GraphCheckpoint,
            stage: OperationStage::GraphCheckpoint(stage),
            outcome: if pending {
                OperationOutcome::Pending
            } else {
                OperationOutcome::Committed
            },
            repository_id: "repo".into(),
            workspace_id: "ws".into(),
            actor: "fixture".into(),
            observed_graph_revision: 7,
            plan: None,
            registered_worktree: None,
            review: None,
            integrated_commit: None,
            verification_evidence: vec![],
            cleanup: None,
            checkpoint: Some(checkpoint),
            diagnostics: vec![],
        })
        .unwrap();
        let record = JournalRecord {
            schema_version: 1,
            operation,
            closure: None,
            checkpoint: Some(runtime),
        };
        record.validate().unwrap();
        fs::write(
            directory.join(format!("{}.json", id.as_str())),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        ids.push(id);
    }
    fs::write(
        directory.join(".catalog"),
        serde_json::to_vec(&serde_json::json!({"schema_version":1,"operations":ids})).unwrap(),
    )
    .unwrap();
    ids
}

#[test]
fn initial_checkpoint_requires_explicit_absence_and_predecessor_bindings() {
    use mine::infrastructure::{graph_checkpoint::GraphBefore, toml_store::render_markdown};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let ids = large_history(root, 1);
    let guard = lock(root);
    let journal = RepositoryJournal::under_lock(root, "repo", &guard).unwrap();
    let mut value = serde_json::to_value(journal.query(&ids[0]).unwrap()).unwrap();
    let mut graph: PlanWorkspace =
        toml::from_str(value["checkpoint"]["source_after"].as_str().unwrap()).unwrap();
    graph.revision = 1;
    graph.plans.clear();
    value["operation"]["observed_graph_revision"] = serde_json::json!(0);
    value["operation"]["checkpoint"]["revision_before"] = serde_json::json!(0);
    value["checkpoint"]["before"] = serde_json::json!({"state":"absent", "predecessors":[]});
    value["checkpoint"]["source_after"] = serde_json::json!(toml::to_string(&graph).unwrap());
    value["checkpoint"]["view_after"] = serde_json::json!(render_markdown(&graph).unwrap());
    let valid: JournalRecord = serde_json::from_value(value.clone()).unwrap();
    valid.validate().unwrap();
    assert!(valid.checkpoint.as_ref().unwrap().before.present().is_err());
    assert_eq!(
        valid.checkpoint.unwrap().before,
        GraphBefore::Absent {
            predecessors: vec![]
        }
    );
    // Neither a missing tag nor an omitted predecessor set is an empty history.
    for before in [
        serde_json::json!({"predecessors":[]}),
        serde_json::json!({"state":"absent"}),
        serde_json::json!({"state":"absent","predecessors":null}),
        serde_json::json!({"state":"present","source":"","view":""}),
        serde_json::json!({"state":"absent","predecessors":[],"source":""}),
    ] {
        let mut malformed = value.clone();
        malformed["checkpoint"]["before"] = before;
        assert!(
            serde_json::from_value::<JournalRecord>(malformed)
                .map(|record| record.validate().is_err())
                .unwrap_or(true)
        );
    }
    for pointer in ["/checkpoint/before", "/checkpoint/before/predecessors"] {
        let mut missing = value.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        missing
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(serde_json::from_value::<JournalRecord>(missing).is_err());
    }
    // Explicit absence is valid only for the empty initial 0 -> 1 graph,
    // never an existing graph mutation or a nonempty candidate.
    for mutation in 0..3 {
        let mut invalid = value.clone();
        if mutation == 0 {
            invalid["operation"]["observed_graph_revision"] = serde_json::json!(7);
            invalid["operation"]["checkpoint"]["revision_before"] = serde_json::json!(7);
        } else {
            let mut wrong = graph.clone();
            if mutation == 1 {
                wrong.revision = 2;
            } else {
                wrong.schema_version -= 1;
            }
            invalid["checkpoint"]["source_after"] =
                serde_json::json!(toml::to_string(&wrong).unwrap());
            invalid["checkpoint"]["view_after"] =
                serde_json::json!(render_markdown(&wrong).unwrap());
        }
        assert!(
            serde_json::from_value::<JournalRecord>(invalid)
                .map(|record| record.validate().is_err())
                .unwrap_or(true)
        );
    }
}

#[test]
fn repository_history_refuses_internal_checkpoint_rebinding() {
    use mine::infrastructure::toml_store::render_markdown;
    for failure in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ids = large_history(root, 2);
        let guard = lock(root);
        let path = root.join(format!(".mine/runtime/operations/{}.json", ids[1].as_str()));
        let mut changed: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if failure < 4 {
            let (source, view) = if failure % 2 == 0 {
                (
                    vec!["checkpoint", "source_after"],
                    vec!["checkpoint", "view_after"],
                )
            } else {
                (
                    vec!["checkpoint", "before", "source"],
                    vec!["checkpoint", "before", "view"],
                )
            };
            let pointer = |parts: &[&str]| format!("/{}", parts.join("/"));
            let mut graph: PlanWorkspace = toml::from_str(
                changed
                    .pointer(&pointer(&source))
                    .unwrap()
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            if failure < 2 {
                graph.workspace_id = "foreign-runtime-workspace".into();
            } else {
                graph.revision += 1;
            }
            *changed.pointer_mut(&pointer(&source)).unwrap() =
                serde_json::json!(toml::to_string(&graph).unwrap());
            *changed.pointer_mut(&pointer(&view)).unwrap() =
                serde_json::json!(render_markdown(&graph).unwrap());
        } else {
            changed["checkpoint"]["view_after"] =
                serde_json::json!("foreign projection with matching stored digest\n");
        }
        // Preserve the superficial digest checks: the original workspace,
        // observer/revision and canonical source/view bindings must still hold.
        changed["operation"]["checkpoint"]["source_sha256"] = serde_json::json!(digest(
            changed["checkpoint"]["source_after"]
                .as_str()
                .unwrap()
                .as_bytes()
        ));
        changed["operation"]["checkpoint"]["view_sha256"] = serde_json::json!(digest(
            changed["checkpoint"]["view_after"]
                .as_str()
                .unwrap()
                .as_bytes()
        ));
        fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        let bytes = fs::read(&path).unwrap();
        let catalog_path = root.join(".mine/runtime/operations/.catalog");
        let catalog = fs::read(&catalog_path).unwrap();
        let journal = RepositoryJournal::under_lock(root, "repo", &guard).unwrap();
        assert!(
            journal.observations().is_err(),
            "failure={failure}: accepted rebound checkpoint runtime"
        );
        assert!(
            journal.query(&ids[1]).is_err(),
            "failure={failure}: historical query accepted rebinding"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read(catalog_path).unwrap(), catalog);
    }
}

#[test]
fn compact_observation_validates_large_history_and_late_corruption_after_early_pending() {
    let dir = tempfile::tempdir().unwrap();
    let guard = lock(dir.path());
    let ids = large_history(dir.path(), 24);
    let store = OperationStore::under_lock(dir.path(), "repo", "ws", &guard).unwrap();
    let serialized_bytes: u64 = ids
        .iter()
        .map(|id| {
            fs::metadata(
                dir.path()
                    .join(format!(".mine/runtime/operations/{}.json", id.as_str())),
            )
            .unwrap()
            .len()
        })
        .sum();
    let started = std::time::Instant::now();
    let observations = store.observations().unwrap();
    let elapsed = started.elapsed();
    assert_eq!(observations.len(), ids.len());
    let compact_bytes = serde_json::to_vec(&observations).unwrap().len();
    assert!(serialized_bytes > 32 * 1024 * 1024);
    assert!((compact_bytes as u64) * 100 < serialized_bytes);
    let barriers = store.barriers().unwrap();
    assert_eq!(barriers.global.len(), 1);
    assert_eq!(barriers.global[0].operation_id, ids[0]);
    assert!(barriers.held_plans.is_empty());
    eprintln!(
        "JOURNAL_SCAN records={} serialized_bytes={serialized_bytes} compact_bytes={compact_bytes} elapsed_ms={}",
        observations.len(),
        elapsed.as_millis()
    );
    let path = dir.path().join(format!(
        ".mine/runtime/operations/{}.json",
        ids.last().unwrap().as_str()
    ));
    fs::write(&path, b"{late-corruption").unwrap();
    let catalog_path = dir.path().join(".mine/runtime/operations/.catalog");
    let catalog = fs::read(&catalog_path).unwrap();
    assert!(store.observations().is_err());
    assert!(store.barriers().is_err());
    // mine-release-allow-plan-reference: new admission must scan past pending
    assert!(store.begin(&record(operation("plan98"))).is_err());
    assert_eq!(fs::read(catalog_path).unwrap(), catalog);
    assert_eq!(fs::read(path).unwrap(), b"{late-corruption");
}

/// Bounded observation fixture refuses every input except its pinned expected
/// facts; real checkpoint Git authority is tested in graph_checkpoint_kernel.
struct Observed {
    expected: LifecycleOperation,
    fail: bool,
    calls: usize,
}
impl OperationEvidenceProvider for Observed {
    fn observe(
        &mut self,
        value: &LifecycleOperation,
        binding: &ClosureBinding,
    ) -> OperationResult<()> {
        self.calls += 1;
        if self.fail || value != &self.expected || binding != &ClosureBinding::Acceptance {
            Err(OperationError::Evidence("fixture observations disagree"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn atomic_intent_roundtrip_one_active_closure_and_per_plan_not_global_hold() {
    let dir = tempfile::tempdir().unwrap();
    let guard = lock(dir.path());
    let store = OperationStore::under_lock(dir.path(), "repo", "ws", &guard).unwrap();
    // mine-release-allow-plan-reference: exact canonical closure identity fixture
    let op = operation("plan01");
    let r = record(op.clone());
    store.begin(&r).unwrap();
    assert_eq!(store.query(&op.fields().operation_id).unwrap(), r);
    assert_eq!(store.begin(&r).unwrap(), r);
    // mine-release-allow-plan-reference: same Plan concurrent closure refusal
    assert!(store.begin(&record(operation("plan01"))).is_err());
    // mine-release-allow-plan-reference: independent disjoint closure fixture
    store.begin(&record(operation("plan02"))).unwrap();
    let barrier = store.barriers().unwrap();
    assert!(barrier.global.is_empty());
    // mine-release-allow-plan-reference: exact effective hold observations
    assert_eq!(barrier.held_plans, ["plan01", "plan02"]);
}

#[test]
fn stage_revisions_binding_skips_and_lost_response_are_discriminating() {
    let dir = tempfile::tempdir().unwrap();
    let guard = lock(dir.path());
    let store = OperationStore::under_lock(dir.path(), "repo", "ws", &guard).unwrap();
    // mine-release-allow-plan-reference: canonical stage recovery fixture
    let op = operation("plan01");
    let query =
        OperationQuery::new("repo".into(), "ws".into(), op.fields().operation_id.clone()).unwrap();
    let service = OperationService::new(&store);
    let mut observed = Observed {
        expected: op.clone(),
        fail: false,
        calls: 0,
    };
    service
        .begin(op.clone(), ClosureBinding::Acceptance, &mut observed)
        .unwrap();
    let mut fields = op.fields().clone();
    fields.stage_revision = 1;
    fields.stage = OperationStage::Acceptance(AcceptanceStage::Integrated);
    fields.integrated_commit = Some(CommitId::new("b".repeat(40)).unwrap());
    let integrated = LifecycleOperation::new(fields.clone()).unwrap();
    observed.expected = integrated.clone();
    observed.fail = true;
    assert!(
        service
            .stage(&query, 0, integrated.clone(), &mut observed)
            .is_err()
    );
    assert_eq!(store.query(query.operation_id()).unwrap().operation, op);
    observed.fail = false;
    service
        .stage(&query, 0, integrated.clone(), &mut observed)
        .unwrap();
    // Same call after a lost response revalidates but makes no extra stage.
    service
        .stage(&query, 0, integrated.clone(), &mut observed)
        .unwrap();
    assert_eq!(
        store
            .query(query.operation_id())
            .unwrap()
            .operation
            .fields()
            .stage_revision,
        1
    );
    fields.stage_revision = 2;
    fields.stage = OperationStage::Acceptance(AcceptanceStage::Accepted);
    fields.cleanup = Some(CleanupEvidence {
        worktree: fields.registered_worktree.clone().unwrap(),
        branch: fields.plan.as_ref().unwrap().fields().branch.clone(),
        worktree_removed: true,
        branch_removed: true,
    });
    let skip = LifecycleOperation::new(fields.clone()).unwrap();
    observed.expected = skip.clone();
    assert!(service.stage(&query, 1, skip, &mut observed).is_err());
    fields.stage = OperationStage::Acceptance(AcceptanceStage::Verified);
    fields.actor = "foreign".into();
    let wrong = LifecycleOperation::new(fields).unwrap();
    observed.expected = wrong.clone();
    assert!(service.stage(&query, 1, wrong, &mut observed).is_err());
    let foreign =
        OperationQuery::new("foreign".into(), "ws".into(), query.operation_id().clone()).unwrap();
    assert!(service.query(&foreign).is_err());
}

#[test]
fn missing_corrupt_foreign_and_unsupported_journals_block_not_grant() {
    for mode in [
        "missing",
        "corrupt",
        "foreign",
        "version",
        "committed_nonfinal",
        "refused_progressed",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let guard = lock(dir.path());
        let store = OperationStore::under_lock(dir.path(), "repo", "ws", &guard).unwrap();
        // mine-release-allow-plan-reference: missing journal must retain this hold
        let r = record(operation("plan01"));
        store.begin(&r).unwrap();
        let path = OperationLocation::new(dir.path(), &r.operation.fields().operation_id)
            .unwrap()
            .resolve()
            .unwrap();
        if mode == "missing" {
            fs::remove_file(path).unwrap();
        } else if mode == "corrupt" {
            fs::write(path, "incomplete").unwrap();
        } else {
            let mut value = serde_json::to_value(&r).unwrap();
            if mode == "foreign" {
                value["operation"]["repository_id"] = "foreign".into();
            } else if mode == "version" {
                value["schema_version"] = 99.into();
            } else if mode == "committed_nonfinal" {
                value["operation"]["outcome"] = "committed".into();
            } else {
                value["operation"]["outcome"] = "refused".into();
                value["operation"]["stage"] =
                    serde_json::json!({"kind":"acceptance","stage":"integrated"});
                value["operation"]["integrated_commit"] = "a".repeat(40).into();
            }
            fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        }
        assert!(
            store.query(&r.operation.fields().operation_id).is_err(),
            "{mode}"
        );
        assert!(store.barriers().is_err(), "{mode}");
        // mine-release-allow-plan-reference: corrupt evidence blocks new closure
        assert!(store.begin(&record(operation("plan02"))).is_err(), "{mode}");
    }
}

#[test]
fn unknown_and_render_repair_are_global_and_terminal_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let guard = lock(dir.path());
    let store = OperationStore::under_lock(dir.path(), "repo", "ws", &guard).unwrap();
    // mine-release-allow-plan-reference: unknown-outcome barrier fixture
    let op = operation("plan01");
    store.begin(&record(op.clone())).unwrap();
    let mut f = op.fields().clone();
    f.stage_revision = 1;
    f.outcome = OperationOutcome::Unknown;
    let unknown = record(LifecycleOperation::new(f.clone()).unwrap());
    store.advance(0, &unknown).unwrap();
    assert_eq!(store.barriers().unwrap().global.len(), 1);
    // mine-release-allow-plan-reference: global pending barrier blocks disjoint Plan
    assert!(store.begin(&record(operation("plan02"))).is_err());
    f.stage_revision = 2;
    f.outcome = OperationOutcome::CommittedRenderRepair;
    store
        .advance(1, &record(LifecycleOperation::new(f.clone()).unwrap()))
        .unwrap();
    assert_eq!(store.barriers().unwrap().global.len(), 1);
    f.stage_revision = 3;
    f.outcome = OperationOutcome::Refused;
    assert!(
        store
            .advance(2, &record(LifecycleOperation::new(f.clone()).unwrap()))
            .is_err()
    );
    // Only a fresh, reconciled pre-effect observation can become refused.
    f.outcome = OperationOutcome::Pending;
    store
        .advance(2, &record(LifecycleOperation::new(f.clone()).unwrap()))
        .unwrap();
    f.stage_revision = 4;
    f.outcome = OperationOutcome::Refused;
    let refused = record(LifecycleOperation::new(f.clone()).unwrap());
    store.advance(3, &refused).unwrap();
    assert!(terminal(&refused.operation));
    assert!(store.barriers().unwrap().held_plans.is_empty());
    f.stage_revision = 5;
    f.outcome = OperationOutcome::Pending;
    assert!(
        store
            .advance(4, &record(LifecycleOperation::new(f).unwrap()))
            .is_err()
    );
}

#[test]
fn exact_root_lock_and_location_containment_are_required() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let wrong = lock(other.path());
    assert!(OperationStore::under_lock(dir.path(), "repo", "ws", &wrong).is_err());
    let id = OperationId::generate();
    assert!(
        OperationLocation::new(dir.path(), &id)
            .unwrap()
            .relative_path()
            .as_str()
            .ends_with(".json")
    );
    assert!(OperationId::parse("../foreign").is_err());
}

#[test]
fn every_acceptance_stage_lost_response_revalidates_without_advancing_twice() {
    let dir = tempfile::tempdir().unwrap();
    let guard = lock(dir.path());
    let store = OperationStore::under_lock(dir.path(), "repo", "ws", &guard).unwrap();
    // mine-release-allow-plan-reference: complete canonical acceptance stage fixture
    let op = operation("plan01");
    let query =
        OperationQuery::new("repo".into(), "ws".into(), op.fields().operation_id.clone()).unwrap();
    let service = OperationService::new(&store);
    let mut observer = Observed {
        expected: op.clone(),
        fail: false,
        calls: 0,
    };
    service
        .begin(op.clone(), ClosureBinding::Acceptance, &mut observer)
        .unwrap();
    let mut fields = op.fields().clone();
    for stage in [
        AcceptanceStage::Integrated,
        AcceptanceStage::Verified,
        AcceptanceStage::Cleaned,
        AcceptanceStage::Accepted,
    ] {
        let before = fields.stage_revision;
        fields.stage_revision += 1;
        fields.stage = OperationStage::Acceptance(stage);
        fields.integrated_commit = Some(CommitId::new("b".repeat(40)).unwrap());
        if stage != AcceptanceStage::Integrated {
            fields.verification_evidence = vec![FileIdentity {
                path: RepoPath::new(".mine/plan/reports/check.json").unwrap(),
                bytes: 2,
                sha256: digest(b"{}"),
            }];
        }
        if matches!(stage, AcceptanceStage::Cleaned | AcceptanceStage::Accepted) {
            fields.cleanup = Some(CleanupEvidence {
                worktree: fields.registered_worktree.clone().unwrap(),
                branch: fields.plan.as_ref().unwrap().fields().branch.clone(),
                worktree_removed: true,
                branch_removed: true,
            });
        }
        if stage == AcceptanceStage::Accepted {
            fields.outcome = OperationOutcome::Committed;
        }
        let next = LifecycleOperation::new(fields.clone()).unwrap();
        observer.expected = next.clone();
        service
            .stage(&query, before, next.clone(), &mut observer)
            .unwrap();
        service.stage(&query, before, next, &mut observer).unwrap();
        assert_eq!(
            store
                .query(query.operation_id())
                .unwrap()
                .operation
                .fields()
                .stage_revision,
            before + 1
        );
        assert_eq!(
            service.barriers().unwrap().held_plans.is_empty(),
            stage == AcceptanceStage::Accepted
        );
    }
    assert_eq!(observer.calls, 9);
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
struct NoReceipts {
    plan: PlanIdentity,
}
impl VerificationEvidenceProvider for NoReceipts {
    fn validate(&self, _: &FileIdentity, _: &LifecycleOperation) -> OperationResult<()> {
        Err(OperationError::Evidence("fixture has no valid receipts"))
    }
    fn writer_stopped(
        &self,
        plan: &PlanIdentity,
        _: Option<&CommittedFilesProof>,
    ) -> OperationResult<()> {
        if plan == &self.plan {
            Ok(())
        } else {
            Err(OperationError::Evidence("unknown fixture writer"))
        }
    }
}

#[test]
fn real_filesystem_provider_pins_body_report_and_actual_git_tip_not_path_strings() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-b", "dev"]);
    for (key, value) in [
        ("user.name", "Journal Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(root, &["config", key, value]);
    }
    fs::create_dir_all(root.join(".mine")).unwrap();
    fs::create_dir_all(root.join(".mine/plan/reports")).unwrap();
    fs::write(
        root.join(".gitignore"),
        ".mine/locks/\n.mine/runtime/\n.mine/worktrees/\n",
    )
    .unwrap();
    let config = fixture_config::canonical();
    fs::write(root.join(".mine/config.toml"), config.to_toml()).unwrap();
    // mine-release-allow-plan-reference: real canonical independent closure fixture
    let mut fields = operation("plan01").fields().clone();
    let mut p = fields.plan.as_ref().unwrap().fields().clone();
    p.repository_id = config.repository_id.clone();
    fields.repository_id = config.repository_id;
    fields.plan = Some(PlanIdentity::new(p).unwrap());
    let plan = fields.plan.as_ref().unwrap().fields();
    fs::write(root.join(plan.plan_path.as_str()), "immutable plan").unwrap();
    let report = fields.review.as_mut().unwrap();
    fs::write(root.join(report.report.path.as_str()), "Verdict: ACCEPTED").unwrap();
    report.report.bytes = b"Verdict: ACCEPTED".len() as u64;
    let mut ws: PlanWorkspace =
        toml::from_str(include_str!("fixtures/development-execution-graph.toml")).unwrap();
    let mut node = ws.plans.remove(0);
    node.id = plan.plan_id.clone();
    node.path = plan.plan_path.as_str().into();
    node.branch = plan.branch.clone();
    node.lineage_id = plan.lineage_id.clone();
    node.worktree_path = fields.registered_worktree.as_ref().unwrap().as_str().into();
    node.status = mine::domain::status::PlanStatus::Implemented;
    node.hard_predecessors.clear();
    node.soft_predecessors.clear();
    node.compensating_plan.clear();
    node.branch_anchor.clear();
    ws.plans = vec![node];
    ws.workspace_id = fields.workspace_id.clone();
    ws.revision = 7;
    fs::write(
        root.join(".mine/plan/execution-graph.toml"),
        toml::to_string(&ws).unwrap(),
    )
    .unwrap();
    git(
        root,
        &["add", ".gitignore", ".mine/config.toml", ".mine/plan"],
    );
    git(root, &["commit", "-m", "reviewed fixture"]);
    let tip = git(root, &["rev-parse", "HEAD"]);
    git(
        root,
        &["branch", &fields.plan.as_ref().unwrap().fields().branch],
    );
    let worktree = root.join(fields.registered_worktree.as_ref().unwrap().as_str());
    git(
        root,
        &[
            "worktree",
            "add",
            worktree.to_str().unwrap(),
            &fields.plan.as_ref().unwrap().fields().branch,
        ],
    );
    fields.review.as_mut().unwrap().reviewed_tip = CommitId::new(tip).unwrap();
    let op = LifecycleOperation::new(fields.clone()).unwrap();
    let mut provider = FilesystemOperationEvidence::new(
        root,
        NoReceipts {
            plan: fields.plan.clone().unwrap(),
        },
    );
    provider.observe(&op, &ClosureBinding::Acceptance).unwrap();
    let report_path = worktree.join(fields.review.as_ref().unwrap().report.path.as_str());
    fs::write(&report_path, "Verdict: REJECTED").unwrap();
    git(&worktree, &["add", ".mine/plan/reports"]);
    git(&worktree, &["commit", "-m", "wrong verdict"]);
    let mut negative = fields.clone();
    negative.review.as_mut().unwrap().report.sha256 = digest(b"Verdict: REJECTED");
    negative.review.as_mut().unwrap().report.bytes = b"Verdict: REJECTED".len() as u64;
    negative.review.as_mut().unwrap().reviewed_tip =
        CommitId::new(git(&worktree, &["rev-parse", "HEAD"])).unwrap();
    assert!(
        provider
            .observe(
                &LifecycleOperation::new(negative).unwrap(),
                &ClosureBinding::Acceptance
            )
            .is_err()
    );
    fs::write(&report_path, "Verdict: ACCEPTED").unwrap();
    git(&worktree, &["add", ".mine/plan/reports"]);
    git(&worktree, &["commit", "-m", "restore report fixture"]);
    fs::write(
        root.join(fields.plan.as_ref().unwrap().fields().plan_path.as_str()),
        "changed plan",
    )
    .unwrap();
    git(root, &["add", ".mine/plan"]);
    git(root, &["commit", "-m", "wrong body"]);
    assert!(provider.observe(&op, &ClosureBinding::Acceptance).is_err());

    // A generic receipt provider cannot replace the native Release composition.
    // Merely matching stable refs never establishes retained source or C checks.
    let baseline = git(root, &["rev-parse", "HEAD"]);
    git(root, &["branch", "master", &baseline]);
    ws.stable_baseline_commit = baseline.clone();
    fs::write(
        root.join(".mine/plan/execution-graph.toml"),
        toml::to_string(&ws).unwrap(),
    )
    .unwrap();
    git(root, &["add", ".mine/plan/execution-graph.toml"]);
    git(root, &["commit", "-m", "release binding fixture"]);
    let candidate = git(root, &["rev-parse", "HEAD"]);
    let binding = ClosureBinding::Release {
        baseline: CommitId::new(baseline.clone()).unwrap(),
        candidate: CommitId::new(candidate.clone()).unwrap(),
    };
    let mut release = fields;
    release.kind = OperationKind::Release;
    release.stage = initial_stage(OperationKind::Release);
    release.plan = None;
    release.registered_worktree = None;
    release.review = None;
    assert!(
        provider
            .observe(&LifecycleOperation::new(release.clone()).unwrap(), &binding)
            .is_err()
    );
    release.stage = OperationStage::Release(ReleaseStage::Preflight);
    release.stage_revision += 1;
    assert!(
        provider
            .observe(&LifecycleOperation::new(release.clone()).unwrap(), &binding)
            .is_err()
    );
    git(
        root,
        &["update-ref", "refs/heads/master", &candidate, &baseline],
    );
    // Stable advancement does not upgrade this intentionally incomplete provider.
    assert!(
        provider
            .observe(&LifecycleOperation::new(release.clone()).unwrap(), &binding)
            .is_err()
    );
    release.stage = OperationStage::Release(ReleaseStage::Integrated);
    release.stage_revision += 1;
    release.integrated_commit = Some(CommitId::new(candidate).unwrap());
    assert!(
        provider
            .observe(&LifecycleOperation::new(release).unwrap(), &binding)
            .is_err()
    );
}

#[test]
fn rejection_and_release_requests_are_typed_bound_and_stage_replay_is_once() {
    for kind in [OperationKind::RejectedHandoff, OperationKind::Release] {
        let dir = tempfile::tempdir().unwrap();
        let guard = lock(dir.path());
        let store = OperationStore::under_lock(dir.path(), "repo", "ws", &guard).unwrap();
        // mine-release-allow-plan-reference: rejected compensation lineage fixture
        let mut f = operation("plan01").fields().clone();
        f.kind = kind;
        f.stage = initial_stage(kind);
        let (binding, stages) = if kind == OperationKind::RejectedHandoff {
            let mut cp = f.plan.as_ref().unwrap().fields().clone();
            cp.plan_id.push_str("-C01");
            cp.plan_path = RepoPath::new(format!(".mine/plan/{}-fixture.md", cp.plan_id)).unwrap();
            cp.branch = format!("plan/{}-fixture", cp.plan_id);
            (
                ClosureBinding::RejectedHandoff {
                    compensation: PlanIdentity::new(cp).unwrap(),
                },
                [
                    RejectionStage::Rejected,
                    RejectionStage::CompensationRegistered,
                    RejectionStage::Rewired,
                    RejectionStage::Released,
                    RejectionStage::Complete,
                ]
                .map(OperationStage::RejectedHandoff)
                .to_vec(),
            )
        } else {
            f.plan = None;
            f.registered_worktree = None;
            f.review = None;
            (
                ClosureBinding::Release {
                    baseline: CommitId::new("a".repeat(40)).unwrap(),
                    candidate: CommitId::new("b".repeat(40)).unwrap(),
                },
                [
                    ReleaseStage::Preflight,
                    ReleaseStage::Integrated,
                    ReleaseStage::Complete,
                ]
                .map(OperationStage::Release)
                .to_vec(),
            )
        };
        let initial = JournalRecord {
            schema_version: 1,
            operation: LifecycleOperation::new(f.clone()).unwrap(),
            closure: Some(binding.clone()),
            checkpoint: None,
        };
        store.begin(&initial).unwrap();
        for (index, stage) in stages.iter().enumerate() {
            let expected = f.stage_revision;
            f.stage_revision += 1;
            f.stage = *stage;
            if index + 1 == stages.len() {
                f.outcome = OperationOutcome::Committed;
            }
            let next = JournalRecord {
                schema_version: 1,
                operation: LifecycleOperation::new(f.clone()).unwrap(),
                closure: Some(binding.clone()),
                checkpoint: None,
            };
            store.advance(expected, &next).unwrap();
            store.advance(expected, &next).unwrap();
            assert_eq!(store.query(&f.operation_id).unwrap(), next);
        }
        assert!(terminal(&store.query(&f.operation_id).unwrap().operation));
    }
}
