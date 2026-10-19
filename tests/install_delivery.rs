//! Real native bootstrap entrypoints with test-only transport, plus archive boundaries.
use mine::setup::{delivery, platform};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};

fn checksum(path: &Path, name: &str) -> String {
    format!("{:x}  {name}\n", Sha256::digest(fs::read(path).unwrap()))
}

fn zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, contents) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap();
}

fn tar(path: &Path, entries: &[(&str, &[u8], tar::EntryType)]) {
    let gzip =
        flate2::write::GzEncoder::new(File::create(path).unwrap(), flate2::Compression::fast());
    let mut writer = tar::Builder::new(gzip);
    for (name, contents, kind) in entries {
        let mut header = tar::Header::new_gnu();
        // Deliberately construct hostile names that Builder::append_path refuses.
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(*kind);
        header.set_mode(0o755);
        header.set_size(contents.len() as u64);
        if kind.is_symlink() || kind.is_hard_link() {
            header.set_link_name("outside").unwrap();
        }
        header.set_cksum();
        writer.append(&header, *contents).unwrap();
    }
    writer.into_inner().unwrap().finish().unwrap();
}

#[test]
fn manifest_and_all_generated_dispatches_match() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("distribution/platforms.json")).unwrap())
            .unwrap();
    assert_eq!(
        value["platforms"].as_array().unwrap().len(),
        platform::PLATFORMS.len()
    );
    for (row, native) in value["platforms"]
        .as_array()
        .unwrap()
        .iter()
        .zip(platform::PLATFORMS)
    {
        assert_eq!(row["os"], native.os);
        assert_eq!(row["arch"], native.arch);
        assert_eq!(row["target"], native.target);
        assert_eq!(row["archive"], native.archive);
        assert_eq!(row["binary"], native.binary);
        assert_eq!(row["runner"], native.runner);
    }
    for (os, arch) in [
        ("macos", "x86_64"),
        ("linux", "aarch64"),
        ("windows", "aarch64"),
    ] {
        assert!(platform::lookup(os, arch).is_err());
    }
    let output = Command::new("python")
        .arg(root.join("scripts/sync-release-platforms.py"))
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn checksum_requires_one_exact_filename_hash_before_extraction() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("archive");
    fs::write(&path, b"archive bytes").unwrap();
    let good = checksum(&path, "mine.zip");
    delivery::verify_checksum(&path, &good, "mine.zip").unwrap();
    for bad in [
        String::new(),
        good.replace("mine.zip", "other.zip"),
        format!("{good}{good}"),
        format!("{good}\n"),
        good.replacen(&good[..1], "z", 1),
        format!("{}  mine.zip\n", "0".repeat(64)),
    ] {
        assert!(
            delivery::verify_checksum(&path, &bad, "mine.zip").is_err(),
            "accepted {bad:?}"
        );
    }
    fs::write(&path, b"truncated").unwrap();
    assert!(delivery::verify_checksum(&path, &good, "mine.zip").is_err());
}

#[test]
fn zip_validates_every_path_and_singular_regular_binary_before_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("archive.zip");
    let output = tmp.path().join("candidate");
    let native = platform::lookup("windows", "x86_64").unwrap();
    for unsafe_name in [
        "../escape",
        "/absolute",
        "C:/drive",
        "\\\\host\\share",
        "dir/../../escape",
        "nul\0name",
        "control\nname",
    ] {
        zip(&archive, &[("mine.exe", b"valid"), (unsafe_name, b"bad")]);
        assert!(
            delivery::stage_archive(&archive, native, &output).is_err(),
            "accepted {unsafe_name:?}"
        );
        assert!(
            !output.exists(),
            "wrote candidate before inspecting final unsafe entry"
        );
    }
    for entries in [
        vec![("mine.exe", b"one".as_slice()), ("nested/mine.exe", b"two")],
        vec![("mine", b"wrong")],
        vec![("mine.exe", b"".as_slice())],
    ] {
        zip(&archive, &entries);
        assert!(delivery::stage_archive(&archive, native, &output).is_err());
        assert!(!output.exists());
    }
    let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
    writer
        .start_file("mine.exe", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"valid").unwrap();
    writer
        .add_symlink("link/", "outside", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.finish().unwrap();
    assert!(delivery::stage_archive(&archive, native, &output).is_err());
    assert!(!output.exists());
    zip(&archive, &[("bundle/mine.exe", b"native")]);
    delivery::stage_archive(&archive, native, &output).unwrap();
    assert_eq!(fs::read(&output).unwrap(), b"native");
    assert!(
        delivery::stage_archive(&archive, native, &output).is_err(),
        "overwrote existing output"
    );
}

#[test]
fn tar_refuses_traversal_links_ambiguity_and_truncation() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("archive.tar.gz");
    let output = tmp.path().join("candidate");
    let native = platform::lookup("linux", "x86_64").unwrap();
    for (name, kind) in [
        ("../escape", tar::EntryType::Regular),
        ("/absolute", tar::EntryType::Regular),
        ("C:/escape", tar::EntryType::Regular),
        ("link", tar::EntryType::Symlink),
        ("hard", tar::EntryType::Link),
        ("pipe", tar::EntryType::Fifo),
        ("nested/mine", tar::EntryType::Regular),
    ] {
        tar(
            &archive,
            &[
                ("mine", b"native", tar::EntryType::Regular),
                (name, b"", kind),
            ],
        );
        assert!(
            delivery::stage_archive(&archive, native, &output).is_err(),
            "accepted {name}"
        );
        assert!(!output.exists());
    }
    tar(
        &archive,
        &[("bundle/mine", b"native", tar::EntryType::Regular)],
    );
    let mut bytes = fs::read(&archive).unwrap();
    bytes.truncate(bytes.len() - 5);
    fs::write(&archive, bytes).unwrap();
    assert!(delivery::stage_archive(&archive, native, &output).is_err());
    assert!(!output.exists());
    tar(
        &archive,
        &[("bundle/mine", b"native", tar::EntryType::Regular)],
    );
    delivery::stage_archive(&archive, native, &output).unwrap();
    assert_eq!(fs::read(&output).unwrap(), b"native");
}

#[test]
fn http_diagnostics_and_pinned_identity_are_not_conflated() {
    assert!(delivery::http_failure(404, None, None, None).contains("no published release"));
    assert!(delivery::http_failure(403, None, None, None).contains("authorization"));
    assert!(delivery::http_failure(403, Some("0"), Some("123"), None).contains("rate limit"));
    assert!(delivery::http_failure(429, None, None, Some("12")).contains("retry-after=12"));
    assert!(
        !delivery::http_failure(429, None, Some("secret-token"), None).contains("secret-token")
    );
    for tag in [
        "",
        "../v1.2.3",
        "v1.2",
        "1.2.3/escape",
        "v1.2.3?token=secret",
        "v1.2.3\n",
    ] {
        assert!(delivery::validated_tag(tag).is_err());
    }
    delivery::validated_tag("v0.2.0").unwrap();
}

struct Bootstrap {
    root: tempfile::TempDir,
    native: platform::Platform,
}
impl Bootstrap {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let native = platform::current().unwrap();
        let candidate = fs::read(env!("CARGO_BIN_EXE_mine")).unwrap();
        let archive = root.path().join(native.archive);
        if cfg!(windows) {
            zip(&archive, &[(native.binary, &candidate)]);
        } else {
            tar(
                &archive,
                &[(native.binary, &candidate, tar::EntryType::Regular)],
            );
        }
        let this = Self { root, native };
        this.resum();
        fs::create_dir(this.root.path().join("bin")).unwrap();
        this
    }
    fn resum(&self) {
        let archive = self.root.path().join(self.native.archive);
        fs::write(
            self.root.path().join("checksum"),
            checksum(&archive, self.native.archive),
        )
        .unwrap();
    }
    fn destination(&self) -> std::path::PathBuf {
        self.root.path().join("bin").join(self.native.binary)
    }
    fn command(&self, reference: &str, status: &str, headers: &str) -> Command {
        let mut command = native_command(self.root.path());
        command
            .env("MINE_TEST_ROOT", self.root.path())
            .env("MINE_TEST_ASSET", self.native.archive)
            .env(
                "MINE_TEST_SCRIPT",
                Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
                    "scripts/bootstrap.ps1"
                } else {
                    "scripts/bootstrap.sh"
                }),
            )
            .env("MINE_TEST_STATUS", status)
            .env("MINE_TEST_HEADERS", headers)
            .env("MINE_TEST_TAG", format!("v{}", env!("CARGO_PKG_VERSION")))
            .env("MINE_REF", reference)
            .env("MINE_NO_PERSIST_PATH", "1")
            .env("MINE_BIN_DIR", self.root.path().join("bin"))
            .env("HOME", self.root.path())
            .env("USERPROFILE", self.root.path());
        command
    }
    fn run(&self, reference: &str, status: &str, headers: &str) -> Output {
        self.command(reference, status, headers).output().unwrap()
    }
}

#[cfg(windows)]
fn native_command(root: &Path) -> Command {
    // Function injection exists only in the parent test host, never production flags.
    let harness = r#"
$ErrorActionPreference='Stop'
function global:curl.exe {
    $a=@($args); $url=[string]$a[-1]
    $output=$a[[Array]::IndexOf($a,'--output')+1]
    $header=$a[[Array]::IndexOf($a,'--dump-header')+1]
    [IO.File]::WriteAllText($header,$env:MINE_TEST_HEADERS)
    if ($url.EndsWith('/latest')) {
        [IO.File]::WriteAllText($output,'')
        $effective="https://github.com/6ixGODD/mine/releases/tag/$env:MINE_TEST_TAG"
    } else {
        $source=if ($url.EndsWith('.sha256')) {'checksum'} else {$env:MINE_TEST_ASSET}
        [IO.File]::Copy((Join-Path $env:MINE_TEST_ROOT $source),$output,$true)
        $effective=$url
    }
    $global:LASTEXITCODE=0
    $env:MINE_TEST_STATUS
    $effective
}
try {
    if ($env:MINE_TEST_LOCK -eq '1') { $locked=[IO.File]::Open((Join-Path $env:MINE_BIN_DIR 'mine.exe'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read) }
    & $env:MINE_TEST_SCRIPT -SetupArgs @('--agents','codex','--yes','--config-root',(Join-Path $env:MINE_TEST_ROOT 'config'))
} finally { if ($locked) {$locked.Dispose()} }
"#;
    let path = root.join("transport-harness.ps1");
    fs::write(&path, harness).unwrap();
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(path);
    command
}

#[cfg(unix)]
fn native_command(root: &Path) -> Command {
    use std::os::unix::fs::PermissionsExt;
    let shim = root.join("transport");
    fs::create_dir_all(&shim).unwrap();
    let curl = shim.join("curl");
    fs::write(
        &curl,
        r#"#!/bin/sh
while [ "$#" -gt 0 ]; do
 case "$1" in --output) shift; output="$1" ;; --dump-header) shift; headers="$1" ;; esac
 url="$1"; shift
done
printf '%s' "$MINE_TEST_HEADERS" >"$headers"
case "$url" in
 */latest) : >"$output"; effective="https://github.com/6ixGODD/mine/releases/tag/$MINE_TEST_TAG" ;;
 *.sha256) cp "$MINE_TEST_ROOT/checksum" "$output"; effective="$url" ;;
 *) cp "$MINE_TEST_ROOT/$MINE_TEST_ASSET" "$output"; effective="$url" ;;
esac
printf '%s\n%s' "$MINE_TEST_STATUS" "$effective"
"#,
    )
    .unwrap();
    fs::set_permissions(curl, fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new("sh");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/bootstrap.sh"))
        .args(["--agents", "codex", "--yes", "--config-root"])
        .arg(root.join("config"));
    let mut paths = vec![shim];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    command.env("PATH", std::env::join_paths(paths).unwrap());
    command
}

fn combined(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn actual_native_bootstrap_pinned_and_latest_install_verified_candidate() {
    let fixture = Bootstrap::new();
    for reference in [format!("v{}", env!("CARGO_PKG_VERSION")), "latest".into()] {
        let output = fixture.run(&reference, "200", "");
        assert!(output.status.success(), "{}", combined(&output));
        assert_eq!(
            fs::read(fixture.destination()).unwrap(),
            fs::read(env!("CARGO_BIN_EXE_mine")).unwrap()
        );
        assert!(!fixture.root.path().join("bin/.mine-binary-swap").exists());
        let config = fixture.root.path().join("config");
        assert!(config.join(".agents/skills/mine-arch/SKILL.md").is_file());
        let codex_config = fs::read_to_string(config.join(".codex/config.toml")).unwrap();
        assert!(codex_config.contains("[mcp_servers.mine]"));
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(config.join(".mine/agent-installs.json")).unwrap())
                .unwrap();
        assert!(state["installs"].as_array().unwrap().iter().any(|record| {
            record["agent"] == "codex" && !record["files"].as_array().unwrap().is_empty()
        }));
    }
}

#[test]
fn actual_native_bootstrap_reports_setup_failure_without_undoing_binary_install() {
    let fixture = Bootstrap::new();
    let config = fixture.root.path().join("config/.codex/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let foreign = b"# user-owned configuration\n[mcp_servers.mine]\ncommand = 'foreign-server'\n";
    fs::write(&config, foreign).unwrap();
    let output = fixture.run(&format!("v{}", env!("CARGO_PKG_VERSION")), "200", "");
    assert!(
        !output.status.success(),
        "bootstrap reported complete success after Agent setup failed: {}",
        combined(&output)
    );
    assert!(combined(&output).contains("setup"), "{}", combined(&output));
    assert_eq!(fs::read(config).unwrap(), foreign);
    assert_eq!(
        fs::read(fixture.destination()).unwrap(),
        fs::read(env!("CARGO_BIN_EXE_mine")).unwrap()
    );
    assert!(!fixture.root.path().join("bin/.mine-binary-swap").exists());
}

#[test]
fn setup_partial_failure_preserves_results_and_returns_existing_partial_exit() {
    let fixture = Bootstrap::new();
    let root = fixture.root.path().join("partial-config");
    let config = root.join(".codex/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let foreign = b"[mcp_servers.mine]\ncommand = 'foreign-server'\n";
    fs::write(&config, foreign).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mine"))
        .args([
            "--format",
            "json",
            "setup",
            "--agents",
            "pi,codex",
            "--yes",
            "--config-root",
        ])
        .arg(&root)
        .env("MINE_REF", format!("v{}", env!("CARGO_PKG_VERSION")))
        .env("HOME", fixture.root.path())
        .env("USERPROFILE", fixture.root.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7), "{}", combined(&output));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let envelope = stdout
        .lines()
        .chain(stderr.lines())
        .find(|line| line.starts_with('{'))
        .expect("structured failure envelope");
    let value: serde_json::Value = serde_json::from_str(envelope).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "MINE_EXTERNAL_DEPENDENCY");
    assert_eq!(
        value["error"]["details"]["installed"],
        serde_json::json!(["pi"])
    );
    assert_eq!(
        value["error"]["details"]["errors"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(fs::read(config).unwrap(), foreign);
}

#[test]
fn actual_native_bootstrap_refuses_bad_sum_version_http_and_keeps_old_install() {
    let fixture = Bootstrap::new();
    fs::write(fixture.destination(), b"prior owned installation").unwrap();
    let reference = format!("v{}", env!("CARGO_PKG_VERSION"));
    for (status, headers, diagnostic) in [
        ("404", "", "no published release"),
        ("403", "", "authorization"),
        (
            "403",
            "x-ratelimit-remaining: 0\r\nx-ratelimit-reset: 123\r\n",
            "rate limit",
        ),
        ("429", "retry-after: 12\r\n", "rate limit"),
    ] {
        let output = fixture.run(&reference, status, headers);
        assert!(!output.status.success());
        assert!(
            combined(&output).contains(diagnostic),
            "{}",
            combined(&output)
        );
        assert_eq!(
            fs::read(fixture.destination()).unwrap(),
            b"prior owned installation"
        );
    }
    fs::write(fixture.root.path().join("checksum"), "missing").unwrap();
    let output = fixture.run(&reference, "200", "");
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("checksum"),
        "{}",
        combined(&output)
    );
    fixture.resum();
    let output = fixture.run("v0.0.0", "200", "");
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("version"),
        "{}",
        combined(&output)
    );
    assert_eq!(
        fs::read(fixture.destination()).unwrap(),
        b"prior owned installation"
    );
    assert!(!fixture.root.path().join("bin/.mine-binary-swap").exists());
}

#[test]
fn actual_native_bootstrap_refuses_late_unsafe_entry_and_interrupted_swap() {
    let fixture = Bootstrap::new();
    let archive = fixture.root.path().join(fixture.native.archive);
    let candidate = fs::read(env!("CARGO_BIN_EXE_mine")).unwrap();
    if cfg!(windows) {
        zip(
            &archive,
            &[(fixture.native.binary, &candidate), ("../escape", b"bad")],
        );
    } else {
        tar(
            &archive,
            &[
                (fixture.native.binary, &candidate, tar::EntryType::Regular),
                ("../escape", b"bad", tar::EntryType::Regular),
            ],
        );
    }
    fixture.resum();
    let reference = format!("v{}", env!("CARGO_PKG_VERSION"));
    let output = fixture.run(&reference, "200", "");
    assert!(!output.status.success());
    assert!(combined(&output).contains("path"), "{}", combined(&output));
    assert!(!fixture.destination().exists());
    assert!(!fixture.root.path().join("escape").exists());
    let stage = fixture.root.path().join("bin/.mine-binary-swap");
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("previous"), b"recover me").unwrap();
    let output = fixture.run(&reference, "200", "");
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("pending/busy"),
        "{}",
        combined(&output)
    );
    assert_eq!(fs::read(stage.join("previous")).unwrap(), b"recover me");
}

#[test]
fn actual_native_bootstrap_rejects_hash_mismatch_truncation_links_and_ambiguity() {
    let fixture = Bootstrap::new();
    fs::write(fixture.destination(), b"old safe installation").unwrap();
    let reference = format!("v{}", env!("CARGO_PKG_VERSION"));
    fs::write(
        fixture.root.path().join("checksum"),
        format!("{}  {}\n", "0".repeat(64), fixture.native.archive),
    )
    .unwrap();
    let output = fixture.run(&reference, "200", "");
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("SHA-256 mismatch"),
        "{}",
        combined(&output)
    );
    let archive = fixture.root.path().join(fixture.native.archive);
    for case in ["truncated", "ambiguous", "link", "wrong-platform"] {
        match case {
            "truncated" => fs::write(&archive, b"not an archive").unwrap(),
            "ambiguous" if cfg!(windows) => zip(
                &archive,
                &[(fixture.native.binary, b"one"), ("nested/mine.exe", b"two")],
            ),
            "ambiguous" => tar(
                &archive,
                &[
                    ("mine", b"one", tar::EntryType::Regular),
                    ("nested/mine", b"two", tar::EntryType::Regular),
                ],
            ),
            "link" if cfg!(windows) => {
                let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
                writer
                    .add_symlink(
                        "mine.exe",
                        "outside",
                        zip::write::SimpleFileOptions::default(),
                    )
                    .unwrap();
                writer.finish().unwrap();
            }
            "link" => tar(&archive, &[("mine", b"", tar::EntryType::Symlink)]),
            _ if cfg!(windows) => zip(&archive, &[("mine", b"wrong platform")]),
            _ => tar(
                &archive,
                &[("mine.exe", b"wrong platform", tar::EntryType::Regular)],
            ),
        }
        fixture.resum();
        let output = fixture.run(&reference, "200", "");
        assert!(!output.status.success(), "{case}: {}", combined(&output));
        assert_eq!(
            fs::read(fixture.destination()).unwrap(),
            b"old safe installation"
        );
        assert!(!fixture.root.path().join("bin/.mine-binary-swap").exists());
    }
}

#[cfg(windows)]
#[test]
fn actual_windows_bootstrap_locked_old_executable_is_truthfully_pending() {
    let fixture = Bootstrap::new();
    fs::write(fixture.destination(), b"locked old install").unwrap();
    let output = fixture
        .command(&format!("v{}", env!("CARGO_PKG_VERSION")), "200", "")
        .env("MINE_TEST_LOCK", "1")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{}", combined(&output));
    assert_eq!(
        fs::read(fixture.destination()).unwrap(),
        b"locked old install"
    );
    let stage = fixture.root.path().join("bin/.mine-binary-swap");
    assert!(stage.join("record.json").exists());
    assert!(stage.join("candidate.exe").exists());
}
