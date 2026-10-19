#![forbid(unsafe_code)]
//! Architect foundation uses the real canonical native Plan path and consumer.
use mine::domain::ports::Clock;
use mine::infrastructure::system::SystemClock;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let file = root.join(relative);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, bytes).unwrap();
}
fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .env("HOME", root.join(".mine/runtime/fixture-home"))
        .env("USERPROFILE", root.join(".mine/runtime/fixture-home"))
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn invoke(root: &Path, arguments: &[&str]) -> (Output, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_mine"))
        .args(["--repo", root.to_str().unwrap(), "--format", "json"])
        .args(arguments)
        .env("HOME", root.join(".mine/runtime/fixture-home"))
        .env("USERPROFILE", root.join(".mine/runtime/fixture-home"))
        .output()
        .unwrap();
    let raw = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let value =
        serde_json::from_slice(raw).unwrap_or_else(|_| panic!("mine {arguments:?}: {output:?}"));
    (output, value)
}
fn mine(root: &Path, arguments: &[&str]) -> Value {
    let (output, value) = invoke(root, arguments);
    assert!(output.status.success(), "mine {arguments:?}: {value}");
    value
}
fn revision(root: &Path) -> String {
    mine(root, &["graph", "status"])["data"]["revision"]
        .as_u64()
        .unwrap()
        .to_string()
}

#[test]
fn real_foundation_consumer_uses_prepared_plan_and_independent_review_not_root_bypass() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let id = format!("plan{:02}", 63);
    let output = root.join(format!(".mine/build/{id}/foundation-consumer"));
    let executable = output.join(format!("consumer{}", std::env::consts::EXE_SUFFIX));
    git(root, &["init", "--initial-branch=master"]);
    for (key, value) in [
        ("user.name", "Architect fixture"),
        ("user.email", "architect@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "true"),
    ] {
        git(root, &["config", key, value]);
    }
    mine(root, &["init"]);
    // This fixture's gate executes its actual compiled consumer, not a fake
    // success script or a gate that merely checks the foundation's file names.
    use mine::domain::verification_receipt::*;
    let policy = VerificationPolicy::new(PolicyFields {
        schema_version: 1,
        design_references: vec![
            mine::domain::graph::RepoPath::new(".mine/design/index.md").unwrap(),
        ],
        checks: vec![CheckPolicy {
            name: "foundation-consumer".into(),
            command: CheckCommand::new(CommandFields {
                argv: vec![executable.to_str().unwrap().into()],
                cwd: CommandDirectory::Root,
            })
            .unwrap(),
            inputs: InputScope::Full,
            tools: vec!["rustc".into(), "foundation-consumer".into()],
            dependency_locks: vec![],
            environment: vec![],
            features: vec![],
            warnings_disqualify: true,
            independent_required: true,
            native_platform: None,
            retention_bytes: 4096,
            retention_files: 8,
        }],
    })
    .unwrap();
    write(
        root,
        ".mine/verification-policy.json",
        &serde_json::to_vec(&policy).unwrap(),
    );
    git(root, &["add", "--", ".mine", "AGENTS.md"]);
    git(
        root,
        &["commit", "-m", "fixture: stable canonical governance"],
    );
    git(root, &["switch", "-c", "dev"]);
    mine(
        root,
        &[
            "workspace",
            "open",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            "0",
        ],
    );
    let body = format!(".mine/plan/{id}-architect-foundation.md");
    write(root, &body, b"# Synthetic ordinary architect foundation Plan\n\nResponsibility: nonzero batch-size invariant and a real consumer.\nMapping: foundation/, consumer/.\nNo Root source edits, speculative interface or self-acceptance.\n");
    git(root, &["add", "--", &body]);
    git(
        root,
        &["commit", "-m", "fixture: scoped foundation contract"],
    );
    mine(
        root,
        &[
            "plan",
            "add",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            &revision(root),
            "--id",
            &id,
            "--path",
            &body,
            "--title",
            "Architect foundation",
            "--design-ref",
            ".mine/design/index.md",
            "--write",
            "foundation",
            "--write",
            "consumer",
        ],
    );
    mine(
        root,
        &[
            "plan",
            "release",
            "--actor",
            "fixture-planner",
            "--expected-revision",
            &revision(root),
            "--id",
            &id,
        ],
    );
    let preparation = uuid::Uuid::new_v4().to_string();
    let prepared = mine(
        root,
        &[
            "plan",
            "prepare",
            "--actor",
            "fixture-architect",
            "--expected-revision",
            &revision(root),
            "--operation-id",
            &preparation,
            "--id",
            &id,
        ],
    );
    let lookup = &prepared["data"]["plan"];
    let checkout = PathBuf::from(lookup["resolved_worktree_path"].as_str().unwrap());
    let branch = lookup["branch"].as_str().unwrap().to_owned();
    assert_eq!(lookup["request_actor"], "fixture-architect");
    assert!(lookup["generation"].is_object());
    assert!(!lookup["checkpoint"].is_null());
    mine(
        root,
        &[
            "plan",
            "start",
            "--id",
            &id,
            "--owner",
            "fixture-architect",
            "--run-id",
            "foundation-run",
            "--expected-revision",
            &revision(root),
        ],
    );
    let root_head = git(root, &["rev-parse", "HEAD"]);
    write(
        &checkout,
        "foundation/mod.rs",
        br#"use std::num::NonZeroU32;
pub struct BatchSize(NonZeroU32);
impl BatchSize {
    pub fn new(value: u32) -> Result<Self, &'static str> {
        NonZeroU32::new(value).map(Self).ok_or("batch size must be positive")
    }
    pub fn chunks(&self, count: u32) -> u32 { count.div_ceil(self.0.get()) }
}
"#,
    );
    write(
        &checkout,
        "consumer/main.rs",
        br#"#[path = "../foundation/mod.rs"] mod foundation;
use foundation::BatchSize;
fn main() {
    assert!(BatchSize::new(0).is_err());
    let size = BatchSize::new(3).unwrap();
    assert_eq!(size.chunks(0), 0);
    assert_eq!(size.chunks(10), 4);
    assert_eq!(BatchSize::new(1).unwrap().chunks(u32::MAX), u32::MAX);
    println!("foundation-invariants-ok");
}
"#,
    );
    fs::create_dir_all(&output).unwrap();
    let compiled = Command::new("rustc")
        .current_dir(&checkout)
        .args(["--edition=2024", "-D", "warnings", "-C", "debuginfo=0"])
        .arg(checkout.join("consumer/main.rs"))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let consumed = Command::new(&executable).output().unwrap();
    assert!(consumed.status.success(), "{consumed:?}");
    assert_eq!(consumed.stdout, b"foundation-invariants-ok\n");
    assert_eq!(git(root, &["branch", "--show-current"]), "dev");
    assert_eq!(git(&checkout, &["branch", "--show-current"]), branch);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), root_head);
    assert!(!root.join("foundation/mod.rs").exists());
    assert!(!checkout.join("target").exists());
    let implementation = format!(".mine/plan/reports/{id}-architect-foundation-implementation.md");
    write(&checkout, &implementation, b"Synthetic ordinary foundation implemented; actual compiled consumer/invariants passed.\nIndependent review and final integration remain distinct obligations.\n");
    git(
        &checkout,
        &[
            "add",
            "--",
            "foundation/mod.rs",
            "consumer/main.rs",
            &implementation,
        ],
    );
    git(
        &checkout,
        &[
            "commit",
            "-m",
            "fixture: working architect foundation and real consumer",
        ],
    );
    let implemented_tip = git(&checkout, &["rev-parse", "HEAD"]);
    let graph_before = fs::read(root.join(".mine/plan/execution-graph.toml")).unwrap();
    // A dirty Root source change cannot be blessed by the normal implementation
    // transition. Delete only this test's exact deliberately injected file.
    write(
        root,
        "foundation/root-bypass.rs",
        b"not legitimate Plan work\n",
    );
    let (refused, diagnostic) = invoke(
        root,
        &[
            "plan",
            "implemented",
            "--actor",
            "fixture-architect",
            "--expected-revision",
            &revision(root),
            "--id",
            &id,
            "--report",
            &implementation,
            "--commit",
            &implemented_tip,
        ],
    );
    assert!(!refused.status.success());
    assert!(
        diagnostic.to_string().contains("Root") && diagnostic.to_string().contains("clean"),
        "{diagnostic}"
    );
    assert_eq!(
        fs::read(root.join(".mine/plan/execution-graph.toml")).unwrap(),
        graph_before
    );
    fs::remove_file(root.join("foundation/root-bypass.rs")).unwrap();
    fs::remove_dir(root.join("foundation")).unwrap();
    mine(
        root,
        &[
            "plan",
            "implemented",
            "--actor",
            "fixture-architect",
            "--expected-revision",
            &revision(root),
            "--id",
            &id,
            "--report",
            &implementation,
            "--commit",
            &implemented_tip,
        ],
    );
    let observed = mine(root, &["plan", "show", "--id", &id]);
    let node = &observed["data"]["plan"];
    assert_eq!(node["status"], "IMPLEMENTED");
    let identity = node["effective_authority"]["plan"].clone();
    let report = format!(".mine/plan/reports/{id}-architect-foundation-review.md");
    let input = ".mine/runtime/foundation-review.json";
    let mut closure = None;
    for (reviewer, accepted_begin) in [
        ("fixture-architect", false),
        ("fixture-independent-reviewer", true),
    ] {
        let assertion = json!({"schema_version":1,"plan":identity,"writer_owner":"fixture-architect",
            "writer_run_id":"foundation-run","reviewer":reviewer,"independent_review":true,
            "writer_stopped":true,"stopped_at":SystemClock.now_utc_rfc3339(),
            "stop_basis":"Synthetic fixture executor has completed its writes and explicitly hands off this checkout; no other process is using it."});
        let bytes = format!("Verdict: ACCEPTED\n\nSynthetic independent fixture review.\n\n```mine-review-closure\n{}\n```\n", serde_json::to_string(&assertion).unwrap()).into_bytes();
        write(&checkout, &report, &bytes);
        git(&checkout, &["add", "--", &report]);
        git(
            &checkout,
            &["commit", "-m", "fixture: pinned review assertion"],
        );
        let tip = git(&checkout, &["rev-parse", "HEAD"]);
        write(
            root,
            input,
            &serde_json::to_vec(&json!({"schema_version":1,"actor":reviewer,
            "intent":{"kind":"acceptance","plan_id":id,"review":{"reviewed_tip":tip,
            "report":{"path":report,"bytes":bytes.len(),"sha256":sha256(&bytes)}}}}))
            .unwrap(),
        );
        let before = fs::read(root.join(".mine/plan/execution-graph.toml")).unwrap();
        let (result, begun) = invoke(
            root,
            &[
                "operation",
                "begin",
                "--expected-revision",
                &revision(root),
                "--input",
                input,
            ],
        );
        assert_eq!(result.status.success(), accepted_begin, "{begun}");
        if !accepted_begin {
            assert!(begun.to_string().contains("independent"), "{begun}");
        }
        assert_eq!(
            fs::read(root.join(".mine/plan/execution-graph.toml")).unwrap(),
            before
        );
        if accepted_begin {
            assert_eq!(begun["data"]["operation"]["actor"], reviewer);
            closure = Some(begun["data"]["operation"].clone());
        }
    }
    use mine::application::{
        graph_service::GraphService, native_lifecycle::NativeLifecycle,
        native_verification::NativeVerification,
    };
    use mine::domain::{
        config::MineConfig,
        graph::{CommitId, FileIdentity, RepoPath, UtcTimestamp},
        lifecycle_operation::*,
    };
    use mine::infrastructure::toml_store::TomlStore;
    let operation: LifecycleOperation = serde_json::from_value(closure.unwrap()).unwrap();
    let reviewed_tip = operation
        .fields()
        .review
        .as_ref()
        .unwrap()
        .reviewed_tip
        .as_str();
    git(
        root,
        &[
            "merge",
            "--no-ff",
            "-m",
            "fixture: independently reviewed foundation",
            reviewed_tip,
        ],
    );
    let integrated = CommitId::new(git(root, &["rev-parse", "HEAD"])).unwrap();
    let config_path = root.join(".mine/config.toml");
    let config =
        MineConfig::parse(&config_path, &fs::read_to_string(&config_path).unwrap()).unwrap();
    let store = TomlStore::from_configuration(root, &config).unwrap();
    let stage_path = RepoPath::new(".mine/runtime/foundation-stage.json").unwrap();
    let mut fields = operation.fields().clone();
    fields.stage = OperationStage::Acceptance(AcceptanceStage::Integrated);
    fields.stage_revision += 1;
    fields.integrated_commit = Some(integrated);
    write(
        root,
        stage_path.as_str(),
        &serde_json::to_vec(&json!({
            "schema_version":1,"expected_stage_revision":0,
            "operation":LifecycleOperation::new(fields).unwrap(),"verification":null
        }))
        .unwrap(),
    );
    let progressed = NativeLifecycle::new(&GraphService::new(&store))
        .stage(revision(root).parse().unwrap(), &stage_path)
        .unwrap();
    let compiler = Command::new("rustc").arg("--version").output().unwrap();
    assert!(compiler.status.success());
    let context = VerificationContext {
        toolchains: std::collections::BTreeMap::from([
            ("rustc".into(), sha256(&compiler.stdout)),
            (
                "foundation-consumer".into(),
                sha256(&fs::read(&executable).unwrap()),
            ),
        ]),
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        features: vec![],
        environment: vec![],
    };
    let capture_path = RepoPath::new(format!(".mine/build/{id}/requests/capture.json")).unwrap();
    write(root, capture_path.as_str(), &serde_json::to_vec(&json!({
        "schema_version":1,"plan_id":id,"actor":"fixture-independent-reviewer",
        "check":"foundation-consumer","context":context,"policy_path":".mine/verification-policy.json",
        "started_at":UtcTimestamp::new(SystemClock.now_utc_rfc3339()).unwrap(),"release_operation_id":null
    })).unwrap());
    let graph = GraphService::new(&store);
    let verification = NativeVerification::new(&graph, root).unwrap();
    let capture = verification.capture(&capture_path).unwrap();
    let observed = Command::new(&executable)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(observed.status.success());
    assert!(observed.stderr.is_empty());
    assert_eq!(observed.stdout, b"foundation-invariants-ok\n");
    let artifact_path =
        RepoPath::new(format!(".mine/plan/reports/verification/{id}/consumer.log")).unwrap();
    write(root, artifact_path.as_str(), &observed.stdout);
    let record_path = RepoPath::new(format!(".mine/build/{id}/requests/record.json")).unwrap();
    write(root, record_path.as_str(), &serde_json::to_vec(&json!({
        "schema_version":1,"capture":capture,"policy_path":".mine/verification-policy.json","release_operation_id":null,
        "observation":{"ended_at":UtcTimestamp::new(SystemClock.now_utc_rfc3339()).unwrap(),
        "exit_code":observed.status.code(),"outcome":"passed","warnings_observed":false,
        "artifacts":[FileIdentity{path:artifact_path.clone(),bytes:observed.stdout.len() as u64,sha256:sha256(&observed.stdout)}]}
    })).unwrap());
    let decision = verification.record(&record_path).unwrap();
    assert_eq!(decision.disposition, VerificationDisposition::Run);
    let receipt = decision.original.unwrap();
    let receipt_path = RepoPath::new(format!(
        ".mine/plan/reports/verification/{id}/{}.json",
        receipt.fields().receipt_id.as_str()
    ))
    .unwrap();
    let receipt_bytes = fs::read(root.join(receipt_path.as_str())).unwrap();
    let receipt_file = FileIdentity {
        path: receipt_path.clone(),
        bytes: receipt_bytes.len() as u64,
        sha256: sha256(&receipt_bytes),
    };
    git(
        root,
        &["add", "--", artifact_path.as_str(), receipt_path.as_str()],
    );
    git(
        root,
        &[
            "commit",
            "-m",
            "fixture: retain real independent consumer run",
        ],
    );
    let gate = json!({"policy_path":".mine/verification-policy.json","checks":[{
        "check":"foundation-consumer","receipt":receipt_file,"context":context,"fresh_observation":null
    }]});
    let mut fields = progressed.operation.fields().clone();
    fields.stage = OperationStage::Acceptance(AcceptanceStage::Verified);
    fields.stage_revision += 1;
    fields.verification_evidence.push(receipt_file);
    write(root, stage_path.as_str(), &serde_json::to_vec(&json!({
        "schema_version":1,"expected_stage_revision":1,"operation":LifecycleOperation::new(fields).unwrap(),"verification":gate
    })).unwrap());
    let verified = NativeLifecycle::new(&GraphService::new(&store))
        .stage(revision(root).parse().unwrap(), &stage_path)
        .unwrap();
    git(root, &["worktree", "remove", checkout.to_str().unwrap()]);
    git(root, &["branch", "-d", "--", &branch]);
    let mut fields = verified.operation.fields().clone();
    fields.stage = OperationStage::Acceptance(AcceptanceStage::Cleaned);
    fields.stage_revision += 1;
    fields.cleanup = Some(CleanupEvidence {
        worktree: RepoPath::new(lookup["registered_worktree_path"].as_str().unwrap()).unwrap(),
        branch: branch.clone(),
        worktree_removed: true,
        branch_removed: true,
    });
    write(root, stage_path.as_str(), &serde_json::to_vec(&json!({
        "schema_version":1,"expected_stage_revision":2,"operation":LifecycleOperation::new(fields).unwrap(),"verification":gate
    })).unwrap());
    let cleaned = NativeLifecycle::new(&GraphService::new(&store))
        .stage(revision(root).parse().unwrap(), &stage_path)
        .unwrap();
    let mut fields = cleaned.operation.fields().clone();
    fields.stage = OperationStage::Acceptance(AcceptanceStage::Accepted);
    fields.stage_revision += 1;
    fields.outcome = OperationOutcome::Committed;
    write(root, stage_path.as_str(), &serde_json::to_vec(&json!({
        "schema_version":1,"expected_stage_revision":3,"operation":LifecycleOperation::new(fields).unwrap(),"verification":gate
    })).unwrap());
    mine(
        root,
        &[
            "plan",
            "accept",
            "--expected-revision",
            &revision(root),
            "--id",
            &id,
            "--operation-id",
            operation.fields().operation_id.as_str(),
            "--input",
            stage_path.as_str(),
        ],
    );
    let accepted = mine(root, &["plan", "show", "--id", &id]);
    assert_eq!(accepted["data"]["plan"]["status"], "ACCEPTED");
    assert!(!checkout.exists());
    assert!(git(root, &["branch", "--list", &branch]).is_empty());
    assert!(git(root, &["status", "--porcelain=v1"]).is_empty());
}
