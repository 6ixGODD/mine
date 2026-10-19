//! Embedded Skill payloads for release binaries.
//!
//! Embeds the five authoritative Skill directories directly into the
//! `mine` binary via `include_str!`, so standalone installation does not
//! require a Git checkout. The authoritative source is the repository-root
//! `skills/` directory (the only hand-edited Skill source, per
//! `docs/design/integrations/distribution.md`); the embedded content here is a
//! generated artifact kept in lock-step at **build time**.
//!
//! # Build-time verification
//!
//! `include_str!` fails the build if a referenced file does not exist, so the
//! list below is a build-time assertion that every embedded path resolves. The
//! companion test `tests/distribution/embedded.rs::embedded_skills_match_root`
//! walks `skills/` at test time and asserts that (a) every file in `skills/`
//! is embedded here and (b) every embedded entry matches its root file
//! byte-for-byte. Together these guarantee the embedded payload cannot drift
//! from the authoritative source without failing a gate.
//!
//! Adding a new Skill file requires adding an `include_str!` entry here; the
//! test catches omission. This is intentional: the list is a compile-time
//! inventory of embedded Skill content.

/// One embedded Skill file: its repository-relative path and static content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedSkillFile {
    /// Repository-relative path using forward slashes (e.g.
    /// `skills/mine-arch/SKILL.md`).
    pub path: &'static str,
    /// The file content, embedded at compile time.
    pub content: &'static str,
}

/// The complete inventory of embedded Skill files, in deterministic
/// (path-sorted) order.
pub const EMBEDDED_SKILL_FILES: &[EmbeddedSkillFile] = &[
    EmbeddedSkillFile {
        path: "skills/mine-arch/SKILL.md",
        content: include_str!("../../skills/mine-arch/SKILL.md"),
    },
    EmbeddedSkillFile {
        path: "skills/mine-arch/references/AGENTS.template.md",
        content: include_str!("../../skills/mine-arch/references/AGENTS.template.md"),
    },
    EmbeddedSkillFile {
        path: "skills/mine-arch/references/architecture-outline.md",
        content: include_str!("../../skills/mine-arch/references/architecture-outline.md"),
    },
    EmbeddedSkillFile {
        path: "skills/mine-plan-create/SKILL.md",
        content: include_str!("../../skills/mine-plan-create/SKILL.md"),
    },
    EmbeddedSkillFile {
        path: "skills/mine-plan-create/references/execution-graph-template.md",
        content: include_str!(
            "../../skills/mine-plan-create/references/execution-graph-template.md"
        ),
    },
    EmbeddedSkillFile {
        path: "skills/mine-plan-create/references/parallel-execution-protocol-template.md",
        content: include_str!(
            "../../skills/mine-plan-create/references/parallel-execution-protocol-template.md"
        ),
    },
    EmbeddedSkillFile {
        path: "skills/mine-plan-create/references/plan-template.md",
        content: include_str!("../../skills/mine-plan-create/references/plan-template.md"),
    },
    EmbeddedSkillFile {
        path: "skills/mine-plan-exec/SKILL.md",
        content: include_str!("../../skills/mine-plan-exec/SKILL.md"),
    },
    EmbeddedSkillFile {
        path: "skills/mine-plan-review/SKILL.md",
        content: include_str!("../../skills/mine-plan-review/SKILL.md"),
    },
    EmbeddedSkillFile {
        path: "skills/mine-plan-review/references/native-closure.md",
        content: include_str!("../../skills/mine-plan-review/references/native-closure.md"),
    },
    EmbeddedSkillFile {
        path: "skills/mine-sync/SKILL.md",
        content: include_str!("../../skills/mine-sync/SKILL.md"),
    },
];

/// Looks up an embedded Skill file by its repository-relative path (forward
/// slashes). Returns `None` when no embedded file matches.
#[must_use]
pub fn get(path: &str) -> Option<&'static str> {
    EMBEDDED_SKILL_FILES
        .iter()
        .find(|f| f.path == path)
        .map(|f| f.content)
}

/// Returns the list of embedded file paths (repository-relative, forward
/// slashes), in the same deterministic order as [`EMBEDDED_SKILL_FILES`].
#[must_use]
pub fn paths() -> Vec<&'static str> {
    EMBEDDED_SKILL_FILES.iter().map(|f| f.path).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_paths_are_sorted_and_unique() {
        let paths = paths();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "EMBEDDED_SKILL_FILES must be path-sorted");
        let mut seen = std::collections::HashSet::new();
        for p in &paths {
            assert!(seen.insert(p), "duplicate embedded path: {p}");
        }
    }

    #[test]
    fn all_five_skill_directories_embedded() {
        let dirs: std::collections::HashSet<&str> = paths()
            .iter()
            .filter_map(|p| p.strip_prefix("skills/").and_then(|s| s.split('/').next()))
            .collect();
        assert_eq!(
            dirs,
            [
                "mine-arch",
                "mine-plan-create",
                "mine-plan-exec",
                "mine-plan-review",
                "mine-sync"
            ]
            .into_iter()
            .collect::<std::collections::HashSet<&str>>(),
            "all five first-class Skills must be embedded"
        );
    }
}
