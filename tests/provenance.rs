#![forbid(unsafe_code)]

#[path = "support/config.rs"]
mod fixture_config;

use std::path::{Path, PathBuf};
use std::process::Command;

use mine::application::graph_service::{
    GraphService, PlanAcceptRequest, PlanAddRequest, PlanImplementedRequest, PlanRejectRequest,
};
use mine::application::plan_service::{PlanReleaseRequest, PlanRewireRequest, PlanService};
use mine::cli;
use mine::cli::context::{GlobalOpts, OutputFormat, build_context};
use mine::domain::graph::{
    ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION, CURRENT_GRAPH_SCHEMA_VERSION, LEGACY_GRAPH_SCHEMA_VERSION,
    PREVIOUS_GRAPH_SCHEMA_VERSION, PlanNode, PlanWorkspace,
};
use mine::domain::plan_id::{CanonicalPlanId, registered_plan_branch};
use mine::domain::status::PlanStatus;
use mine::domain::validation;
use mine::infrastructure::toml_store::TomlStore;

const LIVE_WORKSPACE_ID: &str = "efaa93da-09ca-4888-a411-f896a151ce6d";
const PRIVATE_IGNORE_RULES: &str =
    "/.mine/runtime/\n/.mine/locks/\n/.mine/build/\n/.mine/worktrees/\n";

type WorkspaceMutation = fn(&mut PlanWorkspace);

#[allow(dead_code)]
#[derive(serde::Deserialize)]
struct OldV2Workspace {
    schema_version: String,
    revision: u64,
    project_id: String,
    workspace_id: String,
    stable_branch: String,
    integration_branch: String,
    stable_baseline_commit: String,
    design_root: String,
    ephemeral_workspace: bool,
    purge_before_stable_release: bool,
    plans: Vec<OldV2PlanNode>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OldV2PlanNode {
    id: String,
    path: String,
    title: String,
    branch: String,
    status: PlanStatus,
    hard_predecessors: Vec<String>,
    soft_predecessors: Vec<String>,
    design_references: Vec<String>,
    exclusive_write_paths: Vec<String>,
    read_only_paths: Vec<String>,
    reserved_shared_paths: Vec<String>,
    implementation_report: String,
    review_report: String,
    implementation_commits: Vec<String>,
    owner: String,
    run_id: String,
    started_at: String,
    updated_at: String,
    rejection_reason: String,
    compensating_plan: String,
    lineage_id: String,
    worktree_path: String,
}

fn write_config(root: &Path) {
    if !root.join(".git").exists() {
        init_git_repo(root);
    }
    let has_head = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .unwrap()
        .status
        .success();
    if !has_head {
        commit_file(
            root,
            ".gitignore",
            PRIVATE_IGNORE_RULES,
            "private initial source",
        );
    }
    if !root.join(".mine/config.toml").exists() {
        // Explicit historical source layout is essential to the exact approved
        // bootstrap-path and v1/v2 repair cases below. This is seeded legacy
        // evidence, not fresh init, migration, or authority from live config.
        let mut config = fixture_config::canonical();
        config.design.root = "docs/design/index.md".into();
        config.design.marker = "docs/design/.mine-design.toml".into();
        config.plan.root = "docs/plan".into();
        config.graph.source = "docs/plan/execution-graph.toml".into();
        config.graph.rendered = "docs/plan/execution-graph.md".into();
        config.adr.registry = "docs/design/decisions/adr-registry.toml".into();
        config.adr.rendered_index = "docs/design/decisions/index.md".into();
        config.layout().unwrap();
        std::fs::create_dir_all(root.join(".mine")).unwrap();
        std::fs::create_dir_all(root.join("docs/design/decisions")).unwrap();
        std::fs::write(root.join(".mine/config.toml"), config.to_toml()).unwrap();
        std::fs::write(
            root.join("AGENTS.md"),
            "<!-- mine-managed-agents -->\n# Explicit historical provenance fixture\n\nDesign: docs/design/index.md\n",
        )
        .unwrap();
        std::fs::write(
            root.join("docs/design/index.md"),
            "# Legacy fixture Design\n",
        )
        .unwrap();
        std::fs::write(
            root.join("docs/design/.mine-design.toml"),
            mine::domain::design_marker::DesignMarker::new(
                config.repository_id,
                "2026-10-19T00:00:00Z".into(),
            )
            .to_toml(),
        )
        .unwrap();
        // This explicitly historical fixture freezes docs/ bodies, so its
        // committed byte policy must cover those paths as well as fresh .mine/.
        std::fs::write(
            root.join(".gitattributes"),
            "docs/design/** -text -filter -ident -working-tree-encoding\ndocs/plan/** -text -filter -ident -working-tree-encoding\n",
        )
        .unwrap();
        mine::infrastructure::governance_git::establish(root).unwrap();
        std::fs::write(
            root.join("docs/design/principles.md"),
            "# Private provenance fixture principles\n",
        )
        .unwrap();
        git(
            root,
            &[
                "add",
                "--",
                ".gitattributes",
                ".mine/config.toml",
                ".mine/.gitignore",
                ".mine/.gitattributes",
                "AGENTS.md",
                "docs/design",
            ],
        );
        git(
            root,
            &["commit", "--quiet", "-m", "private managed baseline"],
        );
    }
    if git(root, &["branch", "--show-current"]) == "master" {
        git(root, &["switch", "-c", "dev"]);
    }
    assert_eq!(git(root, &["branch", "--show-current"]), "dev");
}

fn legacy_store(root: &Path) -> TomlStore {
    let path = root.join(".mine/config.toml");
    let config =
        mine::domain::config::MineConfig::parse(&path, &std::fs::read_to_string(&path).unwrap())
            .unwrap();
    // Legacy data is never an alias for the fresh default layout.
    TomlStore::from_configuration(root, &config).unwrap()
}

fn canonical_node(id: &str, status: PlanStatus, hard: &[&str]) -> PlanNode {
    let path = format!("docs/plan/{id}-test.md");
    let parsed = CanonicalPlanId::parse(id).unwrap();
    PlanNode {
        id: id.to_string(),
        path: path.clone(),
        title: format!("Plan {id}"),
        branch: registered_plan_branch("test-workspace", id, &path).unwrap(),
        branch_anchor: if matches!(
            status,
            PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready
        ) {
            String::new()
        } else {
            "test-branch-anchor".to_string()
        },
        status,
        hard_predecessors: hard.iter().map(|value| (*value).to_string()).collect(),
        soft_predecessors: Vec::new(),
        design_references: vec!["docs/design/principles.md".to_string()],
        exclusive_write_paths: vec![format!("src/{id}/")],
        read_only_paths: Vec::new(),
        reserved_shared_paths: Vec::new(),
        implementation_report: String::new(),
        review_report: String::new(),
        implementation_commits: Vec::new(),
        owner: "owner".to_string(),
        run_id: "run".to_string(),
        started_at: "2026-08-21T00:00:00Z".to_string(),
        updated_at: "2026-08-21T00:00:00Z".to_string(),
        rejection_reason: String::new(),
        compensating_plan: String::new(),
        withdraw_reason: String::new(),
        withdrawn_at: String::new(),
        planning_scope: String::new(),
        readiness_checked_at: String::new(),
        adr_registry_revision: 0,
        lineage_id: parsed.lineage_id(),
        worktree_path: parsed.worktree_path(),
        body_sha256: None,
        ownership: None,
        ownership_policy: None,
        clarifications: vec![],
        acceptance: None,
        preparations: vec![],
        started_preparation_generation: None,
    }
}

fn workspace(revision: u64, plans: Vec<PlanNode>) -> PlanWorkspace {
    PlanWorkspace {
        schema_version: CURRENT_GRAPH_SCHEMA_VERSION,
        revision,
        project_id: "mine-is-not-everyones".to_string(),
        workspace_id: "test-workspace".to_string(),
        stable_branch: "master".to_string(),
        integration_branch: "dev".to_string(),
        stable_baseline_commit: "abc123".to_string(),
        design_root: "docs/design/index.md".to_string(),
        ephemeral_workspace: true,
        purge_before_stable_release: true,
        plans,
    }
}

fn compensation_repair_workspace(revision: u64, status: PlanStatus) -> PlanWorkspace {
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let accepted_a = canonical_node("plan01-01", PlanStatus::Accepted, &[]);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let accepted_b = canonical_node("plan01-02", PlanStatus::Accepted, &[]);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let accepted_extra = canonical_node("plan01-04", PlanStatus::Accepted, &[]);
    let mut rejected = canonical_node(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        "plan01-03",
        PlanStatus::Rejected,
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        &["plan01-01", "plan01-02"],
    );
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    rejected.compensating_plan = "plan01-03-C01".to_string();
    let mut compensation = canonical_node(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        "plan01-03-C01",
        status,
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        &["plan01-01", "plan01-02", "plan01-04"],
    );
    // The repair safety gate specifically requires absence of all execution
    // evidence. The shared canonical builder populates these fields for
    // started provenance tests, so this fixture deliberately clears them.
    compensation.branch_anchor.clear();
    compensation.owner.clear();
    compensation.run_id.clear();
    compensation.started_at.clear();
    workspace(
        revision,
        vec![
            accepted_a,
            accepted_b,
            accepted_extra,
            rejected,
            compensation,
        ],
    )
}

fn seed(root: &Path, workspace: &PlanWorkspace) {
    write_config(root);
    let mut workspace = workspace.clone();
    workspace.stable_baseline_commit = git(root, &["rev-parse", "master"]);
    if workspace
        .plans
        .iter()
        .any(|node| node.branch_anchor == "test-branch-anchor")
    {
        if !root.join(".git").exists() {
            init_git_repo(root);
        }
        let head = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--verify", "HEAD^{commit}"])
            .output()
            .unwrap();
        let anchor = if head.status.success() {
            String::from_utf8_lossy(&head.stdout).trim().to_string()
        } else {
            commit_file(root, ".anchor", "anchor\n", "anchor")
        };
        for node in &mut workspace.plans {
            if node.branch_anchor == "test-branch-anchor" {
                node.branch_anchor.clone_from(&anchor);
            }
        }
    }
    std::fs::create_dir_all(root.join("docs/plan")).unwrap();
    for node in &workspace.plans {
        std::fs::write(
            root.join(&node.path),
            format!("# Private {} fixture\n", node.id),
        )
        .unwrap();
    }
    std::fs::write(
        root.join("docs/plan/execution-graph.toml"),
        toml::to_string(&workspace).unwrap(),
    )
    .unwrap();
    // Seed the historical pair explicitly, even when the current reader must
    // refuse its provenance. Native repair needs the exact committed preimage;
    // ordinary public render must not invent a missing, unproven old view.
    match mine::infrastructure::toml_store::render_markdown(&workspace) {
        Ok(view) => std::fs::write(root.join("docs/plan/execution-graph.md"), view).unwrap(),
        // The dangling-predecessor negative fixture has no renderable view.
        // Do not fabricate one; its repair must reject the graph first.
        Err(mine::domain::error::MineError::PlanNotFound { plan_id }) => {
            // mine-release-allow-plan-reference: deliberately missing predecessor fixture
            assert_eq!(plan_id, "plan01-99");
        }
        Err(error) => panic!("unexpected private fixture render error: {error}"),
    }
    git(root, &["add", "--", "docs/plan"]);
    let changed = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .unwrap();
    match changed.code() {
        Some(0) => {}
        Some(1) => {
            git(
                root,
                &[
                    "commit",
                    "--quiet",
                    "-m",
                    "explicit private provenance fixture",
                ],
            );
        }
        _ => panic!("could not inspect private fixture index: {changed}"),
    }
}

fn damaged_live_workspace() -> PlanWorkspace {
    // mine-release-allow-plan-reference: damaged bootstrap provenance fixture
    let mut bootstrap = canonical_node("plan01", PlanStatus::Ready, &[]);
    bootstrap.id = "01".to_string();
    bootstrap.path = "docs/plan/01-isolated-plan-worktrees-and-review-integration.md".to_string();
    bootstrap.title = "Isolated Plan worktrees and review integration".to_string();
    bootstrap.owner = "bootstrap-owner".to_string();

    // mine-release-allow-plan-reference: damaged compensation provenance fixture
    let mut c01 = canonical_node("plan01-C01", PlanStatus::Ready, &[]);
    // mine-release-allow-plan-reference: damaged compensation path fixture
    c01.path = "docs/plan/plan01-C01-fix-bootstrap-adr-integrity.md".to_string();
    c01.title = "Fix bootstrap ADR integrity".to_string();
    // mine-release-allow-plan-reference: damaged compensation provenance fixture
    let mut c02 = canonical_node("plan01-C02", PlanStatus::Ready, &[]);
    // mine-release-allow-plan-reference: damaged compensation path fixture
    c02.path = "docs/plan/plan01-C02-protect-and-repair-plan-provenance.md".to_string();
    c02.title = "Protect and repair Plan provenance".to_string();

    for node in [&mut bootstrap, &mut c01, &mut c02] {
        node.branch.clear();
        node.lineage_id.clear();
        node.worktree_path.clear();
    }

    let mut damaged = workspace(14, vec![bootstrap, c01, c02]);
    damaged.schema_version = LEGACY_GRAPH_SCHEMA_VERSION;
    damaged.workspace_id = LIVE_WORKSPACE_ID.to_string();
    damaged
}

fn assert_only_provenance_changed(before: &PlanNode, after: &PlanNode) {
    let mut expected = before.clone();
    expected.branch.clone_from(&after.branch);
    expected.lineage_id.clone_from(&after.lineage_id);
    expected.worktree_path.clone_from(&after.worktree_path);
    assert_eq!(&expected, after);
}

fn assert_all_canonical_provenance(workspace: &PlanWorkspace) {
    validation::validate(workspace).unwrap();
    for node in &workspace.plans {
        let parsed = CanonicalPlanId::parse(&node.id).unwrap();
        assert_eq!(node.lineage_id, parsed.lineage_id());
        assert_eq!(node.worktree_path, parsed.worktree_path());
        assert_eq!(
            node.branch,
            registered_plan_branch(&workspace.workspace_id, &node.id, &node.path).unwrap()
        );
    }
}

#[test]
fn compensation_predecessor_repair_is_bounded_rendered_and_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let damaged = compensation_repair_workspace(40, PlanStatus::Blocked);
    seed(tmp.path(), &damaged);
    let store = legacy_store(tmp.path());
    let before: PlanWorkspace =
        toml::from_str(&std::fs::read_to_string(store.toml_path()).expect("damaged graph exists"))
            .unwrap();

    let outcome = store
        .repair_compensation_predecessors_with_revision(
            "provenance-fixture",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan01-03-C01",
            40,
            "remove predecessor not inherited from rejected plan",
            "2026-09-04T12:00:00Z",
        )
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.revision_before, 40);
    assert_eq!(outcome.revision_after, 41);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert_eq!(outcome.rejected_predecessor, "plan01-03");
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert_eq!(outcome.predecessors_after, ["plan01-01", "plan01-02"]);
    assert_eq!(outcome.status_before, PlanStatus::Blocked);
    assert_eq!(outcome.status_after, PlanStatus::Ready);

    let after = store.load().unwrap();
    for before_node in &before.plans {
        let after_node = after.get(&before_node.id).unwrap();
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        if before_node.id == "plan01-03-C01" {
            let mut expected = before_node.clone();
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            expected.hard_predecessors = vec!["plan01-01".into(), "plan01-02".into()];
            expected.status = PlanStatus::Ready;
            expected.updated_at = "2026-09-04T12:00:00Z".to_string();
            assert_eq!(after_node, &expected);
        } else {
            assert_eq!(after_node, before_node);
        }
    }
    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(
        std::fs::read_to_string(store.md_path()).unwrap(),
        mine::infrastructure::toml_store::render_markdown(&after).unwrap()
    );

    let toml_before_noop = std::fs::read(store.toml_path()).unwrap();
    let view_before_noop = std::fs::read(store.md_path()).unwrap();
    let repeated = store
        .repair_compensation_predecessors_with_revision(
            "provenance-fixture",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan01-03-C01",
            41,
            "repeat exact repair",
            "2026-09-04T13:00:00Z",
        )
        .unwrap();
    assert!(!repeated.changed);
    assert_eq!(repeated.revision_before, repeated.revision_after);
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), toml_before_noop);
    assert_eq!(std::fs::read(store.md_path()).unwrap(), view_before_noop);
}

#[test]
fn compensation_repair_atomically_upgrades_previous_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let mut damaged = compensation_repair_workspace(45, PlanStatus::Ready);
    damaged.schema_version = PREVIOUS_GRAPH_SCHEMA_VERSION;
    seed(tmp.path(), &damaged);
    let store = legacy_store(tmp.path());

    let outcome = store
        .repair_compensation_predecessors_with_revision(
            "provenance-fixture",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan01-03-C01",
            45,
            "repair supported previous-schema incident",
            "2026-09-04T12:30:00Z",
        )
        .unwrap();

    assert!(outcome.changed);
    assert_eq!(outcome.revision_before, 45);
    assert_eq!(outcome.revision_after, 46);
    assert_eq!(
        outcome.workspace.schema_version,
        CURRENT_GRAPH_SCHEMA_VERSION
    );
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert_eq!(outcome.predecessors_after, ["plan01-01", "plan01-02"]);
    let persisted = store.load().unwrap();
    assert_eq!(persisted.schema_version, CURRENT_GRAPH_SCHEMA_VERSION);
    assert_eq!(persisted.revision, 46);
}

#[test]
fn compensation_repair_refuses_unsupported_schema_without_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut damaged = compensation_repair_workspace(46, PlanStatus::Ready);
    damaged.schema_version = ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION;
    seed(tmp.path(), &damaged);
    let store = legacy_store(tmp.path());
    let before = std::fs::read(store.toml_path()).unwrap();

    let error = store
        .repair_compensation_predecessors_with_revision(
            "provenance-fixture",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan01-03-C01",
            46,
            "unsupported schema must not mutate",
            "2026-09-04T12:45:00Z",
        )
        .unwrap_err();

    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), before);
}

#[test]
fn compensation_predecessor_repair_refusals_preserve_machine_bytes() {
    let cases: [(&str, WorkspaceMutation, &str); 2] = [
        (
            "execution evidence",
            |workspace: &mut PlanWorkspace| {
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                workspace.get_mut("plan01-03-C01").unwrap().owner = "owner".to_string();
            },
            "MINE_GRAPH_INVALID",
        ),
        (
            "unrelated invalidity",
            |workspace: &mut PlanWorkspace| {
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                let mut unrelated = canonical_node("plan01-05", PlanStatus::Draft, &[]);
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                unrelated.hard_predecessors = vec!["plan01-99".to_string()];
                workspace.plans.push(unrelated);
            },
            "MINE_GRAPH_INVALID",
        ),
    ];
    for (name, mutate, expected_code) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let mut damaged = compensation_repair_workspace(50, PlanStatus::Ready);
        mutate(&mut damaged);
        seed(tmp.path(), &damaged);
        let store = legacy_store(tmp.path());
        let before = std::fs::read(store.toml_path()).unwrap();
        let error = store
            .repair_compensation_predecessors_with_revision(
                "provenance-fixture",
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan01-03-C01",
                50,
                name,
                "2026-09-04T12:00:00Z",
            )
            .unwrap_err();
        assert_eq!(error.code(), expected_code, "case {name}");
        if name == "unrelated invalidity" {
            // mine-release-allow-plan-reference: original corruption, not missing-view refusal
            assert!(error.to_string().contains("plan01-99"), "{error}");
        }
        assert_eq!(
            std::fs::read(store.toml_path()).unwrap(),
            before,
            "case {name}"
        );
    }

    let tmp = tempfile::tempdir().unwrap();
    seed(
        tmp.path(),
        &compensation_repair_workspace(60, PlanStatus::Ready),
    );
    let store = legacy_store(tmp.path());
    let before = std::fs::read(store.toml_path()).unwrap();
    for (revision, reason, expected_code) in [
        (59, "stale", "MINE_REVISION_CONFLICT"),
        (60, "", "MINE_GRAPH_INVALID"),
    ] {
        let error = store
            .repair_compensation_predecessors_with_revision(
                "provenance-fixture",
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan01-03-C01",
                revision,
                reason,
                "2026-09-04T12:00:00Z",
            )
            .unwrap_err();
        assert_eq!(error.code(), expected_code);
        assert_eq!(std::fs::read(store.toml_path()).unwrap(), before);
    }
}

#[test]
fn compensation_predecessor_repair_refuses_existing_target_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let damaged = compensation_repair_workspace(70, PlanStatus::Ready);
    seed(tmp.path(), &damaged);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let target = damaged.get("plan01-03-C01").unwrap();
    let status = Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["branch", &target.branch])
        .status()
        .unwrap();
    assert!(status.success());
    let store = legacy_store(tmp.path());
    let before = std::fs::read(store.toml_path()).unwrap();
    let error = store
        .repair_compensation_predecessors_with_revision(
            "provenance-fixture",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan01-03-C01",
            70,
            "branch must remain absent",
            "2026-09-04T12:00:00Z",
        )
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GIT_EVIDENCE");
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), before);
}

#[test]
fn plan_add_refuses_non_inherited_compensation_without_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let mut graph = compensation_repair_workspace(80, PlanStatus::Ready);
    graph.plans.pop();
    seed(tmp.path(), &graph);
    let store = legacy_store(tmp.path());
    let graph_service = GraphService::new(&store);
    let plan_service = PlanService::new(&graph_service);
    let before = std::fs::read(store.toml_path()).unwrap();
    let error = plan_service
        .add(PlanAddRequest {
            actor: "provenance-fixture".into(),
            ownership_input: None,
            expected_revision: store.observed_revision().unwrap(),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            id: "plan01-03-C01".to_string(),
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            path: "docs/plan/plan01-03-C01-test.md".to_string(),
            title: "invalid compensation".to_string(),
            design_references: vec!["docs/design/principles.md".to_string()],
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            exclusive_write_paths: vec!["src/plan01-03-C01".to_string()],
            hard_predecessors: vec![
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan01-01".to_string(),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan01-02".to_string(),
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                "plan01-04".to_string(),
            ],
            planning_scope: "docs/design".to_string(),
        })
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), before);
}

#[test]
fn cli_compensation_predecessor_repair_returns_deterministic_evidence() {
    let tmp = tempfile::tempdir().unwrap();
    seed(
        tmp.path(),
        &compensation_repair_workspace(90, PlanStatus::Ready),
    );
    let args = vec![
        "mine".to_string(),
        "--repo".to_string(),
        tmp.path().display().to_string(),
        "--format".to_string(),
        "json".to_string(),
        "graph".to_string(),
        "repair-compensation-predecessors".to_string(),
        "--actor".to_string(),
        "provenance-fixture".to_string(),
        "--id".to_string(),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        "plan01-03-C01".to_string(),
        "--expected-revision".to_string(),
        "90".to_string(),
        "--reason".to_string(),
        "registration contained an extra predecessor".to_string(),
    ];
    let outcome = cli::dispatch(&args, "mine");
    assert_eq!(outcome.exit_code, 0);
    let (stdout, stderr) = cli::render(&outcome, true, false);
    assert!(stderr.is_empty());
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        envelope["command"],
        "graph.repair-compensation-predecessors"
    );
    assert_eq!(envelope["revision_before"], 90);
    assert_eq!(envelope["revision_after"], 91);
    assert_eq!(envelope["data"]["changed"], true);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    assert_eq!(envelope["data"]["rejected_predecessor"], "plan01-03");
    assert_eq!(
        envelope["data"]["predecessors_after"],
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        serde_json::json!(["plan01-01", "plan01-02"])
    );
}

#[test]
fn damaged_v1_repair_is_deterministic_idempotent_and_stale_writer_incompatible() {
    let tmp = tempfile::tempdir().unwrap();
    let damaged = damaged_live_workspace();
    seed(tmp.path(), &damaged);
    let store = legacy_store(tmp.path());

    let repaired = store
        .repair_provenance_with_revision(14, "provenance-fixture")
        .unwrap();
    assert!(repaired.changed);
    assert_eq!(repaired.revision_before, 14);
    assert_eq!(repaired.revision_after, 15);
    assert_eq!(
        repaired.repaired_plans,
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        vec!["01", "plan01-C01", "plan01-C02"]
    );
    assert_eq!(
        repaired.workspace.schema_version,
        CURRENT_GRAPH_SCHEMA_VERSION
    );
    for (before, after) in damaged.plans.iter().zip(&repaired.workspace.plans) {
        assert_only_provenance_changed(before, after);
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert_eq!(after.lineage_id, "plan01");
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        assert_eq!(after.worktree_path, ".mine/worktrees/plan01");
    }
    validation::validate(&repaired.workspace).unwrap();

    let bytes_after_first =
        std::fs::read(tmp.path().join("docs/plan/execution-graph.toml")).unwrap();
    let text_after_first = String::from_utf8(bytes_after_first.clone()).unwrap();
    assert!(text_after_first.starts_with("schema_version = \"mine-graph-v7\"\n"));

    #[allow(dead_code)]
    #[derive(serde::Deserialize)]
    struct SchemaV1WriterView {
        schema_version: u32,
        revision: u64,
    }
    assert!(
        toml::from_str::<SchemaV1WriterView>(&text_after_first).is_err(),
        "a schema-v1 u32 writer must fail before mutation"
    );
    assert!(
        toml::from_str::<PlanWorkspace>(
            &text_after_first.replace("schema_version = \"mine-graph-v7\"", "schema_version = 7")
        )
        .is_err(),
        "the current reader must not accept a v2 marker an old u32 writer can parse"
    );

    let repeated = store
        .repair_provenance_with_revision(15, "provenance-fixture")
        .unwrap();
    assert!(!repeated.changed);
    assert_eq!(repeated.revision_before, 15);
    assert_eq!(repeated.revision_after, 15);
    assert_eq!(
        std::fs::read(tmp.path().join("docs/plan/execution-graph.toml")).unwrap(),
        bytes_after_first,
        "idempotent repair must not rewrite the graph"
    );
}

#[test]
fn repair_revision_conflict_and_inconsistent_legacy_metadata_write_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let damaged = damaged_live_workspace();
    seed(tmp.path(), &damaged);
    let store = legacy_store(tmp.path());
    let path = tmp.path().join("docs/plan/execution-graph.toml");
    let before = std::fs::read(&path).unwrap();

    let conflict = store
        .repair_provenance_with_revision(13, "provenance-fixture")
        .unwrap_err();
    assert_eq!(conflict.code(), "MINE_REVISION_CONFLICT");
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let mut inconsistent = damaged;
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    inconsistent.plans[1].lineage_id = "plan99".to_string();
    std::fs::write(&path, toml::to_string(&inconsistent).unwrap()).unwrap();
    let inconsistent_before = std::fs::read(&path).unwrap();
    let error = store
        .repair_provenance_with_revision(14, "provenance-fixture")
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(&path).unwrap(), inconsistent_before);
}

#[test]
fn cli_repair_path_migrates_damaged_v1_with_expected_revision() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path(), &damaged_live_workspace());
    let argv = vec![
        "mine".to_string(),
        "graph".to_string(),
        "repair-provenance".to_string(),
        "--actor".to_string(),
        "provenance-fixture".to_string(),
        "--expected-revision".to_string(),
        "14".to_string(),
        "--repo".to_string(),
        tmp.path().display().to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let outcome = mine::cli::dispatch(&argv, "mine");
    assert_eq!(outcome.exit_code, 0, "repair failed: {outcome:?}");
    let repaired = legacy_store(tmp.path()).load().unwrap();
    assert_eq!(repaired.revision, 15);
    assert_eq!(repaired.schema_version, CURRENT_GRAPH_SCHEMA_VERSION);
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    assert_eq!(repaired.get("plan01-C01").unwrap().lineage_id, "plan01");
}

#[test]
fn current_validation_rejects_missing_unsafe_and_inconsistent_provenance() {
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut base = canonical_node("plan01", PlanStatus::Rejected, &[]);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    base.compensating_plan = "plan01-C01".to_string();
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut rejected = canonical_node("plan01-C01", PlanStatus::Rejected, &[]);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    rejected.compensating_plan = "plan01-C02".to_string();
    let valid = workspace(
        1,
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        vec![
            base,
            rejected,
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            canonical_node("plan01-C02", PlanStatus::Ready, &[]),
        ],
    );
    validation::validate(&valid).unwrap();

    let mut missing = valid.clone();
    missing.plans[2].lineage_id.clear();
    assert_eq!(
        validation::validate(&missing).unwrap_err().code(),
        "MINE_GRAPH_INVALID"
    );

    let mut unsafe_path = valid.clone();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    unsafe_path.plans[2].worktree_path = "../plan01".to_string();
    assert_eq!(
        validation::validate(&unsafe_path).unwrap_err().code(),
        "MINE_GRAPH_INVALID"
    );

    let mut inconsistent = valid;
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    inconsistent.plans[2].branch = "plan/plan99-forged".to_string();
    assert_eq!(
        validation::validate(&inconsistent).unwrap_err().code(),
        "MINE_GRAPH_INVALID"
    );
}

#[test]
fn current_writer_rejects_unknown_persisted_fields_without_rewrite() {
    let tmp = tempfile::tempdir().unwrap();
    seed(
        tmp.path(),
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        &workspace(7, vec![canonical_node("plan01", PlanStatus::Ready, &[])]),
    );
    let path = tmp.path().join("docs/plan/execution-graph.toml");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(b"future_required_field = \"must-not-drop\"\n");
    std::fs::write(&path, &bytes).unwrap();
    let store = legacy_store(tmp.path());
    assert_eq!(store.load().unwrap_err().code(), "MINE_GRAPH_INVALID");
    assert_eq!(
        store
            .repair_provenance_with_revision(7, "provenance-fixture")
            .unwrap_err()
            .code(),
        "MINE_GRAPH_INVALID"
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn every_lifecycle_write_retains_all_canonical_provenance() {
    let tmp = tempfile::tempdir().unwrap();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let mut rejected = canonical_node("plan05", PlanStatus::Rejected, &[]);
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    rejected.compensating_plan = "plan05-C01".to_string();
    let plans = vec![
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        canonical_node("plan01", PlanStatus::Accepted, &[]),
        // mine-release-allow-plan-reference: canonical lifecycle Plan fixture
        canonical_node("plan02", PlanStatus::Ready, &[]),
        // mine-release-allow-plan-reference: canonical lifecycle Plan fixture
        canonical_node("plan03", PlanStatus::Implemented, &[]),
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        canonical_node("plan04", PlanStatus::Draft, &[]),
        rejected,
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        canonical_node("plan05-C01", PlanStatus::Ready, &[]),
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        canonical_node("plan07", PlanStatus::Blocked, &["plan05"]),
    ];
    seed(tmp.path(), &workspace(20, plans));
    let store = legacy_store(tmp.path());
    let graph = GraphService::new(&store);
    let service = PlanService::new(&graph);

    let operation = uuid::Uuid::new_v4().to_string();
    // mine-release-allow-plan-reference: private native lifecycle subject
    invoke(
        tmp.path(),
        &[
            "plan",
            "prepare",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan02",
            "--actor",
            "provenance-preparer",
            "--expected-revision",
            "20",
            "--operation-id",
            &operation,
        ],
    );
    // mine-release-allow-plan-reference: private native lifecycle subject
    invoke(
        tmp.path(),
        &[
            "plan",
            "start",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan02",
            "--owner",
            "provenance-executor",
            "--run-id",
            "provenance-run",
            "--expected-revision",
            "21",
        ],
    );
    // mine-release-allow-plan-reference: private linked checkout and canonical report
    let target = tmp.path().join(".mine/worktrees/plan02");
    // mine-release-allow-plan-reference: private committed implementation artifact
    let report = "docs/plan/reports/plan02-test-implementation.md";
    std::fs::create_dir_all(target.join("docs/plan/reports")).unwrap();
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    std::fs::create_dir_all(target.join("src/plan02")).unwrap();
    std::fs::write(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        target.join("src/plan02/fact.rs"),
        "// private implementation\n",
    )
    .unwrap();
    std::fs::write(target.join(report), "# Private implementation evidence\n").unwrap();
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    git(&target, &["add", "--", "src/plan02/fact.rs", report]);
    git(
        &target,
        &[
            "commit",
            "--quiet",
            "-m",
            "private real Plan implementation",
        ],
    );
    let reviewed_tip = git(&target, &["rev-parse", "HEAD"]);

    service
        .mark_implemented(PlanImplementedRequest {
            actor: "provenance-fixture".into(),
            expected_revision: 22,
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            id: "plan02".to_string(),
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            report: report.to_string(),
            commits: vec![reviewed_tip.clone()],
            updated_at: "2026-08-21T02:00:00Z".to_string(),
        })
        .unwrap();
    assert_all_canonical_provenance(&store.load().unwrap());

    std::fs::create_dir_all(tmp.path().join("docs/plan/reports")).unwrap();
    // mine-release-allow-plan-reference: rejection review-path fixture
    let rejection_review = "docs/plan/reports/plan02-test-review.md";
    let current = store.load().unwrap();
    // mine-release-allow-plan-reference: exact prepared/started/implemented subject
    let subject = current.get("plan02").unwrap();
    let config = mine::cli::context::load_config(tmp.path()).unwrap();
    use mine::domain::graph::{PlanIdentity, PlanIdentityFields, RepoPath};
    use mine::domain::ports::Clock;
    use mine::infrastructure::{graph_checkpoint::digest, system::SystemClock};
    let identity = PlanIdentity::new(PlanIdentityFields {
        repository_id: config.repository_id,
        workspace_id: current.workspace_id.clone(),
        plan_id: subject.id.clone(),
        plan_path: RepoPath::new(subject.path.clone()).unwrap(),
        body_sha256: Some(digest(
            &std::fs::read(tmp.path().join(&subject.path)).unwrap(),
        )),
        lineage_id: subject.lineage_id.clone(),
        branch: subject.branch.clone(),
    })
    .unwrap();
    let assertion = serde_json::json!({"schema_version":1,"plan":identity,"writer_owner":"provenance-executor","writer_run_id":"provenance-run","reviewer":"provenance-reviewer","independent_review":true,"writer_stopped":true,"stopped_at":SystemClock.now_utc_rfc3339(),"stop_basis":"Private controller stopped its writer and preserved the exact reviewed checkout"});
    let review_body = format!(
        "# Independent private review\n\nVerdict: REJECTED\n\n```mine-review-closure\n{assertion}\n```\n"
    );
    std::fs::write(tmp.path().join(rejection_review), &review_body).unwrap();
    // mine-release-allow-plan-reference: exact next sibling compensation body
    let compensation_path = "docs/plan/plan02-C01-test.md";
    let compensation_body = b"# Private compensating Plan\n";
    std::fs::write(tmp.path().join(compensation_path), compensation_body).unwrap();
    git(
        tmp.path(),
        &["add", "--", rejection_review, compensation_path],
    );
    git(
        tmp.path(),
        &[
            "commit",
            "--quiet",
            "-m",
            "private independent stopped handoff evidence",
        ],
    );
    let mut compensation = identity.fields().clone();
    // mine-release-allow-plan-reference: exact next sibling compensation identity
    compensation.plan_id = "plan02-C01".into();
    compensation.plan_path = RepoPath::new(compensation_path).unwrap();
    compensation.branch = registered_plan_branch(
        &current.workspace_id,
        &compensation.plan_id,
        compensation_path,
    )
    .unwrap();
    compensation.body_sha256 = Some(digest(compensation_body));
    let input = serde_json::json!({"schema_version":1,"actor":"provenance-reviewer","intent":{"kind":"rejected_handoff","plan_id":subject.id,"review":{"reviewed_tip":reviewed_tip,"report":{"path":rejection_review,"bytes":review_body.len(),"sha256":digest(review_body.as_bytes())}},"compensation":PlanIdentity::new(compensation).unwrap()}});
    std::fs::write(
        tmp.path().join(".mine/runtime/provenance-rejection.json"),
        serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    invoke(
        tmp.path(),
        &[
            "operation",
            "begin",
            "--expected-revision",
            "23",
            "--input",
            ".mine/runtime/provenance-rejection.json",
        ],
    );

    service
        .reject(PlanRejectRequest {
            actor: "provenance-reviewer".into(),
            expected_revision: 23,
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            id: "plan02".to_string(),
            reason: "review".to_string(),
            review_report: rejection_review.to_string(),
            // mine-release-allow-plan-reference: compensation lifecycle fixture
            compensating_plan: "plan02-C01".to_string(),
            updated_at: "2026-08-21T03:00:00Z".to_string(),
        })
        .unwrap();
    assert_all_canonical_provenance(&store.load().unwrap());

    let acceptance_before =
        std::fs::read(tmp.path().join("docs/plan/execution-graph.toml")).unwrap();
    // Report-only acceptance is no longer a mutation API. The actual positive
    // native closure/provenance path is exercised in lifecycle_operations.
    assert!(
        service
            .accept(PlanAcceptRequest {
                expected_revision: store.observed_revision().unwrap(),
                // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                id: "plan03".to_string(),
                operation_id: mine::domain::lifecycle_operation::OperationId::generate(),
                input: mine::domain::graph::RepoPath::new(
                    ".mine/runtime/missing-native-closure.json"
                )
                .unwrap(),
            })
            .is_err()
    );
    assert_eq!(
        std::fs::read(tmp.path().join("docs/plan/execution-graph.toml")).unwrap(),
        acceptance_before
    );
    assert_all_canonical_provenance(&store.load().unwrap());

    service
        .release(PlanReleaseRequest {
            actor: "provenance-fixture".into(),
            expected_revision: store.observed_revision().unwrap(),
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            id: "plan04".to_string(),
            updated_at: "2026-08-21T05:00:00Z".to_string(),
        })
        .unwrap();
    assert_all_canonical_provenance(&store.load().unwrap());

    service
        .rewire_compensation(PlanRewireRequest {
            actor: "provenance-fixture".into(),
            expected_revision: store.observed_revision().unwrap(),
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            id: "plan05".to_string(),
            updated_at: "2026-08-21T06:00:00Z".to_string(),
        })
        .unwrap();
    let final_workspace = store.load().unwrap();
    assert_all_canonical_provenance(&final_workspace);
    assert_eq!(
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        final_workspace.get("plan07").unwrap().hard_predecessors,
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        vec!["plan05-C01"]
    );
}

fn init_git_repo(root: &Path) {
    git(root, &["init", "--quiet", "-b", "master"]);
    for (key, value) in [
        ("user.email", "fixture@example.invalid"),
        ("user.name", "Provenance fixture"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(root, &["config", key, value]);
    }
}

fn invoke(root: &Path, rest: &[&str]) -> serde_json::Value {
    let mut args = vec![
        "mine".into(),
        "--repo".into(),
        root.display().to_string(),
        "--format".into(),
        "json".into(),
    ];
    args.extend(rest.iter().map(|s| s.to_string()));
    let outcome = cli::dispatch(&args, "mine");
    let (stdout, stderr) = cli::render(&outcome, true, false);
    assert_eq!(outcome.exit_code, 0, "{outcome:?} {stderr}");
    serde_json::from_str(&stdout).unwrap()
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_file(root: &Path, name: &str, contents: &str, message: &str) -> String {
    std::fs::write(root.join(name), contents).unwrap();
    git(root, &["add", name]);
    git(root, &["commit", "--quiet", "-m", message]);
    git(root, &["rev-parse", "HEAD"])
}

fn current_git_workspace(root: &Path, status: PlanStatus) -> (PlanWorkspace, String) {
    init_git_repo(root);
    let anchor = commit_file(root, "anchor.txt", "anchor\n", "anchor");
    // mine-release-allow-plan-reference: persisted-anchor Plan fixture
    let mut node = canonical_node("plan01", status, &[]);
    node.branch_anchor.clone_from(&anchor);
    (workspace(10, vec![node]), anchor)
}

#[test]
fn synthetic_workspace_cannot_bypass_public_canonical_provenance() {
    for case in ["empty", "partial", "canonical-missing"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        init_git_repo(root);
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        let mut node = canonical_node("plan01", PlanStatus::Ready, &[]);
        node.branch.clear();
        node.lineage_id.clear();
        node.worktree_path.clear();
        if case != "canonical-missing" {
            node.id = "0001".into();
            node.path = "docs/plan/0001.md".into();
        }
        if case == "partial" {
            node.branch = "plan/foreign".into();
        }
        let mut graph = workspace(10, vec![node]);
        graph.workspace_id = "bootstrap-mine-v1".into();
        seed(root, &graph);
        let source = root.join("docs/plan/execution-graph.toml");
        let view = root.join("docs/plan/execution-graph.md");
        let before = (std::fs::read(&source).unwrap(), std::fs::read(&view).ok());
        let catalog = root.join(".mine/runtime/operations/.catalog");
        let before_catalog = std::fs::read(&catalog).ok();
        for rest in [
            vec!["graph", "validate"],
            vec!["graph", "render", "--expected-revision", "10"],
        ] {
            let mut argv = vec!["mine".into(), "--repo".into(), root.display().to_string()];
            argv.extend(rest.into_iter().map(str::to_string));
            argv.extend(["--format".into(), "json".into()]);
            let outcome = cli::dispatch(&argv, "mine");
            assert_ne!(
                outcome.exit_code, 0,
                "synthetic {case} provenance must refuse: {outcome:?}"
            );
            assert_eq!(
                (std::fs::read(&source).unwrap(), std::fs::read(&view).ok()),
                before
            );
            assert_eq!(std::fs::read(&catalog).ok(), before_catalog);
        }
    }
}

#[test]
fn exact_approved_bootstrap_provenance_is_not_a_workspace_wide_alias() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    init_git_repo(root);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut node = canonical_node("plan01", PlanStatus::Ready, &[]);
    node.id = "01".into();
    node.path = "docs/plan/01-isolated-plan-worktrees-and-review-integration.md".into();
    node.branch = registered_plan_branch(LIVE_WORKSPACE_ID, &node.id, &node.path).unwrap();
    let mut graph = workspace(10, vec![node]);
    graph.workspace_id = LIVE_WORKSPACE_ID.into();
    seed(root, &graph);
    let call = || {
        cli::dispatch(
            &[
                "mine".into(),
                "--repo".into(),
                root.display().to_string(),
                "graph".into(),
                "validate".into(),
                "--format".into(),
                "json".into(),
            ],
            "mine",
        )
    };
    let accepted = call();
    assert_eq!(accepted.exit_code, 0, "{accepted:?}");
    for case in ["uuid", "id", "path"] {
        let mut changed = graph.clone();
        match case {
            "uuid" => changed.workspace_id = uuid::Uuid::new_v4().to_string(),
            "id" => changed.plans[0].id = "02".into(),
            "path" => changed.plans[0].path = "docs/plan/01-another-body.md".into(),
            _ => unreachable!(),
        }
        seed(root, &changed);
        let source = root.join("docs/plan/execution-graph.toml");
        let view = root.join("docs/plan/execution-graph.md");
        let before = (std::fs::read(&source).unwrap(), std::fs::read(&view).ok());
        let refused = call();
        assert_ne!(refused.exit_code, 0, "changed {case}: {refused:?}");
        assert_eq!(
            (std::fs::read(&source).unwrap(), std::fs::read(&view).ok()),
            before
        );
    }
}

#[test]
fn every_post_start_status_rejects_a_forged_persisted_anchor() {
    for status in [
        PlanStatus::InProgress,
        PlanStatus::Implemented,
        PlanStatus::Accepted,
        PlanStatus::Rejected,
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let (mut graph, _) = current_git_workspace(tmp.path(), status);
        graph.plans[0].branch_anchor = "definitely-not-a-git-commit".to_string();
        seed(tmp.path(), &graph);

        let error = legacy_store(tmp.path()).load().unwrap_err();
        assert_eq!(error.code(), "MINE_GRAPH_INVALID", "status {status}");
    }
}

#[test]
fn persisted_anchor_must_be_a_repository_native_exact_full_commit_oid() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut graph, anchor) = current_git_workspace(tmp.path(), PlanStatus::Accepted);
    seed(tmp.path(), &graph);
    assert_eq!(
        legacy_store(tmp.path()).load().unwrap().plans[0].branch_anchor,
        anchor
    );

    std::fs::write(tmp.path().join("blob.txt"), "blob\n").unwrap();
    let blob = git(tmp.path(), &["hash-object", "-w", "blob.txt"]);
    let abbreviation = anchor.chars().take(12).collect::<String>();
    let missing_object = "0".repeat(anchor.len());
    for forged in ["HEAD".to_string(), abbreviation, missing_object, blob] {
        graph.plans[0].branch_anchor = forged;
        seed(tmp.path(), &graph);
        let error = legacy_store(tmp.path()).load().unwrap_err();
        assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    }
}

#[test]
fn terminal_plan_anchor_survives_branch_deletion_when_commit_object_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, anchor) = current_git_workspace(tmp.path(), PlanStatus::Rejected);
    assert!(git(tmp.path(), &["branch", "--list", &graph.plans[0].branch]).is_empty());
    seed(tmp.path(), &graph);

    assert_eq!(
        legacy_store(tmp.path()).load().unwrap().plans[0].branch_anchor,
        anchor
    );
}

#[test]
fn locked_reload_candidate_write_and_v3_repair_reject_forged_anchors_without_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, _) = current_git_workspace(tmp.path(), PlanStatus::InProgress);
    seed(tmp.path(), &graph);
    let store = legacy_store(tmp.path());
    store.render().unwrap();
    let toml_path = store.toml_path().to_path_buf();
    let md_path = store.md_path().to_path_buf();

    let mut forged_reload = graph.clone();
    forged_reload.plans[0].branch_anchor = "HEAD".to_string();
    std::fs::write(&toml_path, toml::to_string(&forged_reload).unwrap()).unwrap();
    let forged_bytes = std::fs::read(&toml_path).unwrap();
    let markdown_before = std::fs::read(&md_path).unwrap();
    let closure_called = std::cell::Cell::new(false);
    let error = store
        .save_with_revision(10, "provenance-fixture", |workspace| {
            closure_called.set(true);
            Ok(workspace)
        })
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert!(
        !closure_called.get(),
        "locked reload must fail before mutation"
    );
    assert_eq!(std::fs::read(&toml_path).unwrap(), forged_bytes);
    assert_eq!(std::fs::read(&md_path).unwrap(), markdown_before);

    let error = store
        .repair_provenance_with_revision(10, "provenance-fixture")
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(&toml_path).unwrap(), forged_bytes);
    assert_eq!(std::fs::read(&md_path).unwrap(), markdown_before);

    seed(tmp.path(), &graph);
    store.render().unwrap();
    let valid_bytes = std::fs::read(&toml_path).unwrap();
    let valid_markdown = std::fs::read(&md_path).unwrap();
    let error = store
        .save_with_revision(10, "provenance-fixture", |mut workspace| {
            workspace.revision += 1;
            workspace.plans[0].branch_anchor = "HEAD".to_string();
            Ok(workspace)
        })
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(&toml_path).unwrap(), valid_bytes);
    assert_eq!(std::fs::read(&md_path).unwrap(), valid_markdown);
}

#[test]
fn v2_accepted_deleted_branch_derives_anchor_from_first_commit_parent() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let anchor = commit_file(tmp.path(), "state.txt", "anchor\n", "anchor");
    let implementation = commit_file(
        tmp.path(),
        "state.txt",
        "implementation\n",
        "implementation",
    );
    // mine-release-allow-plan-reference: schema migration Plan fixture
    let mut accepted = canonical_node("plan01", PlanStatus::Accepted, &[]);
    accepted.branch_anchor.clear();
    accepted.implementation_commits = vec![implementation];
    let mut graph = workspace(8, vec![accepted]);
    graph.schema_version = ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION;
    seed(tmp.path(), &graph);

    let repaired = legacy_store(tmp.path())
        .repair_provenance_with_revision(8, "provenance-fixture")
        .unwrap();
    assert_eq!(repaired.revision_after, 9);
    assert_eq!(
        // mine-release-allow-plan-reference: migrated Plan lookup fixture
        repaired.workspace.get("plan01").unwrap().branch_anchor,
        anchor
    );
    let migrated_text =
        std::fs::read_to_string(tmp.path().join("docs/plan/execution-graph.toml")).unwrap();
    assert!(
        toml::from_str::<OldV2Workspace>(&migrated_text).is_err(),
        "a strict schema-v2 writer must reject the required branch_anchor field"
    );
    assert!(
        // mine-release-allow-plan-reference: deleted Plan branch fixture
        !git(tmp.path(), &["branch", "--list", "plan/plan01-test"]).contains("plan/plan01-test")
    );
}

#[test]
fn v2_active_compensation_derives_preserved_predecessor_tip() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), ".gitignore", PRIVATE_IGNORE_RULES, "initial");
    let rejected_commit = commit_file(tmp.path(), "base.txt", "rejected\n", "rejected");

    // mine-release-allow-plan-reference: rejected predecessor fixture
    let mut rejected = canonical_node("plan01", PlanStatus::Rejected, &[]);
    rejected.branch_anchor.clear();
    rejected.implementation_commits = vec![rejected_commit.clone()];
    // mine-release-allow-plan-reference: compensation relationship fixture
    rejected.compensating_plan = "plan01-C01".to_string();
    git(tmp.path(), &["branch", &rejected.branch, &rejected_commit]);

    // mine-release-allow-plan-reference: active compensation fixture
    let mut active = canonical_node("plan01-C01", PlanStatus::InProgress, &[]);
    active.branch_anchor.clear();
    // mine-release-allow-plan-reference: lineage worktree-path fixture
    let worktree = tmp.path().join(".mine/worktrees/plan01");
    git(
        tmp.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            &active.branch,
            worktree.to_str().unwrap(),
            &rejected_commit,
        ],
    );
    commit_file(&worktree, "compensation.txt", "descendant\n", "descendant");

    let mut graph = workspace(12, vec![rejected, active]);
    graph.schema_version = ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION;
    seed(tmp.path(), &graph);
    let repaired = legacy_store(tmp.path())
        .repair_provenance_with_revision(12, "provenance-fixture")
        .unwrap();
    assert_eq!(
        // mine-release-allow-plan-reference: repaired compensation lookup fixture
        repaired.workspace.get("plan01-C01").unwrap().branch_anchor,
        rejected_commit
    );
}

#[test]
fn v2_ambiguous_active_base_refuses_without_writing() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), ".gitignore", PRIVATE_IGNORE_RULES, "initial");
    // mine-release-allow-plan-reference: ambiguous active-base fixture
    let mut active = canonical_node("plan01", PlanStatus::InProgress, &[]);
    active.branch_anchor.clear();
    let mut graph = workspace(21, vec![active]);
    graph.schema_version = ANCHOR_REPAIR_GRAPH_SCHEMA_VERSION;
    seed(tmp.path(), &graph);
    let path = tmp.path().join("docs/plan/execution-graph.toml");
    let before = std::fs::read(&path).unwrap();

    let error = legacy_store(tmp.path())
        .repair_provenance_with_revision(21, "provenance-fixture")
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn linked_invocation_reads_root_graph_and_resolves_shared_lineage_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    let mut base = canonical_node("plan01", PlanStatus::Rejected, &[]);
    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
    base.compensating_plan = "plan01-C01".to_string();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let mut c01 = canonical_node("plan01-C01", PlanStatus::Rejected, &[]);
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    c01.compensating_plan = "plan01-C02".to_string();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let c02 = canonical_node("plan01-C02", PlanStatus::InProgress, &[]);
    seed(root, &workspace(42, vec![base, c01, c02]));
    assert_eq!(git(root, &["status", "--porcelain=v1", "-uall"]), "");
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let linked = root.join("legacy plan01 lineage");
    Command::new("git")
        .arg("-C")
        .arg(root)
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        .args(["worktree", "add", "--quiet", "-b", "plan/plan01-C02-test"])
        .arg(&linked)
        .status()
        .unwrap();

    let context = build_context(&GlobalOpts {
        format: OutputFormat::Json,
        quiet: true,
        repo: Some(linked.clone()),
        no_color: true,
        config_root: None,
    })
    .unwrap();
    assert_eq!(context.repo_root, linked.canonicalize().unwrap());
    assert_eq!(context.caller_root, linked.canonicalize().unwrap());
    assert_eq!(context.authority_root, root.canonicalize().unwrap());
    let graph = GraphService::new(&context.store);
    let lookup = PlanService::new(&graph)
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        .show_with_locations("plan01-C01", &context.caller_root)
        .unwrap();
    assert_eq!(lookup.authoritative_revision, 42);
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    assert_eq!(lookup.registered_worktree_path, ".mine/worktrees/plan01");
    assert_eq!(
        PathBuf::from(&lookup.resolved_worktree_path)
            .canonicalize()
            .unwrap(),
        linked.canonicalize().unwrap()
    );
    assert_eq!(
        lookup
            .observed_worktree
            .as_ref()
            .and_then(|worktree| worktree.branch.as_deref()),
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        Some("plan/plan01-C02-test")
    );
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    assert_eq!(lookup.node.lineage_id, "plan01");
}
