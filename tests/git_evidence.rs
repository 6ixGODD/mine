#![forbid(unsafe_code)]

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::process::Command as AsyncCommand;

use mine::domain::error::{GitFailure, MineError};
use mine::infrastructure::git;
use mine::infrastructure::git_evidence::{GitChild, GitLimits, GitRequest, GitSpawner};

struct FixtureBinary {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

fn fixture_binary() -> &'static Path {
    static BINARY: OnceLock<FixtureBinary> = OnceLock::new();
    &BINARY.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("fixture.rs");
        let path = directory.path().join(format!("git-fixture{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&source, r#"
use std::io::{Read, Write};
use std::time::Duration;
fn main() {
 let args: Vec<_> = std::env::args().collect();
 if args[1] == "record" { std::fs::write(&args[2], b"transport invoked").unwrap(); std::process::exit(1); }
 match args[1].as_str() {
  "hold-pipes" => std::thread::sleep(Duration::from_secs(2)),
  "hold-pipes-long" => std::thread::sleep(Duration::from_secs(15)),
  "background" | "background-long" | "background-input" => {
   let hold = if args[1] == "background-long" { "hold-pipes-long" } else { "hold-pipes" };
   let stdin = if args[1] == "background-input" { std::process::Stdio::inherit() } else { std::process::Stdio::null() };
   let child = std::process::Command::new(std::env::current_exe().unwrap()).arg(hold)
    .stdin(stdin).stdout(std::process::Stdio::inherit())
    .stderr(std::process::Stdio::inherit()).spawn().unwrap();
   if let Some(marker) = args.get(2) { std::fs::write(marker, child.id().to_string()).unwrap(); }
  }
  "hang" => std::thread::sleep(Duration::from_secs(60)),
  "both" => {
   let a=std::thread::spawn(|| std::io::stdout().write_all(&vec![b'a'; 128*1024]).unwrap());
   let b=std::thread::spawn(|| std::io::stderr().write_all(&vec![b'b'; 128*1024]).unwrap());
   let mut input=Vec::new(); std::io::stdin().read_to_end(&mut input).unwrap();
   assert_eq!(input, vec![b'c'; 128*1024]);
   a.join().unwrap(); b.join().unwrap();
  }
  "out-overflow" => { let _=std::io::stdout().write_all(&vec![b'x'; 2*1024*1024]); },
  "err-overflow" => { let _=std::io::stderr().write_all(&vec![b'x'; 2*1024*1024]); },
  "fail" => { println!("SECRET_NOT_FOR_ERRORS"); std::process::exit(17); }
  "delay" => { std::thread::sleep(Duration::from_millis(60)); }
  "echo" => {
   let mut input=String::new(); std::io::stdin().read_to_string(&mut input).unwrap();
   for line in input.lines() { println!("{} commit",line); }
  }
  mode => {
   let mut input=String::new(); std::io::stdin().read_to_string(&mut input).unwrap();
   let oid=input.lines().next().unwrap();
   match mode {
    "wrong-id" => println!("{} commit", "0".repeat(oid.len())),
    "wrong-type" => println!("{} blob",oid),
    "missing" => println!("{} missing",oid),
    "truncated" => print!("{} commit",oid),
    "extra" => print!("{} commit\n{} commit\n",oid,oid),
    "short" => println!("{} commit",&oid[..8]),
    "partial-fail" => { println!("{} commit",oid); std::process::exit(17); }
    _ => panic!("unknown mode"),
   }
  }
 }
}
"#).unwrap();
        let output = Command::new("rustc").arg(&source).arg("-o").arg(&path).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        FixtureBinary { _directory: directory, path }
    }).path
}

struct InjectedGit {
    mode: &'static str,
    pids: Mutex<Vec<u32>>,
    spawn_times: Mutex<Vec<Duration>>,
    spawned_at: Mutex<Option<Instant>>,
}

struct FailingRead;

impl tokio::io::AsyncRead for FailingRead {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Err(io::Error::other("injected read failure")))
    }
}

impl GitSpawner for InjectedGit {
    fn spawn(&self, repo: &Path, args: &[&str]) -> io::Result<GitChild> {
        let began = Instant::now();
        if self.mode == "delayed-background" {
            // Model an uninterruptible OS process-creation delay separately
            // from the adapter's cancellable pipe collection/cleanup.
            std::thread::sleep(Duration::from_millis(1200));
        }
        let mut command = if args.first() == Some(&"rev-parse") {
            let mut command = AsyncCommand::new("git");
            command.arg("-C").arg(repo).args(args);
            command
        } else {
            let mut command = AsyncCommand::new(fixture_binary());
            command.arg(match self.mode {
                "read-error" => "hang",
                "delayed-background" => "background",
                mode => mode,
            });
            if self.mode.starts_with("background") {
                command.arg(repo.join("holder-started"));
            }
            command
        };
        command.current_dir(repo);
        let mut child = GitChild::spawn(command)?;
        *self.spawned_at.lock().unwrap() = Some(Instant::now());
        if self.mode == "bad-pipe" {
            child.stderr = None;
        }
        if self.mode == "read-error" {
            child.stdout = Some(Box::new(FailingRead));
        }
        self.pids.lock().unwrap().push(child.id().unwrap());
        self.spawn_times.lock().unwrap().push(began.elapsed());
        Ok(child)
    }
}

fn injected(
    mode: &'static str,
    limits: GitLimits,
) -> (GitRequest, Arc<InjectedGit>, tempfile::TempDir) {
    // Compile before starting the request clock; the seam models owned Git
    // children, not compiler setup latency.
    fixture_binary();
    let repo = repo("sha1");
    let spawner = Arc::new(InjectedGit {
        mode,
        pids: Mutex::new(Vec::new()),
        spawn_times: Mutex::new(Vec::new()),
        spawned_at: Mutex::new(None),
    });
    (
        GitRequest::with_spawner(repo.path(), limits, spawner.clone()),
        spawner,
        repo,
    )
}

fn failure(error: &MineError) -> GitFailure {
    match error {
        MineError::Io(error) => *error
            .get_ref()
            .unwrap()
            .downcast_ref::<GitFailure>()
            .unwrap(),
        other => panic!("unexpected error: {other}"),
    }
}

fn assert_reaped(spawner: &InjectedGit) {
    for pid in spawner.pids.lock().unwrap().iter() {
        #[cfg(windows)]
        {
            let output = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert!(
                !String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")),
                "child {pid} still alive"
            );
        }
        #[cfg(unix)]
        {
            let status = Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(!status.success(), "child {pid} still alive or unreaped");
        }
    }
}

fn git_output(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn repo(format: &str) -> tempfile::TempDir {
    let directory = tempfile::Builder::new()
        .prefix("git 空间 fixture-")
        .tempdir()
        .unwrap();
    git_output(
        directory.path(),
        &[
            "init",
            "--quiet",
            "--initial-branch=dev",
            &format!("--object-format={format}"),
        ],
    );
    git_output(directory.path(), &["config", "user.name", "test"]);
    git_output(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    std::fs::write(directory.path().join("file.txt"), "initial\n").unwrap();
    git_output(directory.path(), &["add", "file.txt"]);
    git_output(directory.path(), &["commit", "--quiet", "-m", "initial"]);
    directory
}

fn short_limits() -> GitLimits {
    GitLimits {
        child_time: Duration::from_millis(200),
        ..GitLimits::default()
    }
}

#[test]
fn deadlines_and_blocked_stdin_kill_and_reap() {
    for input in [vec![], vec![b'x'; 1024 * 1024]] {
        let (mut request, spawner, _repo) = injected("hang", short_limits());
        let began = Instant::now();
        assert_eq!(
            failure(&request.run(&["fixture"], &input).unwrap_err()),
            GitFailure::ChildDeadline
        );
        assert!(began.elapsed() < Duration::from_secs(5));
        assert_eq!(request.diagnostics().invocations, 1);
        assert_eq!(request.diagnostics().peak_children, 1);
        assert_reaped(&spawner);
    }
}

#[test]
fn cancellation_before_and_during_owned_child() {
    let (mut request, spawner, _repo) = injected("hang", GitLimits::default());
    let cancellation = request.cancellation();
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        cancellation.cancel();
    });
    assert_eq!(
        failure(&request.run(&["fixture"], &[]).unwrap_err()),
        GitFailure::Cancelled
    );
    cancel.join().unwrap();
    assert_reaped(&spawner);
    let count = request.diagnostics().invocations;
    assert_eq!(
        failure(&request.run(&["fixture"], &[]).unwrap_err()),
        GitFailure::Cancelled
    );
    assert_eq!(count, request.diagnostics().invocations);
}

#[test]
fn read_failure_kills_and_reaps_the_owned_child() {
    let (mut request, spawner, _repo) = injected("read-error", GitLimits::default());
    assert_eq!(
        failure(&request.run(&["fixture"], &[]).unwrap_err()),
        GitFailure::ProcessIo,
    );
    assert_eq!(request.diagnostics().invocations, 1);
    assert_reaped(&spawner);
}

#[test]
fn cancellation_closes_inherited_pipes_without_waiting_for_descendants() {
    let (mut request, spawner, repo) = injected("background", GitLimits::default());
    let cancellation = request.cancellation();
    let marker = repo.path().join("holder-started");
    let cancel = std::thread::spawn(move || {
        let began = Instant::now();
        while !marker.exists() {
            assert!(
                began.elapsed() < Duration::from_secs(5),
                "holder never started"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        // Cancellation must exercise genuinely inherited handles, not fire
        // before Windows has finished creating the fixture's first process.
        let cancelled_at = Instant::now();
        cancellation.cancel();
        cancelled_at
    });
    assert_eq!(
        failure(&request.run(&["fixture"], &[]).unwrap_err()),
        GitFailure::Cancelled,
    );
    let returned_at = Instant::now();
    let cancelled_at = cancel.join().unwrap();
    assert!(
        returned_at.duration_since(cancelled_at) < Duration::from_secs(1),
        "cancel return took {:?}; spawn times: {:?}",
        returned_at.duration_since(cancelled_at),
        spawner.spawn_times.lock().unwrap()
    );
    assert_reaped(&spawner);
}

#[test]
fn short_deadline_closes_owned_pipes_after_process_creation() {
    let (mut request, spawner, _repo) = injected("background", short_limits());
    let began = Instant::now();
    assert_eq!(
        failure(&request.run(&["fixture"], &[]).unwrap_err()),
        GitFailure::ChildDeadline
    );
    // The 200ms product timer is unchanged. A synchronous OS spawn can itself
    // exceed that budget; this assertion bounds pipe cleanup after it returns.
    // The real Git hook regression below separately requires a live holder and
    // checks the unchanged 10s production deadline end to end.
    assert!(
        spawner.spawned_at.lock().unwrap().unwrap().elapsed() < Duration::from_secs(1),
        "deadline return took {:?}; spawn times: {:?}",
        began.elapsed(),
        spawner.spawn_times.lock().unwrap()
    );
    assert_reaped(&spawner);
}

#[test]
fn slow_process_creation_does_not_mask_unbounded_pipe_cleanup() {
    let (mut request, spawner, _repo) = injected("delayed-background", short_limits());
    let began = Instant::now();
    assert_eq!(
        failure(&request.run(&["fixture"], &[]).unwrap_err()),
        GitFailure::ChildDeadline
    );
    assert!(began.elapsed() >= Duration::from_millis(1200));
    assert!(spawner.spawned_at.lock().unwrap().unwrap().elapsed() < Duration::from_secs(1));
    assert_eq!(request.diagnostics().invocations, 1);
    assert_reaped(&spawner);
}

#[test]
fn blocked_stdin_inherited_by_descendant_is_disconnected_on_abort() {
    for cancelled in [false, true] {
        let limits = if cancelled {
            GitLimits::default()
        } else {
            short_limits()
        };
        let (mut request, spawner, _repo) = injected("background-input", limits);
        let cancellation = request.cancellation();
        let marker = _repo.path().join("holder-started");
        let cancel = cancelled.then(|| {
            std::thread::spawn(move || {
                let began = Instant::now();
                while !marker.exists() {
                    assert!(
                        began.elapsed() < Duration::from_secs(5),
                        "holder never started"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
                let cancelled_at = Instant::now();
                cancellation.cancel();
                cancelled_at
            })
        });
        let began = Instant::now();
        assert_eq!(
            failure(
                &request
                    .run(&["fixture"], &vec![b'x'; 1024 * 1024])
                    .unwrap_err()
            ),
            if cancelled {
                GitFailure::Cancelled
            } else {
                GitFailure::ChildDeadline
            },
        );
        let returned_at = Instant::now();
        let cleanup_start = if let Some(cancel) = cancel {
            cancel.join().unwrap()
        } else {
            spawner.spawned_at.lock().unwrap().unwrap()
        };
        assert!(
            returned_at.duration_since(cleanup_start) < Duration::from_secs(1),
            "inherited stdin abort took {:?}; total {:?}; spawn times: {:?}",
            returned_at.duration_since(cleanup_start),
            began.elapsed(),
            spawner.spawn_times.lock().unwrap()
        );
        assert_reaped(&spawner);
    }
}

#[test]
fn concurrent_stream_drain_and_each_stream_limit() {
    let (mut request, spawner, _repo) = injected("both", GitLimits::default());
    let output = request
        .run_raw(&["fixture"], &vec![b'c'; 128 * 1024])
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, vec![b'a'; 128 * 1024]);
    assert_eq!(output.stderr, vec![b'b'; 128 * 1024]);
    assert_reaped(&spawner);
    for mode in ["out-overflow", "err-overflow"] {
        let (mut request, spawner, _repo) = injected(mode, GitLimits::default());
        assert_eq!(
            failure(&request.run(&["fixture"], &[]).unwrap_err()),
            GitFailure::OutputLimit
        );
        assert_reaped(&spawner);
    }
}

#[test]
fn total_request_budget_is_not_reset_between_children_or_refreshes() {
    let limits = GitLimits {
        request_time: Duration::from_millis(150),
        ..GitLimits::default()
    };
    let (mut request, spawner, _repo) = injected("delay", limits);
    let mut saw_budget = false;
    for _ in 0..5 {
        if let Err(error) = request.run(&["fixture"], &[]) {
            assert_eq!(failure(&error), GitFailure::RequestBudget);
            saw_budget = true;
            break;
        }
        request.refresh_observations();
    }
    assert!(saw_budget);
    assert!(request.diagnostics().invocations <= 3);
    assert_reaped(&spawner);
}

#[test]
fn child_failure_and_missing_pipe_never_become_success_or_secret_diagnostics() {
    for (mode, expected) in [
        ("fail", GitFailure::ChildExit),
        ("bad-pipe", GitFailure::ProcessIo),
    ] {
        let (mut request, spawner, _repo) = injected(mode, GitLimits::default());
        let error = request.run(&["fixture"], &[]).unwrap_err();
        assert_eq!(failure(&error), expected);
        assert!(!error.to_string().contains("SECRET_NOT_FOR_ERRORS"));
        assert_eq!(mine::output::exit_code_for(&error), 6);
        assert_reaped(&spawner);
    }
}

#[test]
fn malformed_batch_and_partial_failure_are_rejected_without_caching() {
    for mode in [
        "wrong-id",
        "wrong-type",
        "missing",
        "truncated",
        "extra",
        "short",
        "partial-fail",
    ] {
        let (mut request, spawner, repo) = injected(mode, GitLimits::default());
        let oid = git_output(repo.path(), &["rev-parse", "HEAD"]);
        let error = request
            .validate_commits(std::slice::from_ref(&oid))
            .unwrap_err();
        assert_eq!(
            failure(&error),
            if mode == "partial-fail" {
                GitFailure::ChildExit
            } else {
                GitFailure::Protocol
            },
            "{mode}"
        );
        let count = request.diagnostics().invocations;
        assert!(request.validate_commits(&[oid]).is_err());
        assert_eq!(
            request.diagnostics().invocations,
            count + 1,
            "failed batch was cached: {mode}"
        );
        assert_reaped(&spawner);
    }
}

#[test]
fn exact_native_oids_and_duplicate_queries_use_one_batch() {
    for format in ["sha1", "sha256"] {
        let repo = repo(format);
        let first = git_output(repo.path(), &["rev-parse", "HEAD"]);
        git_output(
            repo.path(),
            &["commit", "--quiet", "--allow-empty", "-m", "second"],
        );
        let oid = git_output(repo.path(), &["rev-parse", "HEAD"]);
        let mut request = GitRequest::new(repo.path());
        request.validate_commits(&[]).unwrap();
        assert_eq!(request.diagnostics().invocations, 0);
        request
            .validate_commits(&[oid.clone(), first, oid.clone()])
            .unwrap();
        assert_eq!(
            request.diagnostics().invocations,
            2,
            "one format query + one batch"
        );
        request
            .validate_commits(std::slice::from_ref(&oid))
            .unwrap();
        assert_eq!(request.diagnostics().invocations, 2);
        assert_eq!(request.diagnostics().peak_children, 1);
        for invalid in [
            &oid[..8],
            "HEAD",
            "HEAD^{commit}",
            "-x",
            &oid.to_uppercase(),
        ] {
            assert_eq!(
                failure(&request.validate_commits(&[invalid.to_owned()]).unwrap_err()),
                GitFailure::Protocol
            );
        }
        assert_eq!(request.diagnostics().invocations, 2);
    }
}

#[test]
fn real_tag_blob_tree_and_missing_objects_never_peel_to_accepted_commits() {
    let repo = repo("sha1");
    git_output(repo.path(), &["tag", "-a", "annotated", "-m", "tag"]);
    let commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
    assert_eq!(
        git::resolve_commit(repo.path(), "annotated").unwrap(),
        commit
    );
    let mut request = GitRequest::new(repo.path());
    for revision in ["annotated", "HEAD^{tree}", "HEAD:file.txt"] {
        let oid = git_output(repo.path(), &["rev-parse", revision]);
        assert_eq!(
            failure(&request.validate_commits(&[oid]).unwrap_err()),
            GitFailure::Protocol
        );
    }
    assert!(
        request
            .validate_commits(&["0".repeat(commit.len())])
            .is_err()
    );
    request
        .validate_commits(std::slice::from_ref(&commit))
        .unwrap();
    assert_eq!(
        git::head_commit(repo.path()).as_deref(),
        Some(commit.as_str()),
        "real consumer uses exact batch"
    );
}

#[test]
fn mixed_batch_failure_caches_no_valid_prefix_and_cancel_refuses_cached_evidence() {
    let repo = repo("sha1");
    let oid = git_output(repo.path(), &["rev-parse", "HEAD"]);
    let mut request = GitRequest::new(repo.path());
    assert!(
        request
            .validate_commits(&[oid.clone(), "0".repeat(oid.len())])
            .is_err()
    );
    let failed_count = request.diagnostics().invocations;
    request
        .validate_commits(std::slice::from_ref(&oid))
        .unwrap();
    assert_eq!(request.diagnostics().invocations, failed_count + 1);
    request
        .validate_commits(std::slice::from_ref(&oid))
        .unwrap();
    assert_eq!(request.diagnostics().invocations, failed_count + 1);
    request.cancellation().cancel();
    assert_eq!(
        failure(&request.validate_commits(&[oid]).unwrap_err()),
        GitFailure::Cancelled
    );
    assert_eq!(failure(&request.refs().unwrap_err()), GitFailure::Cancelled);
    assert_eq!(
        failure(&request.worktrees().unwrap_err()),
        GitFailure::Cancelled
    );
    assert_eq!(request.diagnostics().invocations, failed_count + 1);
}

#[test]
fn replacement_semantics_match_existing_commit_resolution_and_refresh() {
    let repo = repo("sha1");
    let commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
    git_output(
        repo.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "second"],
    );
    let second = git_output(repo.path(), &["rev-parse", "HEAD"]);
    git_output(repo.path(), &["replace", &commit, &second]);
    let mut request = GitRequest::new(repo.path());
    request
        .validate_commits(std::slice::from_ref(&commit))
        .unwrap();
    assert_eq!(git::resolve_commit(repo.path(), &commit).unwrap(), commit);
    git_output(repo.path(), &["replace", "-d", &commit]);
    let blob = git_output(repo.path(), &["rev-parse", "HEAD:file.txt"]);
    git_output(repo.path(), &["replace", "--force", &commit, &blob]);
    request.refresh_observations();
    assert!(
        request
            .validate_commits(std::slice::from_ref(&commit))
            .is_err()
    );
    assert!(git::resolve_commit(repo.path(), &commit).is_err());
}

#[test]
fn bulk_inventories_and_ordered_ancestry_refresh_without_stale_authority() {
    let repo = repo("sha1");
    let first = git_output(repo.path(), &["rev-parse", "HEAD"]);
    git_output(
        repo.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "second"],
    );
    let second = git_output(repo.path(), &["rev-parse", "HEAD"]);
    let mut request = GitRequest::new(repo.path());
    assert!(request.is_ancestor(&first, &second).unwrap());
    assert!(!request.is_ancestor(&second, &first).unwrap());
    let count = request.diagnostics().invocations;
    assert!(request.is_ancestor(&first, &second).unwrap());
    assert_eq!(count, request.diagnostics().invocations);
    let initial_refs = request.refs().unwrap();
    let initial_trees = request.worktrees().unwrap();
    let count = request.diagnostics().invocations;
    assert_eq!(request.refs().unwrap(), initial_refs);
    assert_eq!(request.worktrees().unwrap(), initial_trees);
    assert_eq!(count, request.diagnostics().invocations);
    let linked = repo.path().join("linked 空间");
    git_output(
        repo.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "other",
            linked.to_str().unwrap(),
        ],
    );
    assert_eq!(request.worktrees().unwrap().len(), 1);
    request.refresh_observations();
    assert_eq!(request.worktrees().unwrap().len(), 2);
    assert!(
        request
            .refs()
            .unwrap()
            .iter()
            .any(|r| r.name == "refs/heads/other")
    );
    assert!(request.diagnostics().invocations > count);
    assert!(
        request
            .is_ancestor(&first, &"0".repeat(first.len()))
            .is_err()
    );
}

#[test]
fn promised_missing_object_does_not_invoke_transport_on_git_baseline() {
    let source = repo("sha1");
    git_output(source.path(), &["config", "uploadpack.allowFilter", "true"]);
    let tree = git_output(source.path(), &["rev-parse", "HEAD^{tree}"]);
    let clone_parent = tempfile::tempdir().unwrap();
    let clone = clone_parent.path().join("partial");
    git_output(
        source.path(),
        &[
            "clone",
            "--quiet",
            "--no-local",
            "--no-checkout",
            "--filter=tree:0",
            source.path().to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    let log = clone_parent.path().join("transport.log");
    let escape = |path: &Path| {
        path.to_string_lossy()
            .replace('\\', "/")
            .replace('%', "%%")
            .replace(' ', "% ")
    };
    let remote = format!("ext::{} record {}", escape(fixture_binary()), escape(&log));
    git_output(&clone, &["config", "remote.origin.url", &remote]);
    git_output(&clone, &["config", "protocol.ext.allow", "always"]);
    let mut request = GitRequest::new(&clone);
    assert!(
        request
            .validate_commits(std::slice::from_ref(&tree))
            .is_err()
    );
    assert!(
        !log.exists(),
        "read-only adapter invoked a promisor transport"
    );
    // Positive control: the identical missing object really would fetch in
    // this partial repo. The controlled local helper records then fails;
    // there is no network, shared environment mutation or global PATH change.
    let output = Command::new("git")
        .arg("-C")
        .arg(&clone)
        .args(["cat-file", "-t", &tree])
        .env_remove("GIT_NO_LAZY_FETCH")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        log.exists(),
        "fixture did not actually exercise lazy fetching"
    );
}

#[test]
fn real_git_hook_inherited_pipes_do_not_extend_the_child_deadline() {
    let binary = fixture_binary();
    let repo = repo("sha1");
    let hooks = repo.path().join("hooks");
    std::fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("post-checkout");
    let marker = repo.path().join("holder-started");
    let marker_arg = marker
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''");
    let executable = binary
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nexec '{executable}' background-long '{marker_arg}'\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git_output(
        repo.path(),
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    // Use the actual 10s production limit here so loaded native Git/shell
    // startup cannot impersonate a successful inherited-pipe probe. A separate
    // direct-child probe above retains the 200ms / <1s transport regression.
    let mut request = GitRequest::new(repo.path());
    let began = Instant::now();
    // Exercise the actual Git hook without unrelated checkout/index creation
    // consuming the deliberately short transport deadline before hook startup.
    let result = request.run(&["hook", "run", "post-checkout"], &[]);
    let elapsed = began.elapsed();
    assert_eq!(
        failure(result.as_ref().unwrap_err()),
        GitFailure::ChildDeadline
    );
    assert_eq!(request.diagnostics().invocations, 1);
    assert!(
        marker.exists(),
        "the actual hook did not start the pipe holder"
    );
    eprintln!("inherited-pipe deadline return: {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(11),
        "10s production child deadline was extended by inherited pipes: {elapsed:?}; result={result:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn synchronous_adapter_works_inside_an_existing_runtime() {
    let repo = repo("sha1");
    let expected = git_output(repo.path(), &["rev-parse", "HEAD"]);
    let mut request = GitRequest::new(repo.path());
    assert_eq!(request.text(&["rev-parse", "HEAD"]).unwrap(), expected);
    assert_eq!(request.diagnostics().invocations, 1);
    assert_eq!(request.diagnostics().peak_children, 1);
}
