#!/usr/bin/env python3
"""Native, offline release-delivery harness (stdlib only).

Default: test actual bootstrap with CARGO_TARGET_DIR/debug/mine[.exe].
--binary selects a built candidate; --archive consumes its exact staged archive.
--output owns all logs, isolated HOME/bin/config and fixtures (never user HOME).
--package BINARY packages the native manifest row with README/LICENSE/checksum.
--verify-assets DIR refuses an incomplete/corrupt publication matrix.
--gate-needs JSON checks the real workflow's mandatory dependencies and tag ref.
--contracts-only runs harness counterexamples, not native-platform evidence.

Only the bootstrap transport is injected, in a child-local curl function/PATH.
No production installer changes, network, global installation or publication.
--preflight BINARY drives the accepted live/operation-selected release contract
only in private fixtures; --fixture-build is Cargo's actual JSON build output.
--product-source/--source-key bind a local immutable accepted-source snapshot;
CI defaults to its clean checked-out product commit, never the real dev graph.
"""
import argparse
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MAX_LOG = 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def candidate_identity(binary, source, key=None):
    source = source.resolve(strict=True)
    require(digest(source / "distribution/platforms.json") == digest(ROOT / "distribution/platforms.json"),
            "harness platform policy differs from selected product")
    if key:
        identity = json.loads(key.read_text(encoding="utf-8"))
        require(identity["binary_sha256"] == digest(binary), "candidate artifact identity changed")
        expected = set()
        for entry in identity["files"]:
            path = source / entry["path"]
            require(path.resolve().is_relative_to(source) and path.is_file() and not path.is_symlink(),
                    "snapshot input missing or escaped")
            require(entry["path"] not in expected and digest(path) == entry["sha256"],
                    "snapshot source bytes changed or duplicated")
            expected.add(entry["path"])
        require(expected == {p.relative_to(source).as_posix() for p in source.rglob("*") if p.is_file()},
                "snapshot has undeclared inputs")
        identity = {k: v for k, v in identity.items() if k != "files"}
        identity["source_key_sha256"] = digest(key)
    else:
        top = run(["git", "rev-parse", "--show-toplevel"], source)
        require(top.returncode == 0 and os.path.samefile(top.stdout.decode().strip(), source),
                "product source is not the actual repository Root")
        status = run(["git", "status", "--porcelain=v1", "--untracked-files=all"], source)
        require(status.returncode == 0 and status.stdout == b"",
                "product checkout must be clean before candidate verification")
        commit = run(["git", "rev-parse", "HEAD"], source)
        tree = run(["git", "rev-parse", "HEAD^{tree}"], source)
        require(commit.returncode == tree.returncode == 0, "cannot observe product commit/tree")
        identity = {"source_commit": commit.stdout.decode().strip(), "source_tree": tree.stdout.decode().strip(),
                    "binary_sha256": digest(binary),
                    "artifact_source": "runner supplied; invoking job retains build provenance"}
    require(re.fullmatch(r"[0-9a-f]{40}", identity["source_commit"]) is not None,
            "invalid observed source commit")
    return identity


def platforms():
    manifest = json.loads((ROOT / "distribution/platforms.json").read_text())
    require(manifest["schema_version"] == 1, "unknown platform manifest")
    rows = manifest["platforms"]
    require(bool(rows), "empty platform matrix")
    require(len({(r["os"], r["arch"]) for r in rows}) == len(rows), "duplicate platform")
    for row in rows:
        require(all(re.fullmatch(r"[A-Za-z0-9_.-]+", v) for v in row.values()), "unsafe platform identity")
    return rows


def native_platform():
    kernel, machine = platform.system(), platform.machine()
    if kernel == "Windows":
        # Python 3.11 platform.machine() reads processor environment variables,
        # which may be absent (or describe a WOW64 process, not the native OS).
        # Ask the same actual OS API as bootstrap.ps1; never assume Python's
        # pointer width or accept a user-supplied architecture as native proof.
        result = run(["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
                      "[System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()"], ROOT, timeout=15)
        require(result.returncode == 0 and not result.stderr, "cannot observe native Windows architecture")
        machine = {"X64": "AMD64", "Arm64": "ARM64", "X86": "x86"}.get(result.stdout.decode().strip(), "unknown")
    matches = [r for r in platforms() if r["kernel"] == kernel and r["machine"].lower() == machine.lower()]
    require(len(matches) == 1, f"unsupported native platform: {kernel}/{machine}")
    return matches[0]


def job_blocks(text):
    # Deliberately inspect the checked-in, two-space job mapping, not arbitrary
    # YAML. actionlint validates full YAML/Actions syntax separately in CI.
    matches = list(re.finditer(r"^  ([a-z][a-z0-9-]*):\s*$", text, re.M))
    return {m[1]: text[m.end():matches[i + 1].start() if i + 1 < len(matches) else len(text)]
            for i, m in enumerate(matches)}


def required_jobs(text=None):
    text = text if text is not None else (ROOT / ".github/workflows/release.yml").read_text()
    release = job_blocks(text)["release"]
    match = re.search(r"^    needs: \[([^\n]+)\]$", release, re.M)
    require(match is not None, "publication requires an explicit inline needs list")
    names = [s.strip() for s in match[1].split(",")]
    require(len(names) == len(set(names)), "duplicate publication dependency")
    require(set(names) == {"quality", "preflight", "mcp-discovery", "payload", "native-delivery"},
            "mandatory publication dependency missing or altered")
    return names


def gate_needs(facts, reference, text=None):
    require(re.fullmatch(r"refs/tags/v[0-9]+\.[0-9]+\.[0-9]+", reference) is not None,
            "publication requires an exact version tag")
    names = required_jobs(text)
    require(isinstance(facts, dict) and set(facts) == set(names), "incomplete job evidence")
    for name in names:
        require(isinstance(facts[name], dict) and facts[name].get("result") == "success",
                f"mandatory job did not succeed: {name}")


def verify_assets(directory, rows=None):
    directory = Path(directory)
    rows = rows if rows is not None else platforms()
    expected = {r["archive"] for r in rows} | {r["archive"] + ".sha256" for r in rows}
    actual = {p.name for p in directory.iterdir()}
    require(actual in (expected, expected | {"SHA256SUMS.txt"}), "publication asset matrix is incomplete or contains unexpected files")
    lines = []
    for row in rows:
        name = row["archive"]
        archive = directory / name
        sidecar = directory / (name + ".sha256")
        require(archive.is_file() and not archive.is_symlink() and sidecar.is_file()
                and not sidecar.is_symlink(), "asset must be a regular file")
        require(0 < archive.stat().st_size <= 128 * 1024 * 1024, "archive byte limit")
        require(sidecar.stat().st_size <= 4096, "checksum byte limit")
        text = sidecar.read_text(encoding="ascii")
        require(re.fullmatch(r"[0-9a-f]{64}  " + re.escape(name) + r"\n", text) is not None,
                "checksum must name its exact archive once")
        require(text[:64] == digest(archive), "publication archive SHA-256 mismatch")
        lines.append(text)
    combined = "".join(lines)
    previous = directory / "SHA256SUMS.txt"
    if previous.exists():
        require(previous.is_file() and not previous.is_symlink() and previous.stat().st_size <= 16384
                and previous.read_text(encoding="ascii") == combined, "existing combined checksum differs from verified matrix")
    return combined


def write_archive(path, row, entries):
    """Fixture entries: (name, bytes, regular|link); never extracts them."""
    if row["archive"].endswith(".zip"):
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
            for name, content, kind in entries:
                info = zipfile.ZipInfo(name)
                info.create_system = 3
                info.external_attr = ((stat.S_IFLNK if kind == "link" else stat.S_IFREG) | 0o755) << 16
                archive.writestr(info, content, compress_type=zipfile.ZIP_DEFLATED, compresslevel=1)
    else:
        with tarfile.open(path, "w:gz", compresslevel=1) as archive:
            for name, content, kind in entries:
                info = tarfile.TarInfo(name)
                info.mode = 0o755
                if kind == "link":
                    info.type = tarfile.SYMTYPE
                    info.linkname = "../../outside"
                    content = b""
                info.size = len(content)
                archive.addfile(info, io.BytesIO(content))


def run(argv, cwd, env=None, stdin=None, timeout=120):
    result = subprocess.run([str(a) for a in argv], cwd=cwd, env=env, input=stdin,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
    require(len(result.stdout) <= MAX_LOG and len(result.stderr) <= MAX_LOG, "child output exceeded evidence limit")
    return result


def binary_version(binary):
    result = run([binary, "--version"], ROOT, timeout=15)
    version = result.stdout.decode("utf-8").strip()
    require(result.returncode == 0 and not result.stderr
            and re.fullmatch(r"mine [0-9]+\.[0-9]+\.[0-9]+", version), "invalid native candidate version")
    return version.removeprefix("mine ")


def candidate_tag(version, reference):
    if reference.startswith("refs/tags/"):
        require(reference == "refs/tags/v" + version, "candidate version differs from publication tag")
    return "v" + version


def package(binary, output, row, product_source=ROOT):
    binary = Path(binary).resolve(strict=True)
    candidate_tag(binary_version(binary), os.environ.get("GITHUB_REF", ""))
    output.mkdir(parents=True, exist_ok=True)
    archive = output / row["archive"]
    write_archive(archive, row, [(row["binary"], binary.read_bytes(), "regular"),
                               ("README.md", (product_source / "README.md").read_bytes(), "regular"),
                               ("LICENSE", (product_source / "LICENSE").read_bytes(), "regular")])
    (output / (row["archive"] + ".sha256")).write_text(
        f"{digest(archive)}  {row['archive']}\n", encoding="ascii", newline="\n")
    return archive


def transport(argv):
    """Test-only curl replacement: no sockets, strict pinned URLs and bounds."""
    fixture = Path(os.environ["MINE_HARNESS_FIXTURE"])
    spec = json.loads((fixture / "transport.json").read_text())
    for flag, value in [("--proto", "=https"), ("--proto-redir", "=https"),
                        ("--retry", "0"), ("--max-time", "90"), ("--connect-timeout", "10"),
                        ("--max-redirs", "5")]:
        require(flag in argv and argv[argv.index(flag) + 1] == value, "bootstrap weakened transport bound")
    url = argv[-1]
    base = "https://github.com/6ixGODD/mine/releases/"
    latest = url == base + "latest"
    checksum = url.endswith(".sha256")
    require("--max-filesize" in argv and argv[argv.index("--max-filesize") + 1]
            == str(1048576 if latest else 4096 if checksum else 134217728), "incorrect response bound")
    expected = base + f"download/{spec['tag']}/{spec['asset']}" + (".sha256" if checksum else "")
    require(latest or url == expected, "unpinned or foreign asset request")
    require(("--head" in argv) == latest, "incorrect latest resolution method")
    output = Path(argv[argv.index("--output") + 1]).resolve()
    headers = Path(argv[argv.index("--dump-header") + 1]).resolve()
    swap = (fixture / "bin/.mine-binary-swap").resolve()
    require(output.is_relative_to(swap) and headers.is_relative_to(swap), "transport output escaped swap")
    with (fixture / "requests.jsonl").open("a", encoding="utf-8") as record:
        record.write(json.dumps({"url": url, "argv": argv}) + "\n")
    if spec.get("network_exit"):
        return spec["network_exit"]
    status = spec.get("sum_status", spec.get("status", 200)) if checksum else spec.get("status", 200)
    headers.write_text(spec.get("checksum_headers", spec.get("headers", ""))
                       if checksum else spec.get("headers", ""), encoding="ascii")
    if latest or status != 200:
        output.write_bytes(b"")
    else:
        shutil.copyfile(fixture / ("checksum" if checksum else spec["asset"]), output)
    print(status)
    print(spec.get("effective", base + f"tag/{spec['tag']}") if latest else url)
    return 0


PS_HOST = r"""$ErrorActionPreference='Stop'
function global:curl.exe {
    & $env:MINE_HARNESS_PYTHON $env:MINE_HARNESS_SCRIPT --transport @args
    $global:LASTEXITCODE=$LASTEXITCODE
}
try {
    if ($env:MINE_HARNESS_LOCK -eq '1') {
        $fixtureLock=[IO.File]::Open((Join-Path $env:MINE_BIN_DIR 'mine.exe'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    }
    & $env:MINE_HARNESS_BOOTSTRAP -SetupArgs @('--agents','codex','--yes','--config-root',$env:MINE_HARNESS_CONFIG)
} finally { if ($fixtureLock) {$fixtureLock.Dispose()} }
"""


def isolated_env(fixture):
    env = os.environ.copy()
    for key in ["MINE_REPO", "MINE_RELEASE_ACCOUNT", "MINE_RELEASE_REPO", "MINE_REF",
                "MINE_BIN_DIR", "MINE_NO_PERSIST_PATH"]:
        env.pop(key, None)
    env.update(HOME=str(fixture), USERPROFILE=str(fixture), XDG_CONFIG_HOME=str(fixture / "xdg"),
               APPDATA=str(fixture / "appdata"), LOCALAPPDATA=str(fixture / "localappdata"),
               TMP=str(fixture / "tmp"), TEMP=str(fixture / "tmp"), TMPDIR=str(fixture / "tmp"))
    return env


def bootstrap_case(fixture, row, reference, spec, lock=False, unsupported=False, product_source=ROOT):
    (fixture / "transport.json").write_text(json.dumps(spec), encoding="utf-8")
    config = fixture / "config"
    env = isolated_env(fixture)
    env.update(MINE_BIN_DIR=str(fixture / "bin"), MINE_NO_PERSIST_PATH="1", MINE_REF=reference,
               MINE_HARNESS_PYTHON=sys.executable, MINE_HARNESS_SCRIPT=str(Path(__file__).resolve()),
               MINE_HARNESS_FIXTURE=str(fixture), MINE_HARNESS_CONFIG=str(config),
               MINE_HARNESS_LOCK="1" if lock else "0")
    (fixture / "tmp").mkdir()
    shim = fixture / "transport"
    shim.mkdir()
    if row["os"] == "windows":
        host = fixture / "transport.ps1"
        host.write_text(PS_HOST, encoding="utf-8")
        env["MINE_HARNESS_BOOTSTRAP"] = str(product_source / "scripts/bootstrap.ps1")
        argv = ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", host]
    else:
        curl = shim / "curl"
        curl.write_text('#!/bin/sh\nexec "$MINE_HARNESS_PYTHON" "$MINE_HARNESS_SCRIPT" --transport "$@"\n', encoding="utf-8", newline="\n")
        curl.chmod(0o755)
        if unsupported:
            uname = shim / "uname"
            uname.write_text('#!/bin/sh\ncase "$1" in -s) echo Darwin ;; -m) echo x86_64 ;; *) exit 1 ;; esac\n', encoding="ascii", newline="\n")
            uname.chmod(0o755)
        env["PATH"] = str(shim) + os.pathsep + env["PATH"]
        argv = ["sh", product_source / "scripts/bootstrap.sh", "--agents", "codex", "--yes", "--config-root", config]
    return run(argv, fixture, env)


def native_smoke(binary, archive, output, row, product_source=ROOT, identity=None):
    version = binary_version(binary)
    candidate_tag(version, os.environ.get("GITHUB_REF", ""))
    output.mkdir(parents=True, exist_ok=True)
    archive = archive if archive else package(binary, output / "assets", row, product_source)
    require(archive.name == row["archive"], "wrong native archive name")
    sidecar = archive.with_name(archive.name + ".sha256")
    verify_assets(archive.parent, [row])
    binary_hash = digest(binary)
    cases = ["pinned", "latest", "http404", "http401", "http403", "http403-limit", "http429", "http500",
             "network", "missing-checksum", "wrong-checksum-name", "corrupt-checksum", "hash-mismatch",
             "truncated", "traversal", "link", "duplicate", "wrong-platform", "wrong-version",
             "malformed-latest", "interrupted", "agent-failure", "refresh-success", "refresh-failure"]
    cases += ["locked-old"] if row["os"] == "windows" else ["unsupported"]
    if row["os"] != "windows":
        cases += ["large-redirect-headers", "oversized-checksum", "oversized-metadata", "header-file-limit"]
    results = []
    for case in cases:
        fixture = Path(tempfile.mkdtemp(prefix=case + "-", dir=output))
        bin_dir = fixture / "bin"
        bin_dir.mkdir()
        destination = bin_dir / row["binary"]
        old = binary.read_bytes()  # already version-verified, genuinely usable old executable
        happy = case in ("pinned", "latest", "refresh-success", "refresh-failure", "large-redirect-headers")
        if not happy and case != "agent-failure":
            destination.write_bytes(old)
            if row["os"] != "windows":
                destination.chmod(0o755)
        local_archive = fixture / row["archive"]
        shutil.copyfile(archive, local_archive)
        shutil.copyfile(sidecar, fixture / "checksum")
        spec = {"tag": "v" + version, "asset": row["archive"]}
        reference = "latest" if case in ("latest", "malformed-latest") else spec["tag"]
        diagnostic = ""
        if case.startswith("http"):
            spec["status"] = int(case[4:7])
            diagnostic = "no published release" if case == "http404" else "release request failed"
            if case in ("http401", "http403"):
                diagnostic = "authorization"
            if case == "http403-limit":
                spec["headers"] = "x-ratelimit-remaining: 0\r\nx-ratelimit-reset: 123\r\nauthorization: super-secret-fixture-token\r\n"
                diagnostic = "rate limit"
            if case == "http429":
                spec["headers"] = "retry-after: 12\r\n"
                diagnostic = "rate limit"
        if case == "network":
            spec["network_exit"] = 28
            diagnostic = "network/timeout"
        if case == "missing-checksum":
            spec["sum_status"] = 404
            diagnostic = "no published release"
        if case == "large-redirect-headers":
            # The checksum body is tiny; curl also writes every redirect header.
            spec["checksum_headers"] = "x-redirect-chain: " + "a" * 8192 + "\r\n"
        if case == "oversized-checksum":
            # The transport deliberately ignores max-filesize, like older curl
            # on unknown-length bodies: the final 4096-byte check must refuse it.
            (fixture / "checksum").write_bytes(b"a" * 4097)
            diagnostic = "response exceeded byte limit"
        if case in ("oversized-metadata", "header-file-limit"):
            spec["checksum_headers"] = "a" * (1048577 if case == "oversized-metadata" else 2097152)
            diagnostic = "response metadata exceeded byte limit" if case == "oversized-metadata" else "network/timeout/oversized response"
        if case == "wrong-checksum-name":
            (fixture / "checksum").write_text(f"{digest(local_archive)}  other.zip\n")
            diagnostic = "checksum"
        if case == "corrupt-checksum":
            (fixture / "checksum").write_text("corrupt checksum\n")
            diagnostic = "checksum"
        if case == "hash-mismatch":
            (fixture / "checksum").write_text(f"{'0' * 64}  {row['archive']}\n")
            diagnostic = "SHA-256 mismatch"
        if case in ("traversal", "link", "duplicate", "wrong-platform", "truncated"):
            diagnostic = {"traversal": "archive path", "link": "archive link",
                          "duplicate": "duplicate" if row["os"] == "windows" else "ambiguous",
                          "wrong-platform": "native binary",
                          "truncated": "Central Directory" if row["os"] == "windows" else "malformed"}[case]
            if case == "truncated":
                archive_bytes = local_archive.read_bytes()
                local_archive.write_bytes(archive_bytes[:len(archive_bytes) // 2])
            else:
                entries = [(row["binary"], b"never execute invalid archive", "regular")]
                if case == "wrong-platform":
                    entries = [("mine" if row["binary"] == "mine.exe" else "mine.exe", b"wrong", "regular")]
                else:
                    name = "../escape" if case == "traversal" else row["binary"]
                    if case == "link":
                        name = "escaping-link"
                    if case == "duplicate" and row["os"] == "windows":
                        name = name.upper()  # duplicate under native case-insensitive normalization
                    entries += [(name, b"bad", "link" if case == "link" else "regular")]
                write_archive(local_archive, row, entries)
            (fixture / "checksum").write_text(f"{digest(local_archive)}  {row['archive']}\n")
        if case == "wrong-version":
            spec["tag"] = reference = "v0.0.0" if version != "0.0.0" else "v0.0.1"
            diagnostic = "version"
        if case == "malformed-latest":
            spec["effective"] = "https://untrusted.invalid/releases/tag/v1.2.3"
            diagnostic = "malformed latest"
        if case == "interrupted":
            stage = bin_dir / ".mine-binary-swap"
            stage.mkdir()
            (stage / "previous").write_bytes(old)
            (stage / "record.json").write_text('{"fixture":"interrupted"}')
            diagnostic = "pending/busy"
        foreign = b"# user-owned config\n[mcp_servers.mine]\ncommand = 'foreign-server'\n"
        config_file = fixture / "config/.codex/config.toml"
        if case == "agent-failure":
            config_file.parent.mkdir(parents=True)
            config_file.write_bytes(foreign)
            diagnostic = "setup"
        if case == "unsupported":
            diagnostic = "unsupported platform"
        refresh_exit = None
        try:
            result = bootstrap_case(fixture, row, reference, spec, lock=case == "locked-old", unsupported=case == "unsupported", product_source=product_source)
            (fixture / "stdout.log").write_bytes(result.stdout)
            (fixture / "stderr.log").write_bytes(result.stderr)
            observed = (result.stdout + result.stderr).decode("utf-8", errors="replace")
            require((result.returncode == 0) == happy, f"{case}: unexpected bootstrap exit {result.returncode}: {observed[-4000:]}")
            require(not diagnostic or diagnostic in observed, f"{case}: missing diagnostic {diagnostic}: {observed[-4000:]}")
            require("super-secret-fixture-token" not in observed, "unsafe HTTP header leaked")
            stage = bin_dir / ".mine-binary-swap"
            if happy or case == "agent-failure":
                require(digest(destination) == binary_hash, "installed binary differs from exact candidate")
                require(not stage.exists(), "completed binary swap retained staging")
                if happy:
                    require((fixture / "config/.agents/skills/mine-arch/SKILL.md").is_file(), "Agent payload not installed")
                    require("[mcp_servers.mine]" in config_file.read_text(), "MCP config not installed")
                    state = json.loads((fixture / "config/.mine/agent-installs.json").read_text())
                    require(any(r["agent"] == "codex" and r["files"] for r in state["installs"]), "Agent ownership evidence absent")
                    if case.startswith("refresh-"):
                        if case == "refresh-failure":
                            config_file.write_bytes(foreign)
                        refresh = run([destination, "--format", "json", "__refresh-skills", "--config-root", fixture / "config"], fixture, isolated_env(fixture))
                        (fixture / "refresh.stdout.json").write_bytes(refresh.stdout)
                        (fixture / "refresh.stderr.log").write_bytes(refresh.stderr)
                        report = json.loads(refresh.stdout)
                        refresh_exit = refresh.returncode
                        if case == "refresh-success":
                            require(refresh.returncode == 0 and not report["errors"] and "codex" in report["refreshed"], "verified embedded Agent refresh failed")
                        else:
                            require(refresh.returncode != 0 and report["errors"] and "codex" not in report["refreshed"], "Agent refresh failure was reported as success")
                            require(config_file.read_bytes() == foreign, "refresh overwrote foreign configuration")
                        require(digest(destination) == binary_hash, "refresh failure erased binary installation truth")
                else:
                    require(config_file.read_bytes() == foreign, "foreign config modified after setup failure")
            else:
                require(destination.read_bytes() == old, "refused delivery damaged old executable")
                if case == "interrupted":
                    require((stage / "previous").read_bytes() == old and (stage / "record.json").is_file(), "recovery evidence lost")
                elif case == "locked-old":
                    require((stage / "record.json").is_file() and (stage / "candidate.exe").is_file(), "locked swap was not recoverable")
                else:
                    require(not stage.exists(), "failed validation left a new swap")
            requests = [json.loads(line) for line in (fixture / "requests.jsonl").read_text().splitlines()] if (fixture / "requests.jsonl").exists() else []
            if happy:
                require(len(requests) == (3 if reference == "latest" else 2), "unexpected latest/retry requests")
                require(all(f"/download/v{version}/" in r["url"] for r in requests if not r["url"].endswith("/latest")), "moving latest assets")
            expected_requests = (0 if case in ("interrupted", "unsupported") else 1
                                 if case.startswith("http") or case in ("network", "malformed-latest")
                                 else 3 if reference == "latest" else 2)
            require(len(requests) == expected_requests, "refusal did not reach its intended transport/validation phase")
            require(len({r["url"] for r in requests}) == len(requests), "unbounded/repeated transport retry")
            if case in ("interrupted", "unsupported"):
                require(not requests, "early refusal attempted transport")
            require(not (fixture / "escape").exists(), "archive traversal escaped staging")
            results.append({"case": case, "outcome": "PASS", "bootstrap_exit": result.returncode,
                            "argv": result.args, "fixture": str(fixture),
                            "refresh_exit": refresh_exit,
                            "stdout_sha256": digest(fixture / "stdout.log"),
                            "stderr_sha256": digest(fixture / "stderr.log")})
            print(f"PASS {case} (bootstrap exit {result.returncode})")
        except (ValueError, OSError, subprocess.TimeoutExpired) as error:
            results.append({"case": case, "outcome": "FAIL", "error": str(error), "fixture": str(fixture)})
            print(f"FAIL {case}: {error}", file=sys.stderr)
    evidence = {"kind": "actual-native-bootstrap", "platform": row, "version": version,
                "binary": str(binary), "binary_sha256": binary_hash, "archive_sha256": digest(archive),
                "harness_sha256": digest(__file__), "bootstrap_sha256": digest(product_source / "scripts" / ("bootstrap.ps1" if row["os"] == "windows" else "bootstrap.sh")),
                "source_commit": identity["source_commit"], "product_identity": identity,
                "harness_commit": run(["git", "rev-parse", "HEAD"], ROOT).stdout.decode().strip(),
                "runner": os.environ.get("RUNNER_NAME", "local"), "python": sys.version,
                "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "results": results}
    (output / "native-evidence.json").write_text(json.dumps(evidence, indent=2), encoding="utf-8")
    require(all(r["outcome"] == "PASS" for r in results), "native bootstrap gate failed; see native-evidence.json")


def mcp_discovery(binary, output):
    output.mkdir(parents=True, exist_ok=True)
    private = output / "protocol-repo"
    private.mkdir()
    env = isolated_env(output / "home")
    (output / "home/tmp").mkdir(parents=True)
    env["GIT_CEILING_DIRECTORIES"] = str(output.resolve())
    require(run(["git", "init", "--quiet"], private, env).returncode == 0, "cannot create private MCP context")
    frames = [dict(jsonrpc="2.0", id=1, method="initialize", params={"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "delivery-gate", "version": "1"}}),
              dict(jsonrpc="2.0", method="notifications/initialized"), dict(jsonrpc="2.0", id=2, method="tools/list", params={})]
    result = run([binary, "--repo", private, "mcp", "serve"], private, env,
                 stdin=("\n".join(json.dumps(f) for f in frames) + "\n").encode(), timeout=30)
    (output / "stdout.jsonl").write_bytes(result.stdout)
    (output / "stderr.log").write_bytes(result.stderr)
    require(result.returncode == 0, "actual MCP discovery process failed")
    replies = [json.loads(line) for line in result.stdout.splitlines()]
    init = next(r for r in replies if r.get("id") == 1)
    listed = next(r for r in replies if r.get("id") == 2)
    require("error" not in init and "error" not in listed and init["result"]["capabilities"].get("tools") is not None, "MCP handshake/tool discovery refused")
    tools = listed["result"]["tools"]
    require(tools and len({t["name"] for t in tools}) == len(tools), "empty/duplicate MCP schema")
    for tool in tools:
        require(tool["inputSchema"].get("type") == "object", "MCP input schema absent")
    (output / "tools.json").write_text(json.dumps(tools, indent=2), encoding="utf-8")
    print(f"actual MCP initialization/discovery: {len(tools)} tools (not hardcoded)")
    return len(tools)


def phase_preflight(binary, fixture_build, output, identity):
    """Real executable entry points; no source graph or production journal writes."""
    records = [json.loads(line) for line in fixture_build.read_text(encoding="utf-8").splitlines()]
    helpers = {r["executable"] for r in records if r.get("reason") == "compiler-artifact"
               and r.get("target", {}).get("name") == "release_delivery_contract"
               and r.get("profile", {}).get("test") and r.get("executable")}
    require(len(helpers) == 1, "select one actual compiled fixture-helper executable")
    helper = Path(helpers.pop()).resolve(strict=True)
    output.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="phase-", dir=output))
    home = output / "isolated-home"
    (home / "tmp").mkdir(parents=True, exist_ok=True)
    env = isolated_env(home)
    env["GIT_CEILING_DIRECTORIES"] = str(output.resolve())
    transcript = []

    def invoke(argv, expected=0, extra_env=None):
        result = run(argv, root, {**env, **(extra_env or {})})
        entry = {"argv": [str(a) for a in argv], "exit": result.returncode,
                 "stdout": result.stdout.decode("utf-8", errors="replace"),
                 "stderr": result.stderr.decode("utf-8", errors="replace")}
        transcript.append(entry)
        (output / "phase-commands.json").write_text(json.dumps(transcript, indent=2), encoding="utf-8")
        require(result.returncode == expected, f"phase command exit {result.returncode}, expected {expected}: {entry}")
        return result.stdout

    def git(*args, **kwargs):
        return invoke(["git", *args], **kwargs).decode().strip()

    def cli(*args, expected=0):
        result = invoke([binary, "--repo", root, "--format", "json", *args], expected)
        # Native errors live on stderr, not stdout.
        raw = result or transcript[-1]["stderr"].encode()
        return json.loads(raw)

    def write(relative, content):
        target = root / relative
        require(target.resolve().is_relative_to(root.resolve()) and not target.is_symlink(), "fixture write escaped")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8", newline="\n")

    git("init", "-b", "master")
    for name, value in (("user.name", "Delivery Phase Fixture"), ("user.email", "fixture@example.invalid"),
                        ("commit.gpgSign", "false"), ("core.autocrlf", "false")):
        git("config", name, value)
    write("README.md", "# Isolated phase fixture\n")
    write(".gitignore", ".mine/runtime/\n.mine/locks/\n.mine/build/\n.mine/worktrees/\n.mine/candidates/\n")
    git("add", "README.md", ".gitignore")
    git("commit", "-m", "private baseline")
    cli("init")
    git("add", "-A")
    git("commit", "-m", "private native configuration")
    baseline = git("rev-parse", "HEAD")
    git("switch", "-c", "dev")
    write(".mine/runtime/delivery-fixture.marker", "isolated-delivery-phase-fixture\n")
    invoke([helper, "typed_legacy_source_fixture_for_actual_phase_probes", "--exact", "--nocapture"],
           extra_env={"MINE_DELIVERY_FIXTURE_ROOT": str(root), "MINE_DELIVERY_FIXTURE_BASELINE": baseline})
    config = tomllib.loads((root / ".mine/config.toml").read_text(encoding="utf-8"))
    plan_root = config["plan"]["root"]
    policy_path = ".mine/verification-policy.json"
    write(policy_path, json.dumps({"schema_version": 1, "design_references": [config["design"]["root"]],
          "checks": [{"name": "private-fixture-check", "command": {"argv": ["git", "rev-parse", "HEAD^{tree}"], "cwd": {"kind": "root"}},
                      "inputs": {"kind": "full"}, "tools": ["git"], "dependency_locks": [], "environment": [], "features": [],
                      "warnings_disqualify": True, "independent_required": True, "native_platform": None,
                      "retention_bytes": 4096, "retention_files": 8}]}))
    git("add", "-A")
    git("commit", "-m", "private legacy terminal source")
    require(cli("graph", "validate")["ok"], "legacy fixture graph invalid")
    live = cli("release")["data"]
    require(live["can_release"] and live["all_plans_terminal"] and live["design_valid"], "valid source preflight refused")
    original_readme = (root / "README.md").read_bytes()
    write("README.md", "dirty product counterexample\n")
    require(not cli("release")["data"]["can_release"], "dirty source falsely passed preflight")
    (root / "README.md").write_bytes(original_readme)
    source_commit = git("rev-parse", "HEAD")
    # Only this private index changes; real Root/index/refs remain untouched.
    index_env = {"GIT_INDEX_FILE": ".mine/build/private-candidate.index"}
    (root / ".mine/build").mkdir(parents=True, exist_ok=True)
    git("read-tree", source_commit, extra_env=index_env)
    git("rm", "--cached", "-r", "--", plan_root, extra_env=index_env)
    tree = git("write-tree", extra_env=index_env)
    candidate = git("commit-tree", tree, "-p", baseline, "-m", "private exact Plan purge candidate")
    candidate_root = ".mine/candidates/native-phase"
    git("worktree", "add", "--detach", candidate_root, candidate)
    request = {"schema_version": 1, "actor": "private-release-observer", "intent": {
        "kind": "release", "candidate": candidate, "candidate_root": candidate_root,
        "attribution": [f"plan{91:02}"], "policy_path": policy_path}}
    write(".mine/runtime/phase-begin.json", json.dumps(request))
    operation = cli("operation", "begin", "--expected-revision", "1", "--input", ".mine/runtime/phase-begin.json")["data"]["operation"]
    operation_id = operation["operation_id"]

    def evidence():
        result = {}
        for directory in (plan_root, ".mine/runtime/operations", ".mine/runtime/release-evidence"):
            for path in (root / directory).rglob("*"):
                if path.is_file():
                    require(not path.is_symlink(), "unexpected evidence link")
                    result[path.relative_to(root).as_posix()] = digest(path)
        return result

    def diagnose(expected=0, selected=operation_id):
        before = evidence()
        refs = git("show-ref")
        result = cli("release", "--operation-id", selected, expected=expected)
        require(evidence() == before and git("show-ref") == refs, "read-only diagnostic mutated evidence/refs")
        if expected == 0:
            data = result["data"]
            require(data["can_release"] is False and data["objective_phase_facts_valid"] is True
                    and data["current_gate_admission"] == "not_performed"
                    and data["operation"]["operation_id"] == operation_id,
                    "phase diagnostics claimed fresh permission or selected another operation")
        return result

    diagnose()
    diagnose(4, "00000000-0000-4000-8000-000000000001")
    # Purge only a freshly generated fixture directory, after resolved containment
    # and marker checks. No real registered worktree/source is cleaned here.
    purge = (root / plan_root).resolve(strict=True)
    require(root.resolve().is_relative_to(output.resolve()) and purge.is_relative_to(root.resolve())
            and purge != root.resolve() and (root / ".mine/runtime/delivery-fixture.marker").read_bytes()
            == b"isolated-delivery-phase-fixture\n", "unsafe fixture purge target")
    shutil.rmtree(purge)
    require(not (root / plan_root).exists(), "fixture Plan purge incomplete")
    cli("release", expected=4)  # no implicit retained-source fallback
    diagnose()
    manifest_path = operation["verification_evidence"][0]["path"]
    manifest = root / manifest_path
    original_manifest = manifest.read_bytes()
    manifest.write_bytes(original_manifest + b"\n")
    diagnose(4)
    manifest.write_bytes(original_manifest)
    diagnose()
    final = {"kind": "actual-candidate-phase-probes", "product_identity": identity,
             "binary_sha256": digest(binary), "fixture_helper_sha256": digest(helper),
             "harness_sha256": digest(__file__), "fixture_source": source_commit,
             "fixture_candidate": candidate, "release_operation": operation_id,
             "current_gate_admission": "not_performed", "publication_authorized": False,
             "cases": ["valid-live-source", "dirty-source-refusal", "selected-Intent-read-only",
                       "unknown-operation-refusal", "post-purge-no-implicit-fallback",
                       "selected-retained-source-read-only", "corrupt-bound-manifest-refusal", "same-operation-reobservation"],
             "commands_sha256": digest(output / "phase-commands.json"),
             "outcome": "PASS"}
    (output / "phase-evidence.json").write_text(json.dumps(final, indent=2), encoding="utf-8")
    print("actual candidate phase probes passed; diagnostics grant no fresh admission/publication")


def stable_hygiene(paths, plan_root):
    # Inspect the actual Git tree recursively, not only top-level directory names.
    roots = {plan_root, "docs/plan", ".mine/runtime", ".mine/locks", ".mine/build", ".mine/worktrees"}
    for path in paths:
        require(not any(path == p or path.startswith(p + "/") for p in roots),
                f"stable candidate contains ephemeral/runtime path: {path}")
        require(not path.startswith("docs/design-backup-"), "stable candidate contains a Design backup")


def candidate_checks(binary, source, reference, output):
    """Actual checked-out candidate facts, distinct from fixture/closure evidence."""
    source = source.resolve(strict=True)
    identity = candidate_identity(binary, source)
    version = binary_version(binary)
    package_version = tomllib.loads((source / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    config = tomllib.loads((source / ".mine/config.toml").read_text(encoding="utf-8"))
    require(version == package_version == config["mine_code_version"], "actual binary/source/config versions differ")
    expected = os.environ.get("GITHUB_SHA")
    require(expected is None or expected == identity["source_commit"], "checkout is not the actual event candidate")
    candidate_tag(version, reference)
    tagged = reference.startswith("refs/tags/")
    observations = []
    if tagged:
        target = run(["git", "rev-parse", "--verify", reference + "^{commit}"], source)
        require(target.returncode == 0 and target.stdout.decode().strip() == identity["source_commit"],
                "tag does not bind this exact candidate commit")
        tree = run(["git", "ls-tree", "-r", "--name-only", "-z", identity["source_commit"]], source)
        require(tree.returncode == 0, "cannot inspect actual candidate tree")
        paths = tree.stdout.decode("utf-8").rstrip("\0").split("\0")
        stable_hygiene(paths, config["plan"]["root"])
        for prefix in {config["plan"]["root"], "docs/plan"}:
            path = source / prefix
            require(path.resolve().is_relative_to(source) and path.resolve() != source,
                    "invalid/escaping Plan root in tag candidate")
            require(not path.is_symlink() and not any(p.is_file() or p.is_symlink() for p in path.rglob("*")),
                    "actual tag checkout retains local Plan artifacts")
    for group in ("design", "adr"):
        argv = [binary, "--repo", source, group, "validate", "--format", "json"]
        result = run(argv, source)
        require(result.returncode == 0, f"actual candidate {group} validation failed: {result.stderr.decode('utf-8', errors='replace')}")
        report = json.loads(result.stdout)
        require(report.get("ok") is True and report["data"]["valid"] is True
                and not report.get("warnings"), f"actual candidate {group} is invalid")
        require(os.path.samefile(report["repository"], source), "native validation observed another repository")
        observations.append({"argv": [str(a) for a in argv], "exit": result.returncode, "result": report})
    require(candidate_identity(binary, source) == identity, "candidate inputs changed during checks")
    output.mkdir(parents=True, exist_ok=True)
    (output / "candidate-evidence.json").write_text(json.dumps({
        "kind": "actual-checked-out-candidate", "product_identity": identity, "reference": reference,
        "version": version, "stable_hygiene": "PASS" if tagged else "NOT_APPLICABLE_NON_TAG",
        "observations": observations, "local_closure_observed": False, "publication_authorized": False,
        "harness_sha256": digest(__file__)}, indent=2), encoding="utf-8")
    print("actual candidate identity/Design/ADR checks passed; local closure remains a separate operator obligation")


class Contracts(unittest.TestCase):
    def test_stable_hygiene_refuses_nested_plan_backup_and_runtime_paths(self):
        stable_hygiene(["src/main.rs", ".mine/design/index.md", "docs/user.md"], ".mine/plan")
        for path in [".mine/plan/report.md", "docs/plan/old.md", "docs/design-backup-20260101/nested/leaf.md",
                     ".mine/runtime/design-backups/file", ".mine/build/cache", ".mine/worktrees/a/source"]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                stable_hygiene([path], ".mine/plan")
    def test_candidate_snapshot_binds_actual_files_and_artifact(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("MINE_DELIVERY_OUTPUT") or os.environ.get("TMP")) as tmp:
            root = Path(tmp)
            source = root / "snapshot"
            (source / "distribution").mkdir(parents=True)
            policy = source / "distribution/platforms.json"
            policy.write_bytes((ROOT / "distribution/platforms.json").read_bytes())
            binary = root / "non-executable-identity-fixture"
            binary.write_bytes(b"not native execution evidence")
            key = root / "source-key.json"
            identity = {"source_commit": "a" * 40, "source_tree": "b" * 40,
                        "binary_sha256": digest(binary), "files": [
                            {"path": "distribution/platforms.json", "sha256": digest(policy)}]}
            key.write_text(json.dumps(identity), encoding="utf-8")
            self.assertEqual(candidate_identity(binary, source, key)["source_commit"], "a" * 40)
            binary.write_bytes(b"changed")
            with self.assertRaises(ValueError):
                candidate_identity(binary, source, key)
            binary.write_bytes(b"not native execution evidence")
            extra = source / "undeclared-input"
            extra.write_bytes(b"unexpected source")
            with self.assertRaises(ValueError):
                candidate_identity(binary, source, key)
            extra.unlink()
            policy.write_bytes(b"changed policy")
            with self.assertRaises(ValueError):
                candidate_identity(binary, source, key)

    def test_publication_refuses_every_failed_skipped_cancelled_missing_job(self):
        names = required_jobs()
        facts = {n: {"result": "success"} for n in names}
        gate_needs(facts, "refs/tags/v0.2.0")
        for name in names:
            for outcome in ("failure", "skipped", "cancelled", "unknown", None):
                changed = {**facts, name: {"result": outcome}}
                with self.subTest(job=name, result=outcome), self.assertRaises(ValueError):
                    gate_needs(changed, "refs/tags/v0.2.0")
            with self.assertRaises(ValueError):
                gate_needs({k: v for k, v in facts.items() if k != name}, "refs/tags/v0.2.0")
        with self.assertRaises(ValueError):
            gate_needs({"native-delivery": {"result": "success"}}, "refs/tags/v0.2.0")
        for ref in ("refs/heads/dev", "refs/pull/1/merge", "refs/tags/vbroken"):
            with self.assertRaises(ValueError):
                gate_needs(facts, ref)

    def test_complete_asset_matrix_and_corruption_are_decisive(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("MINE_DELIVERY_OUTPUT") or os.environ.get("TMP")) as tmp:
            directory = Path(tmp)
            for row in platforms():
                path = directory / row["archive"]
                path.write_bytes(b"integrity fixture, not native smoke")
                path.with_name(path.name + ".sha256").write_text(f"{digest(path)}  {path.name}\n", encoding="ascii", newline="\n")
            self.assertEqual(len(verify_assets(directory).splitlines()), len(platforms()))
            (directory / "SHA256SUMS.txt").write_text(verify_assets(directory), encoding="ascii", newline="\n")
            self.assertEqual(len(verify_assets(directory).splitlines()), len(platforms()))
            (directory / "SHA256SUMS.txt").unlink()
            archive = directory / platforms()[0]["archive"]
            sidecar = archive.with_name(archive.name + ".sha256")
            good = sidecar.read_bytes()
            for invalid in (good + good, good.replace(archive.name.encode(), b"wrong.tar.gz"), b"0" * 64 + good[64:]):
                sidecar.write_bytes(invalid)
                with self.assertRaises(ValueError):
                    verify_assets(directory)
            sidecar.write_bytes(good)
            archive.write_bytes(b"corrupt")
            with self.assertRaises(ValueError):
                verify_assets(directory)
            sidecar.unlink()
            with self.assertRaises(ValueError):
                verify_assets(directory)

    def test_workflow_native_path_is_unconditional_and_fail_closed(self):
        text = (ROOT / ".github/workflows/release.yml").read_text()
        jobs = job_blocks(text)
        self.assertNotRegex(text, r"\|\|\s*(?:true|:)|continue-on-error|always\(\)")
        required_jobs(text)
        native = jobs["native-delivery"]
        self.assertNotRegex(native, r"^    if:", "native archives must run on branches/PRs too")
        for command in ("--package", "--archive", "--binary", "cargo test --test install_delivery actual_native", "cargo test --lib setup::"):
            self.assertIn(command, native)
        self.assertIn("fromJSON(needs.platforms.outputs.matrix)", native)
        release = jobs["release"]
        for command in ("--gate-needs", "--verify-assets", "softprops/action-gh-release"):
            self.assertIn(command, release)
        self.assertLess(release.index("--verify-assets"), release.index("softprops/action-gh-release"))
        self.assertIn("startsWith(github.ref, 'refs/tags/v')", release)
        self.assertEqual(candidate_tag("0.2.0", "refs/tags/v0.2.0"), "v0.2.0")
        self.assertEqual(candidate_tag("0.2.0", "refs/heads/dev"), "v0.2.0")
        self.assertIn("--preflight", jobs["preflight"])
        self.assertIn("--fixture-build", jobs["preflight"])
        self.assertNotIn("cargo test --test release --test e2e", jobs["preflight"])
        self.assertIn("contents: write", release)
        for name, block in jobs.items():
            if name != "release":
                self.assertNotIn("contents: write", block)
        with self.assertRaises(ValueError):
            candidate_tag("0.1.13", "refs/tags/v0.2.0")


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--transport":
        return transport(sys.argv[2:])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", action="store_true")
    parser.add_argument("--contracts-only", action="store_true")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--package", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--verify-assets", type=Path)
    parser.add_argument("--gate-needs")
    parser.add_argument("--ref", default=os.environ.get("GITHUB_REF", ""))
    parser.add_argument("--mcp", type=Path)
    parser.add_argument("--preflight", type=Path)
    parser.add_argument("--candidate-check", type=Path)
    parser.add_argument("--fixture-build", type=Path)
    parser.add_argument("--product-source", type=Path, default=ROOT)
    parser.add_argument("--source-key", type=Path)
    args = parser.parse_args()
    if args.matrix:
        print(json.dumps({"include": platforms()}, separators=(",", ":")))
    elif args.contracts_only:
        return 0 if unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Contracts)).wasSuccessful() else 1
    elif args.verify_assets:
        manifest = verify_assets(args.verify_assets)
        # Do not create this file until the exact matrix has been verified.
        (args.verify_assets / "SHA256SUMS.txt").write_text(manifest, encoding="ascii", newline="\n")
    elif args.gate_needs is not None:
        gate_needs(json.loads(args.gate_needs), args.ref)
        print("all mandatory jobs succeeded for exact tag")
    else:
        configured = args.output or (Path(os.environ["MINE_DELIVERY_OUTPUT"]) if "MINE_DELIVERY_OUTPUT" in os.environ else None)
        target = Path(os.environ["CARGO_TARGET_DIR"]).resolve() if "CARGO_TARGET_DIR" in os.environ else None
        require(configured is not None or target is not None, "set --output or Plan-isolated CARGO_TARGET_DIR")
        output = (configured or target.parent / "delivery-harness").resolve()
        output.mkdir(parents=True, exist_ok=True)
        if args.candidate_check:
            candidate_checks(args.candidate_check.resolve(strict=True), args.product_source, args.ref, output)
        elif args.mcp:
            binary = args.mcp.resolve(strict=True)
            identity = candidate_identity(binary, args.product_source, args.source_key)
            count = mcp_discovery(binary, output)
            (output / "discovery-evidence.json").write_text(json.dumps({
                "product_identity": identity, "binary_sha256": digest(binary),
                "tool_count": count, "tools_sha256": digest(output / "tools.json"),
                "harness_sha256": digest(__file__)}, indent=2), encoding="utf-8")
        elif args.preflight:
            require(args.fixture_build is not None, "phase probes need actual Cargo fixture-build JSON")
            binary = args.preflight.resolve(strict=True)
            identity = candidate_identity(binary, args.product_source, args.source_key)
            phase_preflight(binary, args.fixture_build, output, identity)
        elif args.package:
            identity = candidate_identity(args.package, args.product_source, args.source_key)
            archive = package(args.package, output, native_platform(), args.product_source)
            (output.parent / "package-evidence.json").write_text(json.dumps({
                "product_identity": identity, "archive_sha256": digest(archive),
                "harness_sha256": digest(__file__)}, indent=2), encoding="utf-8")
            print(archive)
        else:
            row = native_platform()
            binary = args.binary or (target / "debug" / row["binary"] if target else None)
            require(binary is not None, "select a built native candidate with --binary")
            binary = binary.resolve(strict=True)
            identity = candidate_identity(binary, args.product_source, args.source_key)
            native_smoke(binary, args.archive.resolve(strict=True) if args.archive else None,
                         output, row, args.product_source, identity)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, KeyError, StopIteration, subprocess.TimeoutExpired) as error:
        print(f"delivery gate failed: {error}", file=sys.stderr)
        sys.exit(1)
