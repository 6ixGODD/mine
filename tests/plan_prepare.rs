#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use mine::cli;

// mine-release-allow-plan-reference: canonical base-id protocol fixture
const BASE_ID: &str = "plan01";
// mine-release-allow-plan-reference: canonical base-plan path protocol fixture
const BASE_PATH: &str = "docs/plan/plan01-fixture.md";
// mine-release-allow-plan-reference: canonical base-branch protocol fixture
const BASE_BRANCH: &str = "plan/plan01-fixture";
// mine-release-allow-plan-reference: canonical compensation-id protocol fixture
const COMP_ID: &str = "plan01-C01";
// mine-release-allow-plan-reference: canonical compensation path protocol fixture
const COMP_PATH: &str = "docs/plan/plan01-C01-fixture.md";
// mine-release-allow-plan-reference: canonical compensation branch protocol fixture
const COMP_BRANCH: &str = "plan/plan01-C01-fixture";
// mine-release-allow-plan-reference: canonical second-lineage id protocol fixture
const OTHER_ID: &str = "plan02";
// mine-release-allow-plan-reference: canonical second-lineage path protocol fixture
const OTHER_PATH: &str = "docs/plan/plan02-other.md";

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
        git(&root, &["init", "--quiet"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "user.name", "test"]);
        std::fs::write(root.join("README.md"), "fixture\n").unwrap();
        std::fs::write(root.join(".gitignore"), "/.mine/\n").unwrap();
        clean_commit(&root, "initial");

        assert_eq!(dispatch(&root, &["init"]).exit_code, 0);
        if worktrees_root != ".mine/worktrees" {
            let config_path = root.join(".mine/config.toml");
            let config = std::fs::read_to_string(&config_path).unwrap().replace(
                "worktrees_root = \".mine/worktrees\"",
                &format!("worktrees_root = \"{worktrees_root}\""),
            );
            std::fs::write(config_path, config).unwrap();
        }
        clean_commit(&root, "mine init");
        git(&root, &["switch", "-c", "dev"]);
        assert_eq!(dispatch(&root, &["workspace", "open"]).exit_code, 0);
        register_plan(&root, BASE_ID, BASE_PATH, "src/");
        clean_commit(&root, "register base plan");
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

    fn prepare_base(&self) -> serde_json::Value {
        let outcome = dispatch(
            &self.root,
            &["plan", "prepare", "--id", BASE_ID, "--format", "json"],
        );
        let body = envelope(&outcome);
        assert_eq!(outcome.exit_code, 0, "{body}");
        body
    }

    fn start_and_commit_graph(&self, id: &str) -> String {
        let outcome = dispatch(
            &self.root,
            &["plan", "start", "--id", id, "--format", "json"],
        );
        let body = envelope(&outcome);
        assert_eq!(outcome.exit_code, 0, "{body}");
        let anchor = body["data"]["branch_anchor"]
            .as_str()
            .expect("start returns branch_anchor")
            .to_string();
        clean_commit(&self.root, &format!("start {id}"));
        anchor
    }

    fn commit_plan_change(&self, name: &str) -> String {
        std::fs::write(self.target().join(name), "plan change\n").unwrap();
        clean_commit(&self.target(), &format!("plan change {name}"));
        git_output(&self.target(), &["rev-parse", "HEAD"])
    }

    fn mark_implemented(&self, id: &str, commit: &str) {
        std::fs::create_dir_all(self.root.join("docs/plan/reports")).unwrap();
        let report = match id {
            BASE_ID => {
                // mine-release-allow-plan-reference: canonical report-path protocol fixture
                "docs/plan/reports/plan01-fixture-implementation.md"
            }
            COMP_ID => {
                // mine-release-allow-plan-reference: canonical report-path protocol fixture
                "docs/plan/reports/plan01-C01-fixture-implementation.md"
            }
            _ => panic!("unexpected fixture id {id}"),
        };
        std::fs::write(self.root.join(report), "fixture implementation evidence\n").unwrap();
        let outcome = dispatch(
            &self.root,
            &[
                "plan",
                "implemented",
                "--id",
                id,
                "--report",
                report,
                "--commit",
                commit,
            ],
        );
        assert_eq!(outcome.exit_code, 0, "{}", envelope(&outcome));
        clean_commit(&self.root, &format!("mark {id} implemented"));
    }

    fn reject_base_and_register_compensation(&self) {
        std::fs::create_dir_all(self.root.join("docs/plan/reports")).unwrap();
        // mine-release-allow-plan-reference: canonical report-path protocol fixture
        let review = "docs/plan/reports/plan01-fixture-review.md";
        std::fs::write(self.root.join(review), "independent review\n").unwrap();
        let outcome = dispatch(
            &self.root,
            &[
                "plan",
                "reject",
                "--id",
                BASE_ID,
                "--reason",
                "fixture",
                "--review-report",
                review,
                "--compensating-plan",
                COMP_ID,
            ],
        );
        assert_eq!(outcome.exit_code, 0, "{}", envelope(&outcome));
        register_plan(&self.root, COMP_ID, COMP_PATH, "src/");
        clean_commit(&self.root, "register compensation");
    }

    fn register_compensation(&self) {
        self.start_and_commit_graph(BASE_ID);
        let base_commit = git_output(&self.root, &["rev-parse", BASE_BRANCH]);
        self.mark_implemented(BASE_ID, &base_commit);
        self.reject_base_and_register_compensation();
    }
}

#[test]
fn base_create_and_exact_reuse_keep_root_unchanged() {
    let fixture = Fixture::new("repo", ".mine/worktrees");
    let before = fixture.snapshot();
    let first = fixture.prepare_base();
    assert_eq!(first["data"]["plan"]["action"], "created");
    assert_eq!(
        first["data"]["plan"]["observed_worktree"]["branch"],
        BASE_BRANCH
    );
    let second = fixture.prepare_base();
    assert_eq!(second["data"]["plan"]["action"], "reused");
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", "HEAD"]),
        before.head
    );
    assert_eq!(graph_bytes(&fixture.root), before.graph);
}

#[test]
fn in_progress_base_reuses_committed_descendant_after_root_graph_commit() {
    let fixture = Fixture::new("active-base", ".mine/worktrees");
    let created = fixture.prepare_base();
    assert_eq!(created["data"]["plan"]["action"], "created");
    let creation_anchor = git_output(&fixture.root, &["rev-parse", BASE_BRANCH]);

    let stored_anchor = fixture.start_and_commit_graph(BASE_ID);
    assert_eq!(stored_anchor, creation_anchor);
    let tip = fixture.commit_plan_change("execution.txt");
    assert!(git_is_ancestor(&fixture.root, &creation_anchor, &tip));
    let before = fixture.snapshot();

    let outcome = dispatch(
        &fixture.root,
        &["plan", "prepare", "--id", BASE_ID, "--format", "json"],
    );
    let body = envelope(&outcome);
    assert_eq!(outcome.exit_code, 0, "{body}");
    assert_eq!(body["data"]["plan"]["action"], "reused");
    assert_eq!(body["data"]["plan"]["observed_worktree"]["head"], tip);
    assert_eq!(graph_bytes(&fixture.root), before.graph);
    assert_eq!(graph_revision(&fixture.root), before.revision);

    std::fs::create_dir_all(fixture.root.join("docs/plan/reports")).unwrap();
    // mine-release-allow-plan-reference: canonical report-path protocol fixture
    let report = "docs/plan/reports/plan01-fixture-implementation.md";
    std::fs::write(fixture.root.join(report), "evidence\n").unwrap();
    let implemented = dispatch(
        &fixture.root,
        &[
            "plan",
            "implemented",
            "--id",
            BASE_ID,
            "--report",
            report,
            "--commit",
            &tip,
        ],
    );
    assert_eq!(implemented.exit_code, 0, "{}", envelope(&implemented));
    clean_commit(&fixture.root, "mark implemented");
    let review_reuse = dispatch(&fixture.root, &["plan", "prepare", "--id", BASE_ID]);
    assert_eq!(review_reuse.exit_code, 0, "{}", envelope(&review_reuse));
}

#[test]
fn active_compensation_reuses_implementation_and_report_descendants() {
    let fixture = Fixture::new("active-compensation", ".mine/worktrees");
    fixture.prepare_base();
    fixture.start_and_commit_graph(BASE_ID);
    fixture.commit_plan_change("base-implementation.txt");
    let rejected_tip = fixture.commit_plan_change("base-report.txt");
    fixture.mark_implemented(BASE_ID, &rejected_tip);
    fixture.reject_base_and_register_compensation();

    let switched = dispatch(
        &fixture.root,
        &["plan", "prepare", "--id", COMP_ID, "--format", "json"],
    );
    let switched_body = envelope(&switched);
    assert_eq!(switched.exit_code, 0, "{switched_body}");
    assert_eq!(switched_body["data"]["plan"]["action"], "switched");
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", COMP_BRANCH]),
        rejected_tip
    );

    let stored_anchor = fixture.start_and_commit_graph(COMP_ID);
    assert_eq!(stored_anchor, rejected_tip);
    fixture.commit_plan_change("compensation-implementation.txt");
    let compensation_tip = fixture.commit_plan_change("compensation-report.txt");
    assert!(git_is_ancestor(
        &fixture.root,
        &rejected_tip,
        &compensation_tip
    ));
    let before = fixture.snapshot();

    let active_reuse = dispatch(
        &fixture.root,
        &["plan", "prepare", "--id", COMP_ID, "--format", "json"],
    );
    let active_body = envelope(&active_reuse);
    assert_eq!(active_reuse.exit_code, 0, "{active_body}");
    assert_eq!(active_body["data"]["plan"]["action"], "reused");
    assert_eq!(
        active_body["data"]["plan"]["observed_worktree"]["head"],
        compensation_tip
    );
    assert_eq!(graph_bytes(&fixture.root), before.graph);
    assert_eq!(graph_revision(&fixture.root), before.revision);

    fixture.mark_implemented(COMP_ID, &compensation_tip);
    let review_reuse = dispatch(&fixture.root, &["plan", "prepare", "--id", COMP_ID]);
    assert_eq!(review_reuse.exit_code, 0, "{}", envelope(&review_reuse));
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        rejected_tip
    );
}

#[test]
fn ready_attached_branch_advanced_before_start_is_refused_without_mutation() {
    let fixture = Fixture::new("ready-advanced", ".mine/worktrees");
    fixture.prepare_base();
    let advanced = fixture.commit_plan_change("premature.txt");
    let before = fixture.snapshot();

    let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", BASE_ID]);
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
        fixture.prepare_base();
        let (id, branch, anchor) = if kind == "compensation" {
            fixture.register_compensation();
            let prepared = dispatch(&fixture.root, &["plan", "prepare", "--id", COMP_ID]);
            assert_eq!(prepared.exit_code, 0, "{}", envelope(&prepared));
            (
                COMP_ID,
                COMP_BRANCH,
                fixture.start_and_commit_graph(COMP_ID),
            )
        } else {
            (
                BASE_ID,
                BASE_BRANCH,
                fixture.start_and_commit_graph(BASE_ID),
            )
        };
        let legitimate_tip = fixture.commit_plan_change("legitimate.txt");
        if implemented {
            fixture.mark_implemented(id, &legitimate_tip);
        }
        let unrelated = git_output(&fixture.root, &["rev-parse", "master"]);
        assert!(!git_is_ancestor(&fixture.root, &anchor, &unrelated));
        git(
            &fixture.root,
            &["update-ref", &format!("refs/heads/{branch}"), &unrelated],
        );
        let before = fixture.snapshot();

        let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", id]);
        fixture.assert_refusal(&outcome, &before);
        assert_eq!(git_output(&fixture.root, &["rev-parse", branch]), unrelated);
    }
}

#[test]
fn active_prepare_refuses_missing_invalid_and_unrelated_anchors_without_mutation() {
    for replacement in ["", "not-a-commit", "ROOT_HEAD"] {
        let fixture = Fixture::new(&format!("bad-anchor-{replacement}"), ".mine/worktrees");
        fixture.prepare_base();
        let stored = fixture.start_and_commit_graph(BASE_ID);
        let unrelated = git_output(&fixture.root, &["rev-parse", "HEAD"]);
        let graph_path = fixture.root.join("docs/plan/execution-graph.toml");
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
        let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", BASE_ID]);
        fixture.assert_refusal(&outcome, &before);
    }
}

#[test]
fn active_prepare_refuses_branch_rewritten_before_stored_anchor() {
    let fixture = Fixture::new("rewritten-branch", ".mine/worktrees");
    fixture.prepare_base();
    fixture.start_and_commit_graph(BASE_ID);
    let unrelated = git_output(&fixture.root, &["rev-parse", "HEAD~2"]);
    git(
        &fixture.root,
        &[
            "update-ref",
            &format!("refs/heads/{BASE_BRANCH}"),
            &unrelated,
        ],
    );
    let before = fixture.snapshot();
    let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", BASE_ID]);
    fixture.assert_refusal(&outcome, &before);
    assert_eq!(
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH]),
        unrelated
    );
}

#[test]
fn active_rejection_records_review_without_fake_implementation_evidence() {
    let fixture = Fixture::new("active-rejection", ".mine/worktrees");
    fixture.prepare_base();
    fixture.start_and_commit_graph(BASE_ID);
    let branch_tip = git_output(&fixture.root, &["rev-parse", BASE_BRANCH]);
    // mine-release-allow-plan-reference: canonical report-path protocol fixture
    let review = "docs/plan/reports/plan01-fixture-review.md";
    std::fs::create_dir_all(fixture.root.join("docs/plan/reports")).unwrap();
    std::fs::write(
        fixture.root.join(review),
        "independent infeasibility review\n",
    )
    .unwrap();

    let outcome = dispatch(
        &fixture.root,
        &[
            "plan",
            "reject",
            "--id",
            BASE_ID,
            "--reason",
            "immutable scope is infeasible",
            "--review-report",
            review,
            "--compensating-plan",
            COMP_ID,
            "--format",
            "json",
        ],
    );
    let body = envelope(&outcome);
    assert_eq!(outcome.exit_code, 0, "{body}");
    assert_eq!(body["data"]["review_report"], review);
    let graph: mine::domain::graph::PlanWorkspace = toml::from_str(
        &std::fs::read_to_string(fixture.root.join("docs/plan/execution-graph.toml")).unwrap(),
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
        ("missing", "docs/plan/reports/plan01-fixture-review.md"),
        ("escaped", "../outside-review.md"),
        ("wrong", "docs/plan/reports/wrong-review.md"),
        // mine-release-allow-plan-reference: canonical report-path protocol fixture
        ("directory", "docs/plan/reports/plan01-fixture-review.md"),
    ];
    for (name, review) in cases {
        let fixture = Fixture::new(&format!("reject-{name}"), ".mine/worktrees");
        fixture.prepare_base();
        fixture.start_and_commit_graph(BASE_ID);
        if name == "wrong" {
            std::fs::create_dir_all(fixture.root.join("docs/plan/reports")).unwrap();
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
    fixture.prepare_base();
    fixture.start_and_commit_graph(BASE_ID);
    let before = fixture.snapshot();
    let outcome = dispatch(
        &fixture.root,
        &[
            "plan",
            "reject",
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
    let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", BASE_ID]);
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
    fixture.prepare_base();
    fixture.register_compensation();
    let stale = git_output(&fixture.root, &["rev-parse", "HEAD"]);
    assert_ne!(
        stale,
        git_output(&fixture.root, &["rev-parse", BASE_BRANCH])
    );
    git(&fixture.root, &["branch", COMP_BRANCH, &stale]);
    let before = fixture.snapshot();
    let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", COMP_ID]);
    fixture.assert_refusal(&outcome, &before);
    assert_eq!(
        git_output(&fixture.target(), &["branch", "--show-current"]),
        BASE_BRANCH
    );
}

#[test]
fn valid_compensation_switches_one_lineage_and_preserves_rejected_ref() {
    let fixture = Fixture::new("repo", ".mine/worktrees");
    fixture.prepare_base();
    let rejected_head = git_output(&fixture.root, &["rev-parse", BASE_BRANCH]);
    fixture.register_compensation();
    let result = dispatch(
        &fixture.root,
        &["plan", "prepare", "--id", COMP_ID, "--format", "json"],
    );
    let body = envelope(&result);
    assert_eq!(result.exit_code, 0, "{body}");
    assert_eq!(body["data"]["plan"]["action"], "switched");
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
    let outcome = dispatch(&dirty.root, &["plan", "prepare", "--id", BASE_ID]);
    dirty.assert_refusal(&outcome, &before);

    let wrong = Fixture::new("wrong repo", ".mine/worktrees");
    git(&wrong.root, &["switch", "master"]);
    let before = wrong.snapshot();
    let outcome = dispatch(&wrong.root, &["plan", "prepare", "--id", BASE_ID]);
    wrong.assert_refusal(&outcome, &before);
}

#[test]
fn occupied_or_foreign_registered_target_is_refused() {
    let occupied = Fixture::new("occupied", ".mine/worktrees");
    std::fs::create_dir_all(occupied.target()).unwrap();
    std::fs::write(occupied.target().join("owned.txt"), "user\n").unwrap();
    let before = occupied.snapshot();
    let outcome = dispatch(&occupied.root, &["plan", "prepare", "--id", BASE_ID]);
    occupied.assert_refusal(&outcome, &before);
    assert_eq!(
        std::fs::read_to_string(occupied.target().join("owned.txt")).unwrap(),
        "user\n"
    );

    let foreign = Fixture::new("foreign", ".mine/worktrees");
    add_new_worktree(&foreign.root, &foreign.target(), "foreign-branch");
    let before = foreign.snapshot();
    let outcome = dispatch(&foreign.root, &["plan", "prepare", "--id", BASE_ID]);
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
    let outcome = dispatch(&locked.root, &["plan", "prepare", "--id", BASE_ID]);
    locked.assert_refusal(&outcome, &before);

    let prunable = Fixture::new("prunable", ".mine/worktrees");
    let missing = prunable._tmp.path().join("missing-other");
    add_new_worktree(&prunable.root, &missing, "missing-branch");
    std::fs::remove_dir_all(&missing).unwrap();
    let before = prunable.snapshot();
    let outcome = dispatch(&prunable.root, &["plan", "prepare", "--id", BASE_ID]);
    prunable.assert_refusal(&outcome, &before);

    let elsewhere = Fixture::new("elsewhere", ".mine/worktrees");
    let expected = git_output(&elsewhere.root, &["rev-parse", "HEAD"]);
    git(&elsewhere.root, &["branch", BASE_BRANCH, &expected]);
    let other = elsewhere._tmp.path().join("elsewhere-other");
    add_existing_worktree(&elsewhere.root, &other, BASE_BRANCH);
    let before = elsewhere.snapshot();
    let outcome = dispatch(&elsewhere.root, &["plan", "prepare", "--id", BASE_ID]);
    elsewhere.assert_refusal(&outcome, &before);
    assert!(!elsewhere.target().exists());
}

#[test]
fn dirty_compensation_worktree_and_malformed_declaration_are_refused() {
    let dirty = Fixture::new("dirty-comp", ".mine/worktrees");
    dirty.prepare_base();
    dirty.register_compensation();
    std::fs::write(dirty.target().join("dirty.txt"), "dirty\n").unwrap();
    let before = dirty.snapshot();
    let outcome = dispatch(&dirty.root, &["plan", "prepare", "--id", COMP_ID]);
    dirty.assert_refusal(&outcome, &before);

    let malformed = Fixture::new("malformed", ".mine/worktrees");
    malformed.prepare_base();
    malformed.register_compensation();
    let graph_path = malformed.root.join("docs/plan/execution-graph.toml");
    let graph = std::fs::read_to_string(&graph_path).unwrap().replace(
        // mine-release-allow-plan-reference: malformed compensation graph protocol fixture
        "compensating_plan = \"plan01-C01\"",
        // mine-release-allow-plan-reference: malformed compensation graph protocol fixture
        "compensating_plan = \"plan01-C02\"",
    );
    std::fs::write(&graph_path, graph).unwrap();
    clean_commit(&malformed.root, "malform compensation declaration");
    let before = malformed.snapshot();
    let outcome = dispatch(&malformed.root, &["plan", "prepare", "--id", COMP_ID]);
    malformed.assert_refusal(&outcome, &before);
}

#[test]
fn spaces_non_ascii_and_two_lineages_prepare_independently() {
    let fixture = Fixture::new("repo space-测试", ".mine/worktrees");
    std::fs::write(fixture.root.join(OTHER_PATH), "# other\n").unwrap();
    register_plan(&fixture.root, OTHER_ID, OTHER_PATH, "tests/");
    clean_commit(&fixture.root, "register second lineage");
    let first = fixture.prepare_base();
    assert_eq!(first["data"]["plan"]["action"], "created");
    let second = dispatch(
        &fixture.root,
        &["plan", "prepare", "--id", OTHER_ID, "--format", "json"],
    );
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
    let before = fixture.snapshot();
    let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", BASE_ID]);
    fixture.assert_refusal(&outcome, &before);
    assert!(!fixture.target().exists());
    assert!(!git_branch_exists(&fixture.root, BASE_BRANCH));
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
    let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", BASE_ID]);
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
    let outcome = dispatch(&fixture.root, &["plan", "prepare", "--id", BASE_ID]);
    fixture.assert_refusal(&outcome, &before);
    assert!(!external.join(BASE_ID).exists());
    assert!(!git_branch_exists(&fixture.root, BASE_BRANCH));
}

fn register_plan(root: &Path, id: &str, path: &str, write_scope: &str) {
    std::fs::write(root.join(path), format!("# {id}\n")).unwrap();
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
            "docs/design/index.md",
            "--write",
            write_scope,
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{}", envelope(&outcome));
    let release = dispatch(root, &["plan", "release", "--id", id]);
    assert_eq!(release.exit_code, 0, "{}", envelope(&release));
}

fn dispatch(root: &Path, args: &[&str]) -> cli::Outcome {
    let mut argv = vec![
        "mine".to_string(),
        "--repo".to_string(),
        root.display().to_string(),
    ];
    argv.extend(args.iter().map(ToString::to_string));
    cli::dispatch(&argv, "mine")
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
    let path = root.join("docs/plan/execution-graph.toml");
    if path.exists() {
        std::fs::read(path).unwrap()
    } else {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["show", "dev:docs/plan/execution-graph.toml"])
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
