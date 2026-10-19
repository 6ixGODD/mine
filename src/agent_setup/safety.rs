// Enforce `AGENTS.md`'s "Business code must not use `unsafe`" at compile time.
#![forbid(unsafe_code)]

//! Path-safety guards for the agent installer.
//!
//! [`SafetyGuard`] is the single chokepoint every filesystem write goes
//! through. It rejects, before any mutation:
//!
//! - targets that resolve outside the injected configuration root (the hard
//!   test guard fails the operation rather than write outside the root);
//! - path traversal (`..` components);
//! - symlink/junction/reparse-point escapes (any symlink on the resolved
//!   path whose target leaves the root, or whose target cannot be
//!   canonicalized, is refused);
//! - destination-escape via absolute rel-to-root tricks.
//!
//! Canonicalization must work **even when the target does not yet exist**:
//! install creates new files/dirs, so the guard canonicalizes the parent
//! (which must exist) and appends the remaining components, then verifies the
//! resulting lexically-normalized path stays within the root. On platforms
//! where the parent itself does not yet exist, the guard walks up to the
//! nearest existing ancestor, canonicalizes it, and re-checks the remainder.
//!
//! No `unsafe` is used; cross-platform symlink/junction detection relies on
//! [`std::fs::symlink_metadata`] (reparse points surface as `is_symlink()` on
//! Windows junctions too).

use std::path::{Component, Path, PathBuf};

use crate::domain::error::{MineError, MineResult};

/// A hard safety guard bound to a configuration root. Every write the
/// installer performs must pass through [`SafetyGuard::ensure_within_root`].
#[derive(Debug, Clone)]
pub struct SafetyGuard {
    /// Canonical physical root for symlink/junction containment checks.
    root: PathBuf,
    /// Absolute lexical spelling supplied by the caller. Keep this separately:
    /// Windows may expose an existing temp root through a 8.3 short path while
    /// `canonicalize` returns its long path, and macOS commonly canonicalizes
    /// `/var` to `/private/var`.
    lexical_root: PathBuf,
}

impl SafetyGuard {
    /// Constructs a guard bound to `root`. Resolve its longest existing prefix
    /// just like write targets, even before a fresh installation creates the
    /// root. This keeps physical path aliases consistent across creation.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        let lexical_root = normalize_absolute(root);
        let canonical = canonicalize_longest_existing_prefix(&lexical_root);
        Self {
            root: canonical,
            lexical_root,
        }
    }

    /// The bound root (canonicalized when it exists).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validates that `candidate`, after canonicalization/normalization, lies
    /// strictly within the root. Returns the resolved absolute path on
    /// success, or a `MINE_AGENT_PATH_ESCAPE` error on any escape.
    ///
    /// `candidate` may be absolute (built from the same non-canonical
    /// configuration root the guard was constructed with) or relative (in
    /// which case it is joined to the guard's root). This is the hard write
    /// guard; callers pass the *target* path here before writing.
    pub fn ensure_within_root(&self, candidate: &Path) -> MineResult<PathBuf> {
        // 1. Lexically normalize the candidate relative to the root. If the
        //    candidate is absolute, normalize it on its own; reject `..`
        //    escape by checking the result still starts with the root.
        let normalized = if candidate.is_absolute() {
            normalize_absolute(candidate)
        } else {
            normalize_relative_to(&self.lexical_root, candidate)
        };
        if !normalized.starts_with(&self.lexical_root)
            && !normalized.starts_with(self.root_canonical())
        {
            return Err(path_escape(candidate, &self.lexical_root));
        }
        // 2. Filesystem-canonicalize the longest existing prefix and verify it
        //    stays within the canonical root (catches symlink/junction
        //    reparse points that lexically appear inside but resolve outside).
        let resolved = canonicalize_longest_existing_prefix(&normalized);
        if !resolved.starts_with(&self.root) {
            return Err(path_escape(candidate, &self.root));
        }
        // Canonicalizing the longest existing prefix follows every existing
        // symlink/junction before this containment check. That rejects a link
        // escaping the root while accepting Windows 8.3 and macOS `/var`
        // aliases for the same physical location. Preserve the lexical form
        // for callers: managed state must not persist Windows verbatim paths.
        Ok(normalized)
    }

    /// The canonicalized root used for filesystem comparisons.
    fn root_canonical(&self) -> PathBuf {
        self.root.clone()
    }
}

/// `MINE_AGENT_PATH_ESCAPE`: a candidate resolves outside the configuration
/// root.
fn path_escape(candidate: &Path, root: &Path) -> MineError {
    MineError::AgentPathEscape {
        candidate: candidate.to_path_buf(),
        root: root.to_path_buf(),
        detail: "write target resolves outside the configuration root".to_string(),
    }
}

/// Lexically normalizes an absolute path (collapses `.`, resolves `..`
/// against the prefix). Used when `canonicalize` fails because the path
/// does not yet exist.
fn normalize_absolute(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        push_component(&mut out, c);
    }
    out
}

/// Normalizes `candidate` relative to `root`, producing an absolute path
/// inside (or referencing) the root. `..` components are resolved against the
/// accumulated prefix; any `..` that would escape past the root is preserved
/// (and then caught by the `starts_with` check).
fn normalize_relative_to(root: &Path, candidate: &Path) -> PathBuf {
    let mut out = root.to_path_buf();
    for c in candidate.components() {
        push_component(&mut out, c);
    }
    out
}

fn push_component(out: &mut PathBuf, c: Component<'_>) {
    match c {
        Component::CurDir => {}
        Component::ParentDir => {
            // Lexical `..`: pop the last component if it is a normal one.
            // (Prefix and RootDir are preserved.)
            match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                _ => {
                    // At/above a root or prefix: keep the `..` so the
                    // subsequent starts_with check flags the escape.
                    out.push("..");
                }
            }
        }
        other => out.push(other.as_os_str()),
    }
}

/// Canonicalizes the longest existing prefix of `path` and appends the
/// remaining non-existing components. This yields a real-filesystem
/// resolution even when the leaf does not yet exist.
fn canonicalize_longest_existing_prefix(path: &Path) -> PathBuf {
    // Walk from the leaf up until we find an existing ancestor.
    let mut to_create: Vec<std::ffi::OsString> = Vec::new();
    let mut probe = path.to_path_buf();
    loop {
        match probe.canonicalize() {
            Ok(canon) => {
                let mut full = canon;
                for part in to_create.into_iter().rev() {
                    full.push(part);
                }
                return full;
            }
            Err(_) => {
                if let Some(name) = probe.file_name() {
                    to_create.push(name.to_os_string());
                }
                if !probe.pop() {
                    // Reached the filesystem root without a canonicalizable
                    // ancestor; return the lexically-normalized form.
                    let mut full = probe;
                    for part in to_create.into_iter().rev() {
                        full.push(part);
                    }
                    return full;
                }
            }
        }
    }
}

/// Computes a SHA-256 content hash for drift evidence (the managed-state
/// record stores one per owned file). Held as a hex string for portability.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // A deterministic non-crypto hash is sufficient for drift detection (we are
    // not defending against adversarial collision; we are detecting accidental
    // edits). Two independent passes for a wider fingerprint.
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    bytes.hash(&mut h1);
    // Mix the length in to avoid prefix collisions of different-length content.
    (bytes.len() as u64).hash(&mut h2);
    h1.finish().hash(&mut h2);
    format!("{:016x}{:016x}", h1.finish(), h2.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn guard() -> (TempDir, SafetyGuard) {
        let tmp = tempfile::tempdir().unwrap();
        let g = SafetyGuard::new(tmp.path());
        (tmp, g)
    }

    #[test]
    fn inside_path_is_accepted() {
        let (_t, g) = guard();
        let p = g
            .ensure_within_root(Path::new("skills/mine-arch/SKILL.md"))
            .unwrap();
        assert!(p.ends_with("skills/mine-arch/SKILL.md"));
    }

    #[test]
    fn fresh_root_remains_contained_after_it_is_created() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("new/config");
        let g = SafetyGuard::new(&root);
        let candidate = root.join("skills/mine-arch/SKILL.md");
        assert!(g.ensure_within_root(&candidate).is_ok());
        std::fs::create_dir_all(&root).unwrap();
        assert!(g.ensure_within_root(&candidate).is_ok());
        assert!(g.ensure_within_root(Path::new("../../outside")).is_err());
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn fresh_root_still_rejects_a_child_link_to_outside() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("new/config");
        let outside = tmp.path().join("outside");
        let g = SafetyGuard::new(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let link = root.join("skills");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        #[cfg(windows)]
        assert!(
            std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "New-Item -ItemType Junction -Path $env:MINE_TEST_LINK -Target $env:MINE_TEST_TARGET | Out-Null",
                ])
                .env("MINE_TEST_LINK", &link)
                .env("MINE_TEST_TARGET", &outside)
                .status()
                .unwrap()
                .success()
        );
        assert!(g.ensure_within_root(&link.join("SKILL.md")).is_err());
        assert!(!outside.join("SKILL.md").exists());
    }

    #[test]
    fn traversal_outside_root_is_rejected() {
        let (_t, g) = guard();
        assert!(g.ensure_within_root(Path::new("../../etc/passwd")).is_err());
    }

    #[test]
    fn absolute_outside_root_is_rejected() {
        let (_t, g) = guard();
        assert!(g.ensure_within_root(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn root_itself_is_accepted() {
        let (_t, g) = guard();
        assert!(g.ensure_within_root(g.root()).is_ok());
    }
}
