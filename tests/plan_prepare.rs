#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use mine::cli;
#[allow(dead_code)]
mod common;

// mine-release-allow-plan-reference: canonical base-id protocol fixture
const BASE_ID: &str = "plan01";
// mine-release-allow-plan-reference: canonical base-plan path protocol fixture
const BASE_PATH: &str = ".mine/plan/plan01-fixture.md";
// mine-release-allow-plan-reference: canonical base-branch protocol fixture
const BASE_BRANCH: &str = "plan/plan01-fixture";
// mine-release-allow-plan-reference: canonical compensation-id protocol fixture
const COMP_ID: &str = "plan01-C01";
// mine-release-allow-plan-reference: canonical compensation path protocol fixture
const COMP_PATH: &str = ".mine/plan/plan01-C01-fixture.md";
// mine-release-allow-plan-reference: canonical compensation branch protocol fixture
const COMP_BRANCH: &str = "plan/plan01-C01-fixture";
// mine-release-allow-plan-reference: canonical second-lineage id protocol fixture
const OTHER_ID: &str = "plan02";
// mine-release-allow-plan-reference: canonical second-lineage path protocol fixture
const OTHER_PATH: &str = ".mine/plan/plan02-other.md";

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

#[derive(Debug)]
struct Snapshot {
    branch: String,
    head: String,
    graph: Vec<u8>,
    revision: u64,
}

impl Fixture {
    fn new(root_name: &str, worktrees_root: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(root_name);
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet", "-b", "master"]);
        for (key, value) in [
            ("user.email", "fixture@example.invalid"),
            ("user.name", "Preparation fixture"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
        ] {
            git(&root, &["config", key, value]);
        }
        std::fs::write(root.join("README.md"), "fixture\n").unwrap();
        std::fs::write(
            root.join(".gitignore"),
            format!("/.mine/runtime/\n/.mine/locks/\n/.mine/build/\n/{worktrees_root}\n"),
        )
        .unwrap();
        clean_commit(&root, "private initial product");
        let outcome = dispatch(&root, &["init"]);
        assert_eq!(outcome.exit_code, 0, "{}", envelope(&outcome));
        let mut config = mine::cli::context::load_config(&root).unwrap();
        config.plan.worktrees_root = worktrees_root.into();
        std::fs::write(root.join(".mine/config.toml"), config.to_toml()).unwrap();
        clean_commit(&root, "private managed baseline");
        let baseline = git_output(&root, &["rev-parse", "HEAD"]);
        git(&root, &["switch", "-c", "dev"]);

        // Explicit historical READY fixture, not fabricated native preparation.
        // Public bootstrap/registration remain covered by E2E/lifecycle tests.
        let mut node = common::node(BASE_ID, mine::domain::status::PlanStatus::Ready, &[], &[]);
        node.exclusive_write_paths = vec!["src/".into()];
        node.worktree_path = mine::domain::plan_id::CanonicalPlanId::parse(BASE_ID)
            .unwrap()
            .worktree_path_under(worktrees_root);
        let workspace = mine::domain::graph::PlanWorkspace {
            schema_version: 4,
            revision: 72,
            project_id: "prepare-fixture".into(),
            workspace_id: uuid::Uuid::new_v4().to_string(),
            stable_branch: "master".into(),
            integration_branch: "dev".into(),
            stable_baseline_commit: baseline,
            design_root: ".mine/design/index.md".into(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![node],
        };
        std::fs::create_dir_all(root.join(".mine/plan")).unwrap();
        std::fs::write(root.join(BASE_PATH), format!("# {BASE_ID}\n")).unwrap();
        std::fs::write(
            root.join(".mine/plan/execution-graph.toml"),
            toml::to_string(&workspace).unwrap(),
        )
        .unwrap();
        mine::infrastructure::toml_store::TomlStore::new(&root)
            .render()
            .unwrap();
        clean_commit(&root, "explicit private READY graph and body");
        Self { _tmp: tmp, root }
    }

    fn target(&self) -> PathBuf {
        self.root.join(".mine/worktrees").join(BASE_ID)
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            branch: git_output(&self.root, &["branch", "--show-current"]),
            head: git_output(&self.root, &["rev-parse", "HEAD"]),
            graph: graph_bytes(&self.root),
            revision: graph_revision(&self.root),
        }
    }
    fn assert_refusal(&self, outcome: &cli::Outcome, before: &Snapshot) {
        let body = envelope(outcome);
        assert_ne!(outcome.exit_code, 0, "unexpected success: {body}");
        assert!(
            body["error"]["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("MINE_")),
            "typed error required: {body}"
        );
        assert_eq!(
            git_output(&self.root, &["branch", "--show-current"]),
            before.branch
        );
        assert_eq!(git_output(&self.root, &["rev-parse", "HEAD"]), before.head);
        assert_eq!(graph_bytes(&self.root), before.graph);
        assert_eq!(graph_revision(&self.root), before.revision);
    }
    fn prepare_base(&self, expected: u64) -> serde_json::Value {
        let outcome = prepare(&self.root, BASE_ID, expected);
        let body = envelope(&outcome);
        assert_eq!(outcome.exit_code, 0, "{body}");
        body
    }
    fn start_and_commit_graph(&self, id: &str, expected: u64) -> String {
        let outcome = dispatch(
            &self.root,
            &[
                "plan",
                "start",
                "--id",
                id,
                "--owner",
                "fixture-writer",
                "--run-id",
                "fixture-run",
                "--expected-revision",
                &expected.to_string(),
            ],
        );
        let body = envelope(&outcome);
        assert_eq!(outcome.exit_code, 0, "{body}");
        assert_eq!(
            git_output(&self.root, &["status", "--porcelain=v1", "-uall"]),
            ""
        );
        body["data"]["branch_anchor"].as_str().unwrap().into()
    }
    fn commit_plan_change(&self, name: &str) -> String {
        let target = self.target();
        if git_output(&target, &["branch", "--show-current"]) == COMP_BRANCH
            && !target.join(COMP_PATH).exists()
        {
            // The stopped predecessor is immutable. The private compensation
            // writer commits an exact copy of its new Root-owned Plan body.
            std::fs::write(
                target.join(COMP_PATH),
                std::fs::read(self.root.join(COMP_PATH)).unwrap(),
            )
            .unwrap();
        }
        std::fs::create_dir_all(target.join("src")).unwrap();
        std::fs::write(target.join("src").join(name), "plan change\n").unwrap();
        clean_commit(&target, &format!("private Plan change {name}"));
        git_output(&target, &["rev-parse", "HEAD"])
    }
    fn mark_implemented(&self, id: &str, commit: &str, expected: u64) -> String {
        let report = match id {
            // mine-release-allow-plan-reference: canonical private implementation reports
            BASE_ID => ".mine/plan/reports/plan01-fixture-implementation.md",
            // mine-release-allow-plan-reference: canonical private implementation reports
            COMP_ID => ".mine/plan/reports/plan01-C01-fixture-implementation.md",
            _ => panic!("unexpected fixture subject"),
        };
        let target = self.target();
        std::fs::create_dir_all(target.join(".mine/plan/reports")).unwrap();
        std::fs::write(
            target.join(report),
            "# Actual private implementation evidence\n",
        )
        .unwrap();
        git(&target, &["add", "--", report]);
        git(
            &target,
            &[
                "commit",
                "--quiet",
                "-m",
                "private canonical implementation report",
            ],
        );
        let tip = git_output(&target, &["rev-parse", "HEAD"]);
        assert!(git_is_ancestor(&self.root, commit, &tip));
        let outcome = dispatch(
            &self.root,
            &[
                "plan",
                "implemented",
                "--id",
                id,
                "--actor",
                "fixture-writer",
                "--expected-revision",
                &expected.to_string(),
                "--report",
                report,
                "--commit",
                commit,
                "--commit",
                &tip,
            ],
        );
        assert_eq!(outcome.exit_code, 0, "{}", envelope(&outcome));
        assert_eq!(
            git_output(&self.root, &["status", "--porcelain=v1", "-uall"]),
            ""
        );
        tip
    }
    fn reject_base(&self, reason: &str) -> (cli::Outcome, serde_json::Value, u64) {
        use mine::domain::graph::{PlanIdentity, PlanIdentityFields, RepoPath};
        use mine::domain::ports::Clock;
        use mine::infrastructure::{graph_checkpoint::digest, system::SystemClock};
        let expected = graph_revision(&self.root);
        let current: mine::domain::graph::PlanWorkspace =
            toml::from_str(std::str::from_utf8(&graph_bytes(&self.root)).unwrap()).unwrap();
        let node = current.get(BASE_ID).unwrap();
        let config = mine::cli::context::load_config(&self.root).unwrap();
        let identity = PlanIdentity::new(PlanIdentityFields {
            repository_id: config.repository_id,
            workspace_id: current.workspace_id.clone(),
            plan_id: node.id.clone(),
            plan_path: RepoPath::new(node.path.clone()).unwrap(),
            body_sha256: Some(digest(&std::fs::read(self.root.join(&node.path)).unwrap())),
            lineage_id: node.lineage_id.clone(),
            branch: node.branch.clone(),
        })
        .unwrap();
        let assertion = serde_json::json!({"schema_version":1,"plan":identity,"writer_owner":"fixture-writer","writer_run_id":"fixture-run","reviewer":"fixture-reviewer","independent_review":true,"writer_stopped":true,"stopped_at":SystemClock.now_utc_rfc3339(),"stop_basis":"Private controller stopped the writer and preserved its exact reviewed checkout"});
        // mine-release-allow-plan-reference: independently committed private review
        let review = ".mine/plan/reports/plan01-fixture-review.md";
        let body = format!(
            "# Independent review\n\nVerdict: REJECTED\n\n```mine-review-closure\n{assertion}\n```\n"
        );
        std::fs::create_dir_all(self.root.join(".mine/plan/reports")).unwrap();
        std::fs::write(self.root.join(review), &body).unwrap();
        let compensation_body = format!("# {COMP_ID}\n");
        std::fs::write(self.root.join(COMP_PATH), &compensation_body).unwrap();
        git(&self.root, &["add", "--", review, COMP_PATH]);
        git(
            &self.root,
            &[
                "commit",
                "--quiet",
                "-m",
                "private independent stopped handoff",
            ],
        );
        let mut compensation = identity.fields().clone();
        compensation.plan_id = COMP_ID.into();
        compensation.plan_path = RepoPath::new(COMP_PATH).unwrap();
        compensation.branch = COMP_BRANCH.into();
        compensation.body_sha256 = Some(digest(compensation_body.as_bytes()));
        let input = serde_json::json!({"schema_version":1,"actor":"fixture-reviewer","intent":{"kind":"rejected_handoff","plan_id":BASE_ID,"review":{"reviewed_tip":git_output(&self.target(),&["rev-parse","HEAD"]),"report":{"path":review,"bytes":body.len(),"sha256":digest(body.as_bytes())}},"compensation":PlanIdentity::new(compensation).unwrap()}});
        let input_path = ".mine/runtime/prepare-rejection.json";
        std::fs::write(
            self.root.join(input_path),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        let begun = dispatch(
            &self.root,
            &[
                "operation",
                "begin",
                "--expected-revision",
                &expected.to_string(),
                "--input",
                input_path,
            ],
        );
        assert_eq!(begun.exit_code, 0, "{}", envelope(&begun));
        let operation = envelope(&begun)["data"]["operation"].clone();
        let rejected = dispatch(
            &self.root,
            &[
                "plan",
                "reject",
                "--id",
                BASE_ID,
                "--actor",
                "fixture-reviewer",
                "--expected-revision",
                &expected.to_string(),
                "--reason",
                reason,
                "--review-report",
                review,
                "--compensating-plan",
                COMP_ID,
            ],
        );
        assert_eq!(rejected.exit_code, 0, "{}", envelope(&rejected));
        assert_eq!(envelope(&rejected)["revision_after"], expected + 1);
        (rejected, operation, expected + 1)
    }
    fn advance_handoff(
        &self,
        current: &serde_json::Value,
        stage: mine::domain::lifecycle_operation::RejectionStage,
        revision: u64,
    ) -> serde_json::Value {
        use mine::domain::lifecycle_operation::{OperationStage, RejectionStage};
        let terminal = stage == RejectionStage::Complete;
        let expected = current["stage_revision"].as_u64().unwrap();
        let mut next = current.clone();
        next["stage_revision"] = serde_json::json!(expected + 1);
        next["stage"] = serde_json::json!(OperationStage::RejectedHandoff(stage));
        if terminal {
            next["outcome"] = serde_json::json!("committed");
        }
        let path = ".mine/runtime/prepare-handoff-stage.json";
        std::fs::write(self.root.join(path),serde_json::to_vec(&serde_json::json!({"schema_version":1,"expected_stage_revision":expected,"operation":next,"verification":null})).unwrap()).unwrap();
        let result = dispatch(
            &self.root,
            &[
                "operation",
                if terminal { "finalize" } else { "stage" },
                "--expected-revision",
                &revision.to_string(),
                "--input",
                path,
            ],
        );
        assert_eq!(result.exit_code, 0, "{}", envelope(&result));
        envelope(&result)["data"]["operation"].clone()
    }
    fn reject_base_and_register_compensation(&self) {
        use mine::domain::lifecycle_operation::RejectionStage;
        let (_, mut operation, mut revision) = self.reject_base("fixture");
        operation = self.advance_handoff(&operation, RejectionStage::Rejected, revision);
        revision = add_plan(&self.root, COMP_ID, COMP_PATH, "src/", revision);
        operation =
            self.advance_handoff(&operation, RejectionStage::CompensationRegistered, revision);
        let rewired = dispatch(
            &self.root,
            &[
                "plan",
                "rewire-compensation",
                "--id",
                BASE_ID,
                "--actor",
                "fixture-coordinator",
                "--expected-revision",
                &revision.to_string(),
            ],
        );
        assert_eq!(rewired.exit_code, 0, "{}", envelope(&rewired));
        assert_eq!(envelope(&rewired)["revision_after"], revision);
        operation = self.advance_handoff(&operation, RejectionStage::Rewired, revision);
        let released = dispatch(
            &self.root,
            &[
                "plan",
                "release",
                "--id",
                COMP_ID,
                "--actor",
                "fixture-coordinator",
                "--expected-revision",
                &revision.to_string(),
            ],
        );
        assert_eq!(released.exit_code, 0, "{}", envelope(&released));
        revision += 1;
        operation = self.advance_handoff(&operation, RejectionStage::Released, revision);
        self.advance_handoff(&operation, RejectionStage::Complete, revision);
    }
    fn register_compensation(&self) {
        self.start_and_commit_graph(BASE_ID, graph_revision(&self.root));
        let base_commit = git_output(&self.root, &["rev-parse", BASE_BRANCH]);
        self.mark_implemented(BASE_ID, &base_commit, graph_revision(&self.root));
        self.reject_base_and_register_compensation();
    }
}

#[test]
fn base_create_checkpoints_root_once_and_exact_reuse_is_read_only() {
    let fixture = Fixture::new("repo", ".mine/worktrees");
    let before = fixture.snapshot();
    let first = fixture.prepare_base(graph_revision(&fixture.root));
    assert_eq!(first["data"]["plan"]["action"], "created");
    assert_eq!(
        first["data"]["plan"]["observed_worktree"]["branch"],
        BASE_BRANCH
    );
    let after_creation = fixture.snapshot();
    assert_eq!(after_creation.revision, before.revision + 1);
    assert_ne!(after_creation.head, before.head);
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", "HEAD^"]),
        before.head
    );
    assert_eq!(after_creation.branch, "dev");
    let second = fixture.prepare_base(after_creation.revision);
    assert_eq!(second["data"]["plan"]["action"], "reused");
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", "HEAD"]),
        after_creation.head
    );
    assert_eq!(graph_bytes(&fixture.root), after_creation.graph);
}

#[test]
fn in_progress_base_reuses_committed_descendant_after_root_graph_commit() {
    let fixture = Fixture::new("active-base", ".mine/worktrees");
    let created = fixture.prepare_base(graph_revision(&fixture.root));
    assert_eq!(created["data"]["plan"]["action"], "created");
    let creation_anchor = git_output(&fixture.root, &["rev-parse", BASE_BRANCH]);

    let stored_anchor = fixture.start_and_commit_graph(BASE_ID, graph_revision(&fixture.root));
    assert_eq!(stored_anchor, creation_anchor);
    let tip = fixture.commit_plan_change("execution.txt");
    assert!(git_is_ancestor(&fixture.root, &creation_anchor, &tip));
    let before = fixture.snapshot();

    let outcome = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
    let body = envelope(&outcome);
    assert_eq!(outcome.exit_code, 0, "{body}");
    assert_eq!(body["data"]["plan"]["action"], "reused");
    assert_eq!(body["data"]["plan"]["observed_worktree"]["head"], tip);
    assert_eq!(graph_bytes(&fixture.root), before.graph);
    assert_eq!(graph_revision(&fixture.root), before.revision);

    fixture.mark_implemented(BASE_ID, &tip, graph_revision(&fixture.root));
    let review_reuse = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
    assert_eq!(review_reuse.exit_code, 0, "{}", envelope(&review_reuse));
}

#[test]
fn active_compensation_reuses_implementation_and_report_descendants() {
    let fixture = Fixture::new("active-compensation", ".mine/worktrees");
    fixture.prepare_base(graph_revision(&fixture.root));
    fixture.start_and_commit_graph(BASE_ID, graph_revision(&fixture.root));
    fixture.commit_plan_change("base-implementation.txt");
    let rejected_tip = fixture.commit_plan_change("base-report.txt");
    let rejected_tip =
        fixture.mark_implemented(BASE_ID, &rejected_tip, graph_revision(&fixture.root));
    fixture.reject_base_and_register_compensation();

    let switched = prepare(&fixture.root, COMP_ID, graph_revision(&fixture.root));
    let switched_body = envelope(&switched);
    assert_eq!(switched.exit_code, 0, "{switched_body}");
    assert_eq!(switched_body["data"]["plan"]["action"], "created");
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", COMP_BRANCH]),
        rejected_tip
    );

    let stored_anchor = fixture.start_and_commit_graph(COMP_ID, graph_revision(&fixture.root));
    assert_eq!(stored_anchor, rejected_tip);
    fixture.commit_plan_change("compensation-implementation.txt");
    let compensation_tip = fixture.commit_plan_change("compensation-report.txt");
    assert!(git_is_ancestor(
        &fixture.root,
        &rejected_tip,
        &compensation_tip
    ));
    let before = fixture.snapshot();

    let active_reuse = prepare(&fixture.root, COMP_ID, graph_revision(&fixture.root));
    let active_body = envelope(&active_reuse);
    assert_eq!(active_reuse.exit_code, 0, "{active_body}");
    assert_eq!(active_body["data"]["plan"]["action"], "reused");
    assert_eq!(
        active_body["data"]["plan"]["observed_worktree"]["head"],
        compensation_tip
    );
    assert_eq!(graph_bytes(&fixture.root), before.graph);
    assert_eq!(graph_revision(&fixture.root), before.revision);

    fixture.mark_implemented(COMP_ID, &compensation_tip, graph_revision(&fixture.root));
    let review_reuse = prepare(&fixture.root, COMP_ID, graph_revision(&fixture.root));
    assert_eq!(review_reuse.exit_code, 0, "{}", envelope(&review_reuse));
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        rejected_tip
    );
}

#[test]
fn ready_attached_branch_advanced_before_start_is_refused_without_mutation() {
    let fixture = Fixture::new("ready-advanced", ".mine/worktrees");
    fixture.prepare_base(graph_revision(&fixture.root));
    let advanced = fixture.commit_plan_change("premature.txt");
    let before = fixture.snapshot();

    let outcome = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
    fixture.assert_refusal(&outcome, &before);
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        advanced
    );
    assert_eq!(
        git_output(&fixture.target(), &["branch", "--show-current"]),
        BASE_BRANCH
    );
}

#[test]
fn active_base_and_compensation_reject_rewritten_tips_in_all_active_states() {
    for (kind, implemented) in [
        ("base", false),
        ("base", true),
        ("compensation", false),
        ("compensation", true),
    ] {
        let fixture = Fixture::new(
            &format!("rewritten-{kind}-{implemented}"),
            ".mine/worktrees",
        );
        fixture.prepare_base(graph_revision(&fixture.root));
        let (id, branch, anchor) = if kind == "compensation" {
            fixture.register_compensation();
            let prepared = prepare(&fixture.root, COMP_ID, graph_revision(&fixture.root));
            assert_eq!(prepared.exit_code, 0, "{}", envelope(&prepared));
            (
                COMP_ID,
                COMP_BRANCH,
                fixture.start_and_commit_graph(COMP_ID, graph_revision(&fixture.root)),
            )
        } else {
            (
                BASE_ID,
                BASE_BRANCH,
                fixture.start_and_commit_graph(BASE_ID, graph_revision(&fixture.root)),
            )
        };
        let legitimate_tip = fixture.commit_plan_change("legitimate.txt");
        if implemented {
            fixture.mark_implemented(id, &legitimate_tip, graph_revision(&fixture.root));
        }
        let unrelated = unrelated_same_tree(&fixture.target());
        assert!(!git_is_ancestor(&fixture.root, &anchor, &unrelated));
        git(
            &fixture.root,
            &["update-ref", &format!("refs/heads/{branch}"), &unrelated],
        );
        assert_eq!(
            git_output(&fixture.target(), &["status", "--porcelain=v1", "-uall"]),
            "",
            "history rewrite must not be masked by a dirty checkout"
        );
        let before = fixture.snapshot();

        let outcome = prepare(&fixture.root, id, graph_revision(&fixture.root));
        assert_eq!(outcome.exit_code, 4, "{}", envelope(&outcome));
        assert!(
            envelope(&outcome)["error"]["message"]
                .as_str()
                .unwrap()
                .contains("immutable started anchor")
        );
        fixture.assert_refusal(&outcome, &before);
        assert_eq!(git_output(&fixture.root, &["rev-parse", branch]), unrelated);
    }
}

#[test]
fn active_prepare_refuses_missing_invalid_and_unrelated_anchors_without_mutation() {
    for replacement in ["", "not-a-commit", "ROOT_HEAD"] {
        let fixture = Fixture::new(&format!("bad-anchor-{replacement}"), ".mine/worktrees");
        fixture.prepare_base(graph_revision(&fixture.root));
        let stored = fixture.start_and_commit_graph(BASE_ID, graph_revision(&fixture.root));
        let unrelated = git_output(&fixture.root, &["rev-parse", "HEAD"]);
        let graph_path = fixture.root.join(".mine/plan/execution-graph.toml");
        let graph = std::fs::read_to_string(&graph_path).unwrap().replace(
            &format!("branch_anchor = \"{stored}\""),
            &format!(
                "branch_anchor = \"{}\"",
                if replacement == "ROOT_HEAD" {
                    unrelated.as_str()
                } else {
                    replacement
                }
            ),
        );
        std::fs::write(&graph_path, graph).unwrap();
        clean_commit(&fixture.root, "forge anchor evidence");
        let before = fixture.snapshot();
        let outcome = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
        fixture.assert_refusal(&outcome, &before);
    }
}

#[test]
fn active_prepare_refuses_branch_rewritten_before_stored_anchor() {
    let fixture = Fixture::new("rewritten-branch", ".mine/worktrees");
    fixture.prepare_base(graph_revision(&fixture.root));
    fixture.start_and_commit_graph(BASE_ID, graph_revision(&fixture.root));
    let unrelated = unrelated_same_tree(&fixture.target());
    assert!(!git_is_ancestor(
        &fixture.root,
        &git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        &unrelated
    ));
    git(
        &fixture.root,
        &[
            "update-ref",
            &format!("refs/heads/{BASE_BRANCH}"),
            &unrelated,
        ],
    );
    assert_eq!(
        git_output(&fixture.target(), &["status", "--porcelain=v1", "-uall"]),
        ""
    );
    let before = fixture.snapshot();
    let outcome = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
    assert_eq!(outcome.exit_code, 4, "{}", envelope(&outcome));
    assert!(
        envelope(&outcome)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("immutable started anchor")
    );
    fixture.assert_refusal(&outcome, &before);
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        unrelated
    );
}

#[test]
fn active_rejection_records_review_without_fake_implementation_evidence() {
    let fixture = Fixture::new("active-rejection", ".mine/worktrees");
    fixture.prepare_base(graph_revision(&fixture.root));
    fixture.start_and_commit_graph(BASE_ID, graph_revision(&fixture.root));
    let branch_tip = git_output(&fixture.root, &["rev-parse", BASE_BRANCH]);
    // mine-release-allow-plan-reference: canonical independently committed review
    let review = ".mine/plan/reports/plan01-fixture-review.md";
    let (outcome, _operation, _revision) = fixture.reject_base("immutable scope is infeasible");
    let body = envelope(&outcome);
    assert_eq!(outcome.exit_code, 0, "{body}");
    assert_eq!(body["data"]["review_report"], review);
    let graph: mine::domain::graph::PlanWorkspace = toml::from_str(
        &std::fs::read_to_string(fixture.root.join(".mine/plan/execution-graph.toml")).unwrap(),
    )
    .unwrap();
    let rejected = graph.get(BASE_ID).unwrap();
    assert_eq!(rejected.status, mine::domain::status::PlanStatus::Rejected);
    assert_eq!(rejected.review_report, review);
    assert!(rejected.implementation_report.is_empty());
    assert!(rejected.implementation_commits.is_empty());
    assert_eq!(rejected.compensating_plan, COMP_ID);
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        branch_tip
    );
    assert_eq!(
        git_output(&fixture.target(), &["branch", "--show-current"]),
        BASE_BRANCH
    );
    assert!(!git_branch_exists(&fixture.root, COMP_BRANCH));
}

#[test]
fn active_rejection_refuses_missing_escaped_and_noncanonical_review_evidence() {
    let cases = [
        // mine-release-allow-plan-reference: canonical report-path protocol fixture
        ("missing", ".mine/plan/reports/plan01-fixture-review.md"),
        ("escaped", "../outside-review.md"),
        ("wrong", ".mine/plan/reports/wrong-review.md"),
        // mine-release-allow-plan-reference: canonical report-path protocol fixture
        ("directory", ".mine/plan/reports/plan01-fixture-review.md"),
    ];
    for (name, review) in cases {
        let fixture = Fixture::new(&format!("reject-{name}"), ".mine/worktrees");
        fixture.prepare_base(graph_revision(&fixture.root));
        fixture.start_and_commit_graph(BASE_ID, graph_revision(&fixture.root));
        if name == "wrong" {
            std::fs::create_dir_all(fixture.root.join(".mine/plan/reports")).unwrap();
            std::fs::write(fixture.root.join(review), "wrong artifact\n").unwrap();
        } else if name == "directory" {
            std::fs::create_dir_all(fixture.root.join(review)).unwrap();
        }
        let before = fixture.snapshot();
        let outcome = dispatch(
            &fixture.root,
            &[
                "plan",
                "reject",
                "--actor",
                "fixture-reviewer",
                "--expected-revision",
                &graph_revision(&fixture.root).to_string(),
                "--id",
                BASE_ID,
                "--reason",
                "infeasible",
                "--review-report",
                review,
                "--compensating-plan",
                COMP_ID,
            ],
        );
        fixture.assert_refusal(&outcome, &before);
    }
}

#[test]
fn active_rejection_cli_requires_review_report_argument() {
    let fixture = Fixture::new("reject-missing-argument", ".mine/worktrees");
    fixture.prepare_base(graph_revision(&fixture.root));
    fixture.start_and_commit_graph(BASE_ID, graph_revision(&fixture.root));
    let before = fixture.snapshot();
    let outcome = dispatch(
        &fixture.root,
        &[
            "plan",
            "reject",
            "--actor",
            "fixture-reviewer",
            "--expected-revision",
            &graph_revision(&fixture.root).to_string(),
            "--id",
            BASE_ID,
            "--reason",
            "infeasible",
            "--compensating-plan",
            COMP_ID,
        ],
    );
    assert_eq!(outcome.exit_code, 2, "{}", envelope(&outcome));
    fixture.assert_refusal(&outcome, &before);
}

#[test]
fn stale_existing_base_branch_is_refused_without_mutation() {
    let fixture = Fixture::new("repo", ".mine/worktrees");
    let stale = git_output(&fixture.root, &["rev-parse", "HEAD~1"]);
    git(&fixture.root, &["branch", BASE_BRANCH, &stale]);
    let before = fixture.snapshot();
    let outcome = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
    fixture.assert_refusal(&outcome, &before);
    assert!(!fixture.target().exists());
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        stale
    );
}

#[test]
fn stale_existing_compensation_branch_is_refused_without_switch() {
    let fixture = Fixture::new("repo", ".mine/worktrees");
    fixture.prepare_base(graph_revision(&fixture.root));
    fixture.register_compensation();
    let stale = git_output(&fixture.root, &["rev-parse", "HEAD"]);
    assert_ne!(
        stale,
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH])
    );
    git(&fixture.root, &["branch", COMP_BRANCH, &stale]);
    let before = fixture.snapshot();
    let outcome = prepare(&fixture.root, COMP_ID, graph_revision(&fixture.root));
    fixture.assert_refusal(&outcome, &before);
    assert_eq!(
        git_output(&fixture.target(), &["branch", "--show-current"]),
        BASE_BRANCH
    );
}

#[test]
fn valid_compensation_switches_one_lineage_and_preserves_rejected_ref() {
    let fixture = Fixture::new("repo", ".mine/worktrees");
    fixture.prepare_base(graph_revision(&fixture.root));
    fixture.register_compensation();
    let rejected_head = git_output(&fixture.root, &["rev-parse", BASE_BRANCH]);
    let result = prepare(&fixture.root, COMP_ID, graph_revision(&fixture.root));
    let body = envelope(&result);
    assert_eq!(result.exit_code, 0, "{body}");
    assert_eq!(body["data"]["plan"]["action"], "created");
    assert_eq!(
        git_output(&fixture.target(), &["branch", "--show-current"]),
        COMP_BRANCH
    );
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        rejected_head
    );
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", COMP_BRANCH]),
        rejected_head
    );
}

#[test]
fn dirty_and_wrong_root_are_refused() {
    let dirty = Fixture::new("dirty repo", ".mine/worktrees");
    std::fs::write(dirty.root.join("dirty.txt"), "dirty\n").unwrap();
    let before = dirty.snapshot();
    let outcome = prepare(&dirty.root, BASE_ID, graph_revision(&dirty.root));
    dirty.assert_refusal(&outcome, &before);

    let wrong = Fixture::new("wrong repo", ".mine/worktrees");
    git(&wrong.root, &["switch", "master"]);
    let before = wrong.snapshot();
    let outcome = prepare(&wrong.root, BASE_ID, graph_revision(&wrong.root));
    wrong.assert_refusal(&outcome, &before);
}

#[test]
fn occupied_or_foreign_registered_target_is_refused() {
    let occupied = Fixture::new("occupied", ".mine/worktrees");
    std::fs::create_dir_all(occupied.target()).unwrap();
    std::fs::write(occupied.target().join("owned.txt"), "user\n").unwrap();
    let before = occupied.snapshot();
    let outcome = prepare(&occupied.root, BASE_ID, graph_revision(&occupied.root));
    occupied.assert_refusal(&outcome, &before);
    assert_eq!(
        std::fs::read_to_string(occupied.target().join("owned.txt")).unwrap(),
        "user\n"
    );

    let foreign = Fixture::new("foreign", ".mine/worktrees");
    add_new_worktree(&foreign.root, &foreign.target(), "foreign-branch");
    let before = foreign.snapshot();
    let outcome = prepare(&foreign.root, BASE_ID, graph_revision(&foreign.root));
    foreign.assert_refusal(&outcome, &before);
    assert_eq!(
        git_output(&foreign.target(), &["branch", "--show-current"]),
        "foreign-branch"
    );
}

#[test]
fn locked_prunable_and_elsewhere_branch_registrations_are_refused() {
    let locked = Fixture::new("locked", ".mine/worktrees");
    let other = locked._tmp.path().join("locked-other");
    add_new_worktree(&locked.root, &other, "locked-branch");
    git_path(&locked.root, &["worktree", "lock"], &other);
    let before = locked.snapshot();
    let outcome = prepare(&locked.root, BASE_ID, graph_revision(&locked.root));
    locked.assert_refusal(&outcome, &before);

    let prunable = Fixture::new("prunable", ".mine/worktrees");
    let missing = prunable._tmp.path().join("missing-other");
    add_new_worktree(&prunable.root, &missing, "missing-branch");
    std::fs::remove_dir_all(&missing).unwrap();
    let before = prunable.snapshot();
    let outcome = prepare(&prunable.root, BASE_ID, graph_revision(&prunable.root));
    prunable.assert_refusal(&outcome, &before);

    let elsewhere = Fixture::new("elsewhere", ".mine/worktrees");
    let expected = git_output(&elsewhere.root, &["rev-parse", "HEAD"]);
    git(&elsewhere.root, &["branch", BASE_BRANCH, &expected]);
    let other = elsewhere._tmp.path().join("elsewhere-other");
    add_existing_worktree(&elsewhere.root, &other, BASE_BRANCH);
    let before = elsewhere.snapshot();
    let outcome = prepare(&elsewhere.root, BASE_ID, graph_revision(&elsewhere.root));
    elsewhere.assert_refusal(&outcome, &before);
    assert!(!elsewhere.target().exists());
}

#[test]
fn dirty_compensation_worktree_and_malformed_declaration_are_refused() {
    let dirty = Fixture::new("dirty-comp", ".mine/worktrees");
    dirty.prepare_base(graph_revision(&dirty.root));
    dirty.register_compensation();
    std::fs::write(dirty.target().join("dirty.txt"), "dirty\n").unwrap();
    let before = dirty.snapshot();
    let outcome = prepare(&dirty.root, COMP_ID, graph_revision(&dirty.root));
    dirty.assert_refusal(&outcome, &before);

    let malformed = Fixture::new("malformed", ".mine/worktrees");
    malformed.prepare_base(graph_revision(&malformed.root));
    malformed.register_compensation();
    let graph_path = malformed.root.join(".mine/plan/execution-graph.toml");
    let graph = std::fs::read_to_string(&graph_path).unwrap().replace(
        // mine-release-allow-plan-reference: malformed compensation graph protocol fixture
        "compensating_plan = \"plan01-C01\"",
        // mine-release-allow-plan-reference: malformed compensation graph protocol fixture
        "compensating_plan = \"plan01-C02\"",
    );
    std::fs::write(&graph_path, graph).unwrap();
    clean_commit(&malformed.root, "malform compensation declaration");
    let before = malformed.snapshot();
    let outcome = prepare(&malformed.root, COMP_ID, graph_revision(&malformed.root));
    malformed.assert_refusal(&outcome, &before);
}

#[test]
fn spaces_non_ascii_and_two_lineages_prepare_independently() {
    let fixture = Fixture::new("repo space-测试", ".mine/worktrees");
    std::fs::write(fixture.root.join(OTHER_PATH), "# other\n").unwrap();
    register_plan(
        &fixture.root,
        OTHER_ID,
        OTHER_PATH,
        "tests/",
        graph_revision(&fixture.root),
    );
    let first = fixture.prepare_base(graph_revision(&fixture.root));
    assert_eq!(first["data"]["plan"]["action"], "created");
    let second = prepare(&fixture.root, OTHER_ID, graph_revision(&fixture.root));
    let body = envelope(&second);
    assert_eq!(second.exit_code, 0, "{body}");
    assert_ne!(
        first["data"]["plan"]["resolved_worktree_path"],
        body["data"]["plan"]["resolved_worktree_path"]
    );
}

#[test]
fn non_directory_worktree_root_is_refused_without_mutation() {
    let fixture = Fixture::new("add-failure", ".mine/worktrees");
    std::fs::write(fixture.root.join(".mine/worktrees"), "not a directory\n").unwrap();
    let journal = fixture.root.join(".mine/runtime/operations");
    assert!(!journal.exists());
    let view = fixture.root.join(".mine/plan/execution-graph.md");
    let view_before = std::fs::read(&view).unwrap();
    let index = fixture.root.join(git_output(
        &fixture.root,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    ));
    let index_before = std::fs::read(&index).unwrap();
    let refs_before = git_output(&fixture.root, &["for-each-ref"]);
    let before = fixture.snapshot();
    let outcome = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
    assert_eq!(outcome.exit_code, 4, "{}", envelope(&outcome));
    assert!(
        envelope(&outcome)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("locator ancestor is not a directory")
    );
    fixture.assert_refusal(&outcome, &before);
    assert!(!fixture.target().exists());
    assert!(!git_branch_exists(&fixture.root, BASE_BRANCH));
    assert!(!journal.exists());
    assert_eq!(std::fs::read(&view).unwrap(), view_before);
    assert_eq!(std::fs::read(&index).unwrap(), index_before);
    assert_eq!(git_output(&fixture.root, &["for-each-ref"]), refs_before);
    assert_eq!(
        std::fs::read_to_string(fixture.root.join(".mine/worktrees")).unwrap(),
        "not a directory\n"
    );
}

#[cfg(windows)]
#[test]
fn junction_worktree_root_outside_repository_is_refused_before_mutation() {
    let fixture = Fixture::new("junction-repo", ".mine/wtlink");
    let external = fixture._tmp.path().join("external-junction-target");
    std::fs::create_dir_all(&external).unwrap();
    let link = fixture.root.join(".mine/wtlink");
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "New-Item -ItemType Junction -Path $env:MINE_TEST_LINK -Target $env:MINE_TEST_TARGET | Out-Null",
        ])
        .env("MINE_TEST_LINK", &link)
        .env("MINE_TEST_TARGET", &external)
        .status()
        .unwrap();
    assert!(status.success());
    let before = fixture.snapshot();
    let outcome = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
    fixture.assert_refusal(&outcome, &before);
    assert!(!external.join(BASE_ID).exists());
    assert!(!git_branch_exists(&fixture.root, BASE_BRANCH));
}

#[cfg(unix)]
#[test]
fn symlink_worktree_root_outside_repository_is_refused_before_mutation() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new("symlink-repo", ".mine/wtlink");
    let external = fixture._tmp.path().join("external-symlink-target");
    std::fs::create_dir_all(&external).unwrap();
    symlink(&external, fixture.root.join(".mine/wtlink")).unwrap();
    let before = fixture.snapshot();
    let outcome = prepare(&fixture.root, BASE_ID, graph_revision(&fixture.root));
    fixture.assert_refusal(&outcome, &before);
    assert!(!external.join(BASE_ID).exists());
    assert!(!git_branch_exists(&fixture.root, BASE_BRANCH));
}

fn prepare(root: &Path, id: &str, expected: u64) -> cli::Outcome {
    dispatch(
        root,
        &[
            "plan",
            "prepare",
            "--id",
            id,
            "--actor",
            "fixture-preparer",
            "--expected-revision",
            &expected.to_string(),
            "--operation-id",
            &uuid::Uuid::new_v4().to_string(),
        ],
    )
}

fn add_plan(root: &Path, id: &str, path: &str, write_scope: &str, expected: u64) -> u64 {
    // A previously pinned compensation body is byte-identical. Never rewrite
    // it just to manufacture a fixture commit or invalidate its handoff.
    let content = format!("# {id}\n");
    if std::fs::read(root.join(path)).ok().as_deref() != Some(content.as_bytes()) {
        std::fs::write(root.join(path), content).unwrap();
        git(root, &["add", "--", path]);
        git(
            root,
            &["commit", "--quiet", "-m", "private Plan registration body"],
        );
    }
    let outcome = dispatch(
        root,
        &[
            "plan",
            "add",
            "--id",
            id,
            "--path",
            path,
            "--title",
            "fixture",
            "--design-ref",
            ".mine/design/index.md",
            "--write",
            write_scope,
            "--actor",
            "fixture-planner",
            "--expected-revision",
            &expected.to_string(),
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{}", envelope(&outcome));
    assert_eq!(envelope(&outcome)["revision_after"], expected + 1);
    expected + 1
}

fn register_plan(root: &Path, id: &str, path: &str, write_scope: &str, expected: u64) {
    let revision = add_plan(root, id, path, write_scope, expected);
    let released = dispatch(
        root,
        &[
            "plan",
            "release",
            "--id",
            id,
            "--actor",
            "fixture-planner",
            "--expected-revision",
            &revision.to_string(),
        ],
    );
    assert_eq!(released.exit_code, 0, "{}", envelope(&released));
}

fn dispatch(root: &Path, args: &[&str]) -> cli::Outcome {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        root.display().to_string(),
    ];
    argv.extend(args.iter().map(ToString::to_string));
    let tracing = std::env::var_os("MINE_TEST_REQUEST_SPANS").is_some();
    let sequence = if tracing {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    } else {
        0
    };
    let unix_us = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros()
    };
    if tracing {
        eprintln!(
            "{}",
            serde_json::json!({"event":"test_request_begin","sequence":sequence,
                "unix_us":unix_us(),"root":root,"argv":args})
        );
    }
    let began = std::time::Instant::now();
    let outcome = cli::dispatch(&argv, "mine");
    if tracing {
        let value = envelope(&outcome);
        eprintln!(
            "{}",
            serde_json::json!({"event":"test_request_end","sequence":sequence,
                "unix_us":unix_us(),"elapsed_us":began.elapsed().as_micros(),
                "exit_code":outcome.exit_code,"error":value["error"]["code"],
                "checkpoint_stage":value["error"]["details"]["operation"]["stage"]})
        );
    }
    outcome
}

fn envelope(outcome: &cli::Outcome) -> serde_json::Value {
    let (stdout, stderr) = cli::render(outcome, true, false);
    serde_json::from_str(if outcome.exit_code == 0 {
        &stdout
    } else {
        &stderr
    })
    .unwrap()
}

fn graph_revision(root: &Path) -> u64 {
    let outcome = dispatch(root, &["graph", "status", "--format", "json"]);
    let body = envelope(&outcome);
    body["data"]["revision"].as_u64().unwrap_or_else(|| {
        toml::from_str::<toml::Value>(&String::from_utf8(graph_bytes(root)).unwrap()).unwrap()
            ["revision"]
            .as_integer()
            .unwrap() as u64
    })
}

fn graph_bytes(root: &Path) -> Vec<u8> {
    let path = root.join(".mine/plan/execution-graph.toml");
    if path.exists() {
        std::fs::read(path).unwrap()
    } else {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["show", "dev:.mine/plan/execution-graph.toml"])
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    }
}

fn clean_commit(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "--quiet", "-m", message]);
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn git_output(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn add_new_worktree(root: &Path, path: &Path, branch: &str) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["worktree", "add", "--quiet", "-b", branch])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

fn add_existing_worktree(root: &Path, path: &Path, branch: &str) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["worktree", "add", "--quiet"])
        .arg(path)
        .arg(branch)
        .status()
        .unwrap();
    assert!(status.success());
}

fn git_path(root: &Path, args: &[&str], path: &Path) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

fn git_branch_exists(root: &Path, branch: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .status()
        .unwrap()
        .success()
}

fn git_is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .status()
        .unwrap()
        .success()
}

/// Preserve the exact checkout tree/body/index but replace its ancestry. This
/// isolates the immutable-anchor gate from dirty-checkout or missing-body gates.
fn unrelated_same_tree(worktree: &Path) -> String {
    let tree = git_output(worktree, &["rev-parse", "HEAD^{tree}"]);
    let orphan = git_output(
        worktree,
        &[
            "commit-tree",
            &tree,
            "-m",
            "private unrelated same-tree history",
        ],
    );
    assert_eq!(
        git_output(worktree, &["rev-list", "--parents", "-n", "1", &orphan]),
        orphan
    );
    assert_eq!(
        git_output(worktree, &["rev-parse", &format!("{orphan}^{{tree}}")]),
        tree
    );
    orphan
}
