#![forbid(unsafe_code)]
//! Real isolated Git S/C observations; synthetic facts never touch live Root.
#[path = "support/layout_fixture.rs"]
mod fixture;

use fixture::{LayoutFixture, git, write};
use mine::application::release_service::curation::{
    inspect_layout_candidate, stage_layout_cutover,
};
use mine::domain::{adr::AdrRegistry, config::MineConfig, graph::CommitId};
use std::fs;

fn bytes(root: &std::path::Path, name: &str) -> Vec<u8> {
    fs::read(root.join(name)).unwrap()
}

#[test]
fn source_authorized_cutover_preserves_raw_approvals_metadata_and_user_files() {
    let fixture = LayoutFixture::new();
    let root = fixture.root();
    let graph = bytes(root, &fixture.config.graph.source);
    let index = bytes(root, ".git/index");
    assert!(
        git(
            root,
            &[
                "merge-base",
                "--is-ancestor",
                fixture.implementation.as_str(),
                fixture.source.as_str()
            ]
        )
        .status
        .success()
    );
    let (relative, target) = fixture.checkout("valid spaces 非ASCII");
    let staged = stage_layout_cutover(root, &fixture.source, &relative).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&bytes(root, staged.manifest.as_str())).unwrap();
    assert_eq!(manifest["pending"], false);
    assert!(manifest["writes_verified"].as_u64().unwrap() > 8);
    assert_eq!(
        bytes(&target, ".mine/design/decisions/9001-frozen-crlf.md"),
        fixture::FROZEN_BYTES
    );
    assert_eq!(
        bytes(
            &target,
            ".mine/design/decisions/0023-mine-local-governance-layout.md"
        ),
        bytes(root, fixture::ADR)
    );
    assert!(!target.join("docs/design").exists());
    assert!(!target.join("docs/plan").exists());
    assert!(!target.join(".mine/plan").exists());
    for name in ["README.md", "docs/user.md"] {
        assert_eq!(bytes(&target, name), bytes(root, name));
    }
    assert_eq!(
        bytes(&target, ".mine/design/diagram.png"),
        bytes(root, "docs/design/diagram.png")
    );
    let config_raw = bytes(&target, ".mine/config.toml");
    assert!(config_raw.starts_with(b"# Keep this foreign formatting/comment\r\n"));
    let config = MineConfig::parse(
        &target.join(".mine/config.toml"),
        std::str::from_utf8(&config_raw).unwrap(),
    )
    .unwrap();
    assert_eq!(config.plan.root, ".mine/plan");
    assert_eq!(config.design.root, ".mine/design/index.md");
    let registry_raw = bytes(&target, &config.adr.registry);
    assert!(registry_raw.starts_with(b"# Registry trivia must survive\r\n"));
    let registry: AdrRegistry =
        toml::from_str(std::str::from_utf8(&registry_raw).unwrap()).unwrap();
    let mut expected = fixture.registry.clone();
    for entry in &mut expected.entries {
        entry.path = entry.path.replacen("docs/design", ".mine/design", 1);
        entry.planning_scopes[0] = ".mine/design".into();
    }
    assert_eq!(registry, expected);
    let agents = String::from_utf8(bytes(&target, "AGENTS.md")).unwrap();
    assert!(agents.starts_with("# Foreign prefix docs/design remains\n"));
    assert!(agents.contains("`.mine/plan/`"));
    let candidate = fixture.commit_candidate(&target);
    let evidence = inspect_layout_candidate(root, &fixture.source, &candidate, &relative).unwrap();
    assert_eq!(evidence.source_commit, fixture.source);
    assert_eq!(evidence.candidate_commit, candidate);
    let tracked = git(&target, &["ls-files", "-z"]);
    let tracked = String::from_utf8(tracked.stdout).unwrap();
    assert!(
        !tracked
            .split('\0')
            .any(|name| name.starts_with(".mine/plan/")
                || name.starts_with("docs/plan/")
                || name.starts_with(".mine/runtime/"))
    );
    let blob = git(
        &target,
        &["show", "HEAD:.mine/design/decisions/9001-frozen-crlf.md"],
    );
    assert_eq!(blob.stdout, fixture::FROZEN_BYTES);
    let clone = root.join(".mine/runtime/roundtrip-clone");
    git(
        root,
        &[
            "clone",
            "--no-checkout",
            "--",
            root.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    git(&clone, &["checkout", "--detach", candidate.as_str()]);
    assert_eq!(
        bytes(&clone, ".mine/design/decisions/9001-frozen-crlf.md"),
        fixture::FROZEN_BYTES
    );
    assert_eq!(bytes(root, &fixture.config.graph.source), graph);
    assert_eq!(bytes(root, ".git/index"), index);
    assert_eq!(
        String::from_utf8(git(root, &["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim(),
        fixture.source.as_str()
    );
    assert!(git(root, &["status", "--porcelain=v1"]).stdout.is_empty());
}

#[test]
fn unapproved_or_modified_frozen_source_is_refused_before_staging() {
    for mutation in ["approval", "body", "marker", "implementation"] {
        let mut fixture = LayoutFixture::new();
        match mutation {
            "approval" => {
                let mut registry = fixture.registry.clone();
                registry.entries[0].approved_by.clear();
                write(
                    fixture.root(),
                    &fixture.config.adr.registry,
                    toml::to_string(&registry).unwrap().as_bytes(),
                );
                let path = fixture.config.adr.registry.clone();
                fixture.checkpoint(&[&path]);
            }
            "body" => {
                write(
                    fixture.root(),
                    fixture::FROZEN,
                    b"# Mutated after approval\n",
                );
                fixture.checkpoint(&[fixture::FROZEN]);
            }
            "marker" => {
                let marker = mine::domain::design_marker::DesignMarker::new(
                    uuid::Uuid::new_v4().to_string(),
                    "2026-10-19T00:00:00Z".into(),
                );
                write(
                    fixture.root(),
                    &fixture.config.design.marker,
                    marker.to_toml().as_bytes(),
                );
                let path = fixture.config.design.marker.clone();
                fixture.checkpoint(&[&path]);
            }
            _ => {
                let path = "scripts/stage-layout-cutover.rs";
                let mut modified = bytes(fixture.root(), path);
                modified.extend_from_slice(b"\n// Later unreviewed implementation\n");
                write(fixture.root(), path, &modified);
                fixture.checkpoint(&[path]);
            }
        }
        let (relative, target) = fixture.checkout(mutation);
        let before = bytes(fixture.root(), ".git/index");
        let error = stage_layout_cutover(fixture.root(), &fixture.source, &relative).unwrap_err();
        assert!(!error.to_string().is_empty());
        assert!(!target.join(".mine/design").exists());
        assert!(!fixture.root().join(".mine/runtime/layout-staging").exists());
        assert_eq!(bytes(fixture.root(), ".git/index"), before);
        assert!(target.join("docs/design/index.md").is_file());
    }
}

#[test]
fn collisions_and_foreign_attributes_are_refused_without_writes() {
    for conflict in ["collision", "attributes"] {
        let fixture = LayoutFixture::new();
        let (relative, target) = fixture.checkout(conflict);
        if conflict == "collision" {
            write(&target, ".mine/design/foreign.md", b"owner data");
        } else {
            write(
                fixture.root(),
                ".git/info/attributes",
                b"*.md text eol=lf\n",
            );
        }
        let error = stage_layout_cutover(fixture.root(), &fixture.source, &relative).unwrap_err();
        assert!(!error.to_string().is_empty());
        assert!(!target.join(".mine/.gitattributes").exists());
        assert!(target.join("docs/design/index.md").is_file());
        assert!(!fixture.root().join(".mine/runtime/layout-staging").exists());
        if conflict == "collision" {
            assert_eq!(bytes(&target, ".mine/design/foreign.md"), b"owner data");
        }
    }
}

#[test]
fn forged_candidate_delta_and_post_observation_attribute_changes_are_refused() {
    let fixture = LayoutFixture::new();
    let (relative, target) = fixture.checkout("tampering");
    stage_layout_cutover(fixture.root(), &fixture.source, &relative).unwrap();
    let valid = fixture.commit_candidate(&target);
    inspect_layout_candidate(fixture.root(), &fixture.source, &valid, &relative).unwrap();
    for (name, data) in [
        ("README.md", b"unrelated behavior".as_slice()),
        (".mine/config.toml", b"forged config".as_slice()),
        ("extra.txt", b"smuggled product".as_slice()),
    ] {
        git(&target, &["switch", "--detach", valid.as_str()]);
        write(&target, name, data);
        git(&target, &["add", "--", name]);
        git(
            &target,
            &["commit", "-m", "fixture: unauthorized candidate mutation"],
        );
        let forged = CommitId::new(
            String::from_utf8(git(&target, &["rev-parse", "HEAD"]).stdout)
                .unwrap()
                .trim(),
        )
        .unwrap();
        assert!(
            inspect_layout_candidate(fixture.root(), &fixture.source, &forged, &relative).is_err(),
            "accepted {name}"
        );
    }
    git(&target, &["switch", "--detach", valid.as_str()]);
    write(
        fixture.root(),
        ".git/info/attributes",
        b".mine/design/** text eol=lf\n",
    );
    assert!(inspect_layout_candidate(fixture.root(), &fixture.source, &valid, &relative).is_err());
}

#[test]
fn escaping_destination_link_is_refused_before_candidate_or_source_writes() {
    let fixture = LayoutFixture::new();
    let (relative, target) = fixture.checkout("escaping-link");
    let foreign = tempfile::tempdir().unwrap();
    write(foreign.path(), "owner.md", b"unrelated external data");
    let link = target.join(".mine/design");
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link.to_str().unwrap().replace('/', "\\"))
            .arg(foreign.path().to_str().unwrap().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(foreign.path(), &link).unwrap();
    let before = bytes(fixture.root(), ".git/index");
    assert!(stage_layout_cutover(fixture.root(), &fixture.source, &relative).is_err());
    assert_eq!(
        bytes(foreign.path(), "owner.md"),
        b"unrelated external data"
    );
    assert_eq!(bytes(fixture.root(), ".git/index"), before);
    assert!(!fixture.root().join(".mine/runtime/layout-staging").exists());
    assert!(!target.join(".mine/.gitattributes").exists());
}

#[test]
fn native_release_keeps_source_receipts_after_candidate_layout_integration() {
    use mine::application::{
        graph_service::GraphService, native_lifecycle::NativeLifecycle,
        native_verification::NativeVerification,
    };
    use mine::domain::{
        adr::sha256_hex,
        graph::{FileIdentity, RepoPath, Sha256, UtcTimestamp},
        lifecycle_operation::{LifecycleOperation, OperationOutcome, OperationStage, ReleaseStage},
        ports::Clock,
        verification_receipt::{VerificationContext, VerificationDisposition},
    };
    use mine::infrastructure::{system::SystemClock, toml_store::TomlStore};
    use std::collections::BTreeMap;
    let fixture = LayoutFixture::new();
    let root = fixture.root();
    let (relative, target) = fixture.checkout("native-release");
    stage_layout_cutover(root, &fixture.source, &relative).unwrap();
    let candidate = fixture.commit_candidate(&target);
    let store = TomlStore::from_configuration(root, &fixture.config).unwrap();
    let graph = GraphService::new(&store);
    let input = RepoPath::new(".mine/runtime/release-request.json").unwrap();
    // mine-release-allow-plan-reference: exact synthetic attribution, never a real repository operation
    let id = "plan24";
    let policy = ".mine/verification-policy.json";
    write(
        root,
        input.as_str(),
        &serde_json::to_vec(&serde_json::json!({
            "schema_version":1,"actor":"synthetic-independent-reviewer",
            "intent":{"kind":"release","candidate":candidate,"candidate_root":relative,
            "attribution":[id],"policy_path":policy}
        }))
        .unwrap(),
    );
    let operation = NativeLifecycle::new(&graph).begin(3, &input).unwrap();
    let operation_id = operation.fields().operation_id.clone();
    let context = VerificationContext {
        toolchains: BTreeMap::from([(
            "git".into(),
            Sha256::new(sha256_hex(&git(&target, &["--version"]).stdout).to_ascii_lowercase())
                .unwrap(),
        )]),
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        features: vec![],
        environment: vec![],
    };
    let capture_path = RepoPath::new(format!(".mine/build/{id}/requests/capture.json")).unwrap();
    write(root, capture_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"plan_id":id,"actor":"synthetic-independent-reviewer","check":"fixture",
        "policy_path":policy,"context":context,"started_at":UtcTimestamp::new(SystemClock.now_utc_rfc3339()).unwrap(),
        "release_operation_id":operation_id
    })).unwrap());
    let verification = NativeVerification::new(&graph, root).unwrap();
    let capture = verification.capture(&capture_path).unwrap();
    assert!(
        capture
            .plan
            .fields()
            .plan_path
            .as_str()
            .starts_with("docs/plan/")
    );
    assert!(
        !target
            .join(capture.plan.fields().plan_path.as_str())
            .exists()
    );
    assert!(
        capture
            .manifest
            .entries
            .iter()
            .any(|entry| entry.file.path.as_str() == ".mine/design/index.md")
    );
    assert!(
        !capture.manifest.entries.iter().any(|entry| entry
            .file
            .path
            .as_str()
            .starts_with("docs/design/"))
    );
    let actual = git(&target, &["rev-parse", "--verify", "HEAD^{tree}"]).stdout;
    let artifact = RepoPath::new(format!(
        "docs/plan/reports/verification/{id}/candidate-tree.log"
    ))
    .unwrap();
    let evidence_root = root.join(format!(
        ".mine/runtime/release-evidence/{}/evidence",
        operation_id.as_str()
    ));
    write(&evidence_root, artifact.as_str(), &actual);
    let record_path = RepoPath::new(format!(".mine/build/{id}/requests/record.json")).unwrap();
    write(root, record_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"capture":capture,"policy_path":policy,"release_operation_id":operation_id,
        "observation":{"ended_at":UtcTimestamp::new(SystemClock.now_utc_rfc3339()).unwrap(),
        "exit_code":0,"outcome":"passed","warnings_observed":false,
        "artifacts":[FileIdentity{path:artifact,bytes:actual.len() as u64,sha256:Sha256::new(sha256_hex(&actual).to_ascii_lowercase()).unwrap()}]}
    })).unwrap());
    let recorded = verification.record(&record_path).unwrap();
    assert_eq!(recorded.disposition, VerificationDisposition::Run);
    let receipt = recorded.original.unwrap();
    let logical = format!(
        "docs/plan/reports/verification/{id}/{}.json",
        receipt.fields().receipt_id.as_str()
    );
    assert!(evidence_root.join(&logical).is_file());
    assert!(!root.join(&logical).exists());
    assert!(!target.join(&logical).exists());
    let receipt_bytes = bytes(&evidence_root, &logical);
    let logical_receipt = FileIdentity {
        path: RepoPath::new(logical.clone()).unwrap(),
        bytes: receipt_bytes.len() as u64,
        sha256: Sha256::new(sha256_hex(&receipt_bytes).to_ascii_lowercase()).unwrap(),
    };
    let gate = serde_json::json!({"policy_path":policy,"checks":[{
        "check":"fixture","receipt":logical_receipt,"context":context,"fresh_observation":null
    }]});
    let stage_path = RepoPath::new(".mine/runtime/stage.json").unwrap();
    let mut fields = operation.fields().clone();
    fields.stage_revision += 1;
    fields.stage = OperationStage::Release(ReleaseStage::Preflight);
    let mut physical_receipt = logical_receipt;
    physical_receipt.path = RepoPath::new(format!(
        ".mine/runtime/release-evidence/{}/evidence/{logical}",
        operation_id.as_str()
    ))
    .unwrap();
    fields.verification_evidence.push(physical_receipt);
    write(root, stage_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"expected_stage_revision":0,"operation":LifecycleOperation::new(fields).unwrap(),"verification":gate
    })).unwrap());
    let admitted = NativeLifecycle::new(&graph).stage(3, &stage_path).unwrap();
    // Only this isolated synthetic Root is integrated; live repository is untouched.
    git(
        root,
        &[
            "update-ref",
            "refs/heads/master",
            candidate.as_str(),
            fixture.baseline.as_str(),
        ],
    );
    git(root, &["switch", "master"]);
    git(root, &["worktree", "remove", target.to_str().unwrap()]);
    let new_config = MineConfig::parse(
        &root.join(".mine/config.toml"),
        std::str::from_utf8(&bytes(root, ".mine/config.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(new_config.plan.root, ".mine/plan");
    assert!(!root.join(&fixture.config.graph.source).exists());
    let mut fields = admitted.operation.fields().clone();
    fields.stage_revision += 1;
    fields.stage = OperationStage::Release(ReleaseStage::Integrated);
    fields.integrated_commit = Some(candidate);
    write(root, stage_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"expected_stage_revision":1,"operation":LifecycleOperation::new(fields).unwrap(),"verification":gate
    })).unwrap());
    let integrated = NativeLifecycle::new(&graph).stage(0, &stage_path).unwrap();
    assert_eq!(integrated.operation.fields().stage_revision, 2);
    assert_eq!(
        serde_json::to_value(&integrated.checks).unwrap()[0]["admission"],
        "reused"
    );
    assert!(evidence_root.join(logical).is_file());
    let mut complete = integrated.operation.fields().clone();
    complete.stage_revision += 1;
    complete.stage = OperationStage::Release(ReleaseStage::Complete);
    complete.outcome = OperationOutcome::Committed;
    write(root, stage_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"expected_stage_revision":2,"operation":LifecycleOperation::new(complete).unwrap(),"verification":gate
    })).unwrap());
    let finalized = NativeLifecycle::new(&graph)
        .finalize(0, &stage_path)
        .unwrap();
    assert_eq!(finalized.operation.fields().stage_revision, 3);
    assert!(!finalized.replay);
    let replay = NativeLifecycle::new(&graph)
        .finalize(0, &stage_path)
        .unwrap();
    assert!(replay.replay);
    assert_eq!(replay.operation, finalized.operation);
}
