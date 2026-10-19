#![forbid(unsafe_code)]
#[path = "support/config.rs"]
mod fixture_config;
use mine::domain::{graph::PlanWorkspace, lifecycle_operation::*};
use mine::infrastructure::{
    file_lock,
    graph_checkpoint::CheckpointKernel,
    toml_store::{GRAPH_LOCK_NAME, render_markdown},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}

#[test]
fn ambient_index_redirection_is_refused_in_an_isolated_child_without_global_env_mutation() {
    const MARKER: &str = "MINE_CHECKPOINT_REDIRECTION_PROBE";
    if std::env::var_os(MARKER).is_some() {
        let f = Fixture::new();
        let guard = f.lock();
        let kernel = CheckpointKernel::under_lock(f.root(), &guard).unwrap();
        assert!(matches!(
            kernel.begin(
                OperationId::generate(),
                0,
                "fixture",
                &f.candidate,
                &mut NoInterruption
            ),
            Err(OperationError::Refused(
                "ambient Git redirection or policy override"
            ))
        ));
        assert_eq!(git(f.root(), &["rev-parse", "HEAD"]), f.before);
    } else {
        let dir = tempfile::tempdir().unwrap();
        let output=Command::new(std::env::current_exe().unwrap()).args(["--exact", "ambient_index_redirection_is_refused_in_an_isolated_child_without_global_env_mutation"])
            .env(MARKER, "1").env("GIT_INDEX_FILE", dir.path().join("foreign-index")).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!dir.path().join("foreign-index").exists());
    }
}
struct Fixture {
    dir: Option<tempfile::TempDir>,
    candidate: PlanWorkspace,
    before: String,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-b", "dev"]);
        for (key, value) in [
            ("user.name", "Kernel Fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
        ] {
            git(root, &["config", key, value]);
        }
        fs::write(root.join(".gitignore"), ".mine/runtime/\n.mine/locks/\n").unwrap();
        fs::write(root.join("seed"), "unchanged\n").unwrap();
        git(root, &["add", "seed", ".gitignore"]);
        git(root, &["commit", "-m", "seed"]);
        let baseline = git(root, &["rev-parse", "HEAD"]);
        fs::create_dir_all(root.join(".mine")).unwrap();
        fs::create_dir_all(root.join(".mine/plan")).unwrap();
        fs::write(
            root.join(".mine/config.toml"),
            fixture_config::canonical().to_toml(),
        )
        .unwrap();
        let mut ws = PlanWorkspace {
            schema_version: 4,
            revision: 0,
            project_id: "fixture".into(),
            workspace_id: "checkpoint-fixture".into(),
            stable_branch: "master".into(),
            integration_branch: "dev".into(),
            stable_baseline_commit: baseline,
            design_root: ".mine/design/index.md".into(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans: vec![],
        };
        fs::write(
            root.join(".mine/plan/execution-graph.toml"),
            toml::to_string(&ws).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(".mine/plan/execution-graph.md"),
            render_markdown(&ws).unwrap(),
        )
        .unwrap();
        git(
            root,
            &[
                "add",
                ".mine/config.toml",
                ".mine/plan/execution-graph.toml",
                ".mine/plan/execution-graph.md",
            ],
        );
        git(root, &["commit", "-m", "initial graph"]);
        let before = git(root, &["rev-parse", "HEAD"]);
        ws.revision = 1;
        Self {
            dir: Some(dir),
            candidate: ws,
            before,
        }
    }
    fn root(&self) -> &Path {
        self.dir.as_ref().unwrap().path()
    }
    fn lock(&self) -> file_lock::FileLock {
        file_lock::acquire_exclusive(
            &self.root().join(".mine/locks").join(GRAPH_LOCK_NAME),
            Duration::from_secs(1),
        )
        .unwrap()
    }
    fn begin(&self, kernel: &CheckpointKernel<'_>, id: &OperationId) {
        kernel
            .begin(
                id.clone(),
                0,
                "independent-fixture",
                &self.candidate,
                &mut NoInterruption,
            )
            .unwrap();
    }
    fn assert_complete(&self, kernel: &CheckpointKernel<'_>, id: &OperationId) {
        let result = kernel.resume(id, &mut NoInterruption).unwrap();
        assert!(terminal(&result.operation));
        assert_eq!(
            result
                .operation
                .fields()
                .checkpoint
                .as_ref()
                .unwrap()
                .fields()
                .revision_after,
            Some(1)
        );
        assert!(kernel.store().barriers().unwrap().global.is_empty());
        assert!(git(self.root(), &["status", "--porcelain"]).is_empty());
        assert_eq!(
            git(
                self.root(),
                &["rev-list", "--count", &format!("{}..HEAD", self.before)]
            ),
            "1"
        );
        let tip = git(self.root(), &["rev-parse", "HEAD"]);
        kernel.resume(id, &mut NoInterruption).unwrap();
        assert_eq!(tip, git(self.root(), &["rev-parse", "HEAD"]));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let path = self.dir.take().unwrap().keep();
            eprintln!("preserved failed checkpoint fixture: {}", path.display());
        }
    }
}
struct LoseOnce(Boundary);
impl BoundaryObserver for LoseOnce {
    fn reached(&mut self, b: Boundary) -> OperationResult<()> {
        if b == self.0 {
            Err(OperationError::Interrupted(b))
        } else {
            Ok(())
        }
    }
}

#[test]
fn initial_checkpoint_lost_boundaries_resume_without_seed_graph_or_new_identity() {
    use mine::infrastructure::{
        graph_checkpoint::GraphBefore, operation_store::RepositoryJournal, system::SystemUuidSource,
    };
    for boundary in [
        Boundary::Intent,
        Boundary::GraphWritten,
        Boundary::ViewWritten,
        Boundary::CasReturned,
        Boundary::IndexPublished,
        Boundary::Response,
    ] {
        let (directory, before) = initial_fixture();
        let root = directory.path();
        let guard = file_lock::acquire_exclusive(
            &root.join(".mine/locks").join(GRAPH_LOCK_NAME),
            Duration::from_secs(1),
        )
        .unwrap();
        let result = if boundary == Boundary::Intent {
            CheckpointKernel::initial_under_lock(
                root,
                &guard,
                0,
                "fixture-reviewer",
                &SystemUuidSource,
                &mut LoseOnce(boundary),
            )
            .map(|(_, record)| record)
        } else {
            let (kernel, record) = CheckpointKernel::initial_under_lock(
                root,
                &guard,
                0,
                "fixture-reviewer",
                &SystemUuidSource,
                &mut NoInterruption,
            )
            .unwrap();
            kernel.resume(
                &record.operation.fields().operation_id,
                &mut LoseOnce(boundary),
            )
        };
        assert!(
            matches!(result, Err(OperationError::Interrupted(actual)) if actual == boundary),
            "{boundary:?}: {result:?}"
        );
        let config = mine::domain::config::MineConfig::parse(
            &root.join(".mine/config.toml"),
            &fs::read_to_string(root.join(".mine/config.toml")).unwrap(),
        )
        .unwrap();
        let history = RepositoryJournal::under_lock(root, &config.repository_id, &guard).unwrap();
        let observed = history.observations().unwrap();
        assert_eq!(observed.len(), 1);
        let id = &observed[0].operation.fields().operation_id;
        let original_workspace = observed[0].operation.fields().workspace_id.clone();
        let record = history.query(id).unwrap();
        assert!(
            matches!(record.checkpoint.as_ref().unwrap().before, GraphBefore::Absent { ref predecessors } if predecessors.is_empty())
        );
        if boundary == Boundary::Intent {
            assert!(!root.join(".mine/plan/execution-graph.toml").exists());
            assert!(!root.join(".mine/plan/execution-graph.md").exists());
            assert_eq!(git(root, &["rev-parse", "HEAD"]), before);
        }
        if boundary == Boundary::GraphWritten {
            assert!(root.join(".mine/plan/execution-graph.toml").exists());
            assert!(!root.join(".mine/plan/execution-graph.md").exists());
        }
        let kernel = CheckpointKernel::initial_intent_under_lock(root, &guard, id).unwrap();
        let complete = kernel.resume(id, &mut NoInterruption).unwrap();
        assert!(terminal(&complete.operation));
        assert_eq!(complete.operation.fields().workspace_id, original_workspace);
        assert_eq!(
            complete
                .operation
                .fields()
                .checkpoint
                .as_ref()
                .unwrap()
                .fields()
                .revision_before,
            0
        );
        assert_eq!(
            complete
                .operation
                .fields()
                .checkpoint
                .as_ref()
                .unwrap()
                .fields()
                .revision_after,
            Some(1)
        );
        let graph: PlanWorkspace = toml::from_str(
            &fs::read_to_string(root.join(".mine/plan/execution-graph.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(graph.workspace_id, original_workspace);
        assert_eq!(graph.revision, 1);
        assert_eq!(graph.stable_baseline_commit, before);
        assert!(graph.plans.is_empty());
        assert_eq!(
            git(root, &["rev-list", "--count", &format!("{before}..HEAD")]),
            "1"
        );
        assert_eq!(git(root, &["rev-parse", "HEAD^"]), before);
        assert_eq!(
            git(
                root,
                &["diff-tree", "--no-commit-id", "--name-status", "-r", "HEAD"]
            ),
            "A\t.mine/plan/execution-graph.md\nA\t.mine/plan/execution-graph.toml"
        );
        assert!(git(root, &["status", "--porcelain"]).is_empty());
        let tip = git(root, &["rev-parse", "HEAD"]);
        kernel.resume(id, &mut NoInterruption).unwrap();
        assert_eq!(git(root, &["rev-parse", "HEAD"]), tip);
        assert!(kernel.store().barriers().unwrap().global.is_empty());
    }
}

fn initial_fixture() -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "-b", "dev"]);
    for (key, value) in [
        ("user.name", "Initial Fixture"),
        ("user.email", "fixture@example.invalid"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(root, &["config", key, value]);
    }
    fs::create_dir_all(root.join(".mine")).unwrap();
    fs::create_dir_all(root.join(".mine/design")).unwrap();
    fs::write(root.join(".gitignore"), ".mine/runtime/\n.mine/locks/\n").unwrap();
    fs::write(
        root.join(".mine/config.toml"),
        fixture_config::canonical().to_toml(),
    )
    .unwrap();
    let config = mine::domain::config::MineConfig::parse(
        &root.join(".mine/config.toml"),
        &fs::read_to_string(root.join(".mine/config.toml")).unwrap(),
    )
    .unwrap();
    let marker = mine::domain::design_marker::DesignMarker::new(
        config.repository_id,
        "2026-10-16T00:00:00Z".into(),
    );
    fs::write(
        root.join(".mine/design/.mine-design.toml"),
        marker.to_toml(),
    )
    .unwrap();
    fs::write(
        root.join(".mine/design/index.md"),
        "<!-- mine-managed-design -->\n# Fixture\n",
    )
    .unwrap();
    git(
        root,
        &[
            "add",
            ".gitignore",
            ".mine/config.toml",
            ".mine/design/.mine-design.toml",
            ".mine/design/index.md",
        ],
    );
    git(root, &["commit", "-m", "committed initial setup"]);
    let before = git(root, &["rev-parse", "HEAD"]);
    git(root, &["branch", "master"]);
    (directory, before)
}

#[test]
fn initial_public_entries_query_pending_absence_and_finalize_same_identity() {
    use mine::infrastructure::{operation_store::RepositoryJournal, system::SystemUuidSource};
    for boundary in [Boundary::Intent, Boundary::GraphWritten] {
        let (directory, before) = initial_fixture();
        let root = directory.path();
        let operation = {
            let guard = file_lock::acquire_exclusive(
                &root.join(".mine/locks").join(GRAPH_LOCK_NAME),
                Duration::from_secs(1),
            )
            .unwrap();
            if boundary == Boundary::Intent {
                assert!(matches!(
                    CheckpointKernel::initial_under_lock(
                        root,
                        &guard,
                        0,
                        "original-actor",
                        &SystemUuidSource,
                        &mut LoseOnce(boundary)
                    ),
                    Err(OperationError::Interrupted(Boundary::Intent))
                ));
            } else {
                let (kernel, record) = CheckpointKernel::initial_under_lock(
                    root,
                    &guard,
                    0,
                    "original-actor",
                    &SystemUuidSource,
                    &mut NoInterruption,
                )
                .unwrap();
                assert!(matches!(
                    kernel.resume(
                        &record.operation.fields().operation_id,
                        &mut LoseOnce(boundary)
                    ),
                    Err(OperationError::Interrupted(Boundary::GraphWritten))
                ));
            }
            let config = mine::domain::config::MineConfig::parse(
                &root.join(".mine/config.toml"),
                &fs::read_to_string(root.join(".mine/config.toml")).unwrap(),
            )
            .unwrap();
            let history = RepositoryJournal::under_lock(root, &config.repository_id, &guard)
                .unwrap()
                .observations()
                .unwrap();
            assert_eq!(history.len(), 1);
            history[0].operation.clone()
        };
        let revision = if boundary == Boundary::Intent {
            "0"
        } else {
            "1"
        };
        let id = operation.fields().operation_id.as_str();
        let (code, shown) = candidate_cli(root, &["operation", "show", "--operation-id", id]);
        assert_eq!(code, 0, "{shown}");
        assert_eq!(
            shown["data"]["operation"],
            serde_json::to_value(&operation).unwrap()
        );
        let (code, status) = candidate_cli(root, &["workspace", "status"]);
        assert_eq!(code, 0, "{status}");
        assert_eq!(status["data"]["revision"], revision.parse::<u64>().unwrap());
        assert_eq!(
            status["data"]["pending_operations"][0]["operation"]["operation_id"],
            id
        );
        assert_eq!(status["data"]["has_unresolved"], true);
        let (code, graph_status) = candidate_cli(root, &["graph", "status"]);
        assert_eq!(code, 0, "{graph_status}");
        assert_eq!(
            graph_status["data"]["revision"],
            revision.parse::<u64>().unwrap()
        );
        assert_eq!(
            graph_status["data"]["pending_operations"][0]["operation"]["operation_id"],
            id
        );
        for args in [
            vec![
                "workspace",
                "open",
                "--actor",
                "different-actor",
                "--expected-revision",
                revision,
            ],
            vec!["workspace", "close"],
        ] {
            let (code, refused) = candidate_cli(root, &args);
            assert_ne!(code, 0, "{refused}");
            assert_eq!(
                refused["error"]["code"], "MINE_GRAPH_CHECKPOINT_PENDING",
                "{refused}"
            );
        }
        assert_eq!(git(root, &["rev-parse", "HEAD"]), before);
        let input = ".mine/runtime/initial-recovery.json";
        fs::write(root.join(input), serde_json::to_vec(&serde_json::json!({"schema_version":1,"expected_stage_revision":operation.fields().stage_revision,"operation":operation,"verification":null})).unwrap()).unwrap();
        let (code, completed) = candidate_cli(
            root,
            &[
                "operation",
                "finalize",
                "--expected-revision",
                revision,
                "--input",
                input,
            ],
        );
        assert_eq!(code, 0, "{completed}");
        assert_eq!(completed["data"]["operation"]["operation_id"], id);
        assert_eq!(completed["data"]["operation"]["actor"], "original-actor");
        let tip = git(root, &["rev-parse", "HEAD"]);
        let (code, opened) = candidate_cli(
            root,
            &[
                "workspace",
                "open",
                "--actor",
                "repeat-actor",
                "--expected-revision",
                "1",
            ],
        );
        assert_eq!(code, 0, "{opened}");
        assert_eq!(
            opened["data"]["workspace_id"],
            operation.fields().workspace_id
        );
        assert_eq!(git(root, &["rev-parse", "HEAD"]), tip);
        let current = &completed["data"]["operation"];
        fs::write(root.join(input), serde_json::to_vec(&serde_json::json!({"schema_version":1,"expected_stage_revision":current["stage_revision"],"operation":current,"verification":null})).unwrap()).unwrap();
        let (code, replay) = candidate_cli(
            root,
            &[
                "operation",
                "finalize",
                "--expected-revision",
                "1",
                "--input",
                input,
            ],
        );
        assert_eq!(code, 0, "{replay}");
        assert_eq!(replay["data"]["replay"], true);
        assert_eq!(git(root, &["rev-parse", "HEAD"]), tip);
        assert!(git(root, &["status", "--porcelain"]).is_empty());
    }
}

#[test]
fn initial_workspace_cli_uses_configured_graph_paths_and_actual_stable_baseline() {
    let (directory, baseline) = initial_fixture();
    let root = directory.path();
    let config_path = root.join(".mine/config.toml");
    let mut config = mine::domain::config::MineConfig::parse(
        &config_path,
        &fs::read_to_string(&config_path).unwrap(),
    )
    .unwrap();
    config.graph.source = ".mine/plan/nested/source.toml".into();
    config.graph.rendered = ".mine/plan/nested/view.md".into();
    fs::write(&config_path, config.to_toml()).unwrap();
    git(root, &["add", ".mine/config.toml"]);
    git(root, &["commit", "-m", "configured graph layout"]);
    let parent = git(root, &["rev-parse", "HEAD"]);
    assert_ne!(parent, baseline);
    let (code, opened) = candidate_cli(
        root,
        &[
            "workspace",
            "open",
            "--actor",
            "fixture-actor",
            "--expected-revision",
            "0",
        ],
    );
    assert_eq!(code, 0, "{opened}");
    assert_eq!(opened["data"]["stable_baseline_commit"], baseline);
    assert_eq!(git(root, &["rev-parse", "HEAD^"]), parent);
    assert!(root.join(&config.graph.source).is_file());
    assert!(root.join(&config.graph.rendered).is_file());
    assert!(!root.join(".mine/plan/execution-graph.toml").exists());
    let (code, shown) = candidate_cli(root, &["graph", "status"]);
    assert_eq!(code, 0, "{shown}");
    assert_eq!(shown["data"]["revision"], 1);
    let tip = git(root, &["rev-parse", "HEAD"]);
    let (code, stale) = candidate_cli(
        root,
        &[
            "workspace",
            "open",
            "--actor",
            "fixture-actor",
            "--expected-revision",
            "0",
        ],
    );
    assert_ne!(code, 0, "{stale}");
    assert_eq!(stale["error"]["code"], "MINE_REVISION_CONFLICT");
    assert_eq!(git(root, &["rev-parse", "HEAD"]), tip);
}

fn candidate_cli(root: &Path, args: &[&str]) -> (i32, serde_json::Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_mine"))
        .current_dir(root)
        .args(["--format", "json"])
        .args(args)
        .output()
        .unwrap();
    let body = if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    };
    (
        output.status.code().unwrap(),
        serde_json::from_slice(body)
            .unwrap_or_else(|error| panic!("{args:?}: {error}: {}", String::from_utf8_lossy(body))),
    )
}

#[test]
fn initial_checkpoint_refuses_collisions_and_never_clobbers_late_foreign_view() {
    use mine::infrastructure::{operation_store::RepositoryJournal, system::SystemUuidSource};
    for mode in 0..4 {
        let (directory, _) = initial_fixture();
        let root = directory.path();
        fs::create_dir_all(root.join(".mine/plan")).unwrap();
        let source = root.join(".mine/plan/execution-graph.toml");
        let view = root.join(".mine/plan/execution-graph.md");
        if mode == 2 {
            fs::write(
                root.join(".gitignore"),
                ".mine/runtime/\n.mine/locks/\n.mine/plan/\n",
            )
            .unwrap();
            git(root, &["add", ".gitignore"]);
            git(root, &["commit", "-m", "ignored collision fixture"]);
        }
        match mode {
            0 => fs::write(&source, b"foreign source\n").unwrap(),
            1 | 2 => fs::write(&view, b"foreign view\n").unwrap(),
            _ => {
                fs::write(root.join("foreign"), b"foreign index entry\n").unwrap();
                git(root, &["add", "foreign"]);
            }
        }
        let head = git(root, &["rev-parse", "HEAD"]);
        let index = fs::read(root.join(".git/index")).unwrap();
        let guard = file_lock::acquire_exclusive(
            &root.join(".mine/locks").join(GRAPH_LOCK_NAME),
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(
            CheckpointKernel::initial_under_lock(
                root,
                &guard,
                0,
                "fixture",
                &SystemUuidSource,
                &mut NoInterruption
            )
            .is_err()
        );
        assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        if mode == 0 {
            assert_eq!(fs::read(&source).unwrap(), b"foreign source\n");
        }
        if mode == 1 || mode == 2 {
            assert_eq!(fs::read(&view).unwrap(), b"foreign view\n");
        }
        assert!(!root.join(".mine/runtime/operations/.catalog").exists());
    }
    struct LateView(std::path::PathBuf);
    impl BoundaryObserver for LateView {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == Boundary::GraphWritten {
                fs::write(&self.0, b"late foreign view\n")?;
            }
            Ok(())
        }
    }
    let (directory, before) = initial_fixture();
    let root = directory.path();
    let guard = file_lock::acquire_exclusive(
        &root.join(".mine/locks").join(GRAPH_LOCK_NAME),
        Duration::from_secs(1),
    )
    .unwrap();
    let (kernel, record) = CheckpointKernel::initial_under_lock(
        root,
        &guard,
        0,
        "fixture",
        &SystemUuidSource,
        &mut NoInterruption,
    )
    .unwrap();
    let id = &record.operation.fields().operation_id;
    let view = root.join(".mine/plan/execution-graph.md");
    assert!(kernel.resume(id, &mut LateView(view.clone())).is_err());
    assert_eq!(fs::read(&view).unwrap(), b"late foreign view\n");
    assert_eq!(git(root, &["rev-parse", "HEAD"]), before);
    let pending = kernel.store().query(id).unwrap();
    assert!(!terminal(&pending.operation));
    assert_eq!(
        pending
            .operation
            .fields()
            .checkpoint
            .as_ref()
            .unwrap()
            .fields()
            .revision_after,
        Some(1)
    );
    assert!(kernel.resume(id, &mut NoInterruption).is_err());
    assert_eq!(fs::read(&view).unwrap(), b"late foreign view\n");
    let config = mine::domain::config::MineConfig::parse(
        &root.join(".mine/config.toml"),
        &fs::read_to_string(root.join(".mine/config.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        RepositoryJournal::under_lock(root, &config.repository_id, &guard)
            .unwrap()
            .observations()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn initial_workspace_concurrent_cli_open_has_one_uuid_and_one_reachable_commit() {
    let (directory, before) = initial_fixture();
    let root = directory.path();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            barrier.wait();
            candidate_cli(
                root,
                &[
                    "workspace",
                    "open",
                    "--actor",
                    "first",
                    "--expected-revision",
                    "0",
                ],
            )
        });
        let second = scope.spawn(|| {
            barrier.wait();
            candidate_cli(
                root,
                &[
                    "workspace",
                    "open",
                    "--actor",
                    "second",
                    "--expected-revision",
                    "0",
                ],
            )
        });
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(
        results.iter().filter(|(code, _)| *code == 0).count(),
        1,
        "{results:?}"
    );
    let loser = &results.iter().find(|(code, _)| *code != 0).unwrap().1;
    assert!(
        matches!(
            loser["error"]["code"].as_str(),
            Some("MINE_REVISION_CONFLICT" | "MINE_LOCK_TIMEOUT")
        ),
        "{loser}"
    );
    let graph: PlanWorkspace =
        toml::from_str(&fs::read_to_string(root.join(".mine/plan/execution-graph.toml")).unwrap())
            .unwrap();
    uuid::Uuid::parse_str(&graph.workspace_id).unwrap();
    assert_eq!(graph.revision, 1);
    assert_eq!(
        git(root, &["rev-list", "--count", &format!("{before}..HEAD")]),
        "1"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".mine/runtime/operations/.catalog")).unwrap())
            .unwrap();
    assert_eq!(catalog["operations"].as_array().unwrap().len(), 1);
    assert!(git(root, &["status", "--porcelain"]).is_empty());
}

#[test]
fn checkpoint_is_exact_two_file_commit_and_same_identity_final_response() {
    let f = Fixture::new();
    let lock = f.lock();
    let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
    let id = OperationId::generate();
    f.begin(&kernel, &id);
    f.assert_complete(&kernel, &id);
    assert_eq!(
        git(
            f.root(),
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        ),
        ".mine/plan/execution-graph.md\n.mine/plan/execution-graph.toml"
    );
}

#[test]
fn changed_private_index_flags_refuse_before_staging_with_same_tree_preserved() {
    struct ChangeFlags<'a> {
        root: &'a Path,
        index: String,
        changed: bool,
    }
    impl BoundaryObserver for ChangeFlags<'_> {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == Boundary::PrivateIndexCreated {
                let run = |args: &[&str]| {
                    let output = Command::new("git")
                        .current_dir(self.root)
                        .env("GIT_INDEX_FILE", &self.index)
                        .args(args)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    output.stdout
                };
                let before = run(&["write-tree"]);
                run(&["update-index", "--assume-unchanged", "--", "seed"]);
                assert_eq!(run(&["write-tree"]), before);
                self.changed = true;
            }
            Ok(())
        }
    }
    let fixture = Fixture::new();
    let lock = fixture.lock();
    let kernel = CheckpointKernel::under_lock(fixture.root(), &lock).unwrap();
    let id = OperationId::generate();
    fixture.begin(&kernel, &id);
    let head = git(fixture.root(), &["rev-parse", "HEAD"]);
    let index = fs::read(fixture.root().join(".git/index")).unwrap();
    let relative = format!(".mine/runtime/operations/{}.index", id.as_str());
    let mut observer = ChangeFlags {
        root: fixture.root(),
        index: relative.clone(),
        changed: false,
    };
    let error = kernel.resume(&id, &mut observer).unwrap_err();
    assert!(observer.changed);
    assert!(
        error
            .to_string()
            .contains("private index is not exact validated source copy"),
        "{error}"
    );
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.root().join(".git/index")).unwrap(), index);
    assert!(fixture.root().join(relative).exists());
    let record = kernel.store().query(&id).unwrap();
    assert!(!terminal(&record.operation));
    assert!(!record.checkpoint.as_ref().unwrap().cas_attempted);
    assert!(
        record
            .checkpoint
            .as_ref()
            .unwrap()
            .candidate_commit
            .is_none()
    );
    assert_eq!(
        text_graph(fixture.root()),
        record.checkpoint.as_ref().unwrap().source_after
    );
    let private = fixture.root().join(&observer.index);
    let private_bytes = fs::read(&private).unwrap();
    let journal = fixture
        .root()
        .join(format!(".mine/runtime/operations/{}.json", id.as_str()));
    let journal_bytes = fs::read(&journal).unwrap();
    assert!(kernel.resume(&id, &mut NoInterruption).is_err());
    assert_eq!(fs::read(&private).unwrap(), private_bytes);
    assert_eq!(fs::read(&journal).unwrap(), journal_bytes);
    assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.root().join(".git/index")).unwrap(), index);
}

#[test]
fn every_recorded_boundary_resumes_without_duplicate_transition_or_reachable_commit() {
    for boundary in [
        Boundary::Intent,
        Boundary::GraphWritten,
        Boundary::GraphRecorded,
        Boundary::ViewWritten,
        Boundary::ViewRecorded,
        Boundary::PrivateIndexCreated,
        Boundary::CandidateCreated,
        Boundary::CandidateRecorded,
        Boundary::LockClaimed,
        Boundary::LockWritten,
        Boundary::LockRecorded,
        Boundary::CasRecorded,
        Boundary::CasReturned,
        Boundary::RefRecorded,
        Boundary::PostRefObserved,
        Boundary::IndexPublished,
        Boundary::IndexRecorded,
        Boundary::Complete,
        Boundary::Response,
    ] {
        let f = Fixture::new();
        let lock = f.lock();
        let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
        let id = OperationId::generate();
        let result = if boundary == Boundary::Intent {
            kernel.begin(
                id.clone(),
                0,
                "independent-fixture",
                &f.candidate,
                &mut LoseOnce(boundary),
            )
        } else {
            f.begin(&kernel, &id);
            kernel.resume(&id, &mut LoseOnce(boundary))
        };
        assert!(
            matches!(result, Err(OperationError::Interrupted(b)) if b == boundary),
            "{boundary:?}: {result:?}"
        );
        let recorded = kernel.store().query(&id).unwrap();
        let candidate = recorded
            .checkpoint
            .as_ref()
            .unwrap()
            .candidate_commit
            .clone();
        let revision = recorded.operation.fields().stage_revision;
        drop(kernel);
        let recovered = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
        f.assert_complete(&recovered, &id);
        let final_record = recovered.store().query(&id).unwrap();
        if candidate.is_some() {
            assert_eq!(
                final_record.checkpoint.unwrap().candidate_commit,
                candidate,
                "{boundary:?}"
            );
        }
        assert!(final_record.operation.fields().stage_revision >= revision);
    }
}

#[test]
fn exact_index_witness_never_hides_same_tree_flag_drift_in_one_request() {
    struct Change<'a> {
        root: &'a Path,
        boundary: Boundary,
        changed: Option<Vec<u8>>,
    }
    impl BoundaryObserver for Change<'_> {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == self.boundary {
                // Simulate an arbitrary external writer without trying to
                // acquire or delete MINE's already-owned index.lock.
                let scratch = self.root.join(".mine/runtime/foreign-index-fixture");
                fs::write(&scratch, fs::read(self.root.join(".git/index")).unwrap()).unwrap();
                let run = |args: &[&str]| {
                    let out = Command::new("git")
                        .current_dir(self.root)
                        .env("GIT_INDEX_FILE", &scratch)
                        .args(args)
                        .output()
                        .unwrap();
                    assert!(
                        out.status.success(),
                        "{args:?}: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                    out.stdout
                };
                let before = run(&["write-tree"]);
                run(&["update-index", "--assume-unchanged", "seed"]);
                assert_eq!(run(&["write-tree"]), before);
                let changed = fs::read(scratch).unwrap();
                fs::write(self.root.join(".git/index"), &changed).unwrap();
                self.changed = Some(changed);
            }
            Ok(())
        }
    }
    for boundary in [
        Boundary::ViewRecorded,
        Boundary::LockClaimed,
        Boundary::LockRecorded,
        Boundary::RefRecorded,
        Boundary::PostRefObserved,
        Boundary::IndexPublished,
    ] {
        let fixture = Fixture::new();
        let lock = fixture.lock();
        let kernel = CheckpointKernel::under_lock(fixture.root(), &lock).unwrap();
        let id = OperationId::generate();
        fixture.begin(&kernel, &id);
        let mut observer = Change {
            root: fixture.root(),
            boundary,
            changed: None,
        };
        let error = kernel.resume(&id, &mut observer).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("skip/assume-unchanged/unmerged index entries"),
            "{boundary:?}: {error}"
        );
        assert_eq!(
            fs::read(fixture.root().join(".git/index")).unwrap(),
            observer.changed.unwrap()
        );
        assert!(!terminal(&kernel.store().query(&id).unwrap().operation));
        let head = git(fixture.root(), &["rev-parse", "HEAD"]);
        let graph = fs::read(fixture.root().join(".mine/plan/execution-graph.toml")).unwrap();
        let bytes = fs::read(fixture.root().join(".git/index")).unwrap();
        assert!(kernel.resume(&id, &mut NoInterruption).is_err());
        assert_eq!(git(fixture.root(), &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(fixture.root().join(".git/index")).unwrap(), bytes);
        assert_eq!(
            fs::read(fixture.root().join(".mine/plan/execution-graph.toml")).unwrap(),
            graph
        );
    }
}

#[test]
fn unjournaled_lock_creation_is_unknown_and_never_stolen() {
    let f = Fixture::new();
    let lock = f.lock();
    let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
    let id = OperationId::generate();
    f.begin(&kernel, &id);
    assert!(matches!(
        kernel.resume(&id, &mut LoseOnce(Boundary::LockCreated)),
        Err(OperationError::Interrupted(_))
    ));
    let actual_lock = f.root().join(".git/index.lock");
    let bytes = fs::read(&actual_lock).unwrap();
    assert!(kernel.resume(&id, &mut NoInterruption).is_err());
    assert_eq!(fs::read(actual_lock).unwrap(), bytes);
    assert_eq!(git(f.root(), &["rev-parse", "HEAD"]), f.before);
    assert_eq!(kernel.store().barriers().unwrap().global.len(), 1);
}

#[test]
fn foreign_dirty_index_untracked_merge_sparse_and_skip_worktree_refuse_before_graph_write() {
    for mode in [
        "staged",
        "unstaged",
        "untracked",
        "merge",
        "sparse",
        "split",
        "skip",
        "assume",
        "lock",
        "replacement",
        "graft",
    ] {
        let f = Fixture::new();
        match mode {
            "staged" => {
                fs::write(f.root().join("seed"), "foreign").unwrap();
                git(f.root(), &["add", "seed"]);
            }
            "unstaged" => fs::write(f.root().join("seed"), "foreign").unwrap(),
            "untracked" => fs::write(f.root().join("foreign"), "foreign").unwrap(),
            "merge" => fs::write(f.root().join(".git/MERGE_HEAD"), &f.before).unwrap(),
            "sparse" => {
                git(f.root(), &["config", "core.sparseCheckout", "true"]);
            }
            "split" => {
                git(f.root(), &["update-index", "--split-index"]);
            }
            "skip" => {
                git(f.root(), &["update-index", "--skip-worktree", "seed"]);
            }
            "assume" => {
                git(f.root(), &["update-index", "--assume-unchanged", "seed"]);
            }
            "lock" => fs::write(f.root().join(".git/index.lock"), "unknown").unwrap(),
            "replacement" => {
                git(f.root(), &["replace", &f.before, &format!("{}^", f.before)]);
            }
            "graft" => fs::write(f.root().join(".git/info/grafts"), &f.before).unwrap(),
            _ => unreachable!(),
        }
        let before = fs::read(f.root().join(".mine/plan/execution-graph.toml")).unwrap();
        let index = fs::read(f.root().join(".git/index")).unwrap();
        let lock = f.lock();
        let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
        assert!(
            kernel
                .begin(
                    OperationId::generate(),
                    0,
                    "fixture",
                    &f.candidate,
                    &mut NoInterruption
                )
                .is_err(),
            "{mode}"
        );
        assert_eq!(
            before,
            fs::read(f.root().join(".mine/plan/execution-graph.toml")).unwrap()
        );
        assert_eq!(
            index,
            fs::read(f.root().join(".git/index")).unwrap(),
            "{mode}"
        );
    }
}

#[test]
fn signing_or_identity_failure_is_pending_and_filters_or_index_mode_drift_refuse() {
    for mode in ["signing", "identity", "filter", "mode"] {
        let f = Fixture::new();
        let lock = f.lock();
        let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
        let id = OperationId::generate();
        f.begin(&kernel, &id);
        if mode == "signing" {
            git(f.root(), &["config", "commit.gpgSign", "true"]);
            git(
                f.root(),
                &["config", "gpg.program", "mine-no-such-signing-program"],
            );
        } else if mode == "identity" {
            git(f.root(), &["config", "user.useConfigOnly", "true"]);
            // Empty local values mask inherited global identity. Unsetting
            // local values would let Git legitimately use the host's identity.
            git(f.root(), &["config", "user.name", ""]);
            git(f.root(), &["config", "user.email", ""]);
        } else if mode == "filter" {
            // Repository policy committed before a fresh intent below.
            fs::write(
                f.root().join(".git/info/attributes"),
                ".mine/plan/* filter=not-supported\n",
            )
            .unwrap();
        } else {
            // Actual graph path mode differs: refuse before publication.
            git(
                f.root(),
                &[
                    "update-index",
                    "--chmod=+x",
                    ".mine/plan/execution-graph.toml",
                ],
            );
        }
        assert!(kernel.resume(&id, &mut NoInterruption).is_err(), "{mode}");
        assert_eq!(git(f.root(), &["rev-parse", "HEAD"]), f.before);
        assert_eq!(kernel.store().barriers().unwrap().global.len(), 1);
        if matches!(mode, "signing" | "identity") {
            assert!(text_graph(f.root()).contains("revision = 1"));
            git(f.root(), &["config", "commit.gpgSign", "false"]);
            git(f.root(), &["config", "user.name", "Kernel Fixture"]);
            git(
                f.root(),
                &["config", "user.email", "fixture@example.invalid"],
            );
            f.assert_complete(&kernel, &id);
        }
    }
}
fn text_graph(root: &Path) -> String {
    fs::read_to_string(root.join(".mine/plan/execution-graph.toml")).unwrap()
}

struct Drift {
    root: PathBuf,
    mode: &'static str,
    boundary: Boundary,
}
impl BoundaryObserver for Drift {
    fn reached(&mut self, b: Boundary) -> OperationResult<()> {
        if b == self.boundary {
            match self.mode {
                "head" => {
                    git(&self.root, &["symbolic-ref", "HEAD", "refs/heads/foreign"]);
                }
                "worktree" => {
                    fs::write(self.root.join("seed"), "foreign")?;
                }
                "index" => {
                    fs::write(self.root.join(".git/index"), b"foreign")?;
                }
                "graph" => {
                    fs::write(self.root.join(".mine/plan/execution-graph.md"), b"foreign")?;
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }
}
#[test]
fn drift_after_ref_and_immediately_before_publication_preserves_foreign_evidence() {
    for mode in ["head", "worktree", "index", "graph"] {
        let f = Fixture::new();
        let lock = f.lock();
        let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
        let id = OperationId::generate();
        f.begin(&kernel, &id);
        assert!(
            kernel
                .resume(
                    &id,
                    &mut Drift {
                        root: f.root().into(),
                        mode,
                        boundary: Boundary::PostRefObserved
                    }
                )
                .is_err(),
            "{mode}"
        );
        let record = kernel.store().query(&id).unwrap();
        assert!(record.checkpoint.as_ref().unwrap().cas_attempted);
        assert_eq!(
            git(f.root(), &["rev-parse", "refs/heads/dev"]),
            record
                .checkpoint
                .as_ref()
                .unwrap()
                .candidate_commit
                .as_ref()
                .unwrap()
                .as_str()
        );
        assert!(f.root().join(".git/index.lock").exists());
        assert!(!terminal(&record.operation));
    }
}

#[test]
fn reference_transaction_hook_cannot_switch_head_and_get_false_success() {
    let f = Fixture::new();
    fs::write(f.root().join(".git/hooks/reference-transaction"), "#!/bin/sh\nif test \"$1\" = committed; then git symbolic-ref HEAD refs/heads/foreign; fi\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            f.root().join(".git/hooks/reference-transaction"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let lock = f.lock();
    let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
    let id = OperationId::generate();
    f.begin(&kernel, &id);
    assert!(kernel.resume(&id, &mut NoInterruption).is_err());
    assert_eq!(
        git(f.root(), &["symbolic-ref", "HEAD"]),
        "refs/heads/foreign"
    );
    let record = kernel.store().query(&id).unwrap();
    assert_eq!(
        git(f.root(), &["rev-parse", "refs/heads/dev"]),
        record
            .checkpoint
            .as_ref()
            .unwrap()
            .candidate_commit
            .as_ref()
            .unwrap()
            .as_str()
    );
    assert!(f.root().join(".git/index.lock").exists());
}

#[test]
fn recorded_candidate_cannot_be_replaced_and_wrong_revision_never_creates_intent() {
    let f = Fixture::new();
    let lock = f.lock();
    let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
    assert!(
        kernel
            .begin(
                OperationId::generate(),
                17,
                "fixture",
                &f.candidate,
                &mut NoInterruption
            )
            .is_err()
    );
    assert!(kernel.store().observations().unwrap().is_empty());
    let id = OperationId::generate();
    f.begin(&kernel, &id);
    let journal = mine::infrastructure::operation_store::OperationLocation::new(f.root(), &id)
        .unwrap()
        .resolve()
        .unwrap();
    let original = fs::read(&journal).unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_slice(&original).unwrap();
    corrupt["operation"]["outcome"] = "refused".into();
    fs::write(&journal, serde_json::to_vec(&corrupt).unwrap()).unwrap();
    assert!(
        kernel.store().query(&id).is_err(),
        "checkpoint refusal must not release a recorded intent"
    );
    assert!(kernel.store().barriers().is_err());
    fs::write(&journal, original).unwrap();
    assert!(
        kernel
            .resume(&id, &mut LoseOnce(Boundary::CandidateRecorded))
            .is_err()
    );
    let old = kernel.store().query(&id).unwrap();
    let mut changed_head = old.clone();
    let mut fields = changed_head.operation.fields().clone();
    fields.stage_revision += 1;
    let mut facts = fields.checkpoint.as_ref().unwrap().fields().clone();
    facts.original_head = mine::domain::graph::CommitId::new("a".repeat(40)).unwrap();
    fields.checkpoint = Some(CheckpointFacts::new(facts).unwrap());
    changed_head.operation = LifecycleOperation::new(fields).unwrap();
    assert!(matches!(
        kernel
            .store()
            .advance(old.operation.fields().stage_revision, &changed_head),
        Err(OperationError::Refused("checkpoint evidence changed"))
    ));
    let mut forged = old.clone();
    forged.checkpoint.as_mut().unwrap().candidate_commit =
        Some(mine::domain::graph::CommitId::new(f.before.clone()).unwrap());
    assert!(
        kernel
            .store()
            .advance(old.operation.fields().stage_revision, &forged)
            .is_err()
    );
    f.assert_complete(&kernel, &id);
}

#[test]
fn builtin_eol_conversion_preserves_canonical_blobs_and_checkout_equivalence() {
    let f = Fixture::new();
    git(f.root(), &["config", "core.autocrlf", "true"]);
    // Native coercion and last-value precedence, not a handwritten boolean
    // parser or an OR over obsolete included/overridden values.
    git(f.root(), &["config", "--add", "core.sparseCheckout", "on"]);
    git(f.root(), &["config", "--add", "core.sparseCheckout", "off"]);
    git(f.root(), &["config", "index.sparse", "0"]);
    git(
        f.root(),
        &["config", "--add", "core.fsmonitor", "not-current-policy"],
    );
    git(f.root(), &["config", "--add", "core.fsmonitor", "false"]);
    let lock = f.lock();
    let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
    let id = OperationId::generate();
    f.begin(&kernel, &id);
    f.assert_complete(&kernel, &id);
    assert_eq!(
        git(f.root(), &["show", "HEAD:.mine/plan/execution-graph.toml"]),
        text_graph(f.root()).trim()
    );
}

#[test]
fn batched_index_policy_preserves_native_boolean_and_text_boundaries() {
    type PolicySetting = (&'static str, Option<&'static str>);
    type PolicyCase = (&'static str, &'static [PolicySetting], bool);
    let cases: &[PolicyCase] = &[
        (
            "canonical-last-wins",
            &[
                ("core.sparseCheckout", Some("true")),
                ("core.sparseCheckout", Some("false")),
            ],
            true,
        ),
        (
            "native-aliases",
            &[
                ("core.sparseCheckout", Some("on")),
                ("core.sparseCheckout", Some("off")),
                ("index.sparse", Some("0")),
            ],
            true,
        ),
        ("empty-boolean", &[("core.splitIndex", Some(""))], true),
        (
            "invalid-earlier-boolean",
            &[
                ("core.sparseCheckout", Some("invalid")),
                ("core.sparseCheckout", Some("false")),
            ],
            false,
        ),
        ("valueless-boolean", &[("core.sparseCheckout", None)], false),
        ("enabled-boolean", &[("index.sparse", Some("true"))], false),
        (
            "text-last-wins",
            &[
                ("core.fsmonitor", Some("old-hook")),
                ("core.fsmonitor", Some("false")),
            ],
            true,
        ),
        ("fsmonitor-hook", &[("core.fsmonitor", Some("hook"))], false),
        ("autocrlf-invalid", &[("core.autocrlf", Some("off"))], false),
        ("autocrlf-input", &[("core.autocrlf", Some("input"))], true),
        ("valueless-text", &[("core.fsmonitor", None)], false),
    ];
    for (name, values, accepted) in cases {
        let f = Fixture::new();
        for (key, value) in *values {
            if let Some(value) = value {
                git(f.root(), &["config", "--add", key, value]);
            } else {
                let (section, key) = key.split_once('.').unwrap();
                let path = f.root().join(".git/config");
                let mut bytes = fs::read(&path).unwrap();
                bytes.extend_from_slice(format!("\n[{section}]\n\t{key}\n").as_bytes());
                fs::write(path, bytes).unwrap();
            }
        }
        let before_graph = fs::read(f.root().join(".mine/plan/execution-graph.toml")).unwrap();
        let before_index = fs::read(f.root().join(".git/index")).unwrap();
        let lock = f.lock();
        let kernel = CheckpointKernel::under_lock(f.root(), &lock).unwrap();
        let result = kernel.begin(
            OperationId::generate(),
            0,
            "policy-fixture",
            &f.candidate,
            &mut NoInterruption,
        );
        assert_eq!(result.is_ok(), *accepted, "{name}: {result:?}");
        // Invalid core policy also makes unrelated Git commands refuse. Read
        // the exact fixture ref directly rather than weakening its config to
        // perform our no-mutation assertion.
        assert_eq!(
            fs::read_to_string(f.root().join(".git/refs/heads/dev"))
                .unwrap()
                .trim(),
            f.before,
            "{name}"
        );
        assert_eq!(
            fs::read(f.root().join(".git/index")).unwrap(),
            before_index,
            "{name}"
        );
        assert_eq!(
            fs::read(f.root().join(".mine/plan/execution-graph.toml")).unwrap(),
            before_graph,
            "{name}"
        );
        if !accepted {
            assert!(kernel.store().observations().unwrap().is_empty(), "{name}");
        }
    }
}
