// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Distribution-structure tests: verify the Claude Code, Codex, Pi, and
//! OpenCode distribution assets exist, are self-contained, reference valid
//! in-repository assets, and present exactly one Skill discovery path per
//! Agent.

use super::common::*;
use std::path::Path;

fn read_json(p: &Path) -> serde_json::Value {
    serde_json::from_str(&read_str(p)).unwrap_or_else(|e| panic!("parse JSON {}: {e}", p.display()))
}

#[test]
fn claude_marketplace_layout_is_self_contained() {
    // .claude-plugin/marketplace.json + plugins/mine/.claude-plugin/plugin.json
    // + plugins/mine/skills/<skill>/SKILL.md
    let marketplace = repo_root().join(".claude-plugin/marketplace.json");
    let plugin = repo_root().join("plugins/mine/.claude-plugin/plugin.json");
    assert!(marketplace.exists(), "Claude marketplace.json must exist");
    assert!(plugin.exists(), "Claude plugin.json must exist");

    let m = read_json(&marketplace);
    assert_eq!(m["plugins"][0]["name"], "mine");
    assert_eq!(m["plugins"][0]["source"], "./plugins/mine");

    let p = read_json(&plugin);
    assert_eq!(p["name"], "mine");
    assert_eq!(p["skills"], "./skills/");
    // Self-contained: the plugin skills directory exists inside plugins/mine/.
    for s in FIVE_SKILLS {
        assert!(
            plugin_skills_root().join(s).join("SKILL.md").exists(),
            "Claude plugin must contain skills/{s}/SKILL.md"
        );
    }
}

#[test]
fn claude_standalone_plugin_exists() {
    // Standalone installation for short commands like /mine-arch.
    let standalone = repo_root().join(".claude-plugin/plugin.json");
    assert!(
        standalone.exists(),
        "standalone Claude plugin.json must exist"
    );
    let p = read_json(&standalone);
    assert_eq!(p["name"], "mine-is-not-everyones");
}

#[test]
fn codex_plugin_metadata_exists() {
    let codex = repo_root().join("plugins/mine/.codex-plugin/plugin.json");
    assert!(codex.exists(), "Codex plugin.json must exist");
    let p = read_json(&codex);
    assert_eq!(p["name"], "mine");
    assert_eq!(p["skills"], "./skills/");
    // Codex shares the same skills/ directory as Claude (one generated copy).
    assert!(
        plugin_skills_root().exists(),
        "Codex must discover Skills via plugins/mine/skills/"
    );
}

#[test]
fn pi_package_exposes_root_skills() {
    let pkg = read_json(&repo_root().join("package.json"));
    assert_eq!(pkg["name"], "mine-is-not-everyones");
    assert_eq!(pkg["private"], true);
    // Pi discovers Skills through the root skills/ directory.
    let pi_skills = pkg["pi"]["skills"].as_array().unwrap();
    assert!(
        pi_skills.iter().any(|v| v == "./skills"),
        "package.json must expose ./skills for Pi discovery"
    );
}

#[test]
fn opencode_marketplace_exists() {
    // OpenCode discovers via .agents/plugins/marketplace.json.
    let m = repo_root().join(".agents/plugins/marketplace.json");
    assert!(m.exists(), "OpenCode marketplace.json must exist");
    let v = read_json(&m);
    assert_eq!(v["plugins"][0]["name"], "mine");
}

#[test]
fn mcp_registration_points_to_real_mine_mcp_serve() {
    // The plugin .mcp.json must register the real `mine mcp serve` command.
    let mcp = repo_root().join("plugins/mine/.mcp.json");
    assert!(mcp.exists(), "plugin .mcp.json must exist");
    let v = read_json(&mcp);
    let server = &v["mcpServers"]["mine"];
    assert_eq!(server["command"], "mine");
    let args = server["args"].as_array().unwrap();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], "mcp");
    assert_eq!(args[1], "serve");
}

#[test]
fn plugin_versions_match_mine_version_source() {
    // All plugin metadata must derive from the repository version source.
    let config: toml::Value = toml::from_str(&read_str(&repo_root().join(".mine/config.toml")))
        .expect("repository config parses");
    let expected = config["mine_code_version"]
        .as_str()
        .expect("mine_code_version is a string");
    for p in [
        "plugins/mine/.claude-plugin/plugin.json",
        "plugins/mine/.codex-plugin/plugin.json",
        ".claude-plugin/plugin.json",
    ] {
        let v = read_json(&repo_root().join(p));
        assert_eq!(
            v["version"].as_str().unwrap_or(""),
            expected,
            "{p} version must be {expected} (MINE version source)"
        );
    }
}

#[test]
fn no_duplicate_skill_discovery_for_claude() {
    // Claude Code discovers Skills either via the marketplace plugin or the
    // standalone plugin, not both in conflicting ways. The marketplace plugin
    // points to ./plugins/mine (self-contained copy); the standalone points to
    // root skills/. They are distinct installation modes, not duplicate
    // discovery paths.
    let marketplace = read_json(&repo_root().join(".claude-plugin/marketplace.json"));
    let standalone = read_json(&repo_root().join(".claude-plugin/plugin.json"));
    // The marketplace plugin source is the self-contained plugins/mine/.
    assert_eq!(marketplace["plugins"][0]["source"], "./plugins/mine");
    // The standalone plugin is a different name (namespace isolation).
    assert_ne!(
        marketplace["plugins"][0]["name"], standalone["name"],
        "marketplace plugin and standalone must have distinct names to avoid duplicate discovery"
    );
}

#[test]
fn generated_plugin_skills_are_byte_equivalent_to_root() {
    // The generated plugins/mine/skills/ must be byte-for-byte identical to
    // the authoritative root skills/.
    let src = skills_root();
    let dst = plugin_skills_root();
    let src_files = rel_files(&src);
    let dst_files = rel_files(&dst);
    assert_eq!(
        src_files, dst_files,
        "generated and root skill file sets must match"
    );
    for rel in &src_files {
        let s = std::fs::read(src.join(rel)).unwrap();
        let d = std::fs::read(dst.join(rel)).unwrap();
        assert_eq!(s, d, "generated file {rel} must be byte-equivalent to root");
    }
}

#[test]
fn plugin_directory_is_self_contained_no_outside_links() {
    // The plugin directory must not point outside its root through fragile
    // relative links. Verify no symlink escapes plugins/mine/.
    let plugin_root = repo_root().join("plugins/mine");
    let mut stack = vec![plugin_root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            #[cfg(unix)]
            {
                let md = std::fs::symlink_metadata(&path).unwrap();
                if md.file_type().is_symlink() {
                    let target = std::fs::read_link(&path).unwrap();
                    let resolved = path.parent().unwrap().join(&target);
                    assert!(
                        resolved.starts_with(&plugin_root),
                        "plugin symlink {:?} escapes plugin root to {:?}",
                        path,
                        target
                    );
                }
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
}
