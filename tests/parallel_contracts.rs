#![forbid(unsafe_code)]

use mine::application::operation_service::OperationQuery;
use mine::application::verification_service::VerificationRequest;
use mine::domain::graph::{
    CommitId, FileIdentity, PlanIdentity, PlanIdentityFields, PolicyIdentity, PolicyIdentityFields,
    RepoPath, Sha256, UtcTimestamp,
};
use mine::domain::lifecycle_operation::*;
use mine::domain::ownership::*;
use mine::domain::scheduling::*;
use mine::domain::verification_receipt::*;
use mine::infrastructure::graph_checkpoint::GraphCheckpointPaths;
use mine::infrastructure::operation_store::OperationLocation;
use mine::infrastructure::verification_store::VerificationLocation;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeMap;

// mine-release-allow-plan-reference: canonical Plan identity protocol fixture
const ID: &str = "plan01";
fn digest() -> Sha256 {
    Sha256::new("a".repeat(64)).unwrap()
}
fn path(value: &str) -> RepoPath {
    RepoPath::new(value).unwrap()
}
fn identity() -> PlanIdentity {
    PlanIdentity::new(PlanIdentityFields {
        repository_id: "repository".into(),
        workspace_id: "workspace".into(),
        plan_id: ID.into(),
        plan_path: path(&format!(".mine/plan/{ID}-fixture.md")),
        body_sha256: Some(digest()),
        lineage_id: ID.into(),
        branch: format!("plan/{ID}-fixture"),
    })
    .unwrap()
}
fn policy() -> PolicyIdentity {
    PolicyIdentity::new(PolicyIdentityFields {
        design_references: vec![path(".mine/design/principles.md")],
        sha256: digest(),
    })
    .unwrap()
}
fn ownership() -> OwnershipContract {
    OwnershipContract::new(OwnershipFields {
        schema_version: 1,
        plan: identity(),
        policy: policy(),
        responsibility: "graph reads".into(),
        namespace_claims: vec![NamespaceClaim {
            subtree: Namespace::new("mine.graph").unwrap(),
            excluded: vec![Namespace::new("mine.graph.persistence").unwrap()],
        }],
        owned_contracts: vec!["read_snapshot".into()],
        excluded_responsibilities: vec!["mutation".into()],
        excluded_contracts: vec!["atomic_start".into()],
        physical_mapping: vec![path("src/domain/graph.rs")],
        shared_declarations: vec![SharedDeclaration {
            path: path("src/domain/mod.rs"),
            entry: "graph".into(),
            class: DeclarationClass::Export,
            integration_owner: "integration".into(),
        }],
        generators: vec![GeneratorTransaction {
            name: "graph_view".into(),
            inputs: vec![path(".mine/plan/execution-graph.toml")],
            machine_source: path(".mine/plan/execution-graph.toml"),
            outputs: vec![path(".mine/plan/execution-graph.md")],
            seals: vec![path(".mine/plan/execution-graph.md")],
        }],
    })
    .unwrap()
}
fn command() -> CheckCommand {
    CheckCommand::new(CommandFields {
        argv: vec!["cargo".into(), "test".into(), "--all-features".into()],
        cwd: CommandDirectory::Root,
    })
    .unwrap()
}
fn receipt() -> VerificationReceipt {
    VerificationReceipt::new(ReceiptFields {
        schema_version: 1,
        receipt_id: ReceiptId::generate(),
        plan: identity(),
        actor: "executor".into(),
        command: command(),
        started_at: UtcTimestamp::new("2026-10-12T00:00:00Z").unwrap(),
        ended_at: UtcTimestamp::new("2026-10-12T00:00:01Z").unwrap(),
        exit_code: Some(0),
        outcome: CheckOutcome::Passed,
        policy: policy(),
        input_manifest_sha256: digest(),
        toolchains: BTreeMap::from([("rustc".into(), digest())]),
        dependency_locks: BTreeMap::from([(path("Cargo.lock"), digest())]),
        platform: "windows".into(),
        architecture: "x86_64".into(),
        features: vec!["all".into()],
        environment: vec![],
        artifacts: vec![FileIdentity {
            path: path(&format!(".mine/plan/reports/verification/{ID}/test.log")),
            bytes: 100,
            sha256: digest(),
        }],
    })
    .unwrap()
}
fn operation() -> LifecycleOperation {
    LifecycleOperation::new(OperationFields {
        schema_version: 1,
        operation_id: OperationId::generate(),
        stage_revision: 0,
        kind: OperationKind::Acceptance,
        stage: OperationStage::Acceptance(AcceptanceStage::Reviewed),
        outcome: OperationOutcome::Pending,
        repository_id: "repository".into(),
        workspace_id: "workspace".into(),
        actor: "independent-reviewer".into(),
        observed_graph_revision: 10,
        plan: Some(identity()),
        registered_worktree: Some(path(&format!(".mine/worktrees/{ID}"))),
        review: Some(ReviewEvidence {
            reviewed_tip: CommitId::new("b".repeat(40)).unwrap(),
            report: FileIdentity {
                path: path(&format!(".mine/plan/reports/{ID}-fixture-review.md")),
                bytes: 20,
                sha256: digest(),
            },
        }),
        integrated_commit: None,
        verification_evidence: vec![],
        cleanup: None,
        checkpoint: None,
        diagnostics: vec!["REVIEWED".into()],
    })
    .unwrap()
}
fn scheduling() -> SchedulingSnapshot {
    SchedulingSnapshot::new(SchedulingFields {
        schema_version: 1,
        repository_id: "repository".into(),
        workspace_id: "workspace".into(),
        revision: 10,
        graph_sha256: digest(),
        policy: policy(),
        held: vec![OwnershipHold {
            authority: ownership(),
            occupancy: Occupancy::ImplementedReview,
            owner: "executor".into(),
            run_id: "run".into(),
            worktree: path(&format!(".mine/worktrees/{ID}")),
            mutable_outputs: vec![path(&format!(".mine/build/{ID}/cargo-target"))],
        }],
        candidates: vec!["candidate".into()],
        eligible: vec!["candidate".into()],
        selected: vec![],
        blockers: vec![Blocker {
            candidate: "candidate".into(),
            owner: ID.into(),
            resource: Resource::Namespace(Namespace::new("mine.graph").unwrap()),
            reason: BlockingReason::NamespaceOverlap,
        }],
        pending_checkpoint: None,
    })
    .unwrap()
}

fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T) -> Value {
    let json = serde_json::to_value(value).unwrap();
    assert_eq!(&serde_json::from_value::<T>(json.clone()).unwrap(), value);
    assert_eq!(
        serde_json::to_string(&json).unwrap(),
        serde_json::to_string(&serde_json::to_value(value).unwrap()).unwrap()
    );
    json
}
fn refuses<T: DeserializeOwned>(value: Value, expected: &str) {
    let error = serde_json::from_value::<T>(value)
        .err()
        .expect("invalid contract accepted")
        .to_string();
    assert!(
        error.contains(expected),
        "{error:?} did not identify {expected:?}"
    );
}

#[test]
fn identity_policy_and_namespace_are_validated_on_decode() {
    assert!(ReceiptId::parse("00000000-0000-4000-0000-000000000000").is_err());
    assert!(OperationId::parse("00000000-0000-4000-0000-000000000000").is_err());
    let value = roundtrip(&identity());
    let mut bad = value.clone();
    bad["branch"] = json!("plan/foreign");
    refuses::<PlanIdentity>(bad, "plan_identity");
    let mut bad = value.clone();
    bad["plan_path"] = json!("../escape");
    refuses::<PlanIdentity>(bad, "path");
    let mut bad = value.clone();
    bad["body_sha256"] = json!("ABC");
    refuses::<PlanIdentity>(bad, "sha256");
    let mut bad = value;
    bad["unknown"] = json!(true);
    refuses::<PlanIdentity>(bad, "unknown field");
    let mut legacy = identity().fields().clone();
    legacy.body_sha256 = None;
    assert_eq!(
        roundtrip(&PlanIdentity::new(legacy).unwrap())["body_sha256"],
        Value::Null
    );
    let mut bad = roundtrip(&policy());
    bad["design_references"] = json!([]);
    refuses::<PolicyIdentity>(bad, "policy.design_references");
    for bad in [
        "",
        "mine..graph",
        "mine/graph",
        "mine\\graph",
        "mine.*",
        "mine graph",
    ] {
        assert!(Namespace::new(bad).is_err());
    }
    let namespace = Namespace::new("mine.graph").unwrap();
    assert!(namespace.contains(&Namespace::new("mine.graph.read").unwrap()));
    assert!(!namespace.contains(&Namespace::new("mine.graphical").unwrap()));
    assert!(!namespace.contains(&Namespace::new("Mine.graph.read").unwrap()));
    for bad in ["a/../b", "/a", "a//b", "a/", "a:b", " a", "a\nb"] {
        assert!(RepoPath::new(bad).is_err());
    }
    assert!(CommitId::new("a".repeat(39)).is_err());
    assert!(CommitId::new("a".repeat(64)).is_ok());
    assert!(UtcTimestamp::new("2026-10-12T00:00:00+08:00").is_err());
}

#[test]
fn ownership_schema_roundtrip_and_contradictory_authority_refusals() {
    let value = roundtrip(&ownership());
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["shared_declarations"][0]["class"], "export");
    for (key, bad, diagnostic) in [
        ("schema_version", json!(2), "schema_version"),
        ("responsibility", json!(""), "responsibility"),
        ("physical_mapping", json!([]), "ownership"),
        (
            "excluded_contracts",
            json!(["read_snapshot"]),
            "owned_contracts",
        ),
    ] {
        let mut mutated = value.clone();
        mutated[key] = bad;
        refuses::<OwnershipContract>(mutated, diagnostic);
    }
    let mut bad = value.clone();
    bad["namespace_claims"][0]["excluded"] = json!(["mine.other"]);
    refuses::<OwnershipContract>(bad, "namespace_claims.excluded");
    let mut bad = value.clone();
    bad["shared_declarations"][0]["class"] = json!("algorithm");
    refuses::<OwnershipContract>(bad, "unknown variant");
    let mut bad = value.clone();
    bad["shared_declarations"][0]["integration_owner"] = json!("");
    refuses::<OwnershipContract>(bad, "integration_owner");
    let mut bad = value;
    bad["generators"][0]["outputs"] = json!([".mine/plan/other.md"]);
    refuses::<OwnershipContract>(bad, "generators.outputs");
}

#[test]
fn scheduling_schema_enforces_identity_subsets_and_global_pending_barrier() {
    let value = roundtrip(&scheduling());
    let mut bad = value.clone();
    bad["selected"] = json!(["foreign"]);
    refuses::<SchedulingSnapshot>(bad, "selected");
    let mut bad = value.clone();
    bad["repository_id"] = json!("foreign");
    refuses::<SchedulingSnapshot>(bad, "held.authority");
    let mut bad = value.clone();
    bad["held"][0]["owner"] = json!("");
    refuses::<SchedulingSnapshot>(bad, "held.owner");
    let mut bad = value.clone();
    bad["blockers"] = json!([]);
    bad["selected"] = json!(["candidate"]);
    bad["pending_checkpoint"] = json!("pending");
    refuses::<SchedulingSnapshot>(bad, "pending checkpoint");
    let mut bad = value;
    bad["schema_version"] = json!(5);
    refuses::<SchedulingSnapshot>(bad, "schema_version");
}

#[test]
fn receipt_schema_rejects_forged_time_outcome_secret_and_duplicate_artifacts() {
    let value = roundtrip(&receipt());
    assert_eq!(
        value["command"]["argv"],
        json!(["cargo", "test", "--all-features"])
    );
    assert_eq!(value["command"]["cwd"], json!({"kind":"root"}));
    let mut bad = value.clone();
    bad["exit_code"] = json!(1);
    refuses::<VerificationReceipt>(bad, "outcome");
    let mut bad = value.clone();
    bad["ended_at"] = json!("2026-10-11T23:59:00Z");
    refuses::<VerificationReceipt>(bad, "ended_at");
    let mut bad = value.clone();
    bad["command"]["argv"] = json!([]);
    refuses::<VerificationReceipt>(bad, "command.argv");
    let mut bad = value.clone();
    bad["plan"]["body_sha256"] = Value::Null;
    refuses::<VerificationReceipt>(bad, "plan.body_sha256");
    let mut bad = value.clone();
    bad["receipt_id"] = json!("../../foreign");
    refuses::<VerificationReceipt>(bad, "receipt_id");
    let mut bad = value.clone();
    bad["artifacts"] = json!([value["artifacts"][0], value["artifacts"][0]]);
    refuses::<VerificationReceipt>(bad, "artifacts");
    let mut bad = value.clone();
    bad["environment"] = json!([{"name":"API_TOKEN","sha256":"a".repeat(64),"observable":true,"secret_dependent":true}]);
    refuses::<VerificationReceipt>(bad, "environment");
    let mut nonreusable = value;
    nonreusable["environment"] = json!([{"name":"external-input","sha256":null,"observable":false,"secret_dependent":false}]);
    assert!(
        serde_json::from_value::<VerificationReceipt>(nonreusable)
            .unwrap()
            .has_unobservable_inputs()
    );
}

#[test]
fn operation_schema_pins_review_kind_integration_and_cleanup_identity() {
    let value = roundtrip(&operation());
    let mut bad = value.clone();
    bad["kind"] = json!("release");
    refuses::<LifecycleOperation>(bad, "stage");
    let mut bad = value.clone();
    bad["repository_id"] = json!("foreign");
    refuses::<LifecycleOperation>(bad, "plan");
    let mut bad = value.clone();
    bad["review"]["report"]["path"] = json!(".mine/plan/reports/foreign-review.md");
    refuses::<LifecycleOperation>(bad, "review.report");
    let mut bad = value.clone();
    bad["stage"]["stage"] = json!("integrated");
    refuses::<LifecycleOperation>(bad, "integrated_commit");
    let mut bad = value.clone();
    bad["stage"]["stage"] = json!("cleaned");
    bad["integrated_commit"] = json!("a".repeat(40));
    refuses::<LifecycleOperation>(bad, "cleanup");
    let mut valid = operation().fields().clone();
    valid.stage = OperationStage::Acceptance(AcceptanceStage::Cleaned);
    valid.stage_revision = 3;
    valid.integrated_commit = Some(CommitId::new("a".repeat(40)).unwrap());
    valid.cleanup = Some(CleanupEvidence {
        worktree: valid.registered_worktree.clone().unwrap(),
        branch: identity().fields().branch.clone(),
        worktree_removed: true,
        branch_removed: true,
    });
    roundtrip(&LifecycleOperation::new(valid.clone()).unwrap());
    valid.cleanup.as_mut().unwrap().branch_removed = false;
    assert_eq!(LifecycleOperation::new(valid).unwrap_err().field, "cleanup");
    let mut bad = value;
    bad["diagnostics"] = json!(["SECRET=content"]);
    refuses::<LifecycleOperation>(bad, "diagnostics");
}

#[test]
fn checkpoint_schema_requires_produced_identities_only_at_relevant_boundaries() {
    let mut fields = CheckpointFields {
        schema_version: 1,
        stage: CheckpointStage::Intent,
        revision_before: 10,
        revision_after: None,
        source_sha256: None,
        view_sha256: None,
        original_head: CommitId::new("a".repeat(40)).unwrap(),
        original_index_sha256: digest(),
        candidate_tree: None,
        candidate_commit: None,
        generated_index_sha256: None,
        owned_lock_sha256: None,
    };
    roundtrip(&CheckpointFacts::new(fields.clone()).unwrap());
    fields.stage = CheckpointStage::GraphPersisted;
    assert_eq!(
        CheckpointFacts::new(fields.clone()).unwrap_err().field,
        "checkpoint.graph"
    );
    fields.revision_after = Some(11);
    fields.source_sha256 = Some(digest());
    roundtrip(&CheckpointFacts::new(fields.clone()).unwrap());
    fields.stage = CheckpointStage::RenderComplete;
    assert_eq!(
        CheckpointFacts::new(fields.clone()).unwrap_err().field,
        "checkpoint.view"
    );
    fields.view_sha256 = Some(digest());
    fields.stage = CheckpointStage::CandidatePrepared;
    assert_eq!(
        CheckpointFacts::new(fields.clone()).unwrap_err().field,
        "checkpoint.candidate"
    );
    fields.candidate_tree = Some(CommitId::new("b".repeat(40)).unwrap());
    fields.candidate_commit = Some(CommitId::new("c".repeat(40)).unwrap());
    fields.generated_index_sha256 = Some(digest());
    fields.stage = CheckpointStage::RefAdvanced;
    assert_eq!(
        CheckpointFacts::new(fields.clone()).unwrap_err().field,
        "checkpoint.lock"
    );
    fields.owned_lock_sha256 = Some(digest());
    fields.stage = CheckpointStage::Complete;
    let checkpoint = CheckpointFacts::new(fields).unwrap();
    roundtrip(&checkpoint);
    let mut op = operation().fields().clone();
    op.kind = OperationKind::GraphCheckpoint;
    op.stage = OperationStage::GraphCheckpoint(CheckpointStage::Complete);
    op.checkpoint = Some(checkpoint);
    roundtrip(&LifecycleOperation::new(op.clone()).unwrap());
    op.observed_graph_revision += 1;
    assert_eq!(LifecycleOperation::new(op).unwrap_err().field, "checkpoint");
}

#[test]
fn application_requests_and_all_store_locators_are_callable_without_stub_effects() {
    let request =
        VerificationRequest::new(identity(), "executor".into(), command(), policy()).unwrap();
    assert_eq!(request.plan(), &identity());
    assert_eq!(request.actor(), "executor");
    assert_eq!(request.command(), &command());
    assert_eq!(request.policy(), &policy());
    assert!(VerificationRequest::new(identity(), "".into(), command(), policy()).is_err());
    let operation_id = OperationId::generate();
    let query = OperationQuery::new(
        "repository".into(),
        "workspace".into(),
        operation_id.clone(),
    )
    .unwrap();
    assert_eq!(query.repository_id(), "repository");
    assert_eq!(query.workspace_id(), "workspace");
    assert_eq!(query.operation_id(), &operation_id);
    assert!(OperationQuery::new("".into(), "workspace".into(), operation_id.clone()).is_err());
    let root = tempfile::tempdir().unwrap();
    let receipt_id = ReceiptId::generate();
    let location = VerificationLocation::new(root.path(), &identity(), &receipt_id).unwrap();
    assert!(
        location
            .resolve()
            .unwrap()
            .starts_with(root.path().canonicalize().unwrap())
    );
    assert_eq!(
        location.relative_path().as_str(),
        format!(
            ".mine/plan/reports/verification/{ID}/{}.json",
            receipt_id.as_str()
        )
    );
    assert!(
        location
            .artifact(&path(&format!(
                ".mine/plan/reports/verification/{ID}/test.log"
            )))
            .is_ok()
    );
    assert!(
        location
            .artifact(&path(".mine/plan/reports/verification/foreign/test.log"))
            .is_err()
    );
    assert!(location.artifact(location.relative_path()).is_err());
    let journal = OperationLocation::new(root.path(), &operation_id).unwrap();
    assert_eq!(
        journal.relative_path().as_str(),
        format!(".mine/runtime/operations/{}.json", operation_id.as_str())
    );
    assert!(journal.resolve().is_ok());
    let config = mine::cli::context::load_config(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let paths = GraphCheckpointPaths::from_config(root.path(), &config).unwrap();
    assert_eq!(paths.source().as_str(), config.graph.source);
    assert_eq!(paths.view().as_str(), config.graph.rendered);
    assert!(paths.resolve().is_ok());
    let mut bad = config.clone();
    bad.graph.source = "src/foreign.rs".into();
    assert!(GraphCheckpointPaths::from_config(root.path(), &bad).is_err());
    bad = config;
    bad.graph.rendered = bad.graph.source.clone();
    assert!(GraphCheckpointPaths::from_config(root.path(), &bad).is_err());
    // Locators validate only: they never create a journal/receipt/graph file.
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

use std::path::Path;

#[test]
fn locators_recheck_escaping_links_instead_of_caching_a_permission() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let journal = OperationLocation::new(root.path(), &OperationId::generate()).unwrap();
    std::fs::create_dir_all(root.path().join(".mine")).unwrap();
    #[cfg(windows)]
    {
        // Junctions do not require Windows symlink privilege. No shell-built
        // command or deletion is used; remove_dir removes only this junction.
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(
                root.path()
                    .join(".mine/runtime")
                    .display()
                    .to_string()
                    .replace('/', "\\"),
            )
            .arg(outside.path().display().to_string().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), root.path().join(".mine/runtime")).unwrap();
    assert!(journal.resolve().is_err());
    #[cfg(windows)]
    std::fs::remove_dir(root.path().join(".mine/runtime")).unwrap();
}
