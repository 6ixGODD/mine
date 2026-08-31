// Enforce no `unsafe` in MINE-owned test crates.
#![forbid(unsafe_code)]

//! Shared helpers for the distribution test suite.
//!
//! Sync-algorithm tests build isolated temporary source/destination trees and
//! never touch the real repository or user configuration.

use std::path::{Path, PathBuf};

/// The repository root (resolved from `CARGO_MANIFEST_DIR`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The authoritative root `skills/` directory.
pub fn skills_root() -> PathBuf {
    repo_root().join("skills")
}

/// The generated plugin skills directory.
pub fn plugin_skills_root() -> PathBuf {
    repo_root().join("plugins").join("mine").join("skills")
}

/// The five first-class Skill names, in deterministic order.
pub const FIVE_SKILLS: &[&str] = &[
    "mine-arch",
    "mine-plan-create",
    "mine-plan-exec",
    "mine-plan-review",
    "mine-sync",
];

/// The eighteen accepted MCP tool names exposed by `mine mcp serve`.
pub const ACCEPTED_MCP_TOOLS: &[&str] = &[
    "mine_workspace_status",
    "mine_graph_validate",
    "mine_graph_status",
    "mine_graph_ready",
    "mine_graph_wave",
    "mine_plan_show",
    "mine_design_validate",
    "mine_adr_show",
    "mine_adr_list",
    "mine_adr_validate",
    "mine_design_readiness",
    "mine_plan_add",
    "mine_plan_start",
    "mine_plan_mark_implemented",
    "mine_plan_accept",
    "mine_plan_reject",
    "mine_plan_withdraw",
];

/// Reads a file as a string, panicking on failure.
pub fn read_str(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Reads a Skill's `SKILL.md` by name.
pub fn skill_body(name: &str) -> String {
    read_str(&skills_root().join(name).join("SKILL.md"))
}

/// Collects all regular files under `root`, as a set of forward-slash relative
/// paths.
pub fn rel_files(root: &Path) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    if !root.exists() {
        return out;
    }
    for entry in walkdir(root) {
        if entry.is_file() {
            let rel = entry
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel);
        }
    }
    out
}

/// Recursively collects all paths under `root` (files and dirs).
fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let read = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in read.flatten() {
            let path = entry.path();
            out.push(path.clone());
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    out
}

/// Extracts candidate MCP tool-name tokens (`mine_<verb>_<noun>`) from text.
/// Only tokens that match the accepted MCP tool surface are returned; this
/// is used to verify Skills never reference a stale or invented MCP tool.
pub fn mcp_tool_refs(body: &str) -> Vec<&str> {
    let accepted: std::collections::HashSet<&str> = ACCEPTED_MCP_TOOLS.iter().copied().collect();
    let mut out: Vec<&str> = body
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|t| t.starts_with("mine_") && t.len() > 6 && accepted.contains(t))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A self-contained in-memory sync implementation mirroring
/// `scripts/sync-plugin-assets.py`, for testing the algorithm on isolated temp
/// directories without depending on Python or the real repository.
pub fn sync_to(src: &Path, dst: &Path) {
    // Remove stale files (in dst, not in src).
    let src_set = rel_files(src);
    let dst_set = rel_files(dst);
    for rel in &dst_set - &src_set {
        let _ = std::fs::remove_file(dst.join(rel));
    }
    // Copy/overwrite every source file (binary-faithful).
    for rel in &src_set {
        let src_path = src.join(rel);
        let dst_path = dst.join(rel);
        std::fs::create_dir_all(dst_path.parent().unwrap()).unwrap();
        let bytes = std::fs::read(&src_path).unwrap();
        if !dst_path.exists() || std::fs::read(&dst_path).unwrap() != bytes {
            std::fs::write(&dst_path, &bytes).unwrap();
        }
    }
    // Remove now-empty directories with no source counterpart.
    let mut dirs: Vec<PathBuf> = walkdir(dst).into_iter().filter(|p| p.is_dir()).collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for d in dirs {
        if d == dst {
            continue;
        }
        let rel = d
            .strip_prefix(dst)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !src.join(&rel).exists() && std::fs::read_dir(&d).unwrap().next().is_none() {
            let _ = std::fs::remove_dir(&d);
        }
    }
}

/// Checks whether `dst` matches `src` (no missing, stale, or differing files).
/// Returns `Err(message)` on drift, `Ok(())` when in sync.
pub fn check_sync(src: &Path, dst: &Path) -> Result<(), String> {
    let src_set = rel_files(src);
    let dst_set = rel_files(dst);
    let missing: Vec<_> = (&src_set - &dst_set).into_iter().collect();
    let stale: Vec<_> = (&dst_set - &src_set).into_iter().collect();
    let mut differing = Vec::new();
    for rel in &src_set & &dst_set {
        if std::fs::read(src.join(&rel)).unwrap() != std::fs::read(dst.join(&rel)).unwrap() {
            differing.push(rel.clone());
        }
    }
    if missing.is_empty() && stale.is_empty() && differing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "drift: missing={missing:?}, stale={stale:?}, differing={differing:?}"
        ))
    }
}
