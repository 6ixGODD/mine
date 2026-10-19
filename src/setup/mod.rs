#![forbid(unsafe_code)]

//! `mine setup` / `mine update` / `mine uninstall` — the end-user installation
//! lifecycle.
//!
//! These commands are the user-facing surface that the remote bootstrap scripts
//! delegate to. They own:
//!
//! - the MINE ASCII banner;
//! - latest-release version comparison against GitHub Releases;
//! - detection of installed coding agents (Claude Code, Codex, Pi, OpenCode)
//!   by requiring BOTH the agent CLI on PATH AND its config directory;
//! - an interactive TUI selector (crossterm) for choosing which detected
//!   agents to install/uninstall MINE into, with a non-TTY fallback that
//!   installs into every detected agent;
//! - delegation to the existing transactional `agent_service::install` /
//!   `agent_service::uninstall` for the actual MCP + Skill wiring;
//! - self-update (`mine update`) and full local removal (`mine uninstall`).
//!
//! Design notes:
//! - `mine setup` is the single entry point the bootstrap downloads `mine` for
//!   and then runs. It never assumes a repository context (it is invoked
//!   before `mine init`); it operates on the user machine, not a repo.
//! - Network access is best-effort and reported honestly: if the latest
//!   release cannot be resolved, version comparison is skipped with a clear
//!   note rather than fabricated.
//! - The TUI is gated on stdin being a TTY. When it is not (CI, `curl|sh`
//!   pipes), the selector is skipped and the `--agents` flag (or "all
//!   detected") drives installation non-interactively.

pub mod agent_detect;
pub mod banner;
pub mod delivery;
pub mod platform;
pub mod release_meta;
pub mod selector;
pub mod self_update;

use crate::application::agent_service;
use crate::domain::error::{MineError, MineResult};
use std::io::IsTerminal;
use std::path::PathBuf;

/// Which agents to act on, resolved from CLI flags or the TUI.
#[derive(Debug, Clone)]
pub struct SetupPlan {
    /// Agents to install MINE into.
    pub install: Vec<String>,
    /// Agents to uninstall MINE from (were installed, user deselected).
    pub uninstall: Vec<String>,
    /// True when the user cancelled the interactive selector (Esc/Ctrl+C/q).
    pub cancelled: bool,
}

/// Runs `mine setup`.
///
/// Steps:
/// 1. Print the banner.
/// 2. Resolve the latest published release tag; if the running binary is
///    older and stdin is a TTY, prompt to update first.
/// 3. Detect installed agents.
/// 4. Resolve the install/uninstall plan (TUI if TTY, else `--agents` or all
///    detected).
/// 5. Execute: install for `install`, uninstall for `uninstall`.
pub fn run_setup(args: &SetupArgs) -> MineResult<SetupReport> {
    let current = env!("CARGO_PKG_VERSION");

    // Version check against latest release (best-effort).
    let latest = release_meta::latest_tag();
    let version_note = match &latest {
        Ok(tag) if release_meta::is_newer(tag, current) => {
            format!("mine {} is available (you are running {})", tag, current)
        }
        Ok(tag) => format!("mine {} is up to date (latest release: {})", current, tag),
        Err(e) => format!("could not check for updates: {e}"),
    };

    // Detect installed agents.
    let detections = agent_detect::detect_all();

    // In an interactive (TTY) session, the selector renders the banner,
    // version note, and detection summary inside the alternate screen and
    // restores the original screen on exit. In a non-TTY session (CI,
    // curl|sh pipes), print them inline.
    let interactive = std::io::stdin().is_terminal();
    if !interactive {
        banner::print();
        println!("{version_note}");
        println!();
        agent_detect::print_summary(&detections);
        println!();
    }

    // Build the environment once: isolated when --config-root is given
    // (CI/tests), otherwise the real-env (reads live process env + home).
    let env = match &args.config_root {
        Some(p) => agent_service::isolated_env(p.clone()),
        None => agent_service::real_env(),
    };

    // Resolve the plan.
    let plan = if let Some(list) = &args.agents {
        // Explicit --agents a,b,c (non-interactive override).
        let requested: Vec<String> = list.split(',').map(|s| s.trim().to_string()).collect();
        // Validate slugs; ignore unknowns with a warning.
        let mut install = Vec::new();
        for slug in &requested {
            if agent_detect::slug_is_supported(slug) {
                install.push(slug.clone());
            } else {
                eprintln!("warning: unknown agent {slug:?}, skipping");
            }
        }
        SetupPlan {
            install,
            uninstall: Vec::new(),
            cancelled: false,
        }
    } else {
        selector::resolve_plan(&detections, &version_note, args.yes, &env)?
    };

    // Execute.
    let mut installed = Vec::new();
    let mut uninstalled = Vec::new();
    let mut errors = Vec::new();

    for slug in &plan.install {
        // When the plan came from an explicit --agents list (non-interactive),
        // honor it even if the agent is not detected on this machine (CI
        // installs into an isolated config root without the agent CLI). The
        // TUI/non-TTY fallback path only includes detected agents, so this
        // filter only relaxes the explicit-list case.
        if args.agents.is_none() {
            let det = detections.iter().find(|d| d.slug == *slug);
            if let Some(d) = det {
                if !d.detected {
                    eprintln!("skipping {slug}: agent not detected on this machine");
                    continue;
                }
            }
        }
        match agent_service::install(
            slug,
            &env,
            current,
            false,
            crate::agent_setup::install::FailPhase::None,
        ) {
            Ok(_) => installed.push(slug.clone()),
            Err(e) => errors.push(format!("install {slug}: {e}")),
        }
    }
    for slug in &plan.uninstall {
        match agent_service::uninstall(slug, &env, false) {
            Ok(_) => uninstalled.push(slug.clone()),
            Err(e) => errors.push(format!("uninstall {slug}: {e}")),
        }
    }

    Ok(SetupReport {
        version_note,
        detected: detections,
        installed,
        uninstalled,
        errors,
        cancelled: plan.cancelled,
    })
}

/// Runs `mine update`: compare to latest, optionally download+replace, then
/// refresh installed Agent Skills from the new binary's embedded payload.
pub fn run_update(args: &UpdateArgs) -> MineResult<UpdateReport> {
    let current = env!("CARGO_PKG_VERSION");
    let latest = release_meta::latest_tag();
    match latest {
        Ok(tag) if release_meta::is_newer(&tag, current) => {
            println!("mine {} is available (you are running {}).", tag, current);
            if !args.yes {
                print!("Update now? [y/N] ");
                use std::io::Write;
                std::io::stdout().flush().ok();
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).ok();
                if !line.trim().eq_ignore_ascii_case("y") {
                    return Ok(UpdateReport {
                        from: current.to_string(),
                        to: tag,
                        updated: false,
                        note: "update declined".to_string(),
                        skills_refreshed: Vec::new(),
                        skills_errors: Vec::new(),
                    });
                }
            }
            let to = tag.clone();
            let replacement = self_update::download_and_replace(&to)?;
            // The on-disk binary is now the NEW version. Re-run it in
            // refresh-only mode so installed Agent Skills are rewritten from
            // the new binary's embedded payload (this process still runs the
            // OLD binary, whose embedded payload is stale).
            let (skills_refreshed, skills_errors) =
                refresh_installed_after_update(args, &replacement.destination);
            let note = update_note(replacement.cleanup_pending.as_deref(), &skills_errors);
            Ok(UpdateReport {
                from: current.to_string(),
                to,
                updated: true,
                note,
                skills_refreshed,
                skills_errors,
            })
        }
        Ok(tag) => Ok(UpdateReport {
            from: current.to_string(),
            to: tag,
            updated: false,
            note: "already up to date".to_string(),
            skills_refreshed: Vec::new(),
            skills_errors: Vec::new(),
        }),
        Err(e) => Err(MineError::ExternalDependency {
            detail: format!("could not resolve latest release: {e}"),
        }),
    }
}

/// Spawns the (newly replaced) binary with the internal `__refresh-skills`
/// entry point, waits for it, and returns (refreshed slugs, per-Agent errors).
/// A spawn/parse failure is reported as a refresh error, never as an update
/// failure: the binary update itself already succeeded.
fn update_note(pending: Option<&str>, errors: &[String]) -> String {
    let mut note = if errors.is_empty() {
        "binary updated; Agent refresh completed"
    } else {
        "binary updated; Agent refresh incomplete (see skills_errors)"
    }
    .to_string();
    if let Some(pending) = pending {
        note.push_str("; ");
        note.push_str(pending);
    }
    note
}

fn refresh_installed_after_update(
    args: &UpdateArgs,
    exe: &std::path::Path,
) -> (Vec<String>, Vec<String>) {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--format").arg("json").arg("__refresh-skills");
    if let Some(root) = &args.config_root {
        cmd.arg("--config-root").arg(root);
    }
    let out = match bounded_refresh_output(&mut cmd) {
        Ok(o) => o,
        Err(e) => return (Vec::new(), vec![format!("refresh failed: {e}")]),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let report = parse_refresh_report(stdout.trim());
    match report {
        Some(mut r) => {
            if !out.status.success() {
                r.errors
                    .push(format!("refresh process failed: {}", out.status));
            }
            (r.refreshed, r.errors)
        }
        None => (
            Vec::new(),
            vec![format!(
                "refresh report unparseable (exit {}): {}",
                out.status,
                stdout.trim()
            )],
        ),
    }
}

/// A verified child can still fail/hang. Bound memory and wall time separately
/// from binary replacement, and never let its failure erase replacement truth.
fn bounded_refresh_output(cmd: &mut std::process::Command) -> Result<std::process::Output, String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::{Duration, Instant};
    const LIMIT: usize = 64 * 1024;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    for (index, mut pipe) in [
        Box::new(child.stdout.take().ok_or("missing stdout")?) as Box<dyn Read + Send>,
        Box::new(child.stderr.take().ok_or("missing stderr")?),
    ]
    .into_iter()
    .enumerate()
    {
        let tx = tx.clone();
        let exceeded = exceeded.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = pipe.by_ref().take(LIMIT as u64 + 1).read_to_end(&mut bytes);
            if bytes.len() > LIMIT {
                exceeded.store(true, Ordering::Relaxed);
                bytes.truncate(LIMIT);
            }
            let _ = tx.send((index, result.map(|_| bytes)));
        });
    }
    drop(tx);
    let start = Instant::now();
    let mut captured = [None, None];
    let result = (|| {
        loop {
            while let Ok((index, result)) = rx.try_recv() {
                captured[index] = Some(result.map_err(|e| e.to_string())?);
            }
            if exceeded.load(Ordering::Relaxed) {
                return Err("output exceeded 64 KiB per stream".into());
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                if let [Some(stdout), Some(stderr)] = &mut captured {
                    return Ok(std::process::Output {
                        status,
                        stdout: std::mem::take(stdout),
                        stderr: std::mem::take(stderr),
                    });
                }
            }
            if start.elapsed() >= Duration::from_secs(30) {
                return Err("timed out after 30 seconds".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if result.is_err() && child.try_wait().map_err(|e| e.to_string())?.is_none() {
        child.kill().map_err(|e| e.to_string())?;
        child.wait().map_err(|e| e.to_string())?;
    }
    result
}

/// Runs `mine uninstall`: remove MINE from every agent, then remove the
/// binary and PATH entries.
pub fn run_uninstall(args: &UninstallArgs) -> MineResult<UninstallReport> {
    if !args.yes {
        println!("This will remove MINE from every coding agent and delete the");
        println!("`mine` binary and its PATH entries. This cannot be undone.");
        print!("Proceed? [y/N] ");
        use std::io::Write;
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok();
        if !line.trim().eq_ignore_ascii_case("y") {
            return Ok(UninstallReport {
                agents_cleared: Vec::new(),
                binary_removed: false,
                note: "uninstall declined".to_string(),
            });
        }
    }

    let env = agent_service::real_env();
    let state = crate::agent_setup::managed_state::ManagedState::load(&env.config_root)
        .unwrap_or_else(|_| crate::agent_setup::managed_state::ManagedState::new());
    let mut cleared = Vec::new();
    for agent in crate::agent_setup::targets::Agent::ALL {
        if state.record(agent.slug()).is_some() {
            match agent_service::uninstall(agent.slug(), &env, false) {
                Ok(_) => cleared.push(agent.slug().to_string()),
                Err(e) => eprintln!("warning: uninstall {} failed: {}", agent.slug(), e),
            }
        }
    }
    let binary_removed = self_update::remove_self()?;
    Ok(UninstallReport {
        agents_cleared: cleared,
        binary_removed,
        note: "mine removed".to_string(),
    })
}

/// CLI flags for `mine setup`.
#[derive(Debug, Clone, Default)]
pub struct SetupArgs {
    /// `--agents claude,codex` non-interactive override.
    pub agents: Option<String>,
    /// `--yes` / `-y`: skip interactive prompts (version update prompt,
    /// non-TTY selector fallback uses all detected).
    pub yes: bool,
    /// `--config-root <path>`: isolated config root (CI/tests). When set,
    /// setup uses [`crate::agent_setup::targets::Env::isolated`] and never touches the real environment.
    pub config_root: Option<PathBuf>,
}

/// CLI flags for `mine update`.
#[derive(Debug, Clone, Default)]
pub struct UpdateArgs {
    pub yes: bool,
    /// Explicit isolated config root (CI/tests). When set, the refresh child
    /// process also receives it so real HOME is never touched.
    pub config_root: Option<std::path::PathBuf>,
}

/// CLI flags for `mine uninstall`.
#[derive(Debug, Clone, Default)]
pub struct UninstallArgs {
    pub yes: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SetupReport {
    pub version_note: String,
    pub detected: Vec<agent_detect::Detection>,
    pub installed: Vec<String>,
    pub uninstalled: Vec<String>,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateReport {
    pub from: String,
    pub to: String,
    pub updated: bool,
    pub note: String,
    /// Agent slugs whose Skills were refreshed by the new binary.
    pub skills_refreshed: Vec<String>,
    /// Per-Agent Skill refresh errors.
    pub skills_errors: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UninstallReport {
    pub agents_cleared: Vec<String>,
    pub binary_removed: bool,
    pub note: String,
}

// Re-export Env for submodules.
pub use crate::agent_setup::targets::Env as SetupEnv;

/// Parses the refresh child's JSON output into a `RefreshReport`. The child
/// prints a full envelope (`{"command":"__refresh-skills","data":{...},
/// "ok":true}`), so the report lives under `data`; a bare `RefreshReport`
/// object is also accepted for robustness.
fn parse_refresh_report(stdout: &str) -> Option<crate::application::agent_service::RefreshReport> {
    let value: serde_json::Value = serde_json::from_str(stdout).ok()?;
    if value.get("ok").is_some_and(|ok| ok != true) {
        return None;
    }
    let data = value.get("data").cloned().unwrap_or(value);
    serde_json::from_value(data).ok()
}

#[cfg(test)]
mod tests {
    use super::parse_refresh_report;

    #[test]
    fn parses_envelope_with_data() {
        let out = r#"{"command":"__refresh-skills","data":{"mine_version":"0.1.6","refreshed":["claude-code","pi"],"errors":[]},"ok":true}"#;
        let r = parse_refresh_report(out).expect("envelope must parse");
        assert_eq!(r.mine_version, "0.1.6");
        assert_eq!(
            r.refreshed,
            vec!["claude-code".to_string(), "pi".to_string()]
        );
        assert!(r.errors.is_empty());
    }

    #[test]
    fn parses_bare_report() {
        let out = r#"{"mine_version":"0.1.6","refreshed":[],"errors":["x"]}"#;
        let r = parse_refresh_report(out).expect("bare report must parse");
        assert_eq!(r.errors, vec!["x".to_string()]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_refresh_report("not json").is_none());
        assert!(
            parse_refresh_report(
                r#"{"ok":false,"data":{"mine_version":"0.1.13","refreshed":[],"errors":[]}}"#
            )
            .is_none()
        );
    }

    #[test]
    fn partial_update_never_claims_complete() {
        assert!(super::update_note(None, &["failed".into()]).contains("incomplete"));
        assert!(super::update_note(Some("cleanup pending"), &[]).contains("cleanup pending"));
    }

    #[test]
    fn refresh_output_is_bounded_and_nonzero_status_is_preserved() {
        #[cfg(windows)]
        let mut command = {
            let mut c = std::process::Command::new("powershell.exe");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::Write('x'*70000)",
            ]);
            c
        };
        #[cfg(unix)]
        let mut command = {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", "printf '%70000s' x"]);
            c
        };
        assert!(
            super::bounded_refresh_output(&mut command)
                .unwrap_err()
                .contains("64 KiB")
        );
        #[cfg(windows)]
        let mut command = {
            let mut c = std::process::Command::new("cmd.exe");
            c.args(["/C", "exit 7"]);
            c
        };
        #[cfg(unix)]
        let mut command = {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", "exit 7"]);
            c
        };
        assert_eq!(
            super::bounded_refresh_output(&mut command)
                .unwrap()
                .status
                .code(),
            Some(7)
        );
    }
}
