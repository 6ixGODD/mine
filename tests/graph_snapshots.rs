#![forbid(unsafe_code)]

#[allow(dead_code)]
mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mine::application::graph_service::GraphService;
use mine::domain::error::MineError;
use mine::domain::graph::{GraphIndex, PlanWorkspace};
use mine::domain::status::PlanStatus;
use mine::infrastructure::git_evidence::{GitChild, GitSpawner, SystemGit};
use mine::infrastructure::toml_store::{TomlStore, render_markdown};
use serde_json::{Value, json};

#[derive(Default)]
struct CountingGit(Mutex<Vec<Vec<String>>>, AtomicUsize);

impl GitSpawner for CountingGit {
    fn spawn(&self, root: &Path, args: &[&str]) -> std::io::Result<GitChild> {
        match SystemGit.spawn(root, args) {
            Ok(child) => {
                self.0
                    .lock()
                    .unwrap()
                    .push(args.iter().map(|s| s.to_string()).collect());
                Ok(child)
            }
            Err(error) => {
                self.1.fetch_add(1, Ordering::SeqCst);
                Err(error)
            }
        }
    }
}

fn git(root: &Path, args: &[&str], input: &[u8]) -> String {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn fixture(size: usize, unique: bool) -> (tempfile::TempDir, std::path::PathBuf) {
    let (tmp, root) = common::seeded_repo(vec![]);
    git(&root, &["config", "user.name", "Snapshot Test"], &[]);
    git(
        &root,
        &["config", "user.email", "snapshot@example.invalid"],
        &[],
    );
    // A single setup child produces all real, distinct commit objects. These
    // setup launches are deliberately outside operation-count measurements.
    let baseline = git(&root, &["rev-parse", "HEAD"], &[]);
    let mut import = String::new();
    for i in 0..size.max(1) {
        import.push_str(&format!("commit refs/heads/dev\nmark :{}\ncommitter Snapshot Test <snapshot@example.invalid> {} +0000\ndata {}\nnode-{}\n", i + 1, 1700000000 + i, format!("node-{i}").len(), i));
        let parent = if i == 0 {
            baseline.clone()
        } else {
            format!(":{i}")
        };
        import.push_str(&format!("from {parent}\n\n"));
    }
    import.push_str("done\n");
    git(
        &root,
        &["fast-import", "--quiet", "--done"],
        import.as_bytes(),
    );
    let oids: Vec<_> = git(
        &root,
        &["rev-list", "--reverse", "dev", &format!("^{baseline}")],
        &[],
    )
    .lines()
    .map(str::to_string)
    .collect();
    let mut ws = common::load_graph(&root);
    ws.plans = (0..size)
        .map(|i| {
            let mut node = common::node(&fixture_id(i), PlanStatus::Implemented, &[], &[]);
            node.branch_anchor = oids[if unique { i } else { 0 }].clone();
            // Explicit historical holder identity for the wave/scheduling read;
            // this is not a native start/preparation or acceptance receipt.
            node.owner = "snapshot-fixture-executor".into();
            node.run_id = format!("snapshot-fixture-run-{i}");
            node.started_at = "2026-10-17T00:00:00Z".into();
            node
        })
        .collect();
    committed_seed(&root, &ws);
    std::fs::create_dir_all(root.join(".mine/runtime/snapshot-traces")).unwrap();
    (tmp, root)
}

fn fixture_id(index: usize) -> String {
    format!("plan{:02}-{:02}", index / 99 + 1, index % 99 + 1)
}

fn seed(root: &Path, ws: &PlanWorkspace) {
    let store = TomlStore::new(root);
    for node in &ws.plans {
        let body = root.join(&node.path);
        if !body.exists() {
            std::fs::write(body, format!("# Private {} snapshot fixture\n", node.id)).unwrap();
        }
    }
    std::fs::write(store.toml_path(), toml::to_string(ws).unwrap()).unwrap();
    std::fs::write(store.md_path(), render_markdown(ws).unwrap()).unwrap();
}

fn committed_seed(root: &Path, ws: &PlanWorkspace) {
    seed(root, ws);
    git(root, &["add", "--", ".mine/plan"], &[]);
    if !git(root, &["diff", "--cached", "--name-only"], &[]).is_empty() {
        git(
            root,
            &[
                "commit",
                "--quiet",
                "-m",
                "explicit private snapshot inputs",
            ],
            &[],
        );
    }
    assert_eq!(git(root, &["status", "--porcelain=v1", "-uall"], &[]), "");
}

fn checkpoint_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let (tmp, root) = fixture(10, true);
    git(&root, &["config", "core.autocrlf", "false"], &[]);
    git(&root, &["config", "commit.gpgSign", "false"], &[]);
    std::fs::write(root.join(".gitignore"), ".mine/runtime/\n.mine/locks/\n").unwrap();
    let config = mine::cli::context::load_config(&root).unwrap();
    let design_index = root.join(&config.design.root);
    std::fs::create_dir_all(design_index.parent().unwrap()).unwrap();
    std::fs::write(&design_index, "# Isolated checkpoint fixture Design\n").unwrap();
    std::fs::write(
        root.join(&config.design.marker),
        mine::domain::design_marker::DesignMarker::new(
            config.repository_id.clone(),
            "2026-10-16T00:00:00Z".into(),
        )
        .to_toml(),
    )
    .unwrap();
    git(
        &root,
        &[
            "add",
            ".gitignore",
            ".mine/config.toml",
            &config.design.root,
            &config.design.marker,
            ".mine/plan/execution-graph.toml",
            ".mine/plan/execution-graph.md",
        ],
        &[],
    );
    git(
        &root,
        &["commit", "-m", "clean native checkpoint fixture"],
        &[],
    );
    (tmp, root)
}

#[test]
fn request_snapshot_is_shared_but_never_shared_between_requests() {
    let (_tmp, root) = fixture(10, false);
    let counter = Arc::new(CountingGit::default());
    let store = TomlStore::new(&root).with_git_spawner(counter.clone());
    let graph = GraphService::new(&store);
    let first = graph.snapshot().unwrap();
    graph.validate().unwrap();
    graph.status().unwrap();
    graph.ready().unwrap();
    graph.wave().unwrap();
    assert!(std::rc::Rc::ptr_eq(&first, &graph.snapshot().unwrap()));
    graph.worktrees().unwrap();
    graph.worktrees().unwrap();
    first.refs().unwrap();
    first.refs().unwrap();
    assert_eq!(graph.snapshot_loads(), 1);
    assert_eq!(first.git_diagnostics().object_batches, 1);
    let calls = counter.0.lock().unwrap();
    assert_eq!(calls.iter().filter(|a| a[0] == "cat-file").count(), 1);
    assert_eq!(calls.iter().filter(|a| a[0] == "worktree").count(), 1);
    assert_eq!(calls.iter().filter(|a| a[0] == "for-each-ref").count(), 1);
    drop(calls);
    let mut changed = first.workspace().clone();
    changed.revision += 1;
    seed(&root, &changed);
    let second = GraphService::new(&store).snapshot().unwrap();
    assert_ne!(first.source_sha256, second.source_sha256);
    assert_eq!(second.workspace().revision, changed.revision);
    assert_eq!(first.common_directory, second.common_directory);
}

#[test]
fn stale_revision_refuses_before_snapshot_evidence_or_mutation() {
    let (_tmp, root) = fixture(10, true);
    let counter = Arc::new(CountingGit::default());
    let store = TomlStore::new(&root).with_git_spawner(counter.clone());
    let before = std::fs::read(store.toml_path()).unwrap();
    let revision = store.observed_revision().unwrap();
    let error = store
        .save_with_revision(revision + 1, "snapshot-fixture", |_| {
            panic!("stale closure ran")
        })
        .unwrap_err();
    assert!(matches!(error, MineError::RevisionConflict { .. }));
    assert!(counter.0.lock().unwrap().is_empty());
    assert_eq!(before, std::fs::read(store.toml_path()).unwrap());
    let diagnostics = store.transaction_diagnostics();
    assert_eq!(diagnostics.graph_loads, 1);
    assert_eq!(diagnostics.git.invocations, 0);
}

#[test]
fn lock_timeout_and_missing_object_never_write_graph_bytes() {
    let (_tmp, root) = fixture(10, false);
    let counter = Arc::new(CountingGit::default());
    let store = TomlStore::new(&root)
        .with_git_spawner(counter.clone())
        .with_lock_timeout(std::time::Duration::from_millis(50));
    let revision = store.observed_revision().unwrap();
    let bytes = std::fs::read(store.toml_path()).unwrap();
    let lock = mine::infrastructure::file_lock::acquire_exclusive(
        &root.join(".mine/locks/execution-graph.lock"),
        std::time::Duration::from_secs(1),
    )
    .unwrap();
    assert!(matches!(
        store.save_with_revision(revision, "snapshot-fixture", |_| panic!(
            "locked closure ran"
        )),
        Err(MineError::LockTimeout { .. })
    ));
    assert_eq!(store.transaction_diagnostics().graph_loads, 0);
    assert!(store.transaction_diagnostics().lock_wait >= std::time::Duration::from_millis(50));
    assert!(counter.0.lock().unwrap().is_empty());
    assert_eq!(bytes, std::fs::read(store.toml_path()).unwrap());
    drop(lock);
    let mut ws = common::load_graph(&root);
    ws.plans[0].branch_anchor = "0".repeat(40);
    seed(&root, &ws);
    let bytes = std::fs::read(store.toml_path()).unwrap();
    assert!(
        store
            .save_with_revision(revision, "snapshot-fixture", |_| panic!(
                "invalid evidence closure ran"
            ))
            .is_err()
    );
    assert_eq!(bytes, std::fs::read(store.toml_path()).unwrap());
}

#[test]
fn cli_preexisting_view_directory_refuses_without_false_committed_evidence() {
    let (_tmp, root) = fixture(10, true);
    let mut ws = common::load_graph(&root);
    // mine-release-allow-plan-reference: canonical unstarted withdrawal subject
    ws.plans
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        .push(common::node("plan99", PlanStatus::Ready, &[], &[]));
    committed_seed(&root, &ws);
    let store = TomlStore::new(&root);
    let revision = ws.revision;
    let source = std::fs::read(store.toml_path()).unwrap();
    let head = git(&root, &["rev-parse", "HEAD"], &[]);
    let index = std::fs::read(root.join(".git/index")).unwrap();
    let catalog = root.join(".mine/runtime/operations/.catalog");
    let before_catalog = std::fs::read(&catalog).ok();
    std::fs::remove_file(store.md_path()).unwrap();
    std::fs::create_dir(store.md_path()).unwrap();
    let trace = root.join(".mine/runtime/snapshot-traces/view-refusal.jsonl");
    let result = binary(
        &root,
        &trace,
        &[
            "graph",
            "repair-provenance",
            "--actor",
            "snapshot-fixture",
            "--expected-revision",
            &revision.to_string(),
        ],
    );
    assert_eq!(result.status.code(), Some(4), "{result:?}");
    let error: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(error["error"]["code"], "MINE_GRAPH_INVALID");
    assert!(error["error"]["details"]["committed"].is_null());
    let operation = uuid::Uuid::new_v4().to_string();
    let refused = binary(
        &root,
        &trace,
        &[
            // mine-release-allow-plan-reference: canonical unstarted withdrawal subject
            "plan",
            "withdraw",
            "--id",
            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
            "plan99",
            "--reason",
            "fixture",
            "--actor",
            "snapshot-fixture",
            "--expected-revision",
            &revision.to_string(),
            "--operation-id",
            &operation,
        ],
    );
    assert_eq!(refused.status.code(), Some(4), "{refused:?}");
    let error: Value = serde_json::from_slice(&refused.stderr).unwrap();
    assert_eq!(error["error"]["code"], "MINE_GRAPH_INVALID");
    assert!(error["error"]["details"]["committed"].is_null());
    assert_eq!(store.observed_revision().unwrap(), revision);
    assert_eq!(source, std::fs::read(store.toml_path()).unwrap());
    assert_eq!(head, git(&root, &["rev-parse", "HEAD"], &[]));
    assert_eq!(index, std::fs::read(root.join(".git/index")).unwrap());
    assert_eq!(std::fs::read(catalog).ok(), before_catalog);
    assert!(store.md_path().is_dir());
}

#[test]
fn plan_lookup_propagates_real_inventory_failure() {
    struct FailedInventory;
    impl GitSpawner for FailedInventory {
        fn spawn(&self, root: &Path, args: &[&str]) -> std::io::Result<GitChild> {
            if args.first() == Some(&"worktree") {
                return Err(std::io::Error::other("inventory unavailable"));
            }
            SystemGit.spawn(root, args)
        }
    }
    let (_tmp, root) = fixture(10, false);
    let store = TomlStore::new(&root).with_git_spawner(Arc::new(FailedInventory));
    let graph = GraphService::new(&store);
    let service = mine::application::plan_service::PlanService::new(&graph);
    // mine-release-allow-plan-reference: canonical private lookup subject
    let error = service.show_with_locations("plan01-01", &root).unwrap_err();
    assert_eq!(
        error.code(),
        "MINE_IO",
        "inventory failure must reach the caller: {error}"
    );
}

#[test]
fn failed_batch_spawn_is_not_counted_as_a_launched_batch() {
    struct FailedBatch;
    impl GitSpawner for FailedBatch {
        fn spawn(&self, root: &Path, args: &[&str]) -> std::io::Result<GitChild> {
            if args.first() == Some(&"cat-file") {
                return Err(std::io::Error::other("spawn unavailable"));
            }
            SystemGit.spawn(root, args)
        }
    }
    let (_tmp, root) = fixture(10, false);
    let mut request = mine::infrastructure::git_evidence::GitRequest::with_spawner(
        &root,
        Default::default(),
        Arc::new(FailedBatch),
    );
    let ws = common::load_graph(&root);
    assert!(
        request
            .validate_commits(&[ws.plans[0].branch_anchor.clone()])
            .is_err()
    );
    assert_eq!(request.diagnostics().invocations, 1); // native-width discovery only
    assert_eq!(request.diagnostics().object_batches, 0);
}

#[test]
fn counting_spawner_records_failed_attempt_separately_from_successful_launches() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing-cwd");
    assert!(!missing.exists());
    let counter = Arc::new(CountingGit::default());
    let mut request = mine::infrastructure::git_evidence::GitRequest::with_spawner(
        &missing,
        Default::default(),
        counter.clone(),
    );
    assert!(request.text(&["--version"]).is_err());
    assert!(counter.0.lock().unwrap().is_empty());
    assert_eq!(counter.1.load(Ordering::SeqCst), 1);
    assert_eq!(request.diagnostics().invocations, 0);
    assert_eq!(request.diagnostics().object_batches, 0);
    assert_eq!(request.diagnostics().peak_children, 0);
}

#[test]
fn preexisting_render_directory_refuses_before_any_checkpoint_effect() {
    let (_tmp, root) = checkpoint_fixture();
    let store = TomlStore::new(&root);
    let revision = store.observed_revision().unwrap();
    std::fs::remove_file(store.md_path()).unwrap();
    std::fs::create_dir(store.md_path()).unwrap();
    let source = std::fs::read(store.toml_path()).unwrap();
    let head = git(&root, &["rev-parse", "HEAD"], &[]);
    let index = std::fs::read(root.join(".git/index")).unwrap();
    let catalog_path = root.join(".mine/runtime/operations/.catalog");
    let catalog = std::fs::read(&catalog_path).ok();
    let error = store
        .save_with_revision(revision, "snapshot-fixture", |mut ws| {
            ws.revision += 1;
            Ok(ws)
        })
        .unwrap_err();
    assert!(!matches!(
        error,
        MineError::GraphCommitted { .. } | MineError::GraphCheckpointPending { .. }
    ));
    assert_eq!(store.observed_revision().unwrap(), revision);
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), source);
    assert_eq!(git(&root, &["rev-parse", "HEAD"], &[]), head);
    assert_eq!(std::fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(std::fs::read(&catalog_path).ok(), catalog);
    assert!(store.md_path().is_dir());
}

#[test]
fn committed_render_failure_recovers_same_operation_without_another_transition() {
    use mine::domain::lifecycle_operation::*;
    use mine::infrastructure::{file_lock::acquire_exclusive, graph_checkpoint::CheckpointKernel};
    let (_tmp, root) = checkpoint_fixture();
    let store = TomlStore::new(&root);
    let mut candidate = store.load().unwrap();
    let revision = candidate.revision;
    candidate.revision += 1;
    candidate.schema_version = mine::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION;
    let before_head = git(&root, &["rev-parse", "HEAD"], &[]);
    let before_index = std::fs::read(root.join(".git/index")).unwrap();
    let view = store.md_path().to_path_buf();
    let original_view = std::fs::read(&view).unwrap();
    let retained = root.join(".mine/runtime/injected-render-original.md");
    let id = OperationId::generate();
    struct InjectDirectory<'a> {
        view: &'a Path,
        retained: &'a Path,
        injected: bool,
    }
    impl BoundaryObserver for InjectDirectory<'_> {
        fn reached(&mut self, boundary: Boundary) -> OperationResult<()> {
            if boundary == Boundary::GraphWritten && !self.injected {
                std::fs::rename(self.view, self.retained)?;
                std::fs::create_dir(self.view)?;
                self.injected = true;
            }
            Ok(())
        }
    }
    let pending = {
        let guard = acquire_exclusive(
            &root.join(".mine/locks/execution-graph.lock"),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        let kernel = CheckpointKernel::under_lock(&root, &guard).unwrap();
        kernel
            .begin(
                id.clone(),
                revision,
                "snapshot-fixture",
                &candidate,
                &mut NoInterruption,
            )
            .unwrap();
        let mut observer = InjectDirectory {
            view: &view,
            retained: &retained,
            injected: false,
        };
        assert!(kernel.resume(&id, &mut observer).is_err());
        assert!(observer.injected);
        let record = kernel.store().query(&id).unwrap();
        assert!(!terminal(&record.operation));
        assert_eq!(
            record.operation.fields().outcome,
            OperationOutcome::CommittedRenderRepair
        );
        assert_eq!(
            record
                .operation
                .fields()
                .checkpoint
                .as_ref()
                .unwrap()
                .fields()
                .revision_after,
            Some(revision + 1)
        );
        assert!(!kernel.store().barriers().unwrap().global.is_empty());
        record.operation
    };
    let written_source = std::fs::read(store.toml_path()).unwrap();
    assert_eq!(store.observed_revision().unwrap(), revision + 1);
    assert_eq!(git(&root, &["rev-parse", "HEAD"], &[]), before_head);
    assert_eq!(
        std::fs::read(root.join(".git/index")).unwrap(),
        before_index
    );
    assert!(
        store
            .save_with_revision(revision + 1, "another-fixture", |_| panic!(
                "pending publication granted another mutation"
            ))
            .is_err()
    );
    // Undo only our injected directory and restore the retained original file.
    // Recovery still validates/publishes the real operation, not standalone render.
    std::fs::remove_dir(&view).unwrap();
    std::fs::rename(&retained, &view).unwrap();
    assert_eq!(std::fs::read(&view).unwrap(), original_view);
    let request_path = ".mine/runtime/render-recovery.json";
    std::fs::write(root.join(request_path), serde_json::to_vec(&json!({"schema_version":1,
        "expected_stage_revision":pending.fields().stage_revision,"operation":pending,"verification":null})).unwrap()).unwrap();
    let (outcome, response) = common::dispatch_json(
        &root,
        &[
            "operation",
            "finalize",
            "--expected-revision",
            &(revision + 1).to_string(),
            "--input",
            request_path,
        ],
    );
    assert_eq!(outcome.exit_code, 0, "{response}");
    assert_eq!(response["data"]["checkpoint_operation_id"], id.as_str());
    let operation = response["data"]["operation"].clone();
    assert_eq!(operation["operation_id"], id.as_str());
    let completed = git(&root, &["rev-parse", "HEAD"], &[]);
    assert_eq!(
        git(
            &root,
            &["rev-list", "--count", &format!("{before_head}..HEAD")],
            &[]
        ),
        "1"
    );
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), written_source);
    fs_write_replay_request(&root, request_path, &operation);
    let (repeated, replay) = common::dispatch_json(
        &root,
        &[
            "operation",
            "finalize",
            "--expected-revision",
            &(revision + 1).to_string(),
            "--input",
            request_path,
        ],
    );
    assert_eq!(repeated.exit_code, 0, "{replay}");
    assert_eq!(replay["data"]["replay"], true);
    assert_eq!(git(&root, &["rev-parse", "HEAD"], &[]), completed);
    assert_eq!(std::fs::read(store.toml_path()).unwrap(), written_source);
    assert_eq!(store.observed_revision().unwrap(), revision + 1);
    assert_eq!(git(&root, &["status", "--porcelain"], &[]), "");
    fn fs_write_replay_request(root: &Path, path: &str, operation: &Value) {
        std::fs::write(root.join(path), serde_json::to_vec(&json!({"schema_version":1,
            "expected_stage_revision":operation["stage_revision"],"operation":operation,"verification":null})).unwrap()).unwrap();
    }
}

fn trace_counts(path: &Path) -> (usize, usize, usize) {
    let events: Vec<Value> = std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut active = 0_usize;
    let mut peak = 0;
    let mut total = 0;
    let mut batches = 0;
    let mut cat_file_calls = 0;
    for event in events {
        match event["event"].as_str() {
            Some("start") => {
                active += 1;
                total += 1;
                peak = peak.max(active);
                if event["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "cat-file")
                {
                    cat_file_calls += 1;
                    if event["argv"].as_array().unwrap().iter().any(|arg| {
                        arg.as_str()
                            .is_some_and(|arg| arg.starts_with("--batch-check"))
                    }) {
                        batches += 1;
                    }
                }
            }
            Some("exit") => active = active.checked_sub(1).expect("exit without launch"),
            _ => {}
        }
    }
    assert_eq!(active, 0, "unreaped trace child");
    eprintln!(
        "TRACE2 classification cat_file_calls={cat_file_calls} batch_check_children={batches}"
    );
    (total, peak, batches)
}

fn binary(root: &Path, trace: &Path, args: &[&str]) -> std::process::Output {
    let home = root.join(".mine/runtime/fixture-home");
    std::fs::create_dir_all(&home).unwrap();
    Command::new(env!("CARGO_BIN_EXE_mine"))
        .arg("--repo")
        .arg(root)
        .args(args)
        .args(["--format", "json"])
        .env("GIT_TRACE2_EVENT", trace)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .output()
        .unwrap()
}

// Collect every independent size/anchor cell before failing the unchanged
// budgets. This is evidence collection, not permission to waive a red gate.
fn record_counts(
    failures: &mut Vec<String>,
    label: &str,
    trace: &Path,
    counts: (usize, usize, usize),
    limits: (usize, usize),
) {
    let (total, peak, batches) = counts;
    let (expected_batches, budget) = limits;
    eprintln!(
        "{label} total={total} peak={peak} batches={batches} budget={budget} expected_batches={expected_batches}"
    );
    if let Some(directory) = std::env::var_os("MINE_SNAPSHOT_TRACE_OUTPUT") {
        let directory = std::path::PathBuf::from(directory);
        let name = label
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        let retained = directory.join(format!("{name}.jsonl"));
        let result = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&directory)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&retained)?;
            file.write_all(&std::fs::read(trace)?)
        })();
        match result {
            Ok(()) => eprintln!("TRACE2 retained={}", retained.display()),
            Err(error) => failures.push(format!("{label}: trace retention failed: {error}")),
        }
    }
    if total > budget || peak != 1 || batches != expected_batches {
        failures.push(format!("{label}: observed total={total}/peak={peak}/batches={batches}; required total<={budget}/peak=1/batches={expected_batches}"));
    }
}

// ADR-0033 replaces only the old whole-native-withdraw oracle. Read/show and
// primitive-snapshot gates above/below are intentionally unchanged.
fn record_withdrawal_counts(failures: &mut Vec<String>, label: &str, trace: &Path, size: usize) {
    record_counts(
        failures,
        label,
        trace,
        trace_counts(trace),
        (2 * usize::from(size > 0), 96),
    );
    let launches = std::fs::read_to_string(trace)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|event| event["event"] == "start")
        .map(|event| {
            event["argv"]
                .as_array()
                .unwrap()
                .iter()
                .map(|arg| arg.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let has = |args: &[String], needle: &str| args.iter().any(|arg| arg == needle);
    let batch = |args: &[String]| args.iter().any(|arg| arg.starts_with("--batch-check="));
    let ordinary = launches
        .iter()
        .filter(|args| batch(args) && !has(args, "--no-pager"))
        .count();
    let controlled = launches
        .iter()
        .filter(|args| batch(args) && has(args, "--no-pager"))
        .count();
    let purpose = |prefix: &str| {
        launches
            .iter()
            .filter(|args| args.iter().any(|arg| arg.starts_with(prefix)))
            .count()
    };
    let raw = purpose("--batch=raw-blob ");
    let candidate = purpose("--batch=candidate ");
    let filters = launches
        .iter()
        .filter(|args| has(args, "cat-file") && has(args, "--filters"))
        .count();
    eprintln!(
        "{label} oracle=ADR-0033 ordinary_anchor_batches={ordinary} controlled_anchor_batches={controlled} raw_blob_batches={raw} candidate_proofs={candidate} checkout_filters={filters}"
    );
    let expected = usize::from(size > 0);
    if (ordinary, controlled, raw, candidate, filters) != (expected, expected, 1, 1, 2) {
        failures.push(format!(
            "{label}: wrong exact batch purposes/context counts"
        ));
    }
    let boundaries = launches
        .iter()
        .enumerate()
        .filter(|(_, args)| {
            has(args, "--no-pager")
                && has(args, "--path-format=absolute")
                && has(args, "info/grafts")
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if boundaries.len() != 8 {
        failures.push(format!(
            "{label}: expected eight fresh Root safety phases, found {}",
            boundaries.len()
        ));
        return;
    }
    let status_end = |start: usize, end: usize| {
        (start..end)
            .find(|index| has(&launches[*index], "status"))
            .map(|index| index + 1)
    };
    let Some(rendered_end) = status_end(boundaries[2], boundaries[3]) else {
        failures.push(format!(
            "{label}: persisted-graph observation has no status"
        ));
        return;
    };
    let Some(pre_cas_end) = status_end(boundaries[4], boundaries[5]) else {
        failures.push(format!("{label}: pre-CAS observation has no status"));
        return;
    };
    // The generated-index first proof's two queries move from completion into
    // private-index preparation, per the ADR's explicit movement rule.
    let phases = [
        ("discovery", 0, boundaries[0], 5),
        ("admission", boundaries[0], boundaries[1], 17),
        ("plan-effects", boundaries[1], boundaries[2], 10),
        ("graph-view-persisted", boundaries[2], rendered_end, 7),
        (
            "private-index-tree-candidate",
            rendered_end,
            boundaries[3],
            20,
        ),
        ("index-lock-acquired", boundaries[3], boundaries[4], 7),
        ("pre-CAS", boundaries[4], pre_cas_end, 7),
        ("CAS-confirmation", pre_cas_end, boundaries[5], 2),
        ("post-CAS", boundaries[5], boundaries[6], 7),
        ("post-ref-pre-publication", boundaries[6], boundaries[7], 7),
        ("completion", boundaries[7], launches.len(), 7),
    ];
    for (name, start, end, allowance) in phases {
        let observed = end - start;
        eprintln!("{label} phase={name} launches={observed} allowance={allowance}");
        if observed > allowance {
            failures.push(format!(
                "{label}: phase={name} launches={observed} exceeds {allowance}"
            ));
        }
    }
}

#[test]
fn actual_cli_read_write_counts_do_not_grow_with_duplicate_or_unique_anchors() {
    let mut failures = Vec::new();
    for size in [0, 10, 100, 1000] {
        for unique in [false, true] {
            let (_tmp, root) = fixture(size, unique);
            let trace = root.join(".mine/runtime/snapshot-traces/cli.jsonl");
            let mut samples = Vec::new();
            for iteration in 0..6 {
                std::fs::write(&trace, []).unwrap();
                let began = Instant::now();
                let result = binary(&root, &trace, &["graph", "status"]);
                let elapsed = began.elapsed().as_micros();
                if iteration > 0 {
                    samples.push(elapsed);
                }
                let label = format!(
                    "CLI status n={size} unique={unique} iteration={iteration} us={elapsed}"
                );
                record_counts(
                    &mut failures,
                    &label,
                    &trace,
                    trace_counts(&trace),
                    (usize::from(size > 0), 10),
                );
                if !result.status.success() {
                    failures.push(format!("{label}: command failed: {result:?}"));
                }
            }
            samples.sort_unstable();
            eprintln!(
                "CLI warm n={size} unique={unique} p50_us={} p95_us={}",
                samples[2], samples[4]
            );
            if size > 0 {
                std::fs::write(&trace, []).unwrap();
                // mine-release-allow-plan-reference: canonical private lookup subject
                let result = binary(&root, &trace, &["plan", "show", "--id", "plan01-01"]);
                let label = format!("CLI plan-show n={size} unique={unique}");
                record_counts(&mut failures, &label, &trace, trace_counts(&trace), (1, 11));
                if !result.status.success() {
                    failures.push(format!("{label}: {result:?}"));
                }
            }
            let mut ws = common::load_graph(&root);
            // mine-release-allow-plan-reference: independent unstarted write subject
            ws.plans
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                .push(common::node("plan99", PlanStatus::Ready, &[], &[]));
            committed_seed(&root, &ws);
            let operation = uuid::Uuid::new_v4().to_string();
            std::fs::write(&trace, []).unwrap();
            let began = Instant::now();
            let result = binary(
                &root,
                &trace,
                &[
                    // mine-release-allow-plan-reference: canonical private withdrawal subject
                    "plan",
                    "withdraw",
                    "--id",
                    // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                    "plan99",
                    "--reason",
                    "fixture",
                    "--actor",
                    "snapshot-fixture",
                    "--expected-revision",
                    &ws.revision.to_string(),
                    "--operation-id",
                    &operation,
                ],
            );
            let label = format!(
                "CLI whole-native-withdraw oracle=ADR-0033 n={size} unique={unique} us={}",
                began.elapsed().as_micros()
            );
            record_withdrawal_counts(&mut failures, &label, &trace, size);
            if !result.status.success() {
                failures.push(format!(
                    "{label}: {result:?}; snapshot primitive probe not run after failed mutation"
                ));
                continue;
            }
            let counter = Arc::new(CountingGit::default());
            let store = TomlStore::new(&root).with_git_spawner(counter.clone());
            let revision = store.observed_revision().unwrap();
            let snapshot_start = counter.0.lock().unwrap().len();
            let result = store.save_with_revision(revision, "snapshot-fixture", |mut ws| {
                ws.revision += 1;
                Ok(ws)
            });
            if let Err(error) = result {
                failures.push(format!(
                    "snapshot primitive n={size} unique={unique}: {error}"
                ));
                continue;
            }
            let d = store.transaction_diagnostics();
            // This injected spawner covers ordinary snapshot acquisition only.
            // Native checkpoint children have their own controlled spawner but
            // share d.git's whole-request budget/counters. Never call this
            // partial observation a complete transaction/process total.
            let calls = counter.0.lock().unwrap();
            let calls = &calls[snapshot_start..];
            let snapshot_launches = calls.len();
            let snapshot_batches = calls
                .iter()
                .filter(|args| {
                    args.first().is_some_and(|arg| arg == "cat-file")
                        && args.iter().any(|arg| arg.starts_with("--batch-check="))
                })
                .count();
            let failed_attempts = counter.1.load(Ordering::SeqCst);
            eprintln!(
                "WRITE snapshot-acquisition-only n={size} unique={unique} snapshot_launches={snapshot_launches} snapshot_batches={snapshot_batches} failed_attempts={failed_attempts}"
            );
            eprintln!(
                "WRITE entire-native-transaction n={size} unique={unique} loads={} phase={} whole_invocations={} whole_peak={} whole_batches={} wait_us={} hold_us={}",
                d.graph_loads,
                d.phase,
                d.git.invocations,
                d.git.peak_children,
                d.git.object_batches,
                d.lock_wait.as_micros(),
                d.lock_hold.as_micros()
            );
            if d.graph_loads != 1
                || d.phase != "complete"
                || d.graph_size != size + 1
                || snapshot_batches != usize::from(size > 0)
                || snapshot_launches > 2
                || failed_attempts != 0
                || d.git.peak_children > 1
                || d.git.invocations < snapshot_launches as u64
                || d.git.object_batches < snapshot_batches as u64
                || !d.checkpoint.as_ref().is_some_and(|operation| {
                    operation.fields().stage
                        == mine::domain::lifecycle_operation::OperationStage::GraphCheckpoint(
                            mine::domain::lifecycle_operation::CheckpointStage::Complete,
                        )
                        && operation.fields().outcome
                            == mine::domain::lifecycle_operation::OperationOutcome::Committed
                })
            {
                failures.push(format!(
                    "snapshot primitive n={size} unique={unique}: {d:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "complete CLI matrix violations:\n{}",
        failures.join("\n")
    );
}

// One complete command for bounded optimization diagnosis. Keep the original
// whole-write limits RED until the Architect rules from necessary-phase evidence.
#[test]
fn representative_complete_cli_withdrawal_with_one_thousand_unique_anchors() {
    let (_tmp, root) = fixture(1000, true);
    let mut workspace = common::load_graph(&root);
    workspace
        .plans
        // mine-release-allow-plan-reference: independent canonical private withdrawal subject
        .push(common::node("plan99", PlanStatus::Ready, &[], &[]));
    committed_seed(&root, &workspace);
    let original_head = git(&root, &["rev-parse", "HEAD"], &[]);
    let stable = git(&root, &["rev-parse", "master"], &[]);
    let operation = uuid::Uuid::new_v4().to_string();
    let trace = root.join(".mine/runtime/snapshot-traces/representative-write.jsonl");
    assert!(!trace.exists());
    let began = Instant::now();
    let result = binary(
        &root,
        &trace,
        &[
            "plan",
            "withdraw",
            "--id",
            // mine-release-allow-plan-reference: isolated withdrawal fixture, not repository provenance
            "plan99",
            "--reason",
            "fixture",
            "--actor",
            "snapshot-fixture",
            "--expected-revision",
            &workspace.revision.to_string(),
            "--operation-id",
            &operation,
        ],
    );
    let elapsed = began.elapsed();
    let label = format!(
        "CLI representative whole-native-withdraw oracle=ADR-0033 n=1000 unique=true us={}",
        elapsed.as_micros()
    );
    let mut failures = Vec::new();
    record_withdrawal_counts(&mut failures, &label, &trace, 1000);
    if let Some(directory) = std::env::var_os("MINE_SNAPSHOT_TRACE_OUTPUT") {
        let directory = std::path::PathBuf::from(directory);
        for (name, bytes) in [
            ("representative.stdout.json", &result.stdout),
            ("representative.stderr.json", &result.stderr),
        ] {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(name))
                .unwrap()
                .write_all(bytes)
                .unwrap();
        }
    }
    eprintln!(
        "{label} exit={:?} operation={operation}",
        result.status.code()
    );
    if result.status.success() {
        let current = common::load_graph(&root);
        assert_eq!(current.revision, workspace.revision + 1);
        // mine-release-allow-plan-reference: exact private withdrawal result assertion
        assert_eq!(current.get("plan99").unwrap().status, PlanStatus::Withdrawn);
        assert_eq!(
            git(
                &root,
                &["rev-list", "--count", &format!("{original_head}..HEAD")],
                &[]
            ),
            "1"
        );
        assert_eq!(git(&root, &["rev-parse", "master"], &[]), stable);
        assert_eq!(git(&root, &["status", "--porcelain=v1", "-uall"], &[]), "");
    } else {
        failures.push(format!(
            "{label}: actual command failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    assert!(
        failures.is_empty(),
        "representative complete command violations:\n{}",
        failures.join("\n")
    );
}

#[tokio::test]
async fn actual_mcp_reads_refresh_between_requests_and_batch_once_per_call() {
    use rmcp::model::CallToolRequestParams;
    use rmcp::service::serve_client;
    use rmcp::transport::child_process::TokioChildProcess;
    let mut failures = Vec::new();
    for size in [0, 10, 100, 1000] {
        for unique in [false, true] {
            let (_tmp, root) = fixture(size, unique);
            let trace = root.join(".mine/runtime/snapshot-traces/mcp.jsonl");
            let home = root.join(".mine/runtime/fixture-home");
            std::fs::create_dir_all(&home).unwrap();
            let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_mine"));
            command
                .args(["mcp", "serve", "--repo"])
                .arg(&root)
                .env("GIT_TRACE2_EVENT", &trace)
                .env("HOME", &home)
                .env("USERPROFILE", &home);
            let transport = TokioChildProcess::builder(command).spawn().unwrap().0;
            let client = serve_client((), transport).await.unwrap();
            let tools = client.peer().list_all_tools().await.unwrap();
            assert!(tools.iter().any(|tool| tool.name == "mine_graph_status"));
            let mut samples = Vec::new();
            for iteration in 0..6 {
                std::fs::write(&trace, []).unwrap();
                let began = Instant::now();
                let result = client
                    .peer()
                    .call_tool(CallToolRequestParams::new("mine_graph_status"))
                    .await
                    .unwrap();
                let elapsed = began.elapsed().as_micros();
                if iteration > 0 {
                    samples.push(elapsed);
                }
                let label = format!(
                    "MCP status n={size} unique={unique} iteration={iteration} us={elapsed}"
                );
                record_counts(
                    &mut failures,
                    &label,
                    &trace,
                    trace_counts(&trace),
                    (usize::from(size > 0), 10),
                );
                if result.is_error == Some(true) {
                    failures.push(format!("{label}: {result:?}"));
                } else {
                    let value = result.structured_content.unwrap();
                    assert_eq!(value["revision"], common::load_graph(&root).revision);
                }
                if iteration == 0 {
                    let mut ws = common::load_graph(&root);
                    ws.revision += 1;
                    seed(&root, &ws);
                }
            }
            samples.sort_unstable();
            eprintln!(
                "MCP warm n={size} unique={unique} p50_us={} p95_us={}",
                samples[2], samples[4]
            );
            if size > 0 {
                std::fs::write(&trace, []).unwrap();
                // mine-release-allow-plan-reference: canonical private lookup subject
                let result = client
                    .peer()
                    .call_tool(
                        CallToolRequestParams::new("mine_plan_show")
                            // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                            .with_arguments(json!({"id":"plan01-01"}).as_object().unwrap().clone()),
                    )
                    .await
                    .unwrap();
                let label = format!("MCP plan-show n={size} unique={unique}");
                record_counts(&mut failures, &label, &trace, trace_counts(&trace), (1, 11));
                if result.is_error == Some(true) {
                    failures.push(format!("{label}: {result:?}"));
                }
            }
            let mut ws = common::load_graph(&root);
            // mine-release-allow-plan-reference: independent unstarted write subject
            ws.plans
                // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
                .push(common::node("plan99", PlanStatus::Ready, &[], &[]));
            committed_seed(&root, &ws);
            std::fs::write(&trace, []).unwrap();
            let began = Instant::now();
            let result = client.peer().call_tool(CallToolRequestParams::new("mine_plan_withdraw").with_arguments(
                // mine-release-allow-plan-reference: canonical private withdrawal subject
                json!({"id":"plan99","reason":"fixture","actor":"snapshot-fixture","expected_revision":ws.revision,"operation_id":uuid::Uuid::new_v4().to_string()})
                    .as_object().unwrap().clone())).await.unwrap();
            let label = format!(
                "MCP whole-native-withdraw oracle=ADR-0033 n={size} unique={unique} us={}",
                began.elapsed().as_micros()
            );
            record_withdrawal_counts(&mut failures, &label, &trace, size);
            if result.is_error == Some(true) {
                failures.push(format!("{label}: {result:?}"));
            }
            client.cancel().await.unwrap();
        }
    }
    assert!(
        failures.is_empty(),
        "complete MCP matrix violations:\n{}",
        failures.join("\n")
    );
}

#[test]
fn indexed_kahn_preserves_fifo_declaration_order_and_ancestor_semantics() {
    // Exhaust all forward-edge DAGs over five declaration-ranked nodes, then
    // reverse declarations so topology order is not accidentally lexical.
    let (_tmp, root) = common::seeded_repo(vec![]);
    let mut ws = common::load_graph(&root);
    for edges in 0_u16..1024 {
        ws.plans = (0..5)
            .map(|i| common::node(&format!("plan{:02}", i + 1), PlanStatus::Ready, &[], &[]))
            .collect();
        let mut bit = 0;
        for child in 0..5 {
            for parent in 0..child {
                if edges & (1 << bit) != 0 {
                    ws.plans[child]
                        .hard_predecessors
                        .push(format!("plan{:02}", parent + 1));
                }
                bit += 1;
            }
        }
        ws.plans.reverse();
        let mut remaining: Vec<_> = ws.plans.iter().map(|n| n.hard_predecessors.len()).collect();
        let mut queue: std::collections::VecDeque<_> = remaining
            .iter()
            .enumerate()
            .filter_map(|(i, d)| (*d == 0).then_some(i))
            .collect();
        let mut expected = Vec::new();
        while let Some(parent) = queue.pop_front() {
            expected.push(ws.plans[parent].id.clone());
            for (i, child) in ws.plans.iter().enumerate() {
                for pred in &child.hard_predecessors {
                    if pred == &ws.plans[parent].id {
                        remaining[i] -= 1;
                        if remaining[i] == 0 {
                            queue.push_back(i);
                        }
                    }
                }
            }
        }
        let index = GraphIndex::new(&ws).unwrap();
        assert_eq!(index.topological_sort().unwrap(), expected);
        for a in &ws.plans {
            for b in &ws.plans {
                let mut stack = b.hard_predecessors.clone();
                let mut found = false;
                while let Some(id) = stack.pop() {
                    if id == a.id {
                        found = true;
                        break;
                    }
                    stack.extend(ws.get(&id).unwrap().hard_predecessors.clone());
                }
                assert_eq!(index.is_hard_ancestor(&a.id, &b.id).unwrap(), found);
                assert_eq!(index.is_hard_ancestor(&a.id, &b.id).unwrap(), found);
            }
        }
    }
    ws.plans = vec![
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        common::node("plan02", PlanStatus::Ready, &["plan01"], &[]),
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        common::node("plan01", PlanStatus::Ready, &["plan02"], &[]),
    ];
    assert!(
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        matches!(GraphIndex::new(&ws).unwrap().topological_sort(), Err(MineError::GraphCycle { cycle }) if cycle == "plan02 -> plan01")
    );
}
