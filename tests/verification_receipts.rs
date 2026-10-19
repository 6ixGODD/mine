#![forbid(unsafe_code)]

use mine::application::verification_service::*;
use mine::domain::graph::{PlanIdentity, PlanIdentityFields, RepoPath, UtcTimestamp};
use mine::domain::verification_receipt::*;
use mine::infrastructure::verification_store::FileVerificationStore;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

// mine-release-allow-plan-reference: exact base identity for isolated verification fixtures
const ID: &str = "plan01";
type Service = VerificationService<FileVerificationStore>;
fn p(value: &str) -> RepoPath {
    RepoPath::new(value).unwrap()
}
fn time(value: &str) -> UtcTimestamp {
    UtcTimestamp::new(value).unwrap()
}
fn write(root: &Path, name: &str, bytes: &[u8]) {
    let target = root.join(name);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, bytes).unwrap();
}
fn command() -> CheckCommand {
    CheckCommand::new(CommandFields {
        argv: vec!["check-with-no-process-launch".into(), "source".into()],
        cwd: CommandDirectory::Root,
    })
    .unwrap()
}
fn policy() -> VerificationPolicy {
    VerificationPolicy::new(PolicyFields {
        schema_version: 1,
        design_references: vec![p(".mine/design/checks.md")],
        checks: vec![CheckPolicy {
            name: "suite".into(),
            command: command(),
            inputs: InputScope::Full,
            tools: vec!["tool".into()],
            dependency_locks: vec![p("dependency.lock")],
            environment: vec![EnvironmentRequirement {
                name: "sandbox".into(),
                observable: true,
                secret_dependent: false,
            }],
            features: vec!["all".into()],
            warnings_disqualify: true,
            independent_required: false,
            native_platform: None,
            retention_bytes: 4096,
            retention_files: 8,
        }],
    })
    .unwrap()
}
fn context() -> VerificationContext {
    VerificationContext {
        toolchains: BTreeMap::from([("tool".into(), sha256(b"tool-v1"))]),
        platform: "fixture-os".into(),
        architecture: "fixture-arch".into(),
        features: vec!["all".into()],
        environment: vec![EnvironmentIdentity {
            name: "sandbox".into(),
            observable: true,
            secret_dependent: false,
            sha256: Some(sha256(
                b"isolated immutable home, no network or external data",
            )),
        }],
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    service: Service,
    plan: PlanIdentity,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "source", b"source-v1");
        write(dir.path(), "generator/input", b"generator-v1");
        write(dir.path(), "dependency.lock", b"lock-v1");
        write(dir.path(), ".gitignore", b"ignored-input\n");
        write(dir.path(), "ignored-input", b"ignored but relevant");
        write(dir.path(), "untracked-input", b"untracked but relevant");
        let plan_path = p(&format!(".mine/plan/{ID}-fixture.md"));
        write(dir.path(), plan_path.as_str(), b"immutable fixture Plan");
        let plan = PlanIdentity::new(PlanIdentityFields {
            repository_id: "repo".into(),
            workspace_id: "workspace".into(),
            plan_id: ID.into(),
            plan_path,
            body_sha256: Some(sha256(b"immutable fixture Plan")),
            lineage_id: ID.into(),
            branch: format!("plan/{ID}-fixture"),
        })
        .unwrap();
        let backend = FileVerificationStore::new(dir.path(), dir.path()).unwrap();
        let service = VerificationService::new(backend, "repo".into(), "workspace".into()).unwrap();
        Self { dir, service, plan }
    }
    fn request(&self, policy: &VerificationPolicy, actor: &str) -> VerificationRequest {
        VerificationRequest::new(
            self.plan.clone(),
            actor.into(),
            policy.check("suite").unwrap().command.clone(),
            policy.identity(),
        )
        .unwrap()
    }
    fn capture(
        &self,
        policy: &VerificationPolicy,
        ctx: &VerificationContext,
    ) -> VerificationCapture {
        self.service
            .capture(
                self.request(policy, "executor"),
                policy.clone(),
                "suite",
                ctx.clone(),
                time("2026-10-12T00:00:00Z"),
            )
            .unwrap()
    }
    fn output(&self, policy: &VerificationPolicy) -> ArtifactIdentity {
        let source = p(&format!(".mine/build/{ID}/logs/check.log"));
        write(self.dir.path(), source.as_str(), b"decisive check output\n");
        self.service
            .backend()
            .retain(&self.plan, &source, policy.check("suite").unwrap())
            .unwrap()
    }
    fn record(
        &self,
        policy: &VerificationPolicy,
        ctx: &VerificationContext,
    ) -> VerificationReceipt {
        let capture = self.capture(policy, ctx);
        let artifact = self.output(policy);
        self.finish(capture, artifact, CheckOutcome::Passed, Some(0), false)
    }
    fn finish(
        &self,
        capture: VerificationCapture,
        artifact: ArtifactIdentity,
        outcome: CheckOutcome,
        exit_code: Option<i32>,
        warnings_observed: bool,
    ) -> VerificationReceipt {
        let result = self
            .service
            .record(
                capture,
                RunObservation {
                    ended_at: time("2026-10-12T00:00:01Z"),
                    exit_code,
                    outcome,
                    warnings_observed,
                    artifacts: vec![artifact],
                },
            )
            .unwrap();
        assert_eq!(result.disposition, VerificationDisposition::Run);
        result.original.unwrap()
    }
    fn check(
        &self,
        policy: &VerificationPolicy,
        ctx: &VerificationContext,
        receipt: &VerificationReceipt,
    ) -> VerificationDecision {
        self.service.check(
            &self.request(policy, "reviewer"),
            policy,
            "suite",
            ctx,
            Some(&receipt.fields().receipt_id),
            &ReuseRequirements::default(),
        )
    }
    fn stored_path(&self, receipt: &VerificationReceipt) -> std::path::PathBuf {
        self.dir.path().join(format!(
            ".mine/plan/reports/verification/{ID}/{}.json",
            receipt.fields().receipt_id.as_str()
        ))
    }
}

#[test]
fn retained_plan_attribution_never_substitutes_candidate_inputs_or_old_full_keys() {
    // Adapter fixture only: native release-manifest admission is tested by its
    // own real-Git composition. This fixture isolates the required IO port.
    struct Retained {
        candidate: FileVerificationStore,
        evidence: FileVerificationStore,
        selected: PlanIdentity,
    }
    impl VerificationBackend for Retained {
        fn manifest(
            &self,
            policy: &VerificationPolicy,
            check: &str,
        ) -> VerificationResult<InputManifest> {
            self.candidate.manifest(policy, check)
        }
        fn file(&self, path: &RepoPath) -> VerificationResult<mine::domain::graph::FileIdentity> {
            self.candidate.file(path)
        }
        fn plan_body(
            &self,
            plan: &PlanIdentity,
        ) -> VerificationResult<mine::domain::graph::FileIdentity> {
            if plan != &self.selected {
                return Err(VerificationError::Refused("unselected retained Plan"));
            }
            self.evidence.file(&plan.fields().plan_path)
        }
        fn artifacts(
            &self,
            receipt: &VerificationReceipt,
            policy: &CheckPolicy,
        ) -> VerificationResult<()> {
            self.evidence.artifacts(receipt, policy)
        }
        fn load(
            &self,
            plan: &PlanIdentity,
            id: &ReceiptId,
        ) -> VerificationResult<Option<StoredVerification>> {
            self.evidence.load(plan, id)
        }
        fn save(&self, evidence: &StoredVerification) -> VerificationResult<()> {
            self.evidence.save(evidence)
        }
    }
    let original = Fixture::new();
    let policy = policy();
    let old = original.record(&policy, &context());
    let candidate = tempfile::tempdir().unwrap();
    let authority = tempfile::tempdir().unwrap();
    for path in [
        "source",
        "generator/input",
        "dependency.lock",
        ".gitignore",
        "ignored-input",
        "untracked-input",
    ] {
        write(
            candidate.path(),
            path,
            &fs::read(original.dir.path().join(path)).unwrap(),
        );
    }
    let service = VerificationService::new(
        Retained {
            candidate: FileVerificationStore::new(candidate.path(), authority.path()).unwrap(),
            evidence: FileVerificationStore::new(original.dir.path(), authority.path()).unwrap(),
            selected: original.plan.clone(),
        },
        "repo".into(),
        "workspace".into(),
    )
    .unwrap();
    let request = original.request(&policy, "reviewer");
    let body = &original.plan.fields().plan_path;
    assert!(service.backend().file(body).is_err());
    assert_eq!(
        service.backend().plan_body(&original.plan).unwrap().sha256,
        *original.plan.fields().body_sha256.as_ref().unwrap()
    );
    let old_bytes = fs::read(original.stored_path(&old)).unwrap();
    let invalidated = service.check(
        &request,
        &policy,
        "suite",
        &context(),
        Some(&old.fields().receipt_id),
        &ReuseRequirements::default(),
    );
    assert_eq!(
        invalidated.disposition,
        VerificationDisposition::Invalidated
    );
    assert_eq!(invalidated.reason, VerificationReason::InputsChanged);
    assert_eq!(fs::read(original.stored_path(&old)).unwrap(), old_bytes);
    let capture = service
        .capture(
            request.clone(),
            policy.clone(),
            "suite",
            context(),
            time("2026-10-12T00:00:02Z"),
        )
        .unwrap();
    assert!(
        !capture
            .export()
            .manifest
            .entries
            .iter()
            .any(|entry| &entry.file.path == body)
    );
    let fresh = service
        .record(
            capture,
            RunObservation {
                ended_at: time("2026-10-12T00:00:03Z"),
                exit_code: Some(0),
                outcome: CheckOutcome::Passed,
                warnings_observed: false,
                artifacts: old.fields().artifacts.clone(),
            },
        )
        .unwrap();
    assert_eq!(fresh.disposition, VerificationDisposition::Run);
    let receipt = fresh.original.unwrap();
    assert_eq!(
        service
            .check(
                &request,
                &policy,
                "suite",
                &context(),
                Some(&receipt.fields().receipt_id),
                &ReuseRequirements::default()
            )
            .disposition,
        VerificationDisposition::Reused
    );
    assert!(!candidate.path().join(".mine/plan").exists());

    let mut explicit = policy.fields().clone();
    explicit.checks[0].dependency_locks.push(body.clone());
    let explicit = VerificationPolicy::new(explicit).unwrap();
    let requiring_removed_input = original.request(&explicit, "reviewer");
    assert!(
        service
            .capture(
                requiring_removed_input,
                explicit,
                "suite",
                context(),
                time("2026-10-12T00:00:04Z")
            )
            .is_err()
    );
    assert!(service.backend().file(body).is_err());
    assert!(!candidate.path().join(".mine/plan").exists());
    write(
        original.dir.path(),
        body.as_str(),
        b"tampered retained body",
    );
    assert!(
        service
            .capture(
                request,
                policy,
                "suite",
                context(),
                time("2026-10-12T00:00:05Z")
            )
            .is_err()
    );
}

#[test]
fn unchanged_record_check_is_read_only_and_launches_no_check_process() {
    let f = Fixture::new();
    let receipt = f.record(&policy(), &context());
    let before = fs::read(f.stored_path(&receipt)).unwrap();
    for _ in 0..3 {
        let decision = f.check(&policy(), &context(), &receipt);
        assert_eq!(decision.disposition, VerificationDisposition::Reused);
        assert_eq!(decision.original.as_ref(), Some(&receipt));
    }
    // The command intentionally does not exist: either record or check spawning
    // it would fail. Persisted real logs/manifest/receipt are independently read.
    assert_eq!(fs::read(f.stored_path(&receipt)).unwrap(), before);
    assert!(!f.dir.path().join("target").exists());
}

#[test]
fn real_git_run_is_recorded_then_reused_without_another_trace_event() {
    let f = Fixture::new();
    let mut fields = policy().fields().clone();
    fields.checks[0].command = CheckCommand::new(CommandFields {
        argv: vec!["git".into(), "--version".into()],
        cwd: CommandDirectory::Root,
    })
    .unwrap();
    let policy = VerificationPolicy::new(fields).unwrap();
    let capture = f.capture(&policy, &context());
    let trace = f
        .dir
        .path()
        .join(format!(".mine/build/{ID}/trace/git.json"));
    fs::create_dir_all(trace.parent().unwrap()).unwrap();
    let result = std::process::Command::new("git")
        .arg("--version")
        .current_dir(f.dir.path())
        .env("HOME", f.dir.path())
        .env("USERPROFILE", f.dir.path())
        .env("GIT_TRACE2_EVENT", &trace)
        .output()
        .unwrap();
    assert!(result.status.success());
    let output = p(&format!(".mine/build/{ID}/logs/version.log"));
    write(f.dir.path(), output.as_str(), &result.stdout);
    let artifact = f
        .service
        .backend()
        .retain(&f.plan, &output, policy.check("suite").unwrap())
        .unwrap();
    let receipt = f.finish(
        capture,
        artifact,
        CheckOutcome::Passed,
        result.status.code(),
        false,
    );
    let before = fs::read(&trace).unwrap();
    assert!(!before.is_empty());
    assert_eq!(
        f.check(&policy, &context(), &receipt).disposition,
        VerificationDisposition::Reused
    );
    assert_eq!(fs::read(trace).unwrap(), before);
}

#[test]
fn portable_pre_run_capture_round_trips_and_rejects_mismatched_keys() {
    let f = Fixture::new();
    let capture = f.capture(&policy(), &context());
    let json = serde_json::to_vec(&capture.export()).unwrap();
    let fields: CaptureFields = serde_json::from_slice(&json).unwrap();
    let restored = f.service.import_capture(fields.clone()).unwrap();
    let receipt = f.finish(
        restored,
        f.output(&policy()),
        CheckOutcome::Passed,
        Some(0),
        false,
    );
    assert_eq!(
        f.check(&policy(), &context(), &receipt).disposition,
        VerificationDisposition::Reused
    );
    let mut forged = fields;
    forged.manifest.policy_sha256 = sha256(b"other policy");
    assert!(f.service.import_capture(forged).is_err());
}

#[test]
fn source_generator_lock_ignored_untracked_and_conflict_resolution_invalidate() {
    for name in [
        "source",
        "generator/input",
        "dependency.lock",
        "ignored-input",
        "untracked-input",
    ] {
        let f = Fixture::new();
        let receipt = f.record(&policy(), &context());
        write(f.dir.path(), name, b"resolved or changed input");
        let decision = f.check(&policy(), &context(), &receipt);
        assert_eq!(
            decision.disposition,
            VerificationDisposition::Invalidated,
            "{name}"
        );
        assert_eq!(decision.reason, VerificationReason::InputsChanged);
    }
    let f = Fixture::new();
    let receipt = f.record(&policy(), &context());
    write(f.dir.path(), "new-untracked-generator-input", b"new");
    assert_eq!(
        f.check(&policy(), &context(), &receipt).reason,
        VerificationReason::InputsChanged
    );
}

#[test]
fn path_mode_and_not_mtime_defines_identity() {
    let f = Fixture::new();
    let first = f.service.backend().manifest(&policy(), "suite").unwrap();
    write(f.dir.path(), "source", b"source-v1");
    assert_eq!(
        first,
        f.service.backend().manifest(&policy(), "suite").unwrap()
    );
    let source = f.dir.path().join("source");
    let original = fs::metadata(&source).unwrap().permissions();
    let mut changed = original.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        changed.set_mode(original.mode() ^ 0o100);
    }
    #[cfg(not(unix))]
    changed.set_readonly(!original.readonly());
    fs::set_permissions(&source, changed).unwrap();
    let second = f.service.backend().manifest(&policy(), "suite").unwrap();
    fs::set_permissions(&source, original).unwrap();
    assert_ne!(first.digest(), second.digest());
    fs::rename(&source, f.dir.path().join("renamed-source")).unwrap();
    assert_ne!(
        first.digest(),
        f.service
            .backend()
            .manifest(&policy(), "suite")
            .unwrap()
            .digest()
    );
}

#[test]
fn changed_tool_environment_platform_architecture_features_command_or_policy_is_not_reused() {
    let f = Fixture::new();
    let receipt = f.record(&policy(), &context());
    for kind in 0..5 {
        let mut ctx = context();
        match kind {
            0 => {
                ctx.toolchains.insert("tool".into(), sha256(b"tool-v2"));
            }
            1 => ctx.environment[0].sha256 = Some(sha256(b"changed-home")),
            2 => ctx.platform = "other-os".into(),
            3 => ctx.architecture = "other-arch".into(),
            _ => ctx.features = vec!["other-feature".into()],
        }
        assert_eq!(
            f.check(&policy(), &ctx, &receipt).disposition,
            VerificationDisposition::Invalidated
        );
    }
    let mut fields = policy().fields().clone();
    fields.checks[0].warnings_disqualify = false;
    assert_eq!(
        f.check(
            &VerificationPolicy::new(fields).unwrap(),
            &context(),
            &receipt
        )
        .reason,
        VerificationReason::PolicyChanged
    );
    let bad = VerificationRequest::new(
        f.plan.clone(),
        "reviewer".into(),
        CheckCommand::new(CommandFields {
            argv: vec!["different".into()],
            cwd: CommandDirectory::Root,
        })
        .unwrap(),
        policy().identity(),
    )
    .unwrap();
    assert_eq!(
        f.service
            .check(
                &bad,
                &policy(),
                "suite",
                &context(),
                Some(&receipt.fields().receipt_id),
                &ReuseRequirements::default()
            )
            .reason,
        VerificationReason::IdentityChanged
    );
}

#[test]
fn secret_mutable_home_network_or_external_state_is_non_reusable_and_never_leaked() {
    for (observable, secret) in [(false, false), (true, true), (true, false)] {
        let f = Fixture::new();
        let mut fields = policy().fields().clone();
        fields.checks[0].environment[0].observable = observable;
        fields.checks[0].environment[0].secret_dependent = secret;
        let policy = VerificationPolicy::new(fields).unwrap();
        let mut ctx = context();
        ctx.environment[0].observable = observable;
        ctx.environment[0].secret_dependent = secret;
        ctx.environment[0].sha256 = None;
        let receipt = f.record(&policy, &ctx);
        assert_eq!(
            f.check(&policy, &ctx, &receipt).disposition,
            VerificationDisposition::NonReusable
        );
        let persisted = fs::read_to_string(f.stored_path(&receipt)).unwrap();
        assert!(!persisted.contains("very-secret-value"));
        if secret || !observable {
            ctx.environment[0].sha256 = Some(sha256(b"very-secret-value"));
            assert!(
                f.service
                    .capture(
                        f.request(&policy, "executor"),
                        policy.clone(),
                        "suite",
                        ctx,
                        time("2026-10-12T00:00:00Z")
                    )
                    .is_err()
            );
        }
    }
}

#[test]
fn changed_during_run_records_honest_non_reusable_observation() {
    let f = Fixture::new();
    let capture = f.capture(&policy(), &context());
    let artifact = f.output(&policy());
    write(f.dir.path(), "source", b"changed during the command");
    let receipt = f.finish(capture, artifact, CheckOutcome::Passed, Some(0), false);
    let result = f.check(&policy(), &context(), &receipt);
    assert_eq!(result.disposition, VerificationDisposition::NonReusable);
    assert_eq!(result.reason, VerificationReason::InputsChangedDuringRun);
}

#[test]
fn failed_skipped_timeout_and_disqualifying_warnings_never_turn_green() {
    for (outcome, exit, warning) in [
        (CheckOutcome::Failed, Some(1), false),
        (CheckOutcome::Skipped, None, false),
        (CheckOutcome::TimedOut, None, false),
        (CheckOutcome::Passed, Some(0), true),
    ] {
        let f = Fixture::new();
        let c = f.capture(&policy(), &context());
        let receipt = f.finish(c, f.output(&policy()), outcome, exit, warning);
        assert_eq!(
            f.check(&policy(), &context(), &receipt).reason,
            VerificationReason::UnsuccessfulRun
        );
    }
}

#[test]
fn missing_forged_truncated_artifacts_receipts_and_changed_body_refuse_reuse() {
    for kind in 0..6 {
        let f = Fixture::new();
        let receipt = f.record(&policy(), &context());
        let artifact = f
            .dir
            .path()
            .join(receipt.fields().artifacts[0].path.as_str());
        match kind {
            0 => fs::remove_file(artifact).unwrap(),
            1 => fs::write(artifact, b"forged check output\n").unwrap(),
            2 => fs::write(artifact, b"x").unwrap(),
            3 => fs::write(f.stored_path(&receipt), b"{truncated").unwrap(),
            4 => fs::remove_file(f.stored_path(&receipt)).unwrap(),
            _ => write(
                f.dir.path(),
                f.plan.fields().plan_path.as_str(),
                b"changed Plan body",
            ),
        }
        assert_eq!(
            f.check(&policy(), &context(), &receipt).disposition,
            VerificationDisposition::Invalidated
        );
    }
}

#[test]
fn forged_artifact_path_and_overwriting_receipts_are_refused_without_mutation() {
    let f = Fixture::new();
    let receipt = f.record(&policy(), &context());
    let stored = f
        .service
        .backend()
        .load(&f.plan, &receipt.fields().receipt_id)
        .unwrap()
        .unwrap();
    let before = fs::read(f.stored_path(&receipt)).unwrap();
    assert!(f.service.backend().save(&stored).is_err());
    assert_eq!(fs::read(f.stored_path(&receipt)).unwrap(), before);
    let mut forged = serde_json::to_value(stored).unwrap();
    forged["receipt"]["artifacts"][0] =
        serde_json::to_value(f.service.backend().file(&p("source")).unwrap()).unwrap();
    fs::write(
        f.stored_path(&receipt),
        serde_json::to_vec(&forged).unwrap(),
    )
    .unwrap();
    assert_eq!(
        f.check(&policy(), &context(), &receipt).reason,
        VerificationReason::ArtifactsChanged
    );
    assert!(f.service.backend().cleanup_outputs(&f.plan).is_err());
}

#[test]
fn retention_has_a_plan_wide_bound_including_orphaned_outputs() {
    let f = Fixture::new();
    let directory = format!(".mine/plan/reports/verification/{ID}");
    for i in 0..128 {
        write(
            f.dir.path(),
            &format!("{directory}/orphan-{i}.log"),
            b"small orphan log",
        );
    }
    let source = p(&format!(".mine/build/{ID}/logs/check.log"));
    write(f.dir.path(), source.as_str(), b"new log");
    assert!(
        f.service
            .backend()
            .retain(&f.plan, &source, policy().check("suite").unwrap())
            .is_err()
    );
    assert!(f.dir.path().join(source.as_str()).exists());
}

#[test]
fn absent_receipt_is_not_run_wrong_repository_workspace_and_plan_are_rejected() {
    let f = Fixture::new();
    assert_eq!(
        f.service
            .check(
                &f.request(&policy(), "reviewer"),
                &policy(),
                "suite",
                &context(),
                None,
                &ReuseRequirements::default()
            )
            .disposition,
        VerificationDisposition::NotRun
    );
    for wrong_repo in [true, false] {
        let mut fields = f.plan.fields().clone();
        if wrong_repo {
            fields.repository_id = "foreign".into()
        } else {
            fields.workspace_id = "foreign".into()
        }
        let request = VerificationRequest::new(
            PlanIdentity::new(fields).unwrap(),
            "executor".into(),
            command(),
            policy().identity(),
        )
        .unwrap();
        assert!(
            f.service
                .capture(
                    request,
                    policy(),
                    "suite",
                    context(),
                    time("2026-10-12T00:00:00Z")
                )
                .is_err()
        );
    }
}

fn narrow(f: &Fixture) -> VerificationPolicy {
    narrow_for(f, command())
}
fn narrow_for(f: &Fixture, selected_command: CheckCommand) -> VerificationPolicy {
    let audit = DependencyAudit { schema_version: 1, command: selected_command.clone(), roots: vec![p("source")],
        generator_inputs: vec![p("generator/input")], excluded: vec![p(".mine/plan/execution-graph.toml"), p(".mine/plan/reports/prose.md")],
        reviewed_by: "fixture-owner".into(), rationale: "Fixture check reads source, generator/input and dependency.lock only; no directory discovery, Git, HOME, network or other generators.".into() };
    write(
        f.dir.path(),
        "dependency-audit.json",
        &serde_json::to_vec(&audit).unwrap(),
    );
    let mut fields = policy().fields().clone();
    fields.checks[0].command = selected_command;
    fields.checks[0].inputs = InputScope::DependencyClosure {
        declaration: DependencyScope {
            roots: audit.roots,
            generator_inputs: audit.generator_inputs,
            excluded: audit.excluded,
            audit: f
                .service
                .backend()
                .file(&p("dependency-audit.json"))
                .unwrap(),
        },
    };
    VerificationPolicy::new(fields).unwrap()
}

#[test]
fn dependency_proven_native_fixture_executes_once_then_reuses_graph_report_only_changes() {
    const CHILD_ROOT: &str = "MINE_VERIFICATION_DEPENDENCY_FIXTURE_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = std::path::PathBuf::from(root);
        // Complete repository read set; the executable hash binds this runner
        // and its generator logic. No directory scan or undeclared read.
        let mut inputs = Vec::new();
        for name in ["source", "generator/input", "dependency.lock"] {
            inputs.extend(fs::read(root.join(name)).unwrap());
        }
        println!("observed-inputs={}", sha256(&inputs).as_str());
        use std::io::Write;
        let counter = root.join(format!(".mine/build/{ID}/child-count"));
        fs::create_dir_all(counter.parent().unwrap()).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(counter)
            .unwrap()
            .write_all(b"one invocation\n")
            .unwrap();
        return;
    }
    let f = Fixture::new();
    let executable = std::env::current_exe().unwrap();
    let selected = CheckCommand::new(CommandFields {
        argv: vec![
            executable.to_string_lossy().into_owned(),
            "--exact".into(),
            "dependency_proven_native_fixture_executes_once_then_reuses_graph_report_only_changes"
                .into(),
            "--nocapture".into(),
        ],
        cwd: CommandDirectory::Root,
    })
    .unwrap();
    let policy = narrow_for(&f, selected.clone());
    let mut ctx = context();
    ctx.toolchains
        .insert("tool".into(), sha256(&fs::read(&executable).unwrap()));
    ctx.environment[0].sha256 = Some(sha256(f.dir.path().to_string_lossy().as_bytes()));
    let capture = f.capture(&policy, &ctx);
    let result = std::process::Command::new(&selected.fields().argv[0])
        .args(&selected.fields().argv[1..])
        .env(CHILD_ROOT, f.dir.path())
        .env("HOME", f.dir.path())
        .env("USERPROFILE", f.dir.path())
        .current_dir(f.dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("observed-inputs="));
    let output = p(&format!(".mine/build/{ID}/logs/native-fixture.log"));
    write(f.dir.path(), output.as_str(), &result.stdout);
    let artifact = f
        .service
        .backend()
        .retain(&f.plan, &output, policy.check("suite").unwrap())
        .unwrap();
    let receipt = f.finish(
        capture,
        artifact,
        CheckOutcome::Passed,
        result.status.code(),
        false,
    );
    write(
        f.dir.path(),
        ".mine/plan/execution-graph.toml",
        b"graph-only commit input",
    );
    write(
        f.dir.path(),
        ".mine/plan/reports/prose.md",
        b"review-only commit input",
    );
    assert_eq!(
        f.check(&policy, &ctx, &receipt).disposition,
        VerificationDisposition::Reused
    );
    assert_eq!(
        fs::read(f.dir.path().join(format!(".mine/build/{ID}/child-count"))).unwrap(),
        b"one invocation\n"
    );
    write(f.dir.path(), "source", b"conflict-resolved source");
    assert_eq!(
        f.check(&policy, &ctx, &receipt).reason,
        VerificationReason::InputsChanged
    );
}

#[test]
fn full_policy_invalidates_graph_reports_narrow_dependency_proof_reuses_and_binds_generator() {
    let f = Fixture::new();
    let receipt = f.record(&policy(), &context());
    write(
        f.dir.path(),
        ".mine/plan/execution-graph.toml",
        b"graph-only change",
    );
    assert_eq!(
        f.check(&policy(), &context(), &receipt).reason,
        VerificationReason::InputsChanged
    );
    let policy = narrow(&f);
    let receipt = f.record(&policy, &context());
    write(
        f.dir.path(),
        ".mine/plan/execution-graph.toml",
        b"new graph-only change",
    );
    write(
        f.dir.path(),
        ".mine/plan/reports/prose.md",
        b"report-only change",
    );
    assert_eq!(
        f.check(&policy, &context(), &receipt).disposition,
        VerificationDisposition::Reused
    );
    write(f.dir.path(), "generator/input", b"generator-v2");
    assert_eq!(
        f.check(&policy, &context(), &receipt).reason,
        VerificationReason::InputsChanged
    );
}

#[test]
fn narrowing_without_exact_audit_or_with_overlapping_exclusion_is_refused() {
    let f = Fixture::new();
    let policy = narrow(&f);
    write(f.dir.path(), "dependency-audit.json", b"forged audit");
    assert!(f.service.backend().manifest(&policy, "suite").is_err());
    let mut fields = policy.fields().clone();
    if let InputScope::DependencyClosure { declaration } = &mut fields.checks[0].inputs {
        declaration.excluded.push(p("generator"));
    }
    assert!(VerificationPolicy::new(fields).is_err());
    let mut json = serde_json::to_value(policy).unwrap();
    json["schema_version"] = 2.into();
    assert!(serde_json::from_value::<VerificationPolicy>(json).is_err());
}

#[test]
fn independent_and_native_execution_requirements_cannot_be_replaced_by_a_receipt() {
    let f = Fixture::new();
    let mut fields = policy().fields().clone();
    fields.checks[0].independent_required = true;
    fields.checks[0].native_platform = Some(context().platform);
    let policy = VerificationPolicy::new(fields).unwrap();
    let receipt = f.record(&policy, &context());
    assert_eq!(
        f.check(&policy, &context(), &receipt).reason,
        VerificationReason::IndependentRunRequired
    );
    let decision = |actor| {
        f.service.check(
            &f.request(&policy, "reviewer"),
            &policy,
            "suite",
            &context(),
            Some(&receipt.fields().receipt_id),
            &ReuseRequirements {
                independent_of: Some(actor),
            },
        )
    };
    assert_eq!(
        decision("executor".into()).reason,
        VerificationReason::IndependentRunRequired
    );
    assert_eq!(
        decision("other-actor".into()).disposition,
        VerificationDisposition::Reused
    );
    let mut ctx = context();
    ctx.platform = "other-native-host".into();
    assert_eq!(
        f.service
            .check(
                &f.request(&policy, "reviewer"),
                &policy,
                "suite",
                &ctx,
                Some(&receipt.fields().receipt_id),
                &ReuseRequirements {
                    independent_of: Some("other-actor".into())
                }
            )
            .reason,
        VerificationReason::NativePlatformRequired
    );
}

#[test]
fn self_inclusion_git_linked_worktrees_outputs_and_receipts_do_not_recurse() {
    let f = Fixture::new();
    let before = f.service.backend().manifest(&policy(), "suite").unwrap();
    write(f.dir.path(), ".git/objects/not-input", b"git metadata");
    write(
        f.dir.path(),
        ".mine/worktrees/foreign/source",
        b"foreign mutable checkout",
    );
    write(
        f.dir.path(),
        ".mine/build/foreign/output",
        b"foreign output",
    );
    let receipt = f.record(&policy(), &context());
    assert_eq!(
        before,
        f.service.backend().manifest(&policy(), "suite").unwrap()
    );
    assert_eq!(
        f.check(&policy(), &context(), &receipt).disposition,
        VerificationDisposition::Reused
    );
    // Pruning an empty output ancestor is not permission to hide source
    // directories or prose alongside the receipt namespace.
    let source_directory = ".mine/plan/reports/user-empty-directory";
    fs::create_dir_all(f.dir.path().join(source_directory)).unwrap();
    let with_source = f.service.backend().manifest(&policy(), "suite").unwrap();
    assert_ne!(with_source, before);
    assert!(
        with_source
            .entries
            .iter()
            .any(|entry| { entry.directory && entry.file.path.as_str() == source_directory })
    );
    write(
        f.dir.path(),
        ".mine/plan/reports/prose.md",
        b"source report",
    );
    assert_ne!(
        f.service.backend().manifest(&policy(), "suite").unwrap(),
        with_source
    );
}

#[test]
fn coordinator_locks_are_pruned_before_held_file_io_without_hiding_real_inputs() {
    use mine::infrastructure::file_lock;
    let f = Fixture::new();
    let before = f.service.backend().manifest(&policy(), "suite").unwrap();
    let location = f.dir.path().join(".mine/locks/graph.lock");
    let guard = file_lock::acquire_exclusive(&location, std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(
        f.service.backend().manifest(&policy(), "suite").unwrap(),
        before
    );
    drop(guard);
    write(
        f.dir.path(),
        ".mine/locks/graph.lock",
        b"changed infrastructure bytes",
    );
    assert_eq!(
        f.service.backend().manifest(&policy(), "suite").unwrap(),
        before
    );
    for name in [
        ".mine/locks-extra/source",
        ".mine/config.toml",
        ".mine/verification-policy.json",
        "Cargo.lock",
    ] {
        let before = f.service.backend().manifest(&policy(), "suite").unwrap();
        write(f.dir.path(), name, b"genuine source input");
        assert_ne!(
            f.service.backend().manifest(&policy(), "suite").unwrap(),
            before,
            "{name}"
        );
    }
    #[cfg(windows)]
    {
        let _ordinary = file_lock::acquire_exclusive(
            &f.dir.path().join("source"),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        assert!(
            matches!(f.service.backend().manifest(&policy(), "suite"), Err(VerificationError::Io(error)) if error.raw_os_error() == Some(33))
        );
    }
}

#[test]
fn explicit_coordinator_input_roles_are_typed_refusals_before_reading_a_held_lock() {
    use mine::infrastructure::file_lock;
    let f = Fixture::new();
    let _guard = file_lock::acquire_exclusive(
        &f.dir.path().join(".mine/locks/graph.lock"),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    let forbidden = p(".mine/locks/graph.lock");
    for role in 0..5 {
        let mut fields = policy().fields().clone();
        let selected = &mut fields.checks[0];
        if role == 2 {
            selected.dependency_locks = vec![forbidden.clone()];
        } else if role == 4 {
            selected.command = CheckCommand::new(CommandFields {
                argv: vec!["never-executed".into()],
                cwd: CommandDirectory::Relative(p(".mine/locks")),
            })
            .unwrap();
        } else {
            selected.inputs = InputScope::DependencyClosure {
                declaration: DependencyScope {
                    roots: vec![if role == 0 {
                        forbidden.clone()
                    } else {
                        p("source")
                    }],
                    generator_inputs: if role == 1 {
                        vec![forbidden.clone()]
                    } else {
                        vec![]
                    },
                    excluded: vec![],
                    audit: ArtifactIdentity {
                        path: if role == 3 {
                            forbidden.clone()
                        } else {
                            p("audit.json")
                        },
                        bytes: 1,
                        sha256: sha256(b"unread"),
                    },
                },
            };
        }
        let policy = VerificationPolicy::new(fields).unwrap();
        assert!(
            matches!(
                f.service.backend().manifest(&policy, "suite"),
                Err(VerificationError::Refused(_))
            ),
            "role={role}"
        );
    }
    #[cfg(windows)]
    {
        let mut fields = policy().fields().clone();
        fields.checks[0].dependency_locks = vec![p(".MINE/LOCKS/GRAPH.LOCK")];
        assert!(matches!(
            f.service
                .backend()
                .manifest(&VerificationPolicy::new(fields).unwrap(), "suite"),
            Err(VerificationError::Refused(_))
        ));
    }
}

#[test]
fn active_output_users_block_cleanup_evidence_is_preserved_and_foreign_outputs_untouched() {
    let f = Fixture::new();
    let receipt = f.record(&policy(), &context());
    write(f.dir.path(), ".mine/build/foreign/keep", b"preserved");
    let lease = f.service.backend().lease_outputs(&f.plan).unwrap();
    assert!(f.service.backend().cleanup_outputs(&f.plan).is_err());
    assert!(f.dir.path().join(format!(".mine/build/{ID}")).exists());
    drop(lease);
    assert!(f.service.backend().cleanup_outputs(&f.plan).unwrap());
    assert!(f.stored_path(&receipt).exists());
    assert_eq!(
        f.check(&policy(), &context(), &receipt).disposition,
        VerificationDisposition::Reused
    );
    assert_eq!(
        fs::read(f.dir.path().join(".mine/build/foreign/keep")).unwrap(),
        b"preserved"
    );
    assert!(!f.service.backend().cleanup_outputs(&f.plan).unwrap());
}

#[test]
fn missing_retained_evidence_or_unbounded_logs_refuse_cleanup_and_record() {
    let f = Fixture::new();
    let receipt = f.record(&policy(), &context());
    fs::remove_file(
        f.dir
            .path()
            .join(receipt.fields().artifacts[0].path.as_str()),
    )
    .unwrap();
    assert!(f.service.backend().cleanup_outputs(&f.plan).is_err());
    let output = p(&format!(".mine/build/{ID}/huge.log"));
    write(f.dir.path(), output.as_str(), &vec![b'x'; 4097]);
    assert!(
        f.service
            .backend()
            .retain(&f.plan, &output, policy().check("suite").unwrap())
            .is_err()
    );
    assert!(
        f.service
            .backend()
            .retain(&f.plan, &p("source"), policy().check("suite").unwrap())
            .is_err()
    );
}

#[test]
fn linked_source_uses_root_owned_outputs_and_locks_not_worktree_caches() {
    let f = Fixture::new();
    let root = tempfile::tempdir().unwrap();
    let backend = FileVerificationStore::new(f.dir.path(), root.path()).unwrap();
    let output = p(&format!(".mine/build/{ID}/check.log"));
    write(root.path(), output.as_str(), b"actual root output");
    let artifact = backend
        .retain(&f.plan, &output, policy().check("suite").unwrap())
        .unwrap();
    assert!(f.dir.path().join(artifact.path.as_str()).is_file());
    assert!(!f.dir.path().join(".mine/runtime").exists());
    assert!(!f.dir.path().join(".mine/build").exists());
}

#[test]
fn malformed_paths_and_escaping_or_cyclic_links_are_refused() {
    for bad in [
        "../outside",
        "./source",
        "source/../other",
        "C:/secret",
        "source\\other",
    ] {
        assert!(RepoPath::new(bad).is_err());
    }
    let f = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    {
        let target = f.dir.path().join("escaping-link");
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(target.to_string_lossy().replace('/', "\\"))
            .arg(outside.path().to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(f.service.backend().manifest(&policy(), "suite").is_err());
        fs::remove_dir(target).unwrap();
        let cycle = f.dir.path().join("cycle");
        assert!(
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(cycle.to_string_lossy().replace('/', "\\"))
                .arg(f.dir.path().to_string_lossy().replace('/', "\\"))
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(f.service.backend().manifest(&policy(), "suite").is_err());
        fs::remove_dir(cycle).unwrap();
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), f.dir.path().join("escaping-link")).unwrap();
        assert!(f.service.backend().manifest(&policy(), "suite").is_err());
        fs::remove_file(f.dir.path().join("escaping-link")).unwrap();
        std::os::unix::fs::symlink(f.dir.path(), f.dir.path().join("cycle")).unwrap();
        assert!(f.service.backend().manifest(&policy(), "suite").is_err());
    }
}

#[test]
fn repository_default_policy_is_valid_full_input_and_conservatively_nonhermetic() {
    let policy: VerificationPolicy =
        serde_json::from_str(include_str!("../.mine/verification-policy.json")).unwrap();
    assert_eq!(policy.fields().checks.len(), 7);
    for check in &policy.fields().checks {
        assert_eq!(check.inputs, InputScope::Full);
        assert!(
            check
                .environment
                .iter()
                .any(|e| e.name == "HOME" && !e.observable)
        );
        assert!(
            check
                .environment
                .iter()
                .any(|e| e.name == "NETWORK" && !e.observable)
        );
    }
}

#[test]
fn portable_path_aliases_and_incomplete_publication_cannot_authorize_cleanup() {
    let f = Fixture::new();
    for alias in ["source.", "source ", "NUL", "CON.txt", "LPT1"] {
        match RepoPath::new(alias) {
            Ok(path) => assert!(f.service.backend().file(&path).is_err()),
            Err(error) => assert_eq!(error.field, "path"),
        }
    }
    let receipt = f.record(&policy(), &context());
    write(
        f.dir.path(),
        &format!(".mine/plan/reports/verification/{ID}/.interrupted.tmp"),
        b"partial evidence",
    );
    assert!(f.service.backend().cleanup_outputs(&f.plan).is_err());
    assert!(f.stored_path(&receipt).exists());
}
