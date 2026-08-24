#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use mine::application::graph_service::{
    GraphService, PlanAcceptRequest, PlanImplementedRequest, PlanRejectRequest,
};
use mine::application::plan_service::{PlanReleaseRequest, PlanRewireRequest, PlanService};
use mine::cli::context::{GlobalOpts, OutputFormat, build_context};
use mine::domain::graph::{
    CURRENT_GRAPH_SCHEMA_VERSION, LEGACY_GRAPH_SCHEMA_VERSION, PREVIOUS_GRAPH_SCHEMA_VERSION,
    PlanNode, PlanWorkspace,
};
use mine::domain::plan_id::{CanonicalPlanId, registered_plan_branch};
use mine::domain::status::PlanStatus;
use mine::domain::validation;
use mine::infrastructure::toml_store::TomlStore;

const LIVE_WORKSPACE_ID: &str = "efaa93da-09ca-4888-a411-f896a151ce6d";

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
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(root.join(".mine")).unwrap();
    std::fs::copy(
        manifest.join(".mine/config.toml"),
        root.join(".mine/config.toml"),
    )
    .unwrap();
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
        lineage_id: parsed.lineage_id(),
        worktree_path: parsed.worktree_path(),
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

fn seed(root: &Path, workspace: &PlanWorkspace) {
    let mut workspace = workspace.clone();
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
    write_config(root);
    std::fs::create_dir_all(root.join("docs/plan")).unwrap();
    std::fs::write(
        root.join("docs/plan/execution-graph.toml"),
        toml::to_string(&workspace).unwrap(),
    )
    .unwrap();
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
fn damaged_v1_repair_is_deterministic_idempotent_and_stale_writer_incompatible() {
    let tmp = tempfile::tempdir().unwrap();
    let damaged = damaged_live_workspace();
    seed(tmp.path(), &damaged);
    let store = TomlStore::new(tmp.path());

    let repaired = store.repair_provenance_with_revision(14).unwrap();
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
    assert!(text_after_first.starts_with("schema_version = \"mine-graph-v3\"\n"));

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
            &text_after_first.replace("schema_version = \"mine-graph-v3\"", "schema_version = 3")
        )
        .is_err(),
        "the current reader must not accept a v2 marker an old u32 writer can parse"
    );

    let repeated = store.repair_provenance_with_revision(15).unwrap();
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
    let store = TomlStore::new(tmp.path());
    let path = tmp.path().join("docs/plan/execution-graph.toml");
    let before = std::fs::read(&path).unwrap();

    let conflict = store.repair_provenance_with_revision(13).unwrap_err();
    assert_eq!(conflict.code(), "MINE_REVISION_CONFLICT");
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let mut inconsistent = damaged;
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    inconsistent.plans[1].lineage_id = "plan99".to_string();
    std::fs::write(&path, toml::to_string(&inconsistent).unwrap()).unwrap();
    let inconsistent_before = std::fs::read(&path).unwrap();
    let error = store.repair_provenance_with_revision(14).unwrap_err();
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
        "--expected-revision".to_string(),
        "14".to_string(),
        "--repo".to_string(),
        tmp.path().display().to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let outcome = mine::cli::dispatch(&argv, "mine");
    assert_eq!(outcome.exit_code, 0, "repair failed: {outcome:?}");
    let repaired = TomlStore::new(tmp.path()).load().unwrap();
    assert_eq!(repaired.revision, 15);
    assert_eq!(repaired.schema_version, CURRENT_GRAPH_SCHEMA_VERSION);
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    assert_eq!(repaired.get("plan01-C01").unwrap().lineage_id, "plan01");
}

#[test]
fn current_validation_rejects_missing_unsafe_and_inconsistent_provenance() {
    let valid = workspace(
        1,
        // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
        vec![canonical_node("plan01-C02", PlanStatus::Ready, &[])],
    );
    validation::validate(&valid).unwrap();

    let mut missing = valid.clone();
    missing.plans[0].lineage_id.clear();
    assert_eq!(
        validation::validate(&missing).unwrap_err().code(),
        "MINE_GRAPH_INVALID"
    );

    let mut unsafe_path = valid.clone();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    unsafe_path.plans[0].worktree_path = "../plan01".to_string();
    assert_eq!(
        validation::validate(&unsafe_path).unwrap_err().code(),
        "MINE_GRAPH_INVALID"
    );

    let mut inconsistent = valid;
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    inconsistent.plans[0].branch = "plan/plan99-forged".to_string();
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
    let store = TomlStore::new(tmp.path());
    assert_eq!(store.load().unwrap_err().code(), "MINE_GRAPH_INVALID");
    assert_eq!(
        store.repair_provenance_with_revision(7).unwrap_err().code(),
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
        canonical_node("plan02", PlanStatus::InProgress, &[]),
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
    let store = TomlStore::new(tmp.path());
    let graph = GraphService::new(&store);
    let service = PlanService::new(&graph);

    service
        .mark_implemented(PlanImplementedRequest {
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            id: "plan02".to_string(),
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            report: "docs/plan/reports/plan02-test-implementation.md".to_string(),
            commits: vec!["abc123".to_string()],
            updated_at: "2026-08-21T02:00:00Z".to_string(),
        })
        .unwrap();
    assert_all_canonical_provenance(&store.load().unwrap());

    std::fs::create_dir_all(tmp.path().join("docs/plan/reports")).unwrap();
    // mine-release-allow-plan-reference: rejection review-path fixture
    let rejection_review = "docs/plan/reports/plan02-test-review.md";
    std::fs::write(tmp.path().join(rejection_review), "independent review\n").unwrap();

    service
        .reject(PlanRejectRequest {
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

    service
        .accept(PlanAcceptRequest {
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            id: "plan03".to_string(),
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            review_report: "docs/plan/reports/plan03-test-review.md".to_string(),
            updated_at: "2026-08-21T04:00:00Z".to_string(),
        })
        .unwrap();
    assert_all_canonical_provenance(&store.load().unwrap());

    service
        .release(PlanReleaseRequest {
            // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
            id: "plan04".to_string(),
            updated_at: "2026-08-21T05:00:00Z".to_string(),
        })
        .unwrap();
    assert_all_canonical_provenance(&store.load().unwrap());

    service
        .rewire_compensation(PlanRewireRequest {
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
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["init", "--quiet"])
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.email", "test@example.com"])
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.name", "test"])
        .status()
        .unwrap();
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

        let error = TomlStore::new(tmp.path()).load().unwrap_err();
        assert_eq!(error.code(), "MINE_GRAPH_INVALID", "status {status}");
    }
}

#[test]
fn persisted_anchor_must_be_a_repository_native_exact_full_commit_oid() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut graph, anchor) = current_git_workspace(tmp.path(), PlanStatus::Accepted);
    seed(tmp.path(), &graph);
    assert_eq!(
        TomlStore::new(tmp.path()).load().unwrap().plans[0].branch_anchor,
        anchor
    );

    std::fs::write(tmp.path().join("blob.txt"), "blob\n").unwrap();
    let blob = git(tmp.path(), &["hash-object", "-w", "blob.txt"]);
    let abbreviation = anchor.chars().take(12).collect::<String>();
    let missing_object = "0".repeat(anchor.len());
    for forged in ["HEAD".to_string(), abbreviation, missing_object, blob] {
        graph.plans[0].branch_anchor = forged;
        seed(tmp.path(), &graph);
        let error = TomlStore::new(tmp.path()).load().unwrap_err();
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
        TomlStore::new(tmp.path()).load().unwrap().plans[0].branch_anchor,
        anchor
    );
}

#[test]
fn locked_reload_candidate_write_and_v3_repair_reject_forged_anchors_without_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let (graph, _) = current_git_workspace(tmp.path(), PlanStatus::InProgress);
    seed(tmp.path(), &graph);
    let store = TomlStore::new(tmp.path());
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
        .save_with_revision(10, |workspace| {
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

    let error = store.repair_provenance_with_revision(10).unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(&toml_path).unwrap(), forged_bytes);
    assert_eq!(std::fs::read(&md_path).unwrap(), markdown_before);

    seed(tmp.path(), &graph);
    store.render().unwrap();
    let valid_bytes = std::fs::read(&toml_path).unwrap();
    let valid_markdown = std::fs::read(&md_path).unwrap();
    let error = store
        .save_with_revision(10, |mut workspace| {
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
    graph.schema_version = PREVIOUS_GRAPH_SCHEMA_VERSION;
    seed(tmp.path(), &graph);

    let repaired = TomlStore::new(tmp.path())
        .repair_provenance_with_revision(8)
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
    commit_file(tmp.path(), ".gitignore", "/.mine/\n", "initial");
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
    graph.schema_version = PREVIOUS_GRAPH_SCHEMA_VERSION;
    seed(tmp.path(), &graph);
    let repaired = TomlStore::new(tmp.path())
        .repair_provenance_with_revision(12)
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
    commit_file(tmp.path(), ".gitignore", "/.mine/\n", "initial");
    // mine-release-allow-plan-reference: ambiguous active-base fixture
    let mut active = canonical_node("plan01", PlanStatus::InProgress, &[]);
    active.branch_anchor.clear();
    let mut graph = workspace(21, vec![active]);
    graph.schema_version = PREVIOUS_GRAPH_SCHEMA_VERSION;
    seed(tmp.path(), &graph);
    let path = tmp.path().join("docs/plan/execution-graph.toml");
    let before = std::fs::read(&path).unwrap();

    let error = TomlStore::new(tmp.path())
        .repair_provenance_with_revision(21)
        .unwrap_err();
    assert_eq!(error.code(), "MINE_GRAPH_INVALID");
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn linked_invocation_reads_root_graph_and_resolves_shared_lineage_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let mut c01 = canonical_node("plan01-C01", PlanStatus::Rejected, &[]);
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    c01.compensating_plan = "plan01-C02".to_string();
    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
    let c02 = canonical_node("plan01-C02", PlanStatus::InProgress, &[]);
    seed(root, &workspace(42, vec![c01, c02]));
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "add",
            "-f",
            ".mine/config.toml",
            "docs/plan/execution-graph.toml",
        ])
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["commit", "--quiet", "-m", "seed"])
        .status()
        .unwrap();
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
