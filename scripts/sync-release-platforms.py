#!/usr/bin/env python3
"""Generate the standalone dispatches and Rust table from one release matrix."""
import argparse
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]


def synchronize(check):
    manifest = json.loads((ROOT / "distribution/platforms.json").read_text(encoding="utf-8"))
    if manifest["schema_version"] != 1:
        raise ValueError("unsupported platform manifest")
    rows = manifest["platforms"]
    identities = set()
    for row in rows:
        for value in row.values():
            if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_.-]+", value):
                raise ValueError("unsafe platform identity")
        identity = (row["os"], row["arch"])
        if identity in identities:
            raise ValueError("duplicate platform")
        identities.add(identity)
    sh = 'case "$(uname -s):$(uname -m)" in\n'
    for row in rows:
        if row["os"] != "windows":
            sh += f'  {row["kernel"]}:{row["machine"]}) asset="{row["archive"]}"; bin_name="{row["binary"]}" ;;\n'
    sh += '  *) die "unsupported platform: $(uname -s) $(uname -m)" ;;\nesac'
    windows = [row for row in rows if row["os"] == "windows"]
    ps = 'switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {\n'
    for row in windows:
        if row["arch"] != "x86_64":
            raise ValueError("Windows architecture mapping requires an explicit generator change")
        ps += f'    "X64" {{ $Asset = "{row["archive"]}"; $BinaryName = "{row["binary"]}" }}\n'
    ps += '    default { throw "unsupported Windows architecture" }\n}'
    rust = 'pub const PLATFORMS: &[Platform] = &[\n'
    for row in rows:
        rust += '    Platform {\n'
        for key in ("os", "arch", "target", "archive", "binary", "runner"):
            rust += f'        {key}: "{row[key]}",\n'
        rust += '    },\n'
    rust += '];'
    outputs = [("scripts/bootstrap.sh", "#", sh), ("scripts/bootstrap.ps1", "#", ps),
               ("src/setup/platform.rs", "//", rust)]
    drift = []
    for path, comment, body in outputs:
        target = ROOT / path
        text = target.read_text(encoding="utf-8")
        begin, end = f"{comment} BEGIN MINE PLATFORMS", f"{comment} END MINE PLATFORMS"
        pattern = re.compile(re.escape(begin) + r"\n.*?\n" + re.escape(end), re.S)
        if len(pattern.findall(text)) != 1:
            raise ValueError(f"expected one generated region: {path}")
        expected = pattern.sub(lambda _: begin + "\n" + body + "\n" + end, text)
        if expected != text:
            drift.append(path)
            if not check:
                target.write_text(expected, encoding="utf-8", newline="\n")
    if drift and check:
        print("platform dispatch drift: " + ", ".join(drift), file=sys.stderr)
        return 1
    print("release platform dispatches are in sync")
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    sys.exit(synchronize(parser.parse_args().check))
