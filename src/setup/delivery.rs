#![forbid(unsafe_code)]
//! Bounded release transport, archive inspection and recoverable owned swaps.
use super::platform::Platform;
use crate::domain::error::{MineError, MineResult};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_BINARY: u64 = 256 * 1024 * 1024;
const MAX_UNPACKED: u64 = 512 * 1024 * 1024;

pub(crate) fn failure(detail: impl Into<String>) -> MineError {
    MineError::ExternalDependency {
        detail: detail.into(),
    }
}

pub fn validated_tag(tag: &str) -> MineResult<&str> {
    let pieces: Vec<_> = tag.strip_prefix('v').unwrap_or(tag).split('.').collect();
    if pieces.len() != 3
        || pieces.iter().any(|p| {
            p.is_empty()
                || p.len() > 18
                || !p.bytes().all(|c| c.is_ascii_digit())
                || p.parse::<u64>().is_err()
        })
    {
        return Err(failure(
            "malformed release tag: expected [v]MAJOR.MINOR.PATCH",
        ));
    }
    Ok(tag)
}

pub(crate) fn release_identity() -> MineResult<(String, String)> {
    let account = std::env::var("MINE_RELEASE_ACCOUNT").unwrap_or_else(|_| "6ixGODD".into());
    let repo = std::env::var("MINE_RELEASE_REPO").unwrap_or_else(|_| "mine".into());
    for value in [&account, &repo] {
        if value.is_empty()
            || value.len() > 100
            || !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            || value == "."
            || value == ".."
        {
            return Err(failure("invalid release account/repository identity"));
        }
    }
    Ok((account, repo))
}

pub fn http_failure(
    status: u16,
    remaining: Option<&str>,
    reset: Option<&str>,
    retry: Option<&str>,
) -> String {
    let numeric = |v: Option<&str>| {
        v.filter(|s| !s.is_empty() && s.len() <= 20 && s.bytes().all(|c| c.is_ascii_digit()))
            .unwrap_or("unknown")
            .to_string()
    };
    match status {
        404 => {
            "HTTP 404: no published release or requested asset; check pinned tag/platform".into()
        }
        429 => format!(
            "HTTP 429: rate limit; retry-after={}, reset={}; no automatic retry",
            numeric(retry),
            numeric(reset)
        ),
        403 if remaining == Some("0") || retry.is_some() => format!(
            "HTTP 403: rate limit; retry-after={}, reset={}; no automatic retry",
            numeric(retry),
            numeric(reset)
        ),
        401 | 403 => {
            format!("HTTP {status}: authorization/access denied (not proven rate limiting)")
        }
        _ => format!("HTTP {status}: release request failed"),
    }
}

pub(crate) struct Client(ureq::Agent);
impl Client {
    pub(crate) fn new() -> Self {
        Self(
            ureq::Agent::config_builder()
                .https_only(true)
                .http_status_as_error(false)
                .max_redirects(5)
                .timeout_connect(Some(Duration::from_secs(10)))
                .timeout_recv_response(Some(Duration::from_secs(20)))
                .timeout_global(Some(Duration::from_secs(90)))
                .build()
                .into(),
        )
    }
    pub(crate) fn fetch(&self, url: &str, output: &mut impl Write, limit: u64) -> MineResult<()> {
        let mut response = self
            .0
            .get(url)
            .header("User-Agent", "mine-delivery")
            .call()
            .map_err(|e| match e {
                ureq::Error::Timeout(_) => failure("release request timeout; no automatic retry"),
                _ => failure("release network/TLS request failed; no automatic retry"),
            })?;
        if response.status().as_u16() != 200 {
            let h = |key: &str| response.headers().get(key).and_then(|v| v.to_str().ok());
            return Err(failure(http_failure(
                response.status().as_u16(),
                h("x-ratelimit-remaining"),
                h("x-ratelimit-reset"),
                h("retry-after"),
            )));
        }
        let count = io::copy(&mut response.body_mut().as_reader().take(limit + 1), output)
            .map_err(|_| failure("release response body failed or timed out"))?;
        if count > limit {
            return Err(failure("release response exceeded byte limit"));
        }
        Ok(())
    }
}

pub fn verify_checksum(archive: &Path, sidecar: &str, expected: &str) -> MineResult<()> {
    let mut lines = sidecar.lines();
    let fields: Vec<_> = lines.next().unwrap_or("").split_whitespace().collect();
    if fields.len() != 2
        || lines.next().is_some()
        || fields[1].strip_prefix('*').unwrap_or(fields[1]) != expected
        || fields[0].len() != 64
        || !fields[0].bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(failure(
            "missing/malformed/ambiguous checksum or wrong archive filename",
        ));
    }
    let mut file = File::open(archive)?;
    if file.metadata()?.len() > MAX_ARCHIVE {
        return Err(failure("archive exceeded byte limit"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != fields[0].to_ascii_lowercase() {
        return Err(failure("archive SHA-256 mismatch"));
    }
    Ok(())
}

fn safe_name(raw: &[u8], directory: bool) -> MineResult<String> {
    let name = std::str::from_utf8(raw).map_err(|_| failure("archive path is not UTF-8"))?;
    if name.len() > 1024
        || name
            .bytes()
            .any(|c| c < 32 || c == 127 || c == b'\\' || c == b':')
        || name.starts_with('/')
    {
        return Err(failure("unsafe archive path"));
    }
    let mut pieces = Vec::new();
    for part in name.split('/') {
        match part {
            ".." => return Err(failure("archive path traversal")),
            "." | "" => (),
            _ => pieces.push(part),
        }
    }
    if pieces.is_empty() && !directory {
        return Err(failure("empty archive file path"));
    }
    Ok(pieces.join("/"))
}

/// Validate all names/types before writing; never extract archive paths.
pub fn stage_archive(archive: &Path, platform: Platform, output: &Path) -> MineResult<()> {
    if fs::metadata(archive)?.len() > MAX_ARCHIVE {
        return Err(failure("archive exceeded byte limit"));
    }
    let mut seen = HashSet::new();
    let mut binary = None;
    let mut total = 0_u64;
    let mut inspect =
        |raw: &[u8], directory: bool, regular: bool, size: u64, index: usize| -> MineResult<()> {
            if !directory && !regular {
                return Err(failure("archive links/special entries forbidden"));
            }
            let name = safe_name(raw, directory)?;
            if index >= 10_000 || !seen.insert(name.clone()) {
                return Err(failure("too many or duplicate archive entries"));
            }
            total = total
                .checked_add(size)
                .ok_or_else(|| failure("archive size overflow"))?;
            if total > MAX_UNPACKED {
                return Err(failure("unpacked archive exceeds limit"));
            }
            if !directory
                && name.rsplit('/').next() == Some(platform.binary)
                && (size == 0 || size > MAX_BINARY || binary.replace(index).is_some())
            {
                return Err(failure("ambiguous/empty/oversized binary"));
            }
            Ok(())
        };
    if platform.archive.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(File::open(archive)?)
            .map_err(|_| failure("malformed ZIP archive"))?;
        for index in 0..zip.len() {
            let entry = zip
                .by_index(index)
                .map_err(|_| failure("malformed ZIP entry"))?;
            let kind = entry.unix_mode().unwrap_or(0) & 0o170000;
            if !matches!(kind, 0 | 0o100000 | 0o040000) || (kind == 0o040000 && !entry.is_dir()) {
                return Err(failure(
                    "archive links/special or inconsistent entries forbidden",
                ));
            }
            inspect(
                entry.name_raw(),
                entry.is_dir(),
                kind == 0 || kind == 0o100000,
                entry.size(),
                index,
            )?;
        }
        let mut entry = zip
            .by_index(binary.ok_or_else(|| failure("archive has no native binary"))?)
            .map_err(|_| failure("malformed ZIP binary"))?;
        copy_binary(&mut entry, output)?;
    } else {
        let mut archive_reader =
            tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?));
        for (index, entry) in archive_reader
            .entries()
            .map_err(|_| failure("malformed TAR archive"))?
            .raw(true)
            .enumerate()
        {
            let entry = entry.map_err(|_| failure("malformed/truncated TAR entry"))?;
            inspect(
                &entry.path_bytes(),
                entry.header().entry_type().is_dir(),
                entry.header().entry_type().is_file(),
                entry.size(),
                index,
            )?;
        }
        let count = io::copy(
            &mut archive_reader.into_inner().take(MAX_UNPACKED + 1),
            &mut io::sink(),
        )
        .map_err(|_| failure("truncated/corrupt gzip trailer"))?;
        if count > MAX_UNPACKED {
            return Err(failure("oversized trailing archive data"));
        }
        let index = binary.ok_or_else(|| failure("archive has no native binary"))?;
        let mut archive_reader =
            tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?));
        let mut entry = archive_reader
            .entries()?
            .raw(true)
            .nth(index)
            .ok_or_else(|| failure("missing TAR binary"))??;
        copy_binary(&mut entry, output)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(output, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn copy_binary(reader: &mut impl Read, output: &Path) -> MineResult<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)?;
    let count = io::copy(&mut reader.take(MAX_BINARY + 1), &mut file)?;
    if count == 0 || count > MAX_BINARY {
        return Err(failure("empty/oversized extracted executable"));
    }
    file.sync_all()?;
    Ok(())
}

pub(crate) fn validate_version(binary: &Path, tag: &str) -> MineResult<()> {
    validated_tag(tag)?;
    let out = binary.with_extension("version-output");
    let file = OpenOptions::new().create_new(true).write(true).open(&out)?;
    let result = (|| {
        let mut child = Command::new(binary)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file)
            .spawn()
            .map_err(|_| failure("candidate is not a runnable native executable"))?;
        let result = (|| {
            let start = Instant::now();
            let status = loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                if start.elapsed() > Duration::from_secs(10) || fs::metadata(&out)?.len() > 4096 {
                    return Err(failure(
                        "candidate version check timed out/exceeded output limit",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            if fs::metadata(&out)?.len() > 4096 {
                return Err(failure("oversized candidate version output"));
            }
            let text = fs::read_to_string(&out)?;
            if !status.success()
                || text.trim() != format!("mine {}", tag.strip_prefix('v').unwrap_or(tag))
            {
                return Err(failure("candidate version does not match pinned release"));
            }
            Ok(())
        })();
        if result.is_err() && child.try_wait()?.is_none() {
            child.kill()?;
            child.wait()?;
        }
        result
    })();
    fs::remove_file(out)?;
    result
}

/// Private fixed sibling directory: exclusive lock plus durable recovery record.
pub(crate) struct Swap {
    root: PathBuf,
    destination: PathBuf,
}
pub struct Replacement {
    pub destination: PathBuf,
    pub cleanup_pending: Option<String>,
}
impl Swap {
    pub(crate) fn begin(destination: &Path) -> MineResult<Self> {
        if let Ok(meta) = fs::symlink_metadata(destination) {
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(failure("installation target is not a regular executable"));
            }
        }
        let root = destination
            .parent()
            .ok_or_else(|| failure("installation has no parent"))?
            .join(".mine-binary-swap");
        fs::create_dir(&root).map_err(|_| {
            failure(format!(
                "pending/busy binary swap at {}; recover before retry",
                root.display()
            ))
        })?;
        let swap = Self {
            root,
            destination: destination.to_path_buf(),
        };
        let record = serde_json::json!({"schema_version":1,"destination":swap.destination,"candidate":swap.candidate(),"backup":swap.root.join("previous")});
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(swap.root.join("record.json"))?;
        file.write_all(
            serde_json::to_string(&record)
                .map_err(|_| failure("cannot serialize swap record"))?
                .as_bytes(),
        )?;
        file.sync_all()?;
        Ok(swap)
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn candidate(&self) -> PathBuf {
        self.root.join(if cfg!(windows) {
            "candidate.exe"
        } else {
            "candidate"
        })
    }
    pub(crate) fn commit(self, tag: &str) -> MineResult<Replacement> {
        validate_version(&self.candidate(), tag)?;
        let expected = file_hash(&self.candidate())?;
        let previous = self.root.join("previous");
        let had_old = self.destination.exists();
        if had_old {
            fs::rename(&self.destination, &previous).map_err(|_| {
                failure("old executable cannot move; swap pending, old install preserved")
            })?;
        }
        if let Err(error) = fs::rename(self.candidate(), &self.destination) {
            if had_old && fs::rename(&previous, &self.destination).is_err() {
                return Err(failure(
                    "replacement/rollback failed; recover recorded previous executable",
                ));
            }
            return Err(failure(format!(
                "replacement failed; old install restored; staging retained: {error}"
            )));
        }
        if !file_hash(&self.destination).is_ok_and(|hash| hash == expected) {
            fs::rename(&self.destination, self.candidate())?;
            if had_old {
                fs::rename(&previous, &self.destination)?;
            }
            return Err(failure(
                "installed binary verification failed; prior install restored; swap retained",
            ));
        }
        let destination = self.destination.clone();
        if had_old && fs::remove_file(previous).is_err() {
            return Ok(Replacement {
                destination,
                cleanup_pending: Some(format!(
                    "new binary installed; old backup cleanup pending at {} after the running process exits; inspect record.json before retry",
                    self.root.display()
                )),
            });
        }
        let cleanup_pending = self
            .cleanup()
            .err()
            .map(|error| format!("new binary installed; owned staging cleanup pending: {error}"));
        Ok(Replacement {
            destination,
            cleanup_pending,
        })
    }
    pub(crate) fn cleanup(self) -> MineResult<()> {
        fs::remove_dir_all(self.root)?;
        Ok(())
    }
}

fn file_hash(path: &Path) -> MineResult<Vec<u8>> {
    let mut hash = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hash)?;
    Ok(hash.finalize().to_vec())
}

pub(crate) fn download_replace(tag: &str) -> MineResult<Replacement> {
    validated_tag(tag)?;
    let platform = super::platform::current()?;
    let (account, repo) = release_identity()?;
    let swap = Swap::begin(&std::env::current_exe()?)?;
    let result = (|| {
        let client = Client::new();
        let url = format!(
            "https://github.com/{account}/{repo}/releases/download/{tag}/{}",
            platform.archive
        );
        let archive = swap.root().join(platform.archive);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&archive)?;
        client.fetch(&url, &mut file, MAX_ARCHIVE)?;
        file.sync_all()?;
        drop(file);
        let mut sum = Vec::new();
        client.fetch(&format!("{url}.sha256"), &mut sum, 4096)?;
        verify_checksum(
            &archive,
            std::str::from_utf8(&sum).map_err(|_| failure("checksum is not UTF-8"))?,
            platform.archive,
        )?;
        stage_archive(&archive, platform, &swap.candidate())?;
        validate_version(&swap.candidate(), tag)
    })();
    if let Err(error) = result {
        swap.cleanup().map_err(|_| {
            failure(format!(
                "validation failed ({error}); owned staging cleanup pending"
            ))
        })?;
        return Err(error);
    }
    swap.commit(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn transport_bounds_bytes_status_timeout_and_forbids_plaintext_production() {
        fn request(response: &'static [u8], delay: Duration, limit: u64) -> MineResult<()> {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0; 2048];
                let _ = socket.read(&mut request);
                std::thread::sleep(delay);
                let _ = socket.write_all(response);
            });
            // Local plaintext transport exists only in this private test constructor.
            let client = Client(
                ureq::Agent::config_builder()
                    .http_status_as_error(false)
                    .timeout_global(Some(Duration::from_millis(300)))
                    .build()
                    .into(),
            );
            let result = client.fetch(&format!("http://{address}/asset"), &mut Vec::new(), limit);
            server.join().unwrap();
            result
        }
        request(
            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ndata",
            Duration::ZERO,
            4,
        )
        .unwrap();
        assert!(
            request(
                b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nlarge",
                Duration::ZERO,
                4
            )
            .unwrap_err()
            .to_string()
            .contains("byte limit")
        );
        assert!(
            request(
                b"HTTP/1.1 403 Forbidden\r\nx-ratelimit-remaining: 0\r\nContent-Length: 0\r\n\r\n",
                Duration::ZERO,
                4
            )
            .unwrap_err()
            .to_string()
            .contains("rate limit")
        );
        assert!(
            request(
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
                Duration::from_millis(500),
                4
            )
            .unwrap_err()
            .to_string()
            .contains("timeout")
        );
        assert!(
            Client::new()
                .fetch("http://127.0.0.1:1/asset", &mut Vec::new(), 4)
                .is_err()
        );
    }

    #[test]
    fn swap_verifies_before_touching_old_install_and_preserves_pending_record() {
        let root = tempfile::tempdir().unwrap();
        let destination = root
            .path()
            .join(if cfg!(windows) { "mine.exe" } else { "mine" });
        fs::write(&destination, b"old install").unwrap();
        let swap = Swap::begin(&destination).unwrap();
        assert!(Swap::begin(&destination).is_err());
        fs::write(swap.candidate(), b"not native").unwrap();
        assert!(swap.commit("v0.2.0").is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"old install");
        assert!(root.path().join(".mine-binary-swap/record.json").exists());
        assert!(
            !root
                .path()
                .join(".mine-binary-swap/candidate.version-output")
                .exists()
        );
    }

    #[test]
    fn swap_real_native_candidate_succeeds_and_cleans_only_owned_staging() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("fixture.rs");
        fs::write(&source, "fn main() { if std::env::args().any(|a| a == \"--hold\") { let mut s=String::new(); std::io::stdin().read_line(&mut s).unwrap(); } else { println!(\"mine 0.2.0\"); } }").unwrap();
        let executable = root.path().join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        let output = Command::new("rustc")
            .arg("--crate-name")
            .arg("delivery_fixture")
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let destination = root
            .path()
            .join(if cfg!(windows) { "mine.exe" } else { "mine" });
        fs::write(&destination, b"old install").unwrap();
        fs::write(root.path().join("unrelated"), b"preserve").unwrap();
        let swap = Swap::begin(&destination).unwrap();
        fs::copy(&executable, swap.candidate()).unwrap();
        let replacement = swap.commit("v0.2.0").unwrap();
        assert!(replacement.cleanup_pending.is_none());
        assert_eq!(replacement.destination, destination);
        assert_eq!(
            fs::read(destination).unwrap(),
            fs::read(&executable).unwrap()
        );
        assert!(!root.path().join(".mine-binary-swap").exists());
        assert_eq!(
            fs::read(root.path().join("unrelated")).unwrap(),
            b"preserve"
        );
        #[cfg(windows)]
        {
            // This is a real running .exe, not just an open-file simulation.
            let mut running = Command::new(&replacement.destination)
                .arg("--hold")
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let swap = Swap::begin(&replacement.destination).unwrap();
            fs::copy(&executable, swap.candidate()).unwrap();
            let outcome = swap.commit("v0.2.0");
            running.kill().unwrap();
            running.wait().unwrap();
            match outcome {
                Ok(outcome) => assert!(
                    outcome.cleanup_pending.is_some(),
                    "running image backup cannot be silently retired"
                ),
                Err(error) => assert!(
                    error.to_string().contains("old executable cannot move"),
                    "{error}"
                ),
            }
            assert!(replacement.destination.exists());
            assert!(root.path().join(".mine-binary-swap/record.json").exists());
        }
    }
}
