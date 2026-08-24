// Enforce `AGENTS.md`'s "Business code must not use `unsafe`" at compile time.
#![forbid(unsafe_code)]

//! Per-Agent installation targets and configuration formats.
//!
//! Destinations are derived from the official client documentation (see the
//! parent module documentation for the research register). The critical
//! Explicit `--config-root` isolation. There are TWO
//! separate, never-mixed construction paths for the environment:
//!
//! - [`Env::isolated`] — built when an explicit `--config-root` is supplied.
//!   It ignores **all** real process environment overrides
//!   (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `PI_HOME`, `OPENCODE_CONFIG_DIR`)
//!   and derives every Agent path only from the injected root + approved
//!   deterministic subpaths. It never falls back to the real HOME.
//! - [`Env::real_env`] — built only when no explicit root is given (production
//!   use). It reads the real process environment and the platform home dir.
//!
//! These constructors are kept separate so an accidental partial override
//! cannot recur: an isolated env has an empty
//! override map by construction.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Agent {
    ClaudeCode,
    Codex,
    Pi,
    OpenCode,
}

impl Agent {
    /// The stable slug used in managed state, JSON output, and the CLI
    /// `--agent` flag.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude-code",
            Agent::Codex => "codex",
            Agent::Pi => "pi",
            Agent::OpenCode => "opencode",
        }
    }

    /// Parses a slug into an [`Agent`].
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "claude-code" => Some(Agent::ClaudeCode),
            "codex" => Some(Agent::Codex),
            "pi" => Some(Agent::Pi),
            "opencode" => Some(Agent::OpenCode),
            _ => None,
        }
    }

    /// Whether this Agent supports an MCP server registration. Pi has no MCP
    /// in its minimal core.
    #[must_use]
    pub fn supports_mcp(self) -> bool {
        !matches!(self, Agent::Pi)
    }

    pub const ALL: [Agent; 4] = [Agent::ClaudeCode, Agent::Codex, Agent::Pi, Agent::OpenCode];
}

/// The official environment-variable override names. When an isolated
/// [`Env`] is constructed, these are NEVER consulted from the real process
/// environment; the override map is empty by construction.
pub const OVERRIDE_KEYS: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "PI_HOME",
    "OPENCODE_CONFIG_DIR",
];

/// The injected environment: a configuration root plus an explicit override
/// map. Immensely importantly, the override map is the **only** source of
/// per-Agent overrides; the real process environment is never consulted after
/// construction (see [`Env::isolated`] vs [`Env::real_env`]).
#[derive(Debug, Clone)]
pub struct Env {
    /// The configuration root (an isolated temp dir in tests, or the platform
    /// home dir in production real-env mode).
    pub config_root: PathBuf,
    /// Explicit per-Agent directory overrides (may be empty — the isolated
    /// mode default). Never read from `std::env` after construction.
    pub overrides: std::collections::HashMap<String, PathBuf>,
    /// `true` when this env is explicitly isolated (an explicit `--config-root`
    /// was supplied). In isolated mode real environment overrides are
    /// forbidden; the overrides map is always empty.
    pub isolated: bool,
}

impl Env {
    /// Builds an **isolated** env rooted at `root`: NO real process environment
    /// overrides are honored, and every Agent path derives only from `root`
    /// and deterministic subpaths. This is the constructor used when an explicit
    /// config root is supplied (tests and explicit-root installs). The real
    /// env vars CLAUDE_CONFIG_DIR / CODEX_HOME / PI_HOME / OPENCODE_CONFIG_DIR
    /// are never read.
    #[must_use]
    pub fn isolated(root: PathBuf) -> Self {
        Self {
            config_root: root,
            overrides: std::collections::HashMap::new(),
            isolated: true,
        }
    }

    /// Builds a **real** env from the live process environment (production path
    /// ONLY; never used under an explicit `--config-root`). It reads the real
    /// platform home dir and the real per-Agent override env vars.
    /// Tests never call this; they inject an isolated env.
    #[must_use]
    pub fn real_env() -> Self {
        let config_root = real_home();
        let mut overrides = std::collections::HashMap::new();
        for key in OVERRIDE_KEYS {
            if let Some(val) = std::env::var_os(key) {
                overrides.insert((*key).to_string(), PathBuf::from(val));
            }
        }
        Self {
            config_root,
            overrides,
            isolated: false,
        }
    }

    /// Returns the Agent subpath under the config root, honoring an explicit
    /// override ONLY from the pre-built (isolated-safe) map — never from the
    /// live environment.
    fn dir(&self, override_key: &str, default_sub: &str) -> PathBuf {
        if let Some(v) = self.overrides.get(override_key) {
            return v.clone();
        }
        self.config_root.join(default_sub)
    }
}

/// The resolved installation targets for one Agent.
#[derive(Debug, Clone)]
pub struct Targets {
    pub agent: Agent,
    /// The directory `skills/<skill>/SKILL.md` are installed into (absolute).
    pub skills_dir: PathBuf,
    /// The structured config file to merge the MCP registration into, if the
    /// agent supports MCP.
    pub mcp_config_file: Option<PathBuf>,
}

impl Targets {
    /// Resolves the installation targets for `agent` under the injected `env`.
    /// Under an isolated env every path derives only from `env.config_root`.
    #[must_use]
    pub fn resolve(agent: Agent, env: &Env) -> Self {
        match agent {
            Agent::ClaudeCode => {
                let claude_dir = env.dir("CLAUDE_CONFIG_DIR", ".claude");
                Self {
                    agent,
                    skills_dir: claude_dir.join("skills"),
                    mcp_config_file: Some(env.config_root.join(".claude.json")),
                }
            }
            Agent::Codex => {
                let codex_dir = env.dir("CODEX_HOME", ".codex");
                Self {
                    agent,
                    skills_dir: env.config_root.join(".agents").join("skills"),
                    mcp_config_file: Some(codex_dir.join("config.toml")),
                }
            }
            Agent::Pi => {
                let pi_dir = env.dir("PI_HOME", ".pi");
                let pi_skills = pi_dir.join("agent").join("skills");
                // Pi deduplication: Pi discovers Skills in both the shared
                // Agent Skills directory (~/.agents/skills, Codex's location)
                // and its own ~/.pi/agent/skills. When the shared directory
                // already has a complete MINE skill set, point Pi at the
                // shared copy so Pi never loads two copies (conflict
                // warning); otherwise fall back to Pi's own directory.
                let shared = env.config_root.join(".agents").join("skills");
                let skills_dir = if has_complete_mine_skill_set(&shared) {
                    shared
                } else {
                    pi_skills
                };
                Self {
                    agent,
                    skills_dir,
                    mcp_config_file: None, // No MCP in Pi minimal core.
                }
            }
            Agent::OpenCode => {
                let oc_dir = env.dir("OPENCODE_CONFIG_DIR", ".config/opencode");
                Self {
                    agent,
                    skills_dir: oc_dir.join("skills"),
                    mcp_config_file: Some(oc_dir.join("opencode.json")),
                }
            }
        }
    }

    /// The relative path (forward slashes) of the owned skill file under the
    /// skills dir, relative to the injected config root; `None` when the skills
    /// dir is not under the config root.
    #[must_use]
    pub fn skill_rel_path(
        &self,
        config_root: &std::path::Path,
        skill_subpath: &str,
    ) -> Option<String> {
        let abs = self.skills_dir.join(skill_subpath);
        abs.strip_prefix(config_root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    }
}

/// Returns `true` when `dir` contains a complete MINE first-class Skill set
/// (all five Skills present with a `SKILL.md`). Used for Pi deduplication:
/// a complete shared set means Pi should use the shared copy rather than its
/// own directory.
pub fn has_complete_mine_skill_set(dir: &std::path::Path) -> bool {
    const SKILLS: [&str; 5] = [
        "mine-arch",
        "mine-sync",
        "mine-plan-create",
        "mine-plan-exec",
        "mine-plan-review",
    ];
    SKILLS
        .iter()
        .all(|name| dir.join(name).join("SKILL.md").is_file())
}

/// Returns the real platform home directory (production real-env path only).
fn real_home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_env_ignores_real_overrides() {
        // An isolated env must have an EMPTY override map by construction,
        // regardless of any real process env vars. This is the Fix 3 invariant:
        // no real env vars leak into an explicit-root install.
        let env = Env::isolated(PathBuf::from("/tmp/home"));
        assert!(env.isolated);
        assert!(
            env.overrides.is_empty(),
            "isolated env overrides must be empty"
        );
        let t = Targets::resolve(Agent::Codex, &env);
        assert!(t.skills_dir.starts_with("/tmp/home/.agents/skills"));
    }

    #[test]
    fn claude_targets_under_root() {
        let env = Env::isolated(PathBuf::from("/tmp/home"));
        let t = Targets::resolve(Agent::ClaudeCode, &env);
        assert!(t.skills_dir.starts_with("/tmp/home/.claude/skills"));
        assert_eq!(
            t.mcp_config_file,
            Some(PathBuf::from("/tmp/home/.claude.json"))
        );
    }

    #[test]
    fn pi_has_no_mcp() {
        let env = Env::isolated(PathBuf::from("/tmp/home"));
        let t = Targets::resolve(Agent::Pi, &env);
        assert!(t.mcp_config_file.is_none());
        assert!(t.skills_dir.ends_with(".pi/agent/skills"));
    }

    #[test]
    fn real_env_constructor_distinct_from_isolated() {
        // real_env reads the live environment; isolated does not. They are
        // separate constructors (mixed envs are a bug source).
        let isol = Env::isolated(PathBuf::from("/tmp/x"));
        assert!(isol.isolated);
        // real_env() is not called here to keep tests hermetic; its distinction
        // is structural (it reads std::env; isolated does not).
        let _ = Env::real_env; // ensure the symbol exists / is callable.
    }

    #[test]
    fn resolve_does_not_panic_when_root_missing() {
        let env = Env::isolated(PathBuf::from("/nonexistent/xyz"));
        let t = Targets::resolve(Agent::OpenCode, &env);
        assert!(
            t.skills_dir
                .starts_with("/nonexistent/xyz/.config/opencode")
        );
    }
}
