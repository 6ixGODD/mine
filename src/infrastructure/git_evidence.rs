//! Request-local, bounded Git transport and exact evidence. No shell, daemon,
//! global PATH mutation, persistent cache, or implicit network object fetch.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, Command};

use crate::domain::error::{GitFailure, MineResult};
use crate::infrastructure::git::{WorktreeInfo, parse_worktrees};

#[derive(Debug, Clone, Copy)]
pub struct GitLimits {
    pub child_time: Duration,
    pub request_time: Duration,
    pub stream_bytes: usize,
}

impl Default for GitLimits {
    fn default() -> Self {
        Self {
            child_time: Duration::from_secs(10),
            request_time: Duration::from_secs(30),
            stream_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Counts successful direct-child launches, not aggregate/concurrent guesses.
#[derive(Debug, Clone, Default)]
pub struct GitDiagnostics {
    pub invocations: u64,
    pub object_batches: u64,
    pub peak_children: usize,
    pub git_time: Duration,
}

/// Budget only: object/ref/worktree evidence stays in its individual Git view.
/// Synchronous CLI/MCP handlers own a lexical scope; nothing survives it in TLS.
struct RequestBudget {
    identity: Arc<()>,
    limits: GitLimits,
    started: Instant,
    cancel: Cancellation,
    diagnostics: Mutex<GitDiagnostics>,
    child_active: AtomicBool,
}

impl RequestBudget {
    fn new(limits: GitLimits) -> Self {
        Self {
            identity: Arc::new(()),
            limits,
            started: Instant::now(),
            cancel: Cancellation::default(),
            diagnostics: Mutex::new(GitDiagnostics::default()),
            child_active: AtomicBool::new(false),
        }
    }

    fn check(&self) -> Result<(), GitFailure> {
        if self.cancel.cancelled() {
            return Err(GitFailure::Cancelled);
        }
        if self.started.elapsed() >= self.limits.request_time {
            return Err(GitFailure::RequestBudget);
        }
        Ok(())
    }
}

thread_local! {
    static REQUEST_BUDGETS: RefCell<Vec<Arc<RequestBudget>>> = const { RefCell::new(Vec::new()) };
}

/// Never hold this across an await or move it to another thread. MCP tool
/// handlers are synchronous; the IO bridge receives the Arc explicitly.
/// Drop removes the exact scope, including when nested contexts drop out of order.
pub(crate) struct GitRequestScope {
    budget: Arc<RequestBudget>,
    _same_thread: PhantomData<Rc<()>>,
}

impl GitRequestScope {
    pub(crate) fn new() -> Self {
        Self::with_limits(GitLimits::default())
    }

    fn with_limits(limits: GitLimits) -> Self {
        let budget = Arc::new(RequestBudget::new(limits));
        REQUEST_BUDGETS.with_borrow_mut(|scopes| scopes.push(budget.clone()));
        Self {
            budget,
            _same_thread: PhantomData,
        }
    }

    pub(crate) fn join_or_new() -> Option<Self> {
        Self::current().is_none().then(Self::new)
    }

    fn current() -> Option<Arc<RequestBudget>> {
        REQUEST_BUDGETS.with_borrow(|scopes| scopes.last().cloned())
    }
}

impl Drop for GitRequestScope {
    fn drop(&mut self) {
        REQUEST_BUDGETS.with_borrow_mut(|scopes| {
            scopes.retain(|budget| !Arc::ptr_eq(budget, &self.budget));
        });
    }
}

/// Expensive pure phases must not acquire a fresh clock before the next child.
/// Recovery-only journal reads remain available after expiry to report truth.
pub(crate) fn check_request_budget() -> MineResult<()> {
    if let Some(budget) = GitRequestScope::current() {
        budget.check()?;
    }
    Ok(())
}

/// Identity only, not evidence. Retaining this nonce prevents allocation reuse
/// from letting a journal witness escape into a later public request.
pub(crate) fn request_identity() -> Option<Arc<()>> {
    GitRequestScope::current().map(|budget| budget.identity.clone())
}

struct ChildPermit(Arc<RequestBudget>);
impl Drop for ChildPermit {
    fn drop(&mut self) {
        self.0.child_active.store(false, Ordering::Release);
    }
}

/// Narrow owned-child seam: tests supply one direct fixture child, never PATH.
/// Use GitChild::spawn to obtain cancellable pipes on every supported platform.
/// Called inside the adapter's IO-enabled runtime, never the caller's runtime.
pub trait GitSpawner: Send + Sync {
    fn spawn(&self, repo: &Path, args: &[&str]) -> io::Result<GitChild>;
}

/// One owned child and its IO. Windows Tokio anonymous pipes use blocking IO,
/// so use overlapped named pipes there rather than leaving uncancellable reads
/// in the runtime's blocking pool. Tests inject the same ownership boundary.
pub struct GitChild {
    child: Child,
    abort_io: Arc<AtomicBool>,
    pub stdin: Option<Box<dyn AsyncWrite + Unpin + Send>>,
    pub stdout: Option<Box<dyn AsyncRead + Unpin + Send>>,
    pub stderr: Option<Box<dyn AsyncRead + Unpin + Send>>,
}

impl GitChild {
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Takes the command by value: parent copies of child-side handles must
    /// close immediately after spawn, otherwise they would prevent pipe EOF.
    pub fn spawn(mut command: Command) -> io::Result<Self> {
        command.kill_on_drop(true);
        let abort_io = Arc::new(AtomicBool::new(false));
        #[cfg(windows)]
        {
            let (stdin, input) = windows_pipe(true)?;
            let (stdout, output) = windows_pipe(false)?;
            let (stderr, error) = windows_pipe(false)?;
            command.stdin(input).stdout(output).stderr(error);
            let child = command.spawn()?;
            Ok(Self {
                child,
                stdin: Some(Box::new(WindowsPipe {
                    pipe: stdin,
                    abort_io: abort_io.clone(),
                })),
                stdout: Some(Box::new(WindowsPipe {
                    pipe: stdout,
                    abort_io: abort_io.clone(),
                })),
                stderr: Some(Box::new(WindowsPipe {
                    pipe: stderr,
                    abort_io: abort_io.clone(),
                })),
                abort_io,
            })
        }
        #[cfg(not(windows))]
        {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            Ok(Self {
                stdin: child.stdin.take().map(|pipe| Box::new(pipe) as _),
                stdout: child.stdout.take().map(|pipe| Box::new(pipe) as _),
                stderr: child.stderr.take().map(|pipe| Box::new(pipe) as _),
                child,
                abort_io,
            })
        }
    }
}

#[cfg(windows)]
struct WindowsPipe {
    pipe: tokio::net::windows::named_pipe::NamedPipeServer,
    abort_io: Arc<AtomicBool>,
}

#[cfg(windows)]
impl Drop for WindowsPipe {
    fn drop(&mut self) {
        if self.abort_io.load(Ordering::Acquire) {
            // Mio deliberately keeps pending writes alive on ordinary drop.
            // Abort must disconnect first, including when descendants inherit
            // stdin. Never do this on success: it would discard unread input.
            let _ = self.pipe.disconnect();
        }
    }
}

#[cfg(windows)]
impl AsyncRead for WindowsPipe {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.pipe).poll_read(cx, buf)
    }
}

#[cfg(windows)]
impl AsyncWrite for WindowsPipe {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.pipe).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.pipe).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.pipe).poll_shutdown(cx)
    }
}

#[cfg(windows)]
fn windows_pipe(
    parent_writes: bool,
) -> io::Result<(tokio::net::windows::named_pipe::NamedPipeServer, Stdio)> {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    use tokio::net::windows::named_pipe::ServerOptions;

    let name = format!(r"\\.\pipe\mine-git-{}", uuid::Uuid::new_v4());
    let server = ServerOptions::new()
        .first_pipe_instance(true)
        .max_instances(1)
        .reject_remote_clients(true)
        .access_inbound(!parent_writes)
        .access_outbound(parent_writes)
        .create(&name)?;
    // The child uses ordinary synchronous handles; only the parent's IO must
    // be overlapped/cancellable. This locally owned, already-open client makes
    // connect immediately ready. Refuse an unexpected pending setup before
    // spawning a child, rather than blocking or running nested block_on.
    let client = std::fs::OpenOptions::new()
        .read(parent_writes)
        .write(!parent_writes)
        .open(&name)?;
    {
        let mut connect = std::pin::pin!(server.connect());
        match connect
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result?,
            Poll::Pending => return Err(io::Error::from(io::ErrorKind::WouldBlock)),
        }
    }
    Ok((server, Stdio::from(client)))
}

pub struct SystemGit;

impl GitSpawner for SystemGit {
    fn spawn(&self, repo: &Path, args: &[&str]) -> io::Result<GitChild> {
        let mut command = Command::new("git");
        command
            .current_dir(repo)
            .arg("-C")
            .arg(".")
            .args(args)
            // Verified in Git v2.45.1 promisor-remote.c. Do not depend on a
            // recent --no-lazy-fetch option silently absent on the baseline.
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        GitChild::spawn(command)
    }
}

async fn capture(mut stream: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, GitFailure> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let count = match stream.read(&mut chunk).await {
            Ok(count) => count,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(GitFailure::ProcessIo),
        };
        if count == 0 {
            return Ok(bytes);
        }
        if count > limit.saturating_sub(bytes.len()) {
            return Err(GitFailure::OutputLimit);
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

async fn read_pipe(
    mut stream: Box<dyn AsyncRead + Unpin + Send>,
    limit: usize,
    abort_io: Arc<AtomicBool>,
) -> Result<Vec<u8>, GitFailure> {
    let result = capture(&mut stream, limit).await;
    if result.is_err() {
        abort_io.store(true, Ordering::Release);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefInfo {
    pub name: String,
    pub oid: String,
    pub object_type: String,
}

/// A mutable request handle serializes its children. Immutable evidence and
/// inventories never escape into a process-global cache. Application callers
/// retain this handle through their graph-operation snapshot.
pub struct GitRequest {
    repo: PathBuf,
    budget: Arc<RequestBudget>,
    spawner: Arc<dyn GitSpawner>,
    oid_width: Option<usize>,
    commits: HashSet<String>,
    refs: Option<Vec<RefInfo>>,
    worktrees: Option<Vec<WorktreeInfo>>,
    ancestry: HashMap<(String, String), bool>,
}

impl GitRequest {
    pub fn new(repo: &Path) -> Self {
        Self::with_limits(repo, GitLimits::default())
    }

    pub fn with_limits(repo: &Path, limits: GitLimits) -> Self {
        Self::with_spawner(repo, limits, Arc::new(SystemGit))
    }

    pub fn with_spawner(repo: &Path, limits: GitLimits, spawner: Arc<dyn GitSpawner>) -> Self {
        Self {
            repo: repo.to_path_buf(),
            // A private-index/repository/spawner view is not a policy override
            // or a new public request. Standalone adapters retain explicit limits.
            budget: GitRequestScope::current()
                .unwrap_or_else(|| Arc::new(RequestBudget::new(limits))),
            spawner,
            oid_width: None,
            commits: HashSet::new(),
            refs: None,
            worktrees: None,
            ancestry: HashMap::new(),
        }
    }

    pub fn cancellation(&self) -> Cancellation {
        self.budget.cancel.clone()
    }

    pub fn diagnostics(&self) -> GitDiagnostics {
        self.budget
            .diagnostics
            .lock()
            .expect("Git diagnostic mutex poisoned")
            .clone()
    }

    fn gate(&self) -> Result<(), GitFailure> {
        self.budget.check()
    }

    /// The raw form preserves merge-base's meaningful exit 1. Every caller
    /// must explicitly classify its accepted statuses; all transport errors
    /// remain errors, including partial output followed by a failed child.
    pub fn run_raw(&mut self, args: &[&str], input: &[u8]) -> MineResult<Output> {
        self.gate()?;
        if self.budget.limits.child_time.is_zero()
            || self.budget.limits.request_time.is_zero()
            || self.budget.limits.stream_bytes == 0
            || input.len() > self.budget.limits.stream_bytes
        {
            return Err(GitFailure::InvalidInput.into());
        }
        self.budget
            .child_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| GitFailure::InvalidInput)?;
        let _permit = ChildPermit(self.budget.clone());
        let began = Instant::now();
        // Synchronous Git helpers are also called from MCP's Tokio runtime.
        // A scoped bridge avoids nested block_on and always joins before return.
        // Pipe IO itself is async: no blocking readers or detached IO tasks.
        let result = thread::scope(|scope| {
            scope
                .spawn(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| GitFailure::ProcessIo)?;
                    runtime.block_on(async {
                        self.gate()?;
                        let mut child = self
                            .spawner
                            .spawn(&self.repo, args)
                            .map_err(|_| GitFailure::ProcessIo)?;
                        {
                            let mut diagnostics = self
                                .budget
                                .diagnostics
                                .lock()
                                .expect("Git diagnostic mutex poisoned");
                            diagnostics.invocations += 1;
                            if args.first() == Some(&"cat-file")
                                && args.iter().any(|arg| arg.starts_with("--batch-check"))
                            {
                                diagnostics.object_batches += 1;
                            }
                            diagnostics.peak_children = 1;
                        }
                        let result = self.collect(&mut child, input, began).await;
                        // collect has dropped every pipe future/handle here, even
                        // if a hook's descendant still holds its other pipe end.
                        // Never retry a possibly completed Git mutation.
                        if result.is_err() {
                            let _ = child.child.start_kill();
                        }
                        // Cleanup has a separate bounded allowance even after
                        // the execution budget expires. Never hide its first
                        // failure or claim success from an unreaped child.
                        let reaped =
                            tokio::time::timeout(Duration::from_secs(2), child.child.wait()).await;
                        if result.is_ok() {
                            reaped
                                .map_err(|_| GitFailure::ProcessIo)?
                                .map_err(|_| GitFailure::ProcessIo)?;
                        }
                        result
                    })
                })
                .join()
                .unwrap_or(Err(GitFailure::ProcessIo))
        });
        // Include explicit termination/reaping and the bridge join.
        self.budget
            .diagnostics
            .lock()
            .expect("Git diagnostic mutex poisoned")
            .git_time += began.elapsed();
        result.map_err(Into::into)
    }

    async fn collect(
        &self,
        child: &mut GitChild,
        input: &[u8],
        began: Instant,
    ) -> Result<Output, GitFailure> {
        let mut stdin = child.stdin.take().ok_or(GitFailure::ProcessIo)?;
        let stdout = child.stdout.take().ok_or(GitFailure::ProcessIo)?;
        let stderr = child.stderr.take().ok_or(GitFailure::ProcessIo)?;
        let limit = self.budget.limits.stream_bytes;
        let input_abort = child.abort_io.clone();
        let output_abort = child.abort_io.clone();
        let error_abort = child.abort_io.clone();
        let wait_abort = child.abort_io.clone();
        let io = async {
            let (_, stdout, stderr, status) = tokio::try_join!(
                async move {
                    let result = stdin
                        .write_all(input)
                        .await
                        .map_err(|_| GitFailure::ProcessIo);
                    if result.is_err() {
                        input_abort.store(true, Ordering::Release);
                    }
                    drop(stdin); // EOF, also when write_all fails.
                    result
                },
                read_pipe(stdout, limit, output_abort),
                read_pipe(stderr, limit, error_abort),
                async {
                    let result = child.child.wait().await.map_err(|_| GitFailure::ProcessIo);
                    if result.is_err() {
                        wait_abort.store(true, Ordering::Release);
                    }
                    result
                },
            )?;
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        };
        let deadline = async {
            loop {
                self.gate()?;
                if began.elapsed() >= self.budget.limits.child_time {
                    return Err(GitFailure::ChildDeadline);
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        };
        // try_join polls all streams concurrently in this task. Dropping the
        // losing future synchronously closes all our handles; no reader task
        // survives collect, and inherited handles cannot extend its deadline.
        let abort_io = child.abort_io.clone();
        let mut io = std::pin::pin!(io);
        let result = tokio::select! {
            biased;
            result = deadline => result,
            result = &mut io => result,
        };
        // The pinned IO future still owns the pipes here. Set abort BEFORE it
        // drops, so Windows disconnect cancels pending writes as well as reads.
        if result.is_err() {
            abort_io.store(true, Ordering::Release);
        }
        result
    }

    pub fn run(&mut self, args: &[&str], input: &[u8]) -> MineResult<Vec<u8>> {
        let output = self.run_raw(args, input)?;
        if !output.status.success() {
            return Err(GitFailure::ChildExit.into());
        }
        Ok(output.stdout)
    }

    pub fn text(&mut self, args: &[&str]) -> MineResult<String> {
        let bytes = self.run(args, &[])?;
        Ok(std::str::from_utf8(&bytes)
            .map_err(|_| GitFailure::Protocol)?
            .trim()
            .to_owned())
    }

    fn width(&mut self) -> MineResult<usize> {
        if let Some(width) = self.oid_width {
            return Ok(width);
        }
        let width = match self.text(&["rev-parse", "--show-object-format"])?.as_str() {
            "sha1" => 40,
            "sha256" => 64,
            _ => return Err(GitFailure::Protocol.into()),
        };
        self.oid_width = Some(width);
        Ok(width)
    }

    fn exact_oid(oid: &str, width: usize) -> bool {
        oid.len() == width
            && oid
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    /// Validates all unique uncached full native OIDs in one ordered batch.
    /// Tag OIDs are not accepted by peeling, and replacements remain enabled.
    pub fn validate_commits(&mut self, oids: &[String]) -> MineResult<()> {
        self.gate()?;
        if oids.is_empty() {
            return Ok(());
        }
        let width = self.width()?;
        let mut unique = HashSet::new();
        let mut pending = Vec::new();
        for oid in oids {
            if !Self::exact_oid(oid, width) {
                return Err(GitFailure::Protocol.into());
            }
            if !self.commits.contains(oid) && unique.insert(oid) {
                pending.push(oid);
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        let mut input = Vec::new();
        for oid in &pending {
            input.extend_from_slice(oid.as_bytes());
            input.push(b'\n');
        }
        let bytes = self.run(
            &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
            &input,
        )?;
        let text = std::str::from_utf8(&bytes).map_err(|_| GitFailure::Protocol)?;
        let mut lines = text.split_terminator('\n');
        for oid in &pending {
            let line = lines.next().ok_or(GitFailure::Protocol)?;
            if line != format!("{oid} commit") {
                return Err(GitFailure::Protocol.into());
            }
        }
        if lines.next().is_some() || !text.ends_with('\n') {
            return Err(GitFailure::Protocol.into());
        }
        self.commits.extend(pending.into_iter().cloned());
        Ok(())
    }

    pub fn refs(&mut self) -> MineResult<Vec<RefInfo>> {
        self.gate()?;
        if let Some(refs) = &self.refs {
            return Ok(refs.clone());
        }
        let width = self.width()?;
        let bytes = self.run(
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(objecttype)",
            ],
            &[],
        )?;
        let text = std::str::from_utf8(&bytes).map_err(|_| GitFailure::Protocol)?;
        if !text.is_empty() && !text.ends_with('\n') {
            return Err(GitFailure::Protocol.into());
        }
        let mut refs = Vec::new();
        for line in text.split_terminator('\n') {
            let fields: Vec<_> = line.split('\0').collect();
            if fields.len() != 3
                || !fields[0].starts_with("refs/")
                || !Self::exact_oid(fields[1], width)
                || !matches!(fields[2], "commit" | "tag" | "blob" | "tree")
            {
                return Err(GitFailure::Protocol.into());
            }
            refs.push(RefInfo {
                name: fields[0].to_owned(),
                oid: fields[1].to_owned(),
                object_type: fields[2].to_owned(),
            });
        }
        self.refs = Some(refs.clone());
        Ok(refs)
    }

    pub fn worktrees(&mut self) -> MineResult<Vec<WorktreeInfo>> {
        self.gate()?;
        if let Some(records) = &self.worktrees {
            return Ok(records.clone());
        }
        let bytes = self.run(&["worktree", "list", "--porcelain", "-z"], &[])?;
        let records = parse_worktrees(&bytes)?;
        self.worktrees = Some(records.clone());
        Ok(records)
    }

    pub fn is_ancestor(&mut self, ancestor: &str, descendant: &str) -> MineResult<bool> {
        self.gate()?;
        let width = self.width()?;
        if !Self::exact_oid(ancestor, width) || !Self::exact_oid(descendant, width) {
            return Err(GitFailure::Protocol.into());
        }
        let key = (ancestor.to_owned(), descendant.to_owned());
        if let Some(value) = self.ancestry.get(&key) {
            return Ok(*value);
        }
        let output = self.run_raw(&["merge-base", "--is-ancestor", ancestor, descendant], &[])?;
        let value = match output.status.code() {
            Some(0) => true,
            Some(1) => false,
            _ => return Err(GitFailure::ChildExit.into()),
        };
        self.ancestry.insert(key, value);
        Ok(value)
    }

    /// Required before a mutation safety boundary. Replacement refs can change
    /// the interpreted type of an immutable OID, so discard those results too.
    /// Refresh never resets the deadline, cancellation or diagnostic counters.
    pub fn refresh_observations(&mut self) {
        self.refs = None;
        self.worktrees = None;
        self.ancestry.clear();
        self.commits.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cause(error: &crate::domain::error::MineError) -> GitFailure {
        let crate::domain::error::MineError::Io(error) = error else {
            panic!("expected bounded Git failure")
        };
        *error
            .get_ref()
            .unwrap()
            .downcast_ref::<GitFailure>()
            .unwrap()
    }

    #[test]
    fn request_budget_views_share_counts_cancellation_and_child_ownership() {
        let _scope = GitRequestScope::new();
        let directory = tempfile::tempdir().unwrap();
        let mut first = GitRequest::new(directory.path());
        let mut second = GitRequest::with_limits(
            directory.path(),
            GitLimits {
                request_time: Duration::from_secs(60),
                stream_bytes: 32 * 1024 * 1024,
                ..GitLimits::default()
            },
        );
        assert!(Arc::ptr_eq(&first.budget, &second.budget));
        assert_eq!(second.budget.limits.request_time, Duration::from_secs(30));
        assert_eq!(second.budget.limits.stream_bytes, 1024 * 1024);
        first.text(&["--version"]).unwrap();
        second.text(&["--version"]).unwrap();
        assert_eq!(first.diagnostics().invocations, 2);
        assert_eq!(second.diagnostics().invocations, 2);
        assert_eq!(second.diagnostics().peak_children, 1);
        assert!(!second.budget.child_active.load(Ordering::Acquire));
        first.cancellation().cancel();
        assert_eq!(
            cause(&second.text(&["--version"]).unwrap_err()),
            GitFailure::Cancelled
        );
        assert_eq!(second.diagnostics().invocations, 2);
    }

    #[test]
    fn request_budget_new_view_cannot_restart_elapsed_non_child_phases() {
        let _scope = GitRequestScope::with_limits(GitLimits {
            request_time: Duration::from_millis(20),
            ..GitLimits::default()
        });
        let directory = tempfile::tempdir().unwrap();
        let first = GitRequest::new(directory.path());
        // Deterministic lower bound, not an elapsed-time performance assertion.
        // Work between helpers consumes the same deadline as their children.
        thread::sleep(Duration::from_millis(35));
        let mut second = GitRequest::new(directory.path());
        assert!(Arc::ptr_eq(&first.budget, &second.budget));
        assert_eq!(
            cause(&check_request_budget().unwrap_err()),
            GitFailure::RequestBudget
        );
        assert_eq!(
            cause(&second.text(&["--version"]).unwrap_err()),
            GitFailure::RequestBudget
        );
        assert_eq!(second.diagnostics().invocations, 0);
    }

    #[test]
    fn request_budget_lexical_scopes_do_not_leak_between_mcp_calls() {
        let outer = GitRequestScope::new();
        outer.budget.cancel.cancel();
        {
            let inner = GitRequestScope::new();
            let directory = tempfile::tempdir().unwrap();
            let mut request = GitRequest::new(directory.path());
            assert!(Arc::ptr_eq(&request.budget, &inner.budget));
            request.text(&["--version"]).unwrap();
            assert_eq!(request.diagnostics().invocations, 1);
        }
        assert_eq!(
            cause(&check_request_budget().unwrap_err()),
            GitFailure::Cancelled
        );
        drop(outer);
        assert!(GitRequestScope::current().is_none());
        check_request_budget().unwrap();
    }

    struct FailingRead;

    impl AsyncRead for FailingRead {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<io::Result<()>> {
            std::task::Poll::Ready(Err(io::Error::other("injected read failure")))
        }
    }

    #[tokio::test]
    async fn capture_rejects_read_error_and_never_returns_a_truncated_success() {
        assert_eq!(capture(FailingRead, 8).await, Err(GitFailure::ProcessIo));
        assert_eq!(
            capture(&b"123456789"[..], 8).await,
            Err(GitFailure::OutputLimit)
        );
        assert_eq!(capture(&b"12345678"[..], 8).await.unwrap(), b"12345678");
    }
}
