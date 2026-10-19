#![forbid(unsafe_code)]
//! Verified recoverable update of the current executable; no public target override.
use crate::domain::error::{MineError, MineResult};

pub fn download_and_replace(tag: &str) -> MineResult<super::delivery::Replacement> {
    super::delivery::download_replace(tag)
}

/// Existing uninstall behavior; Unix PATH editing remains user-owned.
pub fn remove_self() -> MineResult<bool> {
    let exe = std::env::current_exe().map_err(|e| MineError::ExternalDependency {
        detail: format!("cannot locate current exe: {e}"),
    })?;
    let bin_dir = exe.parent().map(|p| p.to_path_buf());
    #[cfg(windows)]
    if bin_dir.is_some() {
        eprintln!(
            "mine removed. To clean your user PATH, remove the mine entry via\n  System > Environment Variables > Path (User)."
        );
    }
    let _ = std::fs::remove_file(&exe).or_else(|_| {
        let stale = exe.with_extension("exe.old");
        std::fs::rename(&exe, &stale).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))
    });
    #[cfg(not(windows))]
    if let Some(dir) = bin_dir {
        eprintln!(
            "mine removed PATH entry guidance: remove `{}` from your shell rc if present.",
            dir.display()
        );
    }
    Ok(!exe.exists())
}
