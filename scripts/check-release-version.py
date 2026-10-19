#!/usr/bin/env python3
"""Read-only metadata coherence; not test/native/acceptance/publication authority."""
import argparse
import json
from pathlib import Path
import re
import sys
import tomllib

SKILLS = {"mine-arch", "mine-plan-create", "mine-plan-exec", "mine-plan-review", "mine-sync"}
UPSTREAM = "https://github.com/6ixGODD/mine"


def check(root):
    def toml(relative):
        return tomllib.loads((root / relative).read_text(encoding="utf-8"))

    def data(relative):
        return json.loads((root / relative).read_text(encoding="utf-8"))

    config = toml(".mine/config.toml")
    expected = config["mine_code_version"]
    if not isinstance(expected, str) or not expected:
        raise ValueError("configuration has no release version")
    cargo = toml("Cargo.toml")["package"]
    versions = {"Cargo.toml": cargo["version"], ".mine/config.toml": expected}
    locked = [p for p in toml("Cargo.lock")["package"] if p["name"] == cargo["name"]]
    if len(locked) != 1 or cargo["name"] != "mine":
        raise ValueError("lockfile must contain one exact mine product package")
    versions["Cargo.lock"] = locked[0]["version"]
    for relative in ["package.json", ".claude-plugin/plugin.json",
                     "plugins/mine/.claude-plugin/plugin.json", "plugins/mine/.codex-plugin/plugin.json"]:
        versions[relative] = data(relative)["version"]
    actual = {p.name for p in (root / "skills").iterdir() if p.is_dir()}
    if actual != SKILLS:
        raise ValueError("expected exactly the five first-class Skill directories")
    for skill in sorted(SKILLS):
        for prefix in ["skills", "plugins/mine/skills"]:
            relative = f"{prefix}/{skill}/SKILL.md"
            text = (root / relative).read_text(encoding="utf-8")
            frontmatter = re.match(r"\A---\n(.*?)\n---\n", text, re.S)
            if frontmatter is None:
                raise ValueError(f"invalid Skill frontmatter: {relative}")
            entries = re.findall(r"^version: ([^\n]+)$", frontmatter[1], re.M)
            if len(entries) != 1:
                raise ValueError(f"expected one Skill version: {relative}")
            versions[relative] = entries[0]
    drift = {path: value for path, value in versions.items() if value != expected}
    if drift:
        raise ValueError(f"version drift from {expected}: {drift}")
    repositories = {
        "Cargo.toml": cargo["repository"],
        ".claude-plugin/marketplace.json": data(".claude-plugin/marketplace.json")["plugins"][0]["repository"],
        "plugins/mine/.claude-plugin/plugin.json": data("plugins/mine/.claude-plugin/plugin.json")["repository"],
        "plugins/mine/.codex-plugin/plugin.json": data("plugins/mine/.codex-plugin/plugin.json")["repository"],
    }
    if any(value != UPSTREAM for value in repositories.values()):
        raise ValueError(f"canonical upstream drift: {repositories}")
    marketplace = data(".claude-plugin/marketplace.json")
    if marketplace["name"] == marketplace["plugins"][0]["name"]:
        raise ValueError("marketplace and plugin discovery identities must remain distinct")
    if marketplace["plugins"][0]["source"] != "./plugins/mine":
        raise ValueError("marketplace must resolve the self-contained product plugin")
    platform_data = data("distribution/platforms.json")
    if platform_data["schema_version"] != 1:
        raise ValueError("unsupported platform manifest schema")
    platforms = platform_data["platforms"]
    # This release-scope check must not manufacture a macOS pass or phantom asset.
    if expected == "0.2.0" and {(p["os"], p["arch"]) for p in platforms} != {
            ("windows", "x86_64"), ("linux", "x86_64")}:
        raise ValueError("0.2.0 delivers Windows/Linux x86_64; macOS is owner-deferred")
    if len({(p["os"], p["arch"]) for p in platforms}) != len(platforms):
        raise ValueError("duplicate release platform identity")
    return {"ok": True, "version": expected, "managed_versions": versions,
            "upstream": UPSTREAM, "platforms": [p["target"] for p in platforms],
            "macos": "DEFERRED" if expected == "0.2.0" else "see approved release scope"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1],
                        help="isolated metadata fixture; read-only")
    arguments = parser.parse_args()
    try:
        print(json.dumps(check(arguments.root.resolve()), sort_keys=True))
        return 0
    except (OSError, KeyError, IndexError, ValueError, TypeError) as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
