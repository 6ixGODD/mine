use super::*;
use crate::domain::lifecycle_operation::OperationId;
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

fn path(value: &str) -> RepoPath {
    RepoPath::new(value).unwrap()
}
fn git(root: &Path, index: Option<&Path>, args: &[&str], input: &[u8]) -> String {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(args)
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn write(root: &Path, relative: &str, body: &[u8]) {
    let target = root.join(relative);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, body).unwrap();
}
struct Fixture {
    directory: tempfile::TempDir,
    baseline: CommitId,
    source: CommitId,
    config: MineConfig,
}

// These migration scenarios deliberately start in the legacy namespace. Their
// source must not change when the product checkout itself is curated to stable.
fn legacy_fixture_config() -> MineConfig {
    use crate::domain::config::{AdrConfig, BranchesConfig, DesignConfig, GraphConfig, PlanConfig};
    MineConfig {
        schema_version: 1,
        repository_id: uuid::Uuid::new_v4().to_string(),
        mine_code_version: env!("CARGO_PKG_VERSION").into(),
        branches: BranchesConfig {
            stable: "master".into(),
            integration: "dev".into(),
        },
        design: DesignConfig {
            root: "docs/design/index.md".into(),
            marker: "docs/design/.mine-design.toml".into(),
            language: "en".into(),
            index_soft_limit_lines: 250,
            leaf_soft_limit_lines: 400,
        },
        plan: PlanConfig {
            root: "docs/plan".into(),
            ephemeral: true,
            purge_before_stable_release: true,
            worktrees_root: ".mine/worktrees".into(),
            shared_environment_root: ".".into(),
        },
        graph: GraphConfig {
            source: "docs/plan/execution-graph.toml".into(),
            rendered: "docs/plan/execution-graph.md".into(),
            lock_timeout_ms: 5000,
        },
        adr: AdrConfig {
            registry: "docs/design/decisions/adr-registry.toml".into(),
            rendered_index: "docs/design/decisions/index.md".into(),
            lock_timeout_ms: 5000,
        },
    }
}

impl Fixture {
    fn release_policy(&mut self) -> RepoPath {
        use crate::domain::verification_receipt::*;
        let policy = VerificationPolicy::new(PolicyFields {
            schema_version: 1,
            design_references: vec![path(&self.config.design.root)],
            checks: vec![CheckPolicy {
                name: "fixture-suite".into(),
                command: CheckCommand::new(CommandFields {
                    argv: vec![
                        "git".into(),
                        "rev-parse".into(),
                        "--verify".into(),
                        "HEAD^{tree}".into(),
                    ],
                    cwd: CommandDirectory::Root,
                })
                .unwrap(),
                inputs: InputScope::Full,
                tools: vec!["git".into()],
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
        let policy_path = path(".mine/verification-policy.json");
        write(
            self.root(),
            policy_path.as_str(),
            &serde_json::to_vec(&policy).unwrap(),
        );
        git(self.root(), None, &["add", policy_path.as_str()], b"");
        git(
            self.root(),
            None,
            &["commit", "-m", "source-owned candidate verification policy"],
            b"",
        );
        self.source = CommitId::new(git(self.root(), None, &["rev-parse", "HEAD"], b"")).unwrap();
        policy_path
    }
    fn accepted_source(&mut self) -> String {
        let id = format!("plan{:02}", 73);
        let body_path = format!("{}/{}-fixture.md", self.config.plan.root, id);
        let report_path = format!("{}/reports/{}-fixture-review.md", self.config.plan.root, id);
        let implementation_path = format!(
            "{}/reports/{}-fixture-implementation.md",
            self.config.plan.root, id
        );
        let mut workspace: PlanWorkspace = toml::from_str(
            &fs::read_to_string(self.root().join(&self.config.graph.source)).unwrap(),
        )
        .unwrap();
        let branch = crate::domain::plan_id::registered_plan_branch(
            &workspace.workspace_id,
            &id,
            &body_path,
        )
        .unwrap();
        let node = serde_json::from_value(serde_json::json!({
            "id": id, "path": body_path, "title": "Real historical fixture body",
            "status": "ACCEPTED", "branch": branch, "lineage_id": id,
            "worktree_path": format!("{}/{}", self.config.plan.worktrees_root, id),
            "branch_anchor": self.baseline.as_str(), "design_references": [self.config.design.root],
            "implementation_report": implementation_path, "review_report": report_path,
            "implementation_commits": [self.baseline.as_str()], "owner": "fixture-implementer", "run_id": "fixture-source-run"
        }))
        .unwrap();
        workspace.plans.push(node);
        write(self.root(), &body_path, b"# Genuine fixture Plan body\r\n");
        write(
            self.root(),
            &report_path,
            b"# Independent historical fixture review\r\n\r\nVerdict: ACCEPTED\r\n",
        );
        write(
            self.root(),
            &implementation_path,
            b"# Historical fixture implementation\r\n",
        );
        self.commit_workspace(&workspace);
        id
    }
    fn commit_workspace(&mut self, workspace: &PlanWorkspace) {
        write(
            self.root(),
            &self.config.graph.source,
            toml::to_string(workspace).unwrap().as_bytes(),
        );
        write(
            self.root(),
            &self.config.graph.rendered,
            render_markdown(workspace).unwrap().as_bytes(),
        );
        git(self.root(), None, &["add", &self.config.plan.root], b"");
        git(
            self.root(),
            None,
            &["commit", "-m", "source terminal evidence fixture"],
            b"",
        );
        self.source = CommitId::new(git(self.root(), None, &["rev-parse", "HEAD"], b"")).unwrap();
    }
    fn preflight_source(&mut self) {
        let registry = crate::domain::adr::AdrRegistry::empty();
        write(
            self.root(),
            &self.config.adr.registry,
            toml::to_string(&registry).unwrap().as_bytes(),
        );
        write(
            self.root(),
            &self.config.adr.rendered_index,
            crate::infrastructure::adr_store::AdrStore::new(self.root(), &self.config.adr)
                .render_index(&registry)
                .unwrap()
                .as_bytes(),
        );
        write(
            self.root(),
            ".gitignore",
            b".mine/locks/\n.mine/runtime/\n.mine/build/\n.mine/candidates/\n.mine/worktrees/\nignored-candidate-input\n",
        );
        git(
            self.root(),
            None,
            &[
                "add",
                self.config.layout().unwrap().design_dir().as_str(),
                ".gitignore",
            ],
            b"",
        );
        git(
            self.root(),
            None,
            &["commit", "-m", "valid preflight fixture governance"],
            b"",
        );
        self.source = CommitId::new(git(self.root(), None, &["rev-parse", "HEAD"], b"")).unwrap();
    }
    fn new() -> Self {
        Self::with_config(legacy_fixture_config())
    }
    fn with_config(config: MineConfig) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("curation space 中文-")
            .tempdir()
            .unwrap();
        let root = directory.path();
        git(root, None, &["init", "-b", "dev"], b"");
        for (key, value) in [
            ("user.name", "Curation Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "true"),
        ] {
            git(root, None, &["config", key, value], b"");
        }
        write(root, "README.md", b"unchanged product\n");
        write(root, "docs/user.md", b"unrelated user documentation\n");
        write(
            root,
            ".gitattributes",
            b"* -text -filter -ident -working-tree-encoding\n",
        );
        git(
            root,
            None,
            &["add", "README.md", "docs/user.md", ".gitattributes"],
            b"",
        );
        git(root, None, &["commit", "-m", "stable baseline"], b"");
        let baseline = CommitId::new(git(root, None, &["rev-parse", "HEAD"], b"")).unwrap();
        git(root, None, &["branch", "master"], b"");
        write(root, ".mine/config.toml", config.to_toml().as_bytes());
        write(root, &config.design.root, b"# Source Design\n");
        write(
            root,
            &config.design.marker,
            DesignMarker::new(config.repository_id.clone(), "2026-10-17T00:00:00Z".into())
                .to_toml()
                .as_bytes(),
        );
        let frozen = b"# Frozen ADR\r\n\r\nOriginal bytes remain approved.\r\n";
        let layout = config.layout().unwrap();
        write(
            root,
            &format!("{}/0001-fixture.md", layout.decisions_dir().as_str()),
            frozen,
        );
        write(
            root,
            ".mine/fixture-approval.json",
            &serde_json::to_vec(
                &serde_json::json!({"fixture_approved":true,"body_sha256":digest(frozen)}),
            )
            .unwrap(),
        );
        let workspace = PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 1,
            project_id: "fixture".into(),
            workspace_id: uuid::Uuid::new_v4().to_string(),
            stable_branch: config.branches.stable.clone(),
            integration_branch: config.branches.integration.clone(),
            stable_baseline_commit: baseline.as_str().into(),
            design_root: config.design.root.clone(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![],
        };
        write(
            root,
            &config.graph.source,
            toml::to_string(&workspace).unwrap().as_bytes(),
        );
        write(
            root,
            &config.graph.rendered,
            render_markdown(&workspace).unwrap().as_bytes(),
        );
        git(
            root,
            None,
            &[
                "add",
                ".mine/config.toml",
                ".mine/fixture-approval.json",
                layout.design_dir().as_str(),
                layout.plan_dir().as_str(),
            ],
            b"",
        );
        git(
            root,
            None,
            &["commit", "-m", "frozen terminal development source"],
            b"",
        );
        let source = CommitId::new(git(root, None, &["rev-parse", "HEAD"], b"")).unwrap();
        Self {
            directory,
            baseline,
            source,
            config,
        }
    }
    fn root(&self) -> &Path {
        self.directory.path()
    }
    fn candidate(&self, purge: bool, mutate: impl FnOnce(&Self, &Path)) -> CommitId {
        let index = self.root().join(".git/curation-private-index");
        git(
            self.root(),
            Some(&index),
            &["read-tree", self.source.as_str()],
            b"",
        );
        if purge {
            git(
                self.root(),
                Some(&index),
                &["rm", "--cached", "-r", "--", &self.config.plan.root],
                b"",
            );
        }
        mutate(self, &index);
        let tree = git(self.root(), Some(&index), &["write-tree"], b"");
        CommitId::new(git(
            self.root(),
            None,
            &["commit-tree", &tree, "-p", self.baseline.as_str()],
            b"curated candidate\n",
        ))
        .unwrap()
    }
    fn put(&self, index: &Path, relative: &str, mode: &str, body: &[u8]) {
        let object = git(self.root(), None, &["hash-object", "-w", "--stdin"], body);
        git(
            self.root(),
            Some(index),
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                mode,
                &object,
                relative,
            ],
            b"",
        );
    }
}

#[test]
fn canonical_fixture_stages_acceptance_and_purges_only_configured_plan_root() {
    let mut config = legacy_fixture_config();
    fixture_target_config(&mut config);
    let mut fixture = Fixture::with_config(config);
    assert!(!fixture.root().join("docs/design").exists());
    assert!(!fixture.root().join("docs/plan").exists());
    fixture.preflight_source();
    fixture.accepted_source();
    let candidate = fixture.candidate(true, |_, _| {});
    let proof = inspect_purge_candidate(fixture.root(), &fixture.source, &candidate).unwrap();
    assert_eq!(proof.delta.len(), 5);
    assert!(proof.delta.iter().all(|change| {
        change.path.as_str().starts_with(".mine/plan/") && change.after.is_none()
    }));
    assert!(git(fixture.root(), None, &["status", "--porcelain"], b"").is_empty());
}

#[test]
fn purge_proof_covers_full_tree_and_squash_without_source_ancestry() {
    let fixture = Fixture::new();
    let candidate = fixture.candidate(true, |_, _| {});
    let original = fs::read(fixture.root().join(&fixture.config.graph.source)).unwrap();
    let proof = inspect_purge_candidate(fixture.root(), &fixture.source, &candidate).unwrap();
    assert_eq!(proof.delta.len(), 2);
    assert!(
        proof
            .delta
            .iter()
            .all(|change| change.before.as_ref().unwrap().regular() && change.after.is_none())
    );
    assert_eq!(proof.source_authorizations.len(), 4);
    let mut request = checkpoint_git(fixture.root(), None).unwrap();
    assert!(
        !request
            .is_ancestor(fixture.source.as_str(), candidate.as_str())
            .unwrap()
    );
    assert!(
        request
            .is_ancestor(fixture.baseline.as_str(), candidate.as_str())
            .unwrap()
    );
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
        fixture.source.as_str()
    );
    assert_eq!(
        fs::read(fixture.root().join(&fixture.config.graph.source)).unwrap(),
        original
    );
    assert!(git(fixture.root(), None, &["status", "--porcelain"], b"").is_empty());
}

#[test]
fn purge_proof_refuses_product_deletion_mode_links_config_and_partial_purge() {
    let fixture = Fixture::new();
    for mode in 0..7 {
        let candidate = fixture.candidate(mode != 6, |fixture, index| match mode {
            0 => fixture.put(index, "README.md", "100644", b"smuggled product\n"),
            1 => {
                git(
                    fixture.root(),
                    Some(index),
                    &["update-index", "--force-remove", "--", "docs/user.md"],
                    b"",
                );
            }
            2 => {
                git(
                    fixture.root(),
                    Some(index),
                    &["update-index", "--chmod=+x", "--", "README.md"],
                    b"",
                );
            }
            3 => fixture.put(index, "new-link", "120000", b"../../escape"),
            4 => {
                let mut config = fixture.config.clone();
                config.mine_code_version = "99.0.0".into();
                fixture.put(
                    index,
                    ".mine/config.toml",
                    "100644",
                    config.to_toml().as_bytes(),
                );
            }
            5 => {
                git(
                    fixture.root(),
                    Some(index),
                    &[
                        "update-index",
                        "--add",
                        "--cacheinfo",
                        "160000",
                        fixture.baseline.as_str(),
                        "foreign-submodule",
                    ],
                    b"",
                );
            }
            _ => {}
        });
        assert!(
            inspect_purge_candidate(fixture.root(), &fixture.source, &candidate).is_err(),
            "mode={mode}"
        );
        assert_eq!(
            git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
            fixture.source.as_str()
        );
        assert!(git(fixture.root(), None, &["status", "--porcelain"], b"").is_empty());
    }
}

#[test]
fn declared_evidence_cannot_hide_missing_duplicate_or_forged_deltas() {
    let fixture = Fixture::new();
    let candidate = fixture.candidate(true, |_, _| {});
    let observed =
        CurationObservation::observe(fixture.root(), &fixture.source, &candidate).unwrap();
    let valid = OrdinaryPurge.validate(&observed).unwrap();
    for mode in 0..3 {
        let mut declared: CurationEvidence =
            serde_json::from_slice(&serde_json::to_vec(valid.evidence()).unwrap()).unwrap();
        match mode {
            0 => {
                declared.delta.pop();
            }
            1 => {
                declared.delta.push(declared.delta[0].clone());
            }
            _ => declared.delta[0].before.as_mut().unwrap().object = fixture.baseline.clone(),
        }
        assert!(
            observed
                .checked_expected(&declared.delta, declared.source_authorizations)
                .is_err()
        );
    }
}

/// Finite test-only trusted composition, not the production layout adapter.
/// Expected fields and byte-preserving moves derive from S after its fixture
/// approval/digest validates. No caller paths, plugins or C-created approval.
struct FixtureLayout;
impl CurationValidator for FixtureLayout {
    fn validate(&self, observed: &CurationObservation) -> OperationResult<CheckedCuration> {
        let authority = source_contract(observed)?;
        let approval_path = path(".mine/fixture-approval.json");
        let approval_bytes = observed
            .source_blobs(std::slice::from_ref(&approval_path))?
            .remove(0);
        let approval: serde_json::Value =
            serde_json::from_slice(&approval_bytes).map_err(|_| OperationError::Journal)?;
        let adr = path("docs/design/decisions/0001-fixture.md");
        let frozen = observed.source_blobs(std::slice::from_ref(&adr))?.remove(0);
        if approval["fixture_approved"] != true
            || approval["body_sha256"] != digest(&frozen).as_str()
        {
            return Err(OperationError::Refused(
                "source does not approve this fixture transformation",
            ));
        }
        let mut config = authority.config;
        let original = observed
            .source_blobs(&[path(".mine/config.toml")])?
            .remove(0);
        if original != config.to_toml().as_bytes() {
            return Err(OperationError::Refused(
                "fixture cannot rewrite unknown config formatting",
            ));
        }
        fixture_target_config(&mut config);
        let candidate_config = observed
            .candidate_blobs(&[path(".mine/config.toml")])?
            .remove(0);
        if candidate_config != config.to_toml().as_bytes() {
            return Err(OperationError::Refused(
                "candidate contains an unexplained config byte",
            ));
        }
        let mut expected = Vec::new();
        for (name, entry) in &observed.before.entries {
            if name.starts_with(b"docs/plan/") && !entry.directory() {
                expected.push(TreeDelta {
                    path: RepoPath::new(
                        std::str::from_utf8(name).map_err(|_| OperationError::Journal)?,
                    )?,
                    before: Some(entry.clone()),
                    after: None,
                });
            }
        }
        for (source, target) in fixture_moves() {
            let source = path(source);
            let target = path(target);
            let entry = observed
                .source_entry(&source)
                .ok_or(OperationError::Journal)?
                .clone();
            expected.push(TreeDelta {
                path: source,
                before: Some(entry.clone()),
                after: None,
            });
            expected.push(TreeDelta {
                path: target,
                before: None,
                after: Some(entry),
            });
        }
        let config_path = path(".mine/config.toml");
        let before = observed
            .source_entry(&config_path)
            .ok_or(OperationError::Journal)?
            .clone();
        let after = observed
            .candidate_entry(&config_path)
            .ok_or(OperationError::Journal)?
            .clone();
        if after.mode != before.mode || !after.regular() {
            return Err(OperationError::Refused("configuration mode/type changed"));
        }
        expected.push(TreeDelta {
            path: config_path,
            before: Some(before),
            after: Some(after),
        });
        expected.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
        let mut authorizations = authority.authorizations;
        authorizations.push(FileIdentity {
            path: approval_path,
            bytes: approval_bytes.len() as u64,
            sha256: digest(&approval_bytes),
        });
        authorizations.push(FileIdentity {
            path: adr,
            bytes: frozen.len() as u64,
            sha256: digest(&frozen),
        });
        observed.checked_expected(&expected, authorizations)
    }
}
fn fixture_target_config(config: &mut MineConfig) {
    config.design.root = ".mine/design/index.md".into();
    config.design.marker = ".mine/design/.mine-design.toml".into();
    config.adr.registry = ".mine/design/decisions/adr-registry.toml".into();
    config.adr.rendered_index = ".mine/design/decisions/index.md".into();
    config.plan.root = ".mine/plan".into();
    config.graph.source = ".mine/plan/execution-graph.toml".into();
    config.graph.rendered = ".mine/plan/execution-graph.md".into();
}
fn fixture_moves() -> [(&'static str, &'static str); 3] {
    [
        ("docs/design/index.md", ".mine/design/index.md"),
        (
            "docs/design/.mine-design.toml",
            ".mine/design/.mine-design.toml",
        ),
        (
            "docs/design/decisions/0001-fixture.md",
            ".mine/design/decisions/0001-fixture.md",
        ),
    ]
}
fn layout_candidate(fixture: &Fixture, tamper: bool) -> CommitId {
    fixture.candidate(true, |fixture, index| {
        for (source, target) in fixture_moves() {
            fixture.put(
                index,
                target,
                "100644",
                &fs::read(fixture.root().join(source)).unwrap(),
            );
            git(
                fixture.root(),
                Some(index),
                &["update-index", "--force-remove", "--", source],
                b"",
            );
        }
        let mut config = fixture.config.clone();
        fixture_target_config(&mut config);
        if tamper {
            config.mine_code_version = "not-an-approved-version-change".into();
        }
        fixture.put(
            index,
            ".mine/config.toml",
            "100644",
            config.to_toml().as_bytes(),
        );
    })
}

#[test]
fn trusted_fixture_preserves_frozen_crlf_but_production_rejects_unsupported_layout() {
    let fixture = Fixture::new();
    let candidate = layout_candidate(&fixture, false);
    let observed =
        CurationObservation::observe(fixture.root(), &fixture.source, &candidate).unwrap();
    let checked = FixtureLayout.validate(&observed).unwrap();
    let raw = observed
        .candidate_blobs(&[path(".mine/design/decisions/0001-fixture.md")])
        .unwrap()
        .remove(0);
    assert_eq!(
        raw,
        fs::read(fixture.root().join("docs/design/decisions/0001-fixture.md")).unwrap()
    );
    assert!(raw.windows(2).any(|pair| pair == b"\r\n"));
    assert_eq!(checked.evidence().source_commit, fixture.source);
    assert!(inspect_purge_candidate(fixture.root(), &fixture.source, &candidate).is_err());
    let bad = layout_candidate(&fixture, true);
    let observed = CurationObservation::observe(fixture.root(), &fixture.source, &bad).unwrap();
    assert!(FixtureLayout.validate(&observed).is_err());
}

#[test]
fn blob_protocol_refuses_truncation_identity_type_size_and_extra_responses() {
    let object = "a".repeat(40);
    let requests = vec![(object.clone(), 3)];
    let valid = format!("{object} blob 3\nabc\n").into_bytes();
    assert_eq!(
        decode_blob_batch(&valid, &requests).unwrap()[&object],
        b"abc"
    );
    for length in 0..valid.len() {
        assert!(decode_blob_batch(&valid[..length], &requests).is_err());
    }
    for invalid in [
        format!("{} blob 3\nabc\n", "b".repeat(40)),
        format!("{object} tree 3\nabc\n"),
        format!("{object} blob 2\nabc\n"),
        format!("{object} blob 3\nabc!"),
        format!("{object} blob 3\nabc\nextra"),
    ] {
        assert!(decode_blob_batch(invalid.as_bytes(), &requests).is_err());
    }
    let repeated = [valid.clone(), valid].concat();
    assert!(decode_blob_batch(&repeated, &[requests[0].clone(), requests[0].clone()]).is_err());
}

#[test]
fn layout_port_refuses_rewritten_frozen_body_and_candidate_self_approval() {
    let mut fixture = Fixture::new();
    let candidate = layout_candidate(&fixture, false);
    let altered = fixture.candidate(true, |fixture, index| {
        // Start from the already-produced layout tree, then alter frozen bytes.
        git(
            fixture.root(),
            Some(index),
            &["read-tree", candidate.as_str()],
            b"",
        );
        fixture.put(
            index,
            ".mine/design/decisions/0001-fixture.md",
            "100644",
            b"rewritten ADR\n",
        );
    });
    let observed = CurationObservation::observe(fixture.root(), &fixture.source, &altered).unwrap();
    assert!(FixtureLayout.validate(&observed).is_err());

    // Remove authority from S, preserving a previously valid approval only in C.
    git(
        fixture.root(),
        None,
        &["rm", "--", ".mine/fixture-approval.json"],
        b"",
    );
    git(
        fixture.root(),
        None,
        &["commit", "-m", "source has no layout approval"],
        b"",
    );
    fixture.source = CommitId::new(git(fixture.root(), None, &["rev-parse", "HEAD"], b"")).unwrap();
    let observed =
        CurationObservation::observe(fixture.root(), &fixture.source, &candidate).unwrap();
    assert!(FixtureLayout.validate(&observed).is_err());
}

#[test]
fn source_ownership_and_render_are_authority_not_candidate_declarations() {
    for mode in 0..3 {
        let mut fixture = Fixture::new();
        match mode {
            0 => {
                write(
                    fixture.root(),
                    &fixture.config.design.marker,
                    DesignMarker::new(
                        uuid::Uuid::new_v4().to_string(),
                        "2026-10-17T00:00:00Z".into(),
                    )
                    .to_toml()
                    .as_bytes(),
                );
            }
            1 => write(
                fixture.root(),
                &fixture.config.graph.rendered,
                b"forged generated view\n",
            ),
            _ => {
                fixture.config.plan.ephemeral = false;
                write(
                    fixture.root(),
                    ".mine/config.toml",
                    fixture.config.to_toml().as_bytes(),
                );
            }
        }
        git(
            fixture.root(),
            None,
            &["add", ".mine/config.toml", "docs/design", "docs/plan"],
            b"",
        );
        git(
            fixture.root(),
            None,
            &["commit", "-m", "invalid source authority fixture"],
            b"",
        );
        fixture.source =
            CommitId::new(git(fixture.root(), None, &["rev-parse", "HEAD"], b"")).unwrap();
        let candidate = fixture.candidate(true, |_, _| {});
        assert!(
            inspect_purge_candidate(fixture.root(), &fixture.source, &candidate).is_err(),
            "mode={mode}"
        );
    }
}

#[test]
fn preflight_compares_actual_stable_tip_with_declared_baseline() {
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let ready = super::super::preflight(fixture.root()).unwrap();
    assert!(ready.can_release, "{:?}", ready.errors);
    assert_eq!(ready.release_version, fixture.config.mine_code_version);
    assert!(ready.stable_unchanged);
    git(
        fixture.root(),
        None,
        &[
            "update-ref",
            "refs/heads/master",
            fixture.source.as_str(),
            fixture.baseline.as_str(),
        ],
        b"",
    );
    let changed = super::super::preflight(fixture.root()).unwrap();
    assert!(!changed.can_release);
    assert!(!changed.stable_unchanged);
    assert!(
        changed
            .errors
            .iter()
            .any(|error| error.contains("stored pre-integration baseline"))
    );
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
        fixture.source.as_str()
    );
}

#[test]
fn preflight_requires_real_design_ownership_and_adr_index_readiness() {
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    assert!(super::super::preflight(fixture.root()).unwrap().can_release);
    write(
        fixture.root(),
        &fixture.config.adr.rendered_index,
        b"stale generated ADR index\n",
    );
    let stale = super::super::preflight(fixture.root()).unwrap();
    assert!(!stale.can_release);
    assert!(!stale.design_valid);
    let registry = crate::domain::adr::AdrRegistry::empty();
    write(
        fixture.root(),
        &fixture.config.adr.rendered_index,
        crate::infrastructure::adr_store::AdrStore::new(fixture.root(), &fixture.config.adr)
            .render_index(&registry)
            .unwrap()
            .as_bytes(),
    );
    write(
        fixture.root(),
        &fixture.config.design.marker,
        DesignMarker::new(
            uuid::Uuid::new_v4().to_string(),
            "2026-10-17T00:00:00Z".into(),
        )
        .to_toml()
        .as_bytes(),
    );
    let foreign = super::super::preflight(fixture.root()).unwrap();
    assert!(!foreign.can_release);
    assert!(!foreign.design_valid);
}

#[test]
fn preflight_observes_pending_repository_operations_without_rewriting_graph() {
    use crate::domain::lifecycle_operation::*;
    use crate::infrastructure::operation_store::{JournalRecord, OperationStore};
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let source_path = fixture.root().join(&fixture.config.graph.source);
    let before = fs::read(&source_path).unwrap();
    let workspace: PlanWorkspace = toml::from_str(std::str::from_utf8(&before).unwrap()).unwrap();
    let operation = LifecycleOperation::new(OperationFields {
        schema_version: 1,
        operation_id: OperationId::generate(),
        stage_revision: 0,
        kind: OperationKind::Release,
        stage: OperationStage::Release(ReleaseStage::Intent),
        outcome: OperationOutcome::Pending,
        repository_id: fixture.config.repository_id.clone(),
        workspace_id: workspace.workspace_id.clone(),
        actor: "fixture-release-reviewer".into(),
        observed_graph_revision: workspace.revision,
        plan: None,
        registered_worktree: None,
        review: None,
        integrated_commit: None,
        verification_evidence: vec![],
        cleanup: None,
        checkpoint: None,
        diagnostics: vec![],
    })
    .unwrap();
    let candidate = fixture.candidate(true, |_, _| {});
    let store = crate::infrastructure::toml_store::TomlStore::from_configuration(
        fixture.root(),
        &fixture.config,
    )
    .unwrap();
    store
        .with_coordinator_lock(|guard| {
            OperationStore::under_lock(
                fixture.root(),
                &fixture.config.repository_id,
                &workspace.workspace_id,
                guard,
            )
            .unwrap()
            .begin(&JournalRecord {
                schema_version: 1,
                operation: operation.clone(),
                closure: Some(ClosureBinding::Release {
                    baseline: fixture.baseline.clone(),
                    candidate,
                }),
                checkpoint: None,
            })
            .unwrap();
            Ok(())
        })
        .unwrap();
    let pending = super::super::preflight(fixture.root()).unwrap();
    assert!(!pending.can_release);
    assert_eq!(pending.pending_operations.len(), 1);
    assert_eq!(pending.pending_operations[0].operation, operation);
    assert_eq!(fs::read(source_path).unwrap(), before);
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
        fixture.source.as_str()
    );
    assert!(git(fixture.root(), None, &["status", "--porcelain"], b"").is_empty());
}

#[test]
fn preflight_does_not_treat_hidden_config_drift_as_committed_governance() {
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    git(
        fixture.root(),
        None,
        &["update-index", "--skip-worktree", ".mine/config.toml"],
        b"",
    );
    let mut altered = fixture.config.clone();
    altered.mine_code_version = "99.0.0".into();
    write(
        fixture.root(),
        ".mine/config.toml",
        altered.to_toml().as_bytes(),
    );
    assert!(git(fixture.root(), None, &["status", "--porcelain"], b"").is_empty());
    let refused = super::super::preflight(fixture.root()).unwrap();
    assert!(refused.working_tree_clean);
    assert!(!refused.can_release);
    assert!(
        refused
            .errors
            .iter()
            .any(|error| error.contains("committed source governance proof failed"))
    );
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
        fixture.source.as_str()
    );
    assert_eq!(
        fs::read(fixture.root().join(".mine/config.toml")).unwrap(),
        altered.to_toml().as_bytes()
    );
}

#[test]
fn source_review_proof_retains_legacy_absence_and_rejects_unmerged_implementation() {
    let mut fixture = Fixture::new();
    let id = fixture.accepted_source();
    let candidate = fixture.candidate(true, |_, _| {});
    let proof = inspect_purge_candidate(fixture.root(), &fixture.source, &candidate).unwrap();
    assert_eq!(proof.source_authorizations.len(), 7);
    assert_eq!(proof.delta.len(), 5);
    let before = fs::read(fixture.root().join(&fixture.config.graph.source)).unwrap();
    let mut workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&before).unwrap()).unwrap();
    assert!(workspace.get(&id).unwrap().body_sha256.is_none());
    assert!(workspace.get(&id).unwrap().acceptance.is_none());
    workspace.get_mut(&id).unwrap().implementation_commits = vec![candidate.as_str().into()];
    fixture.commit_workspace(&workspace);
    let bad = fixture.candidate(true, |_, _| {});
    assert!(inspect_purge_candidate(fixture.root(), &fixture.source, &bad).is_err());
    let unchanged: PlanWorkspace = toml::from_str(
        &fs::read_to_string(fixture.root().join(&fixture.config.graph.source)).unwrap(),
    )
    .unwrap();
    assert!(unchanged.get(&id).unwrap().body_sha256.is_none());
    assert!(unchanged.get(&id).unwrap().acceptance.is_none());
}

#[test]
fn source_review_proof_refuses_missing_or_contradictory_accepted_evidence() {
    for mode in 0..3 {
        let mut fixture = Fixture::new();
        let id = fixture.accepted_source();
        let workspace: PlanWorkspace = toml::from_str(
            &fs::read_to_string(fixture.root().join(&fixture.config.graph.source)).unwrap(),
        )
        .unwrap();
        let node = workspace.get(&id).unwrap();
        match mode {
            0 => write(fixture.root(), &node.review_report, b"Verdict: REJECTED\n"),
            1 => {
                git(
                    fixture.root(),
                    None,
                    &["rm", "--", &node.review_report],
                    b"",
                );
            }
            _ => {
                git(
                    fixture.root(),
                    None,
                    &["rm", "--", &node.implementation_report],
                    b"",
                );
            }
        }
        git(fixture.root(), None, &["add", "docs/plan"], b"");
        git(
            fixture.root(),
            None,
            &["commit", "-m", "invalid acceptance evidence fixture"],
            b"",
        );
        fixture.source =
            CommitId::new(git(fixture.root(), None, &["rev-parse", "HEAD"], b"")).unwrap();
        let candidate = fixture.candidate(true, |_, _| {});
        assert!(
            inspect_purge_candidate(fixture.root(), &fixture.source, &candidate).is_err(),
            "mode={mode}"
        );
    }
}

fn release_request(
    fixture: &Fixture,
    candidate: &CommitId,
    attribution: &[String],
    policy_path: &RepoPath,
) -> RepoPath {
    let candidate_root = path(".mine/candidates/selected");
    let checkout = fixture.root().join(candidate_root.as_str());
    if !checkout.exists() {
        git(
            fixture.root(),
            None,
            &[
                "worktree",
                "add",
                "--detach",
                checkout.to_str().unwrap(),
                candidate.as_str(),
            ],
            b"",
        );
    }
    // The positive Full-key reuse fixture intentionally gives C and Root the
    // same observable directory modes; production must never project them away.
    #[cfg(unix)]
    fs::set_permissions(
        &checkout,
        fs::metadata(fixture.root()).unwrap().permissions(),
    )
    .unwrap();
    let input = path(".mine/runtime/release-request.json");
    write(fixture.root(), input.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version": 1, "actor": "fixture-independent-release-owner",
        "intent": { "kind": "release", "candidate": candidate, "candidate_root":candidate_root, "attribution": attribution, "policy_path": policy_path }
    })).unwrap());
    input
}

#[test]
fn release_intent_refusal_requires_no_effects_and_unblocks_without_sealing() {
    use crate::application::{graph_service::GraphService, native_lifecycle::NativeLifecycle};
    use crate::domain::lifecycle_operation::*;
    use crate::infrastructure::{operation_store::OperationStore, toml_store::TomlStore};
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    let input = release_request(&fixture, &candidate, &[id], &policy);
    let store = TomlStore::from_configuration(fixture.root(), &fixture.config).unwrap();
    let graph = GraphService::new(&store);
    let native = NativeLifecycle::new(&graph);
    let original_graph = fs::read(store.toml_path()).unwrap();
    let workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&original_graph).unwrap()).unwrap();
    let intent = native.begin(1, &input).unwrap();
    assert!(store.save_with_revision(1, "blocked-writer", Ok).is_err());
    let request_path = path(".mine/runtime/intent-refusal.json");
    let mut fields = intent.fields().clone();
    fields.stage_revision += 1;
    fields.outcome = OperationOutcome::Refused;
    fields
        .diagnostics
        .push("MINE_RELEASE_CANDIDATE_ABANDONED".into());
    let request =
        |fields: &OperationFields, expected_stage_revision: u64| {
            write(fixture.root(), request_path.as_str(), &serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "expected_stage_revision":expected_stage_revision,
            "operation":LifecycleOperation::new(fields.clone()).unwrap(), "verification":null
        })).unwrap());
        };
    request(&fields, 0);
    assert!(
        native.finalize(0, &request_path).is_err(),
        "stale graph revision"
    );
    request(&fields, 2);
    assert!(
        native.stage(1, &request_path).is_err(),
        "stale stage revision"
    );
    request(&fields, 0);
    let readme = fixture.root().join("README.md");
    let original_readme = fs::read(&readme).unwrap();
    fs::write(&readme, b"uncommitted source drift").unwrap();
    assert!(native.finalize(1, &request_path).is_err());
    git(fixture.root(), None, &["add", "README.md"], b"");
    assert!(
        native.finalize(1, &request_path).is_err(),
        "staged source drift"
    );
    fs::write(&readme, &original_readme).unwrap();
    git(fixture.root(), None, &["add", "README.md"], b"");
    git(
        fixture.root(),
        None,
        &["update-index", "--skip-worktree", "README.md"],
        b"",
    );
    assert!(
        native.finalize(1, &request_path).is_err(),
        "hidden index flags"
    );
    git(
        fixture.root(),
        None,
        &["update-index", "--no-skip-worktree", "README.md"],
        b"",
    );
    let stable = format!("refs/heads/{}", fixture.config.branches.stable);
    git(
        fixture.root(),
        None,
        &[
            "update-ref",
            &stable,
            fixture.source.as_str(),
            fixture.baseline.as_str(),
        ],
        b"",
    );
    assert!(
        native.finalize(1, &request_path).is_err(),
        "stable already advanced"
    );
    git(
        fixture.root(),
        None,
        &[
            "update-ref",
            &stable,
            fixture.baseline.as_str(),
            fixture.source.as_str(),
        ],
        b"",
    );
    let config_path = fixture.root().join(".mine/config.toml");
    let original_config = fs::read(&config_path).unwrap();
    fs::write(
        &config_path,
        [original_config.as_slice(), b"\n# foreign drift\n"].concat(),
    )
    .unwrap();
    assert!(native.finalize(1, &request_path).is_err());
    fs::write(&config_path, &original_config).unwrap();
    let foreign_lock = fixture.root().join(".git/index.lock");
    fs::write(&foreign_lock, b"foreign-lock").unwrap();
    assert!(native.finalize(1, &request_path).is_err());
    assert_eq!(fs::read(&foreign_lock).unwrap(), b"foreign-lock");
    fs::remove_file(&foreign_lock).unwrap();
    let plan_path = fixture.root().join(&workspace.plans[0].path);
    let original_plan = fs::read(&plan_path).unwrap();
    fs::remove_file(&plan_path).unwrap();
    assert!(
        native.finalize(1, &request_path).is_err(),
        "purge has already started"
    );
    fs::write(&plan_path, original_plan).unwrap();
    let mut no_reason = fields.clone();
    no_reason.diagnostics.clear();
    request(&no_reason, 0);
    assert!(native.finalize(1, &request_path).is_err());
    request(&fields, 0);
    // Private fixture discards only its own prepared candidate. The production
    // cancellation must neither require it nor perform cleanup itself.
    let checkout = fixture.root().join(".mine/candidates/selected");
    git(
        fixture.root(),
        None,
        &["worktree", "remove", checkout.to_str().unwrap()],
        b"",
    );
    let refused = native.finalize(1, &request_path).unwrap();
    assert!(!refused.replay);
    assert!(refused.workspace.is_none());
    assert_eq!(refused.revision_before, 1);
    assert_eq!(refused.revision_after, 1);
    assert_eq!(
        refused.operation.fields().outcome,
        OperationOutcome::Refused
    );
    let replay = native.finalize(1, &request_path).unwrap();
    assert!(replay.replay);
    assert_eq!(replay.operation, refused.operation);
    assert_eq!(fs::read(store.toml_path()).unwrap(), original_graph);
    store
        .with_coordinator_lock(|guard| {
            let operations = OperationStore::under_lock(
                fixture.root(),
                &fixture.config.repository_id,
                &workspace.workspace_id,
                guard,
            )
            .unwrap();
            assert_eq!(operations.observations().unwrap().len(), 1);
            let journal = crate::infrastructure::operation_store::RepositoryJournal::under_lock(
                fixture.root(),
                &fixture.config.repository_id,
                guard,
            )
            .unwrap();
            let observed = journal.observations().unwrap();
            assert!(
                crate::infrastructure::operation_store::history::predecessors(
                    &journal, &observed, None,
                )
                .is_err(),
                "Refused is not a completed workspace seal"
            );
            Ok(())
        })
        .unwrap();
    store
        .save_with_revision(1, "unblocked-writer", |mut workspace| {
            workspace.revision += 1;
            Ok(workspace)
        })
        .unwrap();
    assert_eq!(store.observed_revision().unwrap(), 2);
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", &stable], b""),
        fixture.baseline.as_str()
    );
    assert!(
        fixture
            .root()
            .join(intent.fields().verification_evidence[0].path.as_str())
            .is_file()
    );
    assert!(!checkout.exists());
}

#[test]
fn release_unknown_cannot_be_refused_and_verification_scans_complete_catalog() {
    use crate::application::{graph_service::GraphService, native_lifecycle::NativeLifecycle};
    use crate::domain::lifecycle_operation::*;
    use crate::infrastructure::{
        operation_store::{OperationLocation, OperationStore},
        toml_store::TomlStore,
    };
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    let input = release_request(&fixture, &candidate, std::slice::from_ref(&id), &policy);
    let store = TomlStore::from_configuration(fixture.root(), &fixture.config).unwrap();
    let graph = GraphService::new(&store);
    let native = NativeLifecycle::new(&graph);
    let intent = native.begin(1, &input).unwrap();
    let operation_id = &intent.fields().operation_id;
    super::super::retained::ReleaseVerification::open(fixture.root(), operation_id, &id).unwrap();
    let foreign_path = OperationLocation::new(fixture.root(), &OperationId::generate())
        .unwrap()
        .resolve()
        .unwrap();
    fs::write(&foreign_path, b"{}").unwrap();
    assert!(
        super::super::retained::ReleaseVerification::open(fixture.root(), operation_id, &id)
            .is_err(),
        "unlisted corrupt record must not be missed by querying only this operation"
    );
    fs::remove_file(&foreign_path).unwrap();
    super::super::retained::ReleaseVerification::open(fixture.root(), operation_id, &id).unwrap();
    let original_graph = fs::read(store.toml_path()).unwrap();
    let unknown = store
        .with_coordinator_lock(|guard| {
            let operations = OperationStore::under_lock(
                fixture.root(),
                &fixture.config.repository_id,
                &intent.fields().workspace_id,
                guard,
            )
            .unwrap();
            let mut record = operations.query(operation_id).unwrap();
            let mut fields = record.operation.fields().clone();
            fields.stage_revision += 1;
            fields.outcome = OperationOutcome::Unknown;
            fields
                .diagnostics
                .push("MINE_RELEASE_UNCERTAIN_EFFECT".into());
            record.operation = LifecycleOperation::new(fields).unwrap();
            Ok(operations.advance(0, &record).unwrap().operation)
        })
        .unwrap();
    let mut refused = unknown.fields().clone();
    refused.stage_revision += 1;
    refused.outcome = OperationOutcome::Refused;
    refused.diagnostics.push("MINE_RELEASE_ABANDONED".into());
    let request_path = path(".mine/runtime/unknown-refusal.json");
    write(fixture.root(), request_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"expected_stage_revision":1,"operation":LifecycleOperation::new(refused).unwrap(),"verification":null
    })).unwrap());
    assert!(native.stage(1, &request_path).is_err());
    assert!(native.finalize(1, &request_path).is_err());
    assert!(
        store
            .save_with_revision(1, "blocked-by-unknown", Ok)
            .is_err()
    );
    assert_eq!(fs::read(store.toml_path()).unwrap(), original_graph);
    store
        .with_coordinator_lock(|guard| {
            let operations = OperationStore::under_lock(
                fixture.root(),
                &fixture.config.repository_id,
                &intent.fields().workspace_id,
                guard,
            )
            .unwrap();
            assert_eq!(operations.query(operation_id).unwrap().operation, unknown);
            Ok(())
        })
        .unwrap();
}

#[test]
fn release_begin_races_real_graph_write_under_one_root_lock() {
    use crate::application::{graph_service::GraphService, native_lifecycle::NativeLifecycle};
    use crate::infrastructure::{operation_store::RepositoryJournal, toml_store::TomlStore};
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    const MODE: &str = "MINE_RELEASE_RACE_MODE";
    if let Ok(mode) = std::env::var(MODE) {
        let root = std::path::PathBuf::from(std::env::var_os("MINE_RELEASE_RACE_ROOT").unwrap())
            .canonicalize()
            .unwrap();
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("curation space 中文-")
        );
        let runtime = root.join(".mine/runtime/release-race");
        assert_eq!(
            fs::read_to_string(runtime.join("owner")).unwrap(),
            std::env::var("MINE_RELEASE_RACE_NONCE").unwrap()
        );
        let config_path = root.join(".mine/config.toml");
        let config =
            MineConfig::parse(&config_path, &fs::read_to_string(&config_path).unwrap()).unwrap();
        let store = TomlStore::from_configuration(&root, &config).unwrap();
        fs::write(runtime.join(format!("ready-{mode}")), b"ready").unwrap();
        let other = if mode == "begin" { "write" } else { "begin" };
        let deadline = Instant::now() + Duration::from_secs(15);
        while !runtime.join(format!("ready-{other}")).exists() {
            assert!(
                Instant::now() < deadline,
                "other real client did not reach the start gate"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let result = match mode.as_str() {
            "begin" => NativeLifecycle::new(&GraphService::new(&store))
                .begin(1, &path(".mine/runtime/release-request.json"))
                .map(|_| ()),
            "write" => store
                .save_with_revision(1, "fixture-racing-graph-writer", |mut workspace| {
                    workspace.revision += 1;
                    Ok(workspace)
                })
                .map(|_| ()),
            _ => panic!("invalid fixture client mode"),
        };
        fs::write(
            runtime.join(format!("result-{mode}.json")),
            serde_json::to_vec(&serde_json::json!({
                "ok":result.is_ok(), "error":result.err().map(|error|error.to_string())
            }))
            .unwrap(),
        )
        .unwrap();
        return;
    }
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    release_request(&fixture, &candidate, &[id], &policy);
    let runtime = fixture.root().join(".mine/runtime/release-race");
    fs::create_dir_all(&runtime).unwrap();
    let nonce = uuid::Uuid::new_v4().to_string();
    fs::write(runtime.join("owner"), &nonce).unwrap();
    let mut children = Vec::new();
    for mode in ["begin", "write"] {
        children.push(Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "application::release_service::curation::tests::release_begin_races_real_graph_write_under_one_root_lock", "--nocapture"])
            .env(MODE, mode).env("MINE_RELEASE_RACE_ROOT", fixture.root()).env("MINE_RELEASE_RACE_NONCE", &nonce)
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap());
    }
    let outputs = children
        .into_iter()
        .map(|child| child.wait_with_output().unwrap())
        .collect::<Vec<_>>();
    for output in outputs {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let results = ["begin", "write"].map(|mode| {
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(runtime.join(format!("result-{mode}.json"))).unwrap(),
        )
        .unwrap()
    });
    assert_eq!(
        results.iter().filter(|result| result["ok"] == true).count(),
        1,
        "{results:?}"
    );
    let store = TomlStore::from_configuration(fixture.root(), &fixture.config).unwrap();
    store
        .with_coordinator_lock(|guard| {
            let journal =
                RepositoryJournal::under_lock(fixture.root(), &fixture.config.repository_id, guard)
                    .unwrap();
            let records = journal.observations().unwrap();
            assert_eq!(records.len(), 1);
            if results[0]["ok"] == true {
                assert_eq!(store.observed_revision().unwrap(), 1);
                assert_eq!(
                    records[0].operation.fields().kind,
                    crate::domain::lifecycle_operation::OperationKind::Release
                );
                assert_eq!(
                    git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
                    fixture.source.as_str()
                );
            } else {
                assert_eq!(store.observed_revision().unwrap(), 2);
                assert_eq!(
                    records[0].operation.fields().kind,
                    crate::domain::lifecycle_operation::OperationKind::GraphCheckpoint
                );
                assert!(
                    !fixture
                        .root()
                        .join(".mine/runtime/release-evidence")
                        .exists()
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn release_intent_freezes_real_source_and_refuses_manifest_or_candidate_replacement() {
    use crate::application::{graph_service::GraphService, native_lifecycle::NativeLifecycle};
    use crate::domain::lifecycle_operation::*;
    use crate::infrastructure::operation_store::{OperationStore, read_bounded, regular_path};
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    let input = release_request(&fixture, &candidate, std::slice::from_ref(&id), &policy);
    let store = crate::infrastructure::toml_store::TomlStore::from_configuration(
        fixture.root(),
        &fixture.config,
    )
    .unwrap();
    let graph = GraphService::new(&store);
    let before = fs::read(store.toml_path()).unwrap();
    let workspace: PlanWorkspace = toml::from_str(std::str::from_utf8(&before).unwrap()).unwrap();
    let operation = NativeLifecycle::new(&graph)
        .begin(workspace.revision, &input)
        .unwrap();
    assert_eq!(operation.fields().kind, OperationKind::Release);
    assert_eq!(
        operation.fields().stage,
        initial_stage(OperationKind::Release)
    );
    assert_eq!(operation.fields().outcome, OperationOutcome::Pending);
    assert!(operation.fields().plan.is_none());
    assert_eq!(operation.fields().verification_evidence.len(), 1);
    let record = store
        .with_coordinator_lock(|guard| {
            Ok(OperationStore::under_lock(
                fixture.root(),
                &fixture.config.repository_id,
                &workspace.workspace_id,
                guard,
            )
            .unwrap()
            .query(&operation.fields().operation_id)
            .unwrap())
        })
        .unwrap();
    let binding = record.closure.as_ref().unwrap();
    super::super::retained::observe_intent(fixture.root(), &operation, binding).unwrap();
    assert!(
        NativeLifecycle::new(&graph)
            .begin(workspace.revision, &input)
            .is_err()
    );
    let manifest_file = &operation.fields().verification_evidence[0];
    let manifest_path = regular_path(fixture.root(), &manifest_file.path).unwrap();
    let original = read_bounded(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(manifest["source_commit"], fixture.source.as_str());
    assert_eq!(manifest["selected_plans"][0]["plan_id"], id);
    let body_path = &workspace.get(&id).unwrap().path;
    let copied = fixture.root().join(format!(
        ".mine/runtime/release-evidence/{}/source/{body_path}",
        operation.fields().operation_id.as_str()
    ));
    assert_eq!(
        fs::read(copied).unwrap(),
        fs::read(fixture.root().join(body_path)).unwrap()
    );
    assert!(workspace.get(&id).unwrap().body_sha256.is_none());
    assert!(workspace.get(&id).unwrap().acceptance.is_none());
    fs::write(&manifest_path, b"{}").unwrap();
    assert!(super::super::retained::observe_intent(fixture.root(), &operation, binding).is_err());
    fs::write(&manifest_path, &original).unwrap();
    let different = ClosureBinding::Release {
        baseline: fixture.baseline.clone(),
        candidate: fixture.source.clone(),
    };
    assert!(
        super::super::retained::observe_intent(fixture.root(), &operation, &different).is_err()
    );
    super::super::retained::observe_intent(fixture.root(), &operation, binding).unwrap();
    assert_eq!(fs::read(store.toml_path()).unwrap(), before);
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
        fixture.source.as_str()
    );
    assert!(git(fixture.root(), None, &["status", "--porcelain"], b"").is_empty());
}

#[test]
fn release_intent_refuses_guessed_duplicate_or_missing_plan_attribution_before_export() {
    use crate::application::{graph_service::GraphService, native_lifecycle::NativeLifecycle};
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    let store = crate::infrastructure::toml_store::TomlStore::from_configuration(
        fixture.root(),
        &fixture.config,
    )
    .unwrap();
    let graph = GraphService::new(&store);
    let before = fs::read(store.toml_path()).unwrap();
    let workspace: PlanWorkspace = toml::from_str(std::str::from_utf8(&before).unwrap()).unwrap();
    for attribution in [
        vec![],
        vec![id.clone(), id.clone()],
        vec![format!("plan{:02}", 74)],
    ] {
        let input = release_request(&fixture, &candidate, &attribution, &policy);
        assert!(
            NativeLifecycle::new(&graph)
                .begin(workspace.revision, &input)
                .is_err()
        );
        assert!(
            !fixture
                .root()
                .join(".mine/runtime/release-evidence")
                .exists()
        );
        assert_eq!(fs::read(store.toml_path()).unwrap(), before);
    }
}

#[test]
fn release_candidate_verification_uses_retained_body_but_never_archived_command_inputs() {
    use crate::application::{
        graph_service::GraphService, native_lifecycle::NativeLifecycle,
        native_verification::NativeVerification, verification_service::VerificationBackend,
    };
    use crate::domain::{graph::UtcTimestamp, ports::Clock, verification_receipt::*};
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    let input = release_request(&fixture, &candidate, std::slice::from_ref(&id), &policy);
    let store = crate::infrastructure::toml_store::TomlStore::from_configuration(
        fixture.root(),
        &fixture.config,
    )
    .unwrap();
    let graph = GraphService::new(&store);
    let operation = NativeLifecycle::new(&graph).begin(1, &input).unwrap();
    let operation_id = &operation.fields().operation_id;
    let candidate_root = fixture.root().join(".mine/candidates/selected");
    assert_release_diagnostic(fixture.root(), &candidate_root, operation_id, true);
    assert_release_diagnostic(
        fixture.root(),
        fixture.root(),
        &OperationId::generate(),
        false,
    );
    let other_root = fixture.root().join(".mine/runtime/other-candidate");
    git(
        fixture.root(),
        None,
        &[
            "worktree",
            "add",
            "--detach",
            other_root.to_str().unwrap(),
            candidate.as_str(),
        ],
        b"",
    );
    write(
        &other_root,
        "belongs-only-to-other-checkout",
        b"not a selected command input",
    );
    // Only the isolated fixture's live source Plan tree is moved. The native
    // verification request must use its frozen authority, not a graph lookup.
    fs::rename(
        fixture.root().join("docs/plan"),
        fixture.root().join(".mine/runtime/sidelined-plan-source"),
    )
    .unwrap();
    let graph = GraphService::new(&store);
    let (plan, backend) =
        super::super::retained::ReleaseVerification::open(fixture.root(), operation_id, &id)
            .unwrap();
    assert!(
        !candidate_root
            .join(plan.fields().plan_path.as_str())
            .exists()
    );
    assert!(
        !fixture
            .root()
            .join(plan.fields().plan_path.as_str())
            .exists()
    );
    assert!(backend.file(&plan.fields().plan_path).is_err());
    assert_eq!(
        backend.plan_body(&plan).unwrap().sha256,
        *plan.fields().body_sha256.as_ref().unwrap()
    );
    let source_before = fs::read(fixture.root().join(format!(
        ".mine/runtime/release-evidence/{}/source/{}",
        operation_id.as_str(),
        plan.fields().plan_path.as_str()
    )))
    .unwrap();
    let context = VerificationContext {
        toolchains: BTreeMap::from([(
            "git".into(),
            digest(git(fixture.root(), None, &["--version"], b"").as_bytes()),
        )]),
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        features: vec![],
        environment: vec![],
    };
    let started =
        UtcTimestamp::new(crate::infrastructure::system::SystemClock.now_utc_rfc3339()).unwrap();
    let capture_path = path(&format!(".mine/build/{id}/requests/capture.json"));
    write(fixture.root(), capture_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1, "plan_id":id, "actor":"fixture-release-reviewer", "check":"fixture-suite",
        "policy_path":policy, "context":context, "started_at":started, "release_operation_id":operation_id
    })).unwrap());
    let verification = NativeVerification::new(&graph, fixture.root()).unwrap();
    let capture = verification.capture(&capture_path).unwrap();
    assert_eq!(capture.plan, plan);
    assert!(
        !capture
            .manifest
            .entries
            .iter()
            .any(|entry| entry.file.path.as_str() == "belongs-only-to-other-checkout")
    );
    assert!(
        !capture.manifest.entries.iter().any(|entry| entry
            .file
            .path
            .as_str()
            .starts_with("docs/plan/"))
    );
    let actual_tree = git(
        &candidate_root,
        None,
        &["rev-parse", "--verify", "HEAD^{tree}"],
        b"",
    );
    assert_eq!(
        actual_tree,
        git(
            fixture.root(),
            None,
            &["rev-parse", &format!("{}^{{tree}}", candidate.as_str())],
            b""
        )
    );
    let artifact = path(&format!(
        "docs/plan/reports/verification/{id}/actual-candidate-command.log"
    ));
    let evidence_root = fixture.root().join(format!(
        ".mine/runtime/release-evidence/{}/evidence",
        operation_id.as_str()
    ));
    write(&evidence_root, artifact.as_str(), actual_tree.as_bytes());
    let record_path = path(&format!(".mine/build/{id}/requests/record.json"));
    write(fixture.root(), record_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1, "capture":capture, "policy_path":policy, "release_operation_id":operation_id,
        "observation": { "ended_at":UtcTimestamp::new(crate::infrastructure::system::SystemClock.now_utc_rfc3339()).unwrap(),
            "exit_code":0, "outcome":"passed", "warnings_observed":false,
            "artifacts":[FileIdentity { path:artifact.clone(), bytes:actual_tree.len() as u64, sha256:digest(actual_tree.as_bytes()) }] }
    })).unwrap());
    let recorded = verification.record(&record_path).unwrap();
    assert_eq!(recorded.disposition, VerificationDisposition::Run);
    let receipt = recorded.original.unwrap();
    let receipt_path = path(&format!(
        "docs/plan/reports/verification/{id}/{}.json",
        receipt.fields().receipt_id.as_str()
    ));
    assert!(evidence_root.join(receipt_path.as_str()).is_file());
    assert!(!candidate_root.join(receipt_path.as_str()).exists());
    assert!(!fixture.root().join(receipt_path.as_str()).exists());
    let check_path = path(&format!(".mine/build/{id}/requests/check.json"));
    write(fixture.root(), check_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1, "plan_id":id, "actor":"fixture-release-reviewer", "check":"fixture-suite",
        "context":context, "independent_of":"fixture-implementer", "release_operation_id":operation_id
    })).unwrap());
    let reused = verification
        .check(&receipt_path, &check_path, Some(&policy))
        .unwrap();
    assert_eq!(reused.disposition, VerificationDisposition::Reused);
    assert_eq!(reused.original.as_ref(), Some(&receipt));
    // The public lifecycle gate must consume the actual candidate receipt,
    // rather than treating a valid frozen manifest as verification success.
    let stage_path = path(&format!(".mine/build/{id}/requests/stage.json"));
    let receipt_bytes = fs::read(evidence_root.join(receipt_path.as_str())).unwrap();
    let original_json = std::str::from_utf8(&receipt_bytes).unwrap();
    let hash_json = serde_json::to_string(&context.toolchains["git"]).unwrap();
    let tools = format!("\"toolchains\":{{\"git\":{hash_json}}}");
    assert_eq!(original_json.matches(&tools).count(), 1);
    let duplicate = original_json.replace(
        &tools,
        &format!("\"toolchains\":{{\"git\":{hash_json},\"git\":{hash_json}}}"),
    );
    write(&evidence_root, receipt_path.as_str(), duplicate.as_bytes());
    let refused_duplicate = verification
        .check(&receipt_path, &check_path, Some(&policy))
        .unwrap();
    assert_eq!(
        refused_duplicate.disposition,
        VerificationDisposition::Invalidated
    );
    assert_eq!(
        refused_duplicate.reason,
        VerificationReason::EvidenceUnavailable
    );
    write(&evidence_root, receipt_path.as_str(), &receipt_bytes);
    let logical_receipt = FileIdentity {
        path: receipt_path.clone(),
        bytes: receipt_bytes.len() as u64,
        sha256: digest(&receipt_bytes),
    };
    let mut next = operation.fields().clone();
    next.stage = crate::domain::lifecycle_operation::OperationStage::Release(
        crate::domain::lifecycle_operation::ReleaseStage::Preflight,
    );
    next.stage_revision += 1;
    let gate = serde_json::json!({ "policy_path":policy, "checks":[{
        "check":"fixture-suite", "receipt":logical_receipt, "context":context,
        "fresh_observation":null
    }] });
    let stage_request = |next: &crate::domain::lifecycle_operation::OperationFields,
                         gate: serde_json::Value| {
        let operation =
            crate::domain::lifecycle_operation::LifecycleOperation::new(next.clone()).unwrap();
        write(
            fixture.root(),
            stage_path.as_str(),
            &serde_json::to_vec(&serde_json::json!({
                "schema_version":1, "expected_stage_revision":0, "operation":operation,
                "verification":gate
            }))
            .unwrap(),
        );
    };
    stage_request(&next, serde_json::Value::Null);
    assert!(NativeLifecycle::new(&graph).stage(0, &stage_path).is_err());
    stage_request(&next, gate.clone());
    assert!(
        NativeLifecycle::new(&graph).stage(0, &stage_path).is_err(),
        "unretained receipt cannot satisfy Release"
    );
    let mut physical_receipt = logical_receipt.clone();
    physical_receipt.path = path(&format!(
        ".mine/runtime/release-evidence/{}/evidence/{}",
        operation_id.as_str(),
        receipt_path.as_str()
    ));
    next.verification_evidence.push(physical_receipt);
    stage_request(&next, gate.clone());
    assert!(
        NativeLifecycle::new(&graph).stage(1, &stage_path).is_err(),
        "stale graph observer cannot be refreshed"
    );
    let root_config_path = fixture.root().join(".mine/config.toml");
    let root_config_bytes = fs::read(&root_config_path).unwrap();
    let mut changed_root_config = fixture.config.clone();
    changed_root_config.mine_code_version = "0.1.99".into();
    fs::write(&root_config_path, changed_root_config.to_toml()).unwrap();
    assert!(
        NativeLifecycle::new(&graph).stage(0, &stage_path).is_err(),
        "Preflight cannot ignore drift of Root's frozen source configuration"
    );
    fs::write(&root_config_path, &root_config_bytes).unwrap();
    let root_product = fixture.root().join("README.md");
    let root_product_bytes = fs::read(&root_product).unwrap();
    fs::write(&root_product, b"foreign uncommitted Root product change").unwrap();
    assert!(
        NativeLifecycle::new(&graph).stage(0, &stage_path).is_err(),
        "Preflight cannot hide uncommitted Root product changes behind clean C"
    );
    fs::write(&root_product, &root_product_bytes).unwrap();
    git(
        fixture.root(),
        None,
        &["update-index", "--skip-worktree", "README.md"],
        b"",
    );
    assert!(NativeLifecycle::new(&graph).stage(0, &stage_path).is_err());
    git(
        fixture.root(),
        None,
        &["update-index", "--no-skip-worktree", "README.md"],
        b"",
    );
    let root_lock = git(
        fixture.root(),
        None,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "index.lock",
        ],
        b"",
    );
    fs::write(&root_lock, b"foreign Root lock").unwrap();
    assert!(NativeLifecycle::new(&graph).stage(0, &stage_path).is_err());
    assert_eq!(fs::read(&root_lock).unwrap(), b"foreign Root lock");
    fs::remove_file(&root_lock).unwrap();
    let attributes_path = fixture.root().join(".gitattributes");
    let original_attributes = fs::read(&attributes_path).unwrap();
    git(
        fixture.root(),
        None,
        &[
            "config",
            "filter.fixture-unsafe.clean",
            "git config --local fixture.unexpected-filter-executed true",
        ],
        b"",
    );
    fs::write(&attributes_path, b"* filter=fixture-unsafe\n").unwrap();
    fs::write(
        &root_product,
        b"dirty product with configured clean callback",
    )
    .unwrap();
    assert!(NativeLifecycle::new(&graph).stage(0, &stage_path).is_err());
    let mut filter_probe = checkpoint_git(fixture.root(), None).unwrap();
    assert_eq!(
        filter_probe
            .run_raw(
                &[
                    "config",
                    "--local",
                    "--get",
                    "fixture.unexpected-filter-executed"
                ],
                b""
            )
            .unwrap()
            .status
            .code(),
        Some(1),
        "Root refusal must precede status/any external clean-filter callback"
    );
    fs::write(&attributes_path, &original_attributes).unwrap();
    fs::write(&root_product, &root_product_bytes).unwrap();
    git(
        fixture.root(),
        None,
        &["config", "--unset", "filter.fixture-unsafe.clean"],
        b"",
    );
    let admitted = NativeLifecycle::new(&graph).stage(0, &stage_path).unwrap();
    assert_release_diagnostic(fixture.root(), fixture.root(), operation_id, true);
    assert_eq!(admitted.operation.fields().stage_revision, 1);
    assert_eq!(
        serde_json::to_value(&admitted.checks).unwrap()[0]["admission"],
        "reused"
    );
    let replay = NativeLifecycle::new(&graph).stage(0, &stage_path).unwrap();
    assert_eq!(replay.operation, admitted.operation);
    let mut invalid_refusal = admitted.operation.fields().clone();
    invalid_refusal.stage = crate::domain::lifecycle_operation::OperationStage::Release(
        crate::domain::lifecycle_operation::ReleaseStage::Intent,
    );
    invalid_refusal.stage_revision += 1;
    invalid_refusal.outcome = crate::domain::lifecycle_operation::OperationOutcome::Refused;
    invalid_refusal
        .diagnostics
        .push("MINE_RELEASE_ABANDONED".into());
    write(fixture.root(), stage_path.as_str(), &serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"expected_stage_revision":1,
        "operation":crate::domain::lifecycle_operation::LifecycleOperation::new(invalid_refusal).unwrap(),"verification":null
    })).unwrap());
    assert!(
        NativeLifecycle::new(&graph)
            .finalize(0, &stage_path)
            .is_err(),
        "Preflight cannot cancel backwards to Intent"
    );
    stage_request(&next, gate.clone());
    write(
        &candidate_root,
        "ignored-candidate-input",
        b"relevant ignored input changed",
    );
    assert!(
        NativeLifecycle::new(&graph).stage(0, &stage_path).is_err(),
        "replay must reobserve current candidate inputs"
    );
    let changed_inputs = verification
        .check(&receipt_path, &check_path, Some(&policy))
        .unwrap();
    assert_eq!(
        changed_inputs.disposition,
        VerificationDisposition::Invalidated
    );
    assert_eq!(changed_inputs.reason, VerificationReason::InputsChanged);
    fs::remove_file(candidate_root.join("ignored-candidate-input")).unwrap();
    let mut incomplete = gate.clone();
    incomplete["checks"] = serde_json::json!([]);
    stage_request(&next, incomplete);
    assert!(NativeLifecycle::new(&graph).stage(0, &stage_path).is_err());
    stage_request(&next, gate);
    assert_eq!(
        NativeLifecycle::new(&graph)
            .stage(0, &stage_path)
            .unwrap()
            .operation,
        admitted.operation
    );
    write(
        &evidence_root,
        artifact.as_str(),
        b"tampered command evidence",
    );
    let invalidated = verification
        .check(&receipt_path, &check_path, Some(&policy))
        .unwrap();
    assert_eq!(
        invalidated.disposition,
        VerificationDisposition::Invalidated
    );
    assert_eq!(invalidated.reason, VerificationReason::ArtifactsChanged);
    let source_path = fixture.root().join(format!(
        ".mine/runtime/release-evidence/{}/source/{}",
        operation_id.as_str(),
        plan.fields().plan_path.as_str()
    ));
    assert_eq!(fs::read(&source_path).unwrap(), source_before);
    fs::write(&source_path, b"forged retained Plan").unwrap();
    assert!(verification.capture(&capture_path).is_err());
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
        fixture.source.as_str()
    );
    // Continue the same real native operation through curated integration and
    // Complete; I differs from C, both trees match, and the C locator is gone.
    fs::write(&source_path, &source_before).unwrap();
    write(&evidence_root, artifact.as_str(), actual_tree.as_bytes());
    let integrated = CommitId::new(git(
        fixture.root(),
        None,
        &["commit-tree", &actual_tree, "-p", fixture.baseline.as_str()],
        b"native curated integration\n",
    ))
    .unwrap();
    assert_ne!(integrated, candidate);
    git(
        fixture.root(),
        None,
        &[
            "update-ref",
            "refs/heads/master",
            integrated.as_str(),
            fixture.baseline.as_str(),
        ],
        b"",
    );
    git(fixture.root(), None, &["checkout", "master"], b"");
    git(
        fixture.root(),
        None,
        &["worktree", "remove", candidate_root.to_str().unwrap()],
        b"",
    );
    let mut integrated_fields = admitted.operation.fields().clone();
    integrated_fields.stage = crate::domain::lifecycle_operation::OperationStage::Release(
        crate::domain::lifecycle_operation::ReleaseStage::Integrated,
    );
    integrated_fields.stage_revision += 1;
    integrated_fields.integrated_commit = Some(integrated.clone());
    let final_gate = serde_json::json!({"policy_path":policy,"checks":[{
        "check":"fixture-suite","receipt":logical_receipt,"context":context,"fresh_observation":null
    }]});
    let write_progress = |fields: &crate::domain::lifecycle_operation::OperationFields,
                          expected_stage: u64| {
        let operation =
            crate::domain::lifecycle_operation::LifecycleOperation::new(fields.clone()).unwrap();
        write(fixture.root(), stage_path.as_str(), &serde_json::to_vec(&serde_json::json!({
            "schema_version":1,"expected_stage_revision":expected_stage,"operation":operation,"verification":final_gate
        })).unwrap());
    };
    write_progress(&integrated_fields, 1);
    // Git leaves the now-empty, relevant ignored locator parent in Root. Tree
    // equivalence is not input equivalence: Full policy must refuse this drift.
    assert!(NativeLifecycle::new(&graph).stage(0, &stage_path).is_err());
    fs::remove_dir(fixture.root().join(".mine/candidates")).unwrap();
    let frozen_tree = git(
        fixture.root(),
        None,
        &[
            "rev-parse",
            &format!("{}^{{tree}}", fixture.source.as_str()),
        ],
        b"",
    );
    let unrelated_dev = git(
        fixture.root(),
        None,
        &["commit-tree", &frozen_tree, "-p", fixture.source.as_str()],
        b"unrelated post-freeze commit\n",
    );
    git(
        fixture.root(),
        None,
        &[
            "update-ref",
            "refs/heads/dev",
            &unrelated_dev,
            fixture.source.as_str(),
        ],
        b"",
    );
    assert!(
        NativeLifecycle::new(&graph).stage(0, &stage_path).is_err(),
        "stable tree equivalence must not hide post-freeze dev advancement"
    );
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "dev"], b""),
        unrelated_dev
    );
    // Restore only this fixture's deliberately injected contrary ref evidence.
    git(
        fixture.root(),
        None,
        &[
            "update-ref",
            "refs/heads/dev",
            fixture.source.as_str(),
            &unrelated_dev,
        ],
        b"",
    );
    let integrated_result = NativeLifecycle::new(&graph).stage(0, &stage_path).unwrap();
    assert_release_diagnostic(fixture.root(), fixture.root(), operation_id, true);
    let no_operation = crate::cli::dispatch(
        &[
            "mine".into(),
            "--repo".into(),
            fixture.root().display().to_string(),
            "release".into(),
            "--format".into(),
            "json".into(),
        ],
        "mine",
    );
    assert_ne!(
        no_operation.exit_code, 0,
        "no-ID preflight cannot silently use a retained archive after purge"
    );
    assert_eq!(
        integrated_result
            .operation
            .fields()
            .integrated_commit
            .as_ref(),
        Some(&integrated)
    );
    assert_eq!(
        serde_json::to_value(&integrated_result.checks).unwrap()[0]["admission"],
        "reused"
    );
    let mut complete = integrated_result.operation.fields().clone();
    complete.stage = crate::domain::lifecycle_operation::OperationStage::Release(
        crate::domain::lifecycle_operation::ReleaseStage::Complete,
    );
    complete.stage_revision += 1;
    complete.outcome = crate::domain::lifecycle_operation::OperationOutcome::Committed;
    write_progress(&complete, 2);
    assert!(NativeLifecycle::new(&graph).stage(0, &stage_path).is_err());
    let finalized = NativeLifecycle::new(&graph)
        .finalize(0, &stage_path)
        .unwrap();
    assert!(!finalized.replay);
    assert!(finalized.workspace.is_none());
    assert_eq!(finalized.revision_before, 0);
    assert_eq!(finalized.revision_after, 0);
    assert_eq!(finalized.operation.fields().stage_revision, 3);
    let completed_replay = NativeLifecycle::new(&graph)
        .finalize(0, &stage_path)
        .unwrap();
    assert!(completed_replay.replay);
    assert_eq!(completed_replay.operation, finalized.operation);
    assert_release_diagnostic(fixture.root(), fixture.root(), operation_id, true);
    assert!(!fixture.root().join(&fixture.config.graph.source).exists());
    assert!(!candidate_root.exists());
    assert_eq!(
        git(fixture.root(), None, &["branch", "--show-current"], b""),
        "master"
    );
    // A retired source is not needed by a genuine historical seal. The owner
    // establishes a fresh clean dev at I; the binary does not switch branches.
    git(
        fixture.root(),
        None,
        &[
            "update-ref",
            "refs/heads/dev",
            integrated.as_str(),
            fixture.source.as_str(),
        ],
        b"",
    );
    git(fixture.root(), None, &["checkout", "dev"], b"");
    let workspace_service = crate::application::workspace_service::WorkspaceService::new(
        &crate::infrastructure::system::SystemUuidSource,
    );
    let manifest_path = fixture.root().join(
        finalized.operation.fields().verification_evidence[0]
            .path
            .as_str(),
    );
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    fs::write(&manifest_path, b"corrupt retired manifest").unwrap();
    assert!(
        workspace_service
            .open(&store, 0, "fixture-next-workspace-owner")
            .is_err()
    );
    assert!(!store.toml_path().exists());
    fs::write(&manifest_path, &manifest_bytes).unwrap();
    fs::remove_file(evidence_root.join(artifact.as_str())).unwrap();
    assert!(
        workspace_service
            .open(&store, 0, "fixture-next-workspace-owner")
            .is_err(),
        "missing decisive release log must block successor admission"
    );
    assert!(!store.toml_path().exists());
    write(&evidence_root, artifact.as_str(), actual_tree.as_bytes());
    git(
        fixture.root(),
        None,
        &["reflog", "expire", "--expire=now", "--all"],
        b"",
    );
    git(fixture.root(), None, &["gc", "--prune=now"], b"");
    let mut source_probe = checkpoint_git(fixture.root(), None).unwrap();
    assert!(
        !source_probe
            .run_raw(&["cat-file", "-e", fixture.source.as_str()], b"")
            .unwrap()
            .status
            .success(),
        "test must actually remove S rather than merely stop referring to it"
    );
    assert_release_diagnostic(fixture.root(), fixture.root(), operation_id, true);
    let opened = workspace_service
        .open(&store, 0, "fixture-next-workspace-owner")
        .unwrap();
    assert_ne!(
        opened.workspace_id,
        finalized.operation.fields().workspace_id
    );
    assert_eq!(opened.revision_before, 0);
    assert_eq!(opened.revision_after, 1);
    assert_eq!(opened.stable_baseline_commit, integrated.as_str());
    store
        .save_with_revision(1, "fixture-next-workspace-owner", |mut workspace| {
            workspace.revision += 1;
            Ok(workspace)
        })
        .unwrap();
    assert_eq!(store.observed_revision().unwrap(), 2);
    let current_graph_bytes = fs::read(store.toml_path()).unwrap();
    fs::write(&manifest_path, b"late retired-manifest corruption").unwrap();
    assert_release_diagnostic(fixture.root(), fixture.root(), operation_id, false);
    assert!(
        workspace_service
            .open(&store, 2, "fixture-next-workspace-owner")
            .is_err(),
        "existing-workspace no-op must not bypass retired authority"
    );
    assert!(workspace_service.status(&store).is_err());
    assert!(super::super::preflight(fixture.root()).is_err());
    assert!(
        store
            .save_with_revision(2, "fixture-next-workspace-owner", |mut workspace| {
                workspace.revision += 1;
                Ok(workspace)
            })
            .is_err()
    );
    assert_eq!(fs::read(store.toml_path()).unwrap(), current_graph_bytes);
    fs::write(&manifest_path, &manifest_bytes).unwrap();
    assert_eq!(
        workspace_service
            .open(&store, 2, "fixture-next-workspace-owner")
            .unwrap()
            .revision_after,
        2
    );
    assert!(!workspace_service.status(&store).unwrap().has_unresolved);
    let checkpoint_id = store
        .with_coordinator_lock(|guard| {
            use crate::infrastructure::operation_store::{OperationStore, RepositoryJournal};
            let journal =
                RepositoryJournal::under_lock(fixture.root(), &fixture.config.repository_id, guard)
                    .unwrap();
            let historical = journal.query(operation_id).unwrap();
            assert_eq!(historical.operation, finalized.operation);
            assert_ne!(
                historical.operation.fields().workspace_id,
                opened.workspace_id
            );
            let records = journal.observations().unwrap();
            assert_eq!(
                records.len(),
                3,
                "old release plus both new workspace checkpoints must remain catalogued"
            );
            let initial = records
                .iter()
                .find(|record| record.initial_predecessors.is_some())
                .unwrap();
            let predecessors = initial.initial_predecessors.as_ref().unwrap();
            assert_eq!(predecessors.len(), 1);
            assert_eq!(predecessors[0].release_operation_id, *operation_id);
            let writer = OperationStore::under_lock(
                fixture.root(),
                &fixture.config.repository_id,
                &opened.workspace_id,
                guard,
            )
            .unwrap();
            assert!(
                writer.query(operation_id).is_err(),
                "sealed history is read-only, never cross-workspace mutation authority"
            );
            assert_eq!(writer.observations().unwrap().len(), 2);
            Ok(records
                .iter()
                .find(|record| {
                    record.operation.fields().kind
                        == crate::domain::lifecycle_operation::OperationKind::GraphCheckpoint
                })
                .unwrap()
                .operation
                .fields()
                .operation_id
                .clone())
        })
        .unwrap();
    assert_release_diagnostic(fixture.root(), fixture.root(), &checkpoint_id, false);
}

/// Public Clap/handler diagnostics are checked against byte-identical owned
/// state. This is callable CLI evidence, not a spawned-binary/native-runner claim.
fn assert_release_diagnostic(root: &Path, caller: &Path, id: &OperationId, accepted: bool) {
    fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn visit(root: &Path, path: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
            if !path.exists() {
                return;
            }
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    visit(root, &entry.path(), files);
                } else {
                    files.insert(
                        entry
                            .path()
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into(),
                        fs::read(entry.path()).unwrap(),
                    );
                }
            }
        }
        let mut files = BTreeMap::new();
        for name in [
            ".mine/runtime/operations",
            ".mine/runtime/release-evidence",
            "docs/plan",
        ] {
            visit(root, &root.join(name), &mut files);
        }
        files
    }
    let before = snapshot(root);
    let outcome = crate::cli::dispatch(
        &[
            "mine".into(),
            "--repo".into(),
            caller.display().to_string(),
            "release".into(),
            "--operation-id".into(),
            id.as_str().into(),
            "--format".into(),
            "json".into(),
        ],
        "mine",
    );
    assert_eq!(outcome.exit_code == 0, accepted, "{outcome:?}");
    if accepted {
        let crate::cli::OutcomePayload::Success { envelope, .. } = outcome.payload else {
            panic!("expected diagnostic success")
        };
        assert_eq!(envelope.data["can_release"], false);
        assert_eq!(envelope.data["objective_phase_facts_valid"], true);
        assert_eq!(envelope.data["current_gate_admission"], "not_performed");
        assert_eq!(envelope.data["operation"]["operation_id"], id.as_str());
        assert_eq!(
            envelope.repository.as_deref(),
            Some(root.canonicalize().unwrap().to_str().unwrap())
        );
    }
    assert_eq!(
        snapshot(root),
        before,
        "diagnostics must not mutate graph, journal or evidence"
    );
}

#[test]
fn source_review_legacy_short_commits_remain_original_but_resolve_as_real_commits() {
    let mut fixture = Fixture::new();
    let id = fixture.accepted_source();
    let mut workspace: PlanWorkspace = toml::from_str(
        &fs::read_to_string(fixture.root().join(&fixture.config.graph.source)).unwrap(),
    )
    .unwrap();
    let abbreviated = fixture.baseline.as_str()[..7].to_string();
    workspace.get_mut(&id).unwrap().implementation_commits = vec![abbreviated.clone()];
    fixture.commit_workspace(&workspace);
    let candidate = fixture.candidate(true, |_, _| {});
    inspect_purge_candidate(fixture.root(), &fixture.source, &candidate).unwrap();
    let current: PlanWorkspace = toml::from_str(
        &fs::read_to_string(fixture.root().join(&fixture.config.graph.source)).unwrap(),
    )
    .unwrap();
    assert_eq!(
        current.get(&id).unwrap().implementation_commits,
        [abbreviated]
    );
    assert!(current.get(&id).unwrap().body_sha256.is_none());
    assert!(current.get(&id).unwrap().acceptance.is_none());
}

#[test]
fn source_review_commit_resolution_batches_zero_ten_hundred_thousand_and_duplicates() {
    let fixture = Fixture::new();
    let mut input = String::new();
    for ordinal in 1..=1000 {
        let message = format!("evidence commit {ordinal}\n");
        let parent = if ordinal == 1 {
            fixture.source.as_str().to_string()
        } else {
            format!(":{}", ordinal - 1)
        };
        input.push_str(&format!("commit refs/heads/source-evidence-fixture\nmark :{ordinal}\ncommitter Fixture <fixture@example.invalid> 1700000000 +0000\ndata {}\n{message}from {parent}\n\n", message.len()));
    }
    git(
        fixture.root(),
        None,
        &["fast-import", "--quiet"],
        input.as_bytes(),
    );
    let history = git(
        fixture.root(),
        None,
        &["rev-list", "--reverse", "source-evidence-fixture"],
        b"",
    );
    let commits = history
        .lines()
        .take(1000)
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(commits.len(), 1000);
    for size in [0, 10, 100, 1000] {
        for duplicate in [false, true] {
            let requested = (0..size)
                .map(|index| {
                    if duplicate {
                        commits[0].clone()
                    } else {
                        commits[index].clone()
                    }
                })
                .collect::<BTreeSet<_>>();
            let mut request = checkpoint_git(fixture.root(), None).unwrap();
            let before = request.diagnostics().invocations;
            let resolved =
                resolve_commit_evidence(&mut request, &requested, fixture.source.as_str().len())
                    .unwrap();
            let launched = request.diagnostics().invocations - before;
            assert_eq!(resolved, requested);
            assert_eq!(launched, u64::from(size != 0));
            eprintln!(
                "source_commit_resolution input={size} duplicate={duplicate} unique={} direct_children={launched}",
                resolved.len()
            );
        }
    }
    let mut request = checkpoint_git(fixture.root(), None).unwrap();
    let before = request.diagnostics().invocations;
    assert!(
        resolve_commit_evidence(
            &mut request,
            &BTreeSet::from(["HEAD".into()]),
            fixture.source.as_str().len()
        )
        .is_err()
    );
    assert_eq!(request.diagnostics().invocations, before);
}

#[test]
fn source_review_commit_protocol_rejects_missing_tags_and_mismatched_framing() {
    let full = "a".repeat(40);
    let requested = BTreeSet::from(["a".repeat(7)]);
    assert_eq!(
        decode_commit_evidence(format!("{full} commit\n").as_bytes(), &requested, 40).unwrap(),
        BTreeSet::from([full.clone()])
    );
    for output in [
        format!("{full} tag\n"),
        format!("{} missing\n", "a".repeat(7)),
        format!("{} commit\n", "b".repeat(40)),
        format!("{full} commit"),
        format!("{full} commit\nextra\n"),
        String::new(),
    ] {
        assert!(decode_commit_evidence(output.as_bytes(), &requested, 40).is_err());
    }
}

#[test]
fn release_candidate_locator_is_frozen_and_recovery_revalidates_same_checkout() {
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    let input = release_request(&fixture, &candidate, std::slice::from_ref(&id), &policy);
    let store = crate::infrastructure::toml_store::TomlStore::from_configuration(
        fixture.root(),
        &fixture.config,
    )
    .unwrap();
    let graph = crate::application::graph_service::GraphService::new(&store);
    let operation = crate::application::native_lifecycle::NativeLifecycle::new(&graph)
        .begin(1, &input)
        .unwrap();
    let op = &operation.fields().operation_id;
    let checkout = fixture.root().join(".mine/candidates/selected");
    let moved = fixture.root().join(".mine/candidates/moved");
    git(
        fixture.root(),
        None,
        &[
            "worktree",
            "move",
            checkout.to_str().unwrap(),
            moved.to_str().unwrap(),
        ],
        b"",
    );
    assert!(super::super::retained::ReleaseVerification::open(fixture.root(), op, &id).is_err());
    git(
        fixture.root(),
        None,
        &[
            "worktree",
            "move",
            moved.to_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
        b"",
    );
    super::super::retained::ReleaseVerification::open(fixture.root(), op, &id).unwrap();
    git(
        &checkout,
        None,
        &["checkout", "--detach", fixture.baseline.as_str()],
        b"",
    );
    assert!(super::super::retained::ReleaseVerification::open(fixture.root(), op, &id).is_err());
    git(
        &checkout,
        None,
        &["checkout", "--detach", candidate.as_str()],
        b"",
    );
    git(
        &checkout,
        None,
        &["checkout", "-b", &format!("plan/{id}-fixture")],
        b"",
    );
    assert!(super::super::retained::ReleaseVerification::open(fixture.root(), op, &id).is_err());
    git(
        &checkout,
        None,
        &["checkout", "--detach", candidate.as_str()],
        b"",
    );
    git(
        &checkout,
        None,
        &["update-index", "--skip-worktree", "README.md"],
        b"",
    );
    assert!(super::super::retained::ReleaseVerification::open(fixture.root(), op, &id).is_err());
    git(
        &checkout,
        None,
        &["update-index", "--no-skip-worktree", "README.md"],
        b"",
    );
    let lock = git(
        &checkout,
        None,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "index.lock",
        ],
        b"",
    );
    fs::write(&lock, b"foreign owner").unwrap();
    assert!(super::super::retained::ReleaseVerification::open(fixture.root(), op, &id).is_err());
    assert_eq!(fs::read(&lock).unwrap(), b"foreign owner");
    fs::remove_file(lock).unwrap();
    super::super::retained::ReleaseVerification::open(fixture.root(), op, &id).unwrap();
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "HEAD"], b""),
        fixture.source.as_str()
    );
}

#[test]
fn release_candidate_locator_admission_rejects_root_output_plan_foreign_and_escape() {
    use crate::application::{graph_service::GraphService, native_lifecycle::NativeLifecycle};
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    let input = release_request(&fixture, &candidate, std::slice::from_ref(&id), &policy);
    let output = fixture.root().join(".mine/build/foreign-candidate");
    git(
        fixture.root(),
        None,
        &[
            "worktree",
            "add",
            "--detach",
            output.to_str().unwrap(),
            candidate.as_str(),
        ],
        b"",
    );
    let foreign = fixture.root().join(".mine/candidates/foreign");
    fs::create_dir_all(&foreign).unwrap();
    git(&foreign, None, &["init", "-b", "foreign"], b"");
    let plan = fixture
        .root()
        .join(&fixture.config.plan.worktrees_root)
        .join(&id);
    git(
        fixture.root(),
        None,
        &[
            "worktree",
            "add",
            "--detach",
            plan.to_str().unwrap(),
            candidate.as_str(),
        ],
        b"",
    );
    let original: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root().join(input.as_str())).unwrap()).unwrap();
    let store = crate::infrastructure::toml_store::TomlStore::from_configuration(
        fixture.root(),
        &fixture.config,
    )
    .unwrap();
    let graph = GraphService::new(&store);
    let before = fs::read(store.toml_path()).unwrap();
    for locator in [
        ".",
        ".mine/build/foreign-candidate",
        ".mine/candidates/foreign",
        ".mine/runtime",
        "../outside",
        &format!("{}/{}", fixture.config.plan.worktrees_root, id),
    ] {
        let mut request = original.clone();
        request["intent"]["candidate_root"] = locator.into();
        write(
            fixture.root(),
            input.as_str(),
            &serde_json::to_vec(&request).unwrap(),
        );
        assert!(
            NativeLifecycle::new(&graph).begin(1, &input).is_err(),
            "locator={locator}"
        );
        assert!(
            !fixture
                .root()
                .join(".mine/runtime/release-evidence")
                .exists()
        );
        assert_eq!(fs::read(store.toml_path()).unwrap(), before);
    }
}

#[test]
fn release_candidate_integrated_phase_uses_actual_stable_result_not_missing_locator() {
    use crate::application::{
        graph_service::GraphService, native_lifecycle::NativeLifecycle,
        verification_service::VerificationBackend,
    };
    use crate::domain::lifecycle_operation::*;
    use crate::infrastructure::operation_store::OperationStore;
    let mut fixture = Fixture::new();
    fixture.preflight_source();
    let id = fixture.accepted_source();
    let policy = fixture.release_policy();
    let candidate = fixture.candidate(true, |_, _| {});
    let input = release_request(&fixture, &candidate, std::slice::from_ref(&id), &policy);
    let store = crate::infrastructure::toml_store::TomlStore::from_configuration(
        fixture.root(),
        &fixture.config,
    )
    .unwrap();
    let graph = GraphService::new(&store);
    let operation = NativeLifecycle::new(&graph).begin(1, &input).unwrap();
    let tree = git(
        fixture.root(),
        None,
        &["rev-parse", &format!("{}^{{tree}}", candidate.as_str())],
        b"",
    );
    let integrated = CommitId::new(git(
        fixture.root(),
        None,
        &["commit-tree", &tree, "-p", fixture.baseline.as_str()],
        b"different curated integration result\n",
    ))
    .unwrap();
    assert_ne!(integrated, candidate);
    git(
        fixture.root(),
        None,
        &[
            "update-ref",
            "refs/heads/master",
            integrated.as_str(),
            fixture.baseline.as_str(),
        ],
        b"",
    );
    git(fixture.root(), None, &["checkout", "master"], b"");
    let checkout = fixture.root().join(".mine/candidates/selected");
    git(
        fixture.root(),
        None,
        &["worktree", "remove", checkout.to_str().unwrap()],
        b"",
    );
    assert!(
        super::super::retained::ReleaseVerification::open(
            fixture.root(),
            &operation.fields().operation_id,
            &id
        )
        .is_err()
    );
    // Import synthetic phase records solely to exercise the reader boundary.
    // This deliberately does NOT call/claim a successful native release gate.
    store
        .with_coordinator_lock(|guard| {
            let journal = OperationStore::under_lock(
                fixture.root(),
                &fixture.config.repository_id,
                &operation.fields().workspace_id,
                guard,
            )
            .unwrap();
            let mut record = journal.query(&operation.fields().operation_id).unwrap();
            let mut fields = record.operation.fields().clone();
            fields.stage = OperationStage::Release(ReleaseStage::Preflight);
            fields.stage_revision = 1;
            record.operation = LifecycleOperation::new(fields).unwrap();
            journal.advance(0, &record).unwrap();
            let mut fields = record.operation.fields().clone();
            fields.stage = OperationStage::Release(ReleaseStage::Integrated);
            fields.stage_revision = 2;
            fields.integrated_commit = Some(integrated.clone());
            record.operation = LifecycleOperation::new(fields).unwrap();
            journal.advance(1, &record).unwrap();
            Ok(())
        })
        .unwrap();
    let (plan, backend) = super::super::retained::ReleaseVerification::open(
        fixture.root(),
        &operation.fields().operation_id,
        &id,
    )
    .unwrap();
    backend.plan_body(&plan).unwrap();
    assert_eq!(
        backend.file(&path("README.md")).unwrap().sha256,
        digest(&fs::read(fixture.root().join("README.md")).unwrap())
    );
    assert!(!checkout.exists());
    assert!(!fixture.root().join("docs/plan").exists());
    let current_policy = backend.load_policy(&policy).unwrap();
    let before = backend.manifest(&current_policy, "fixture-suite").unwrap();
    write(
        fixture.root(),
        "ignored-candidate-input",
        b"actual stable cwd has a different relevant input",
    );
    let after = backend.manifest(&current_policy, "fixture-suite").unwrap();
    assert_ne!(before, after);
    assert_eq!(
        git(fixture.root(), None, &["rev-parse", "HEAD^{tree}"], b""),
        tree
    );
}
