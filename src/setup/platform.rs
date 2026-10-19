#![forbid(unsafe_code)]
//! Generated release identities; only the marked table is generator-owned.
use crate::domain::error::{MineError, MineResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: &'static str,
    pub arch: &'static str,
    pub target: &'static str,
    pub archive: &'static str,
    pub binary: &'static str,
    pub runner: &'static str,
}

// BEGIN MINE PLATFORMS
pub const PLATFORMS: &[Platform] = &[
    Platform {
        os: "linux",
        arch: "x86_64",
        target: "x86_64-unknown-linux-gnu",
        archive: "mine-linux-x86_64.tar.gz",
        binary: "mine",
        runner: "ubuntu-latest",
    },
    Platform {
        os: "windows",
        arch: "x86_64",
        target: "x86_64-pc-windows-msvc",
        archive: "mine-windows-x86_64.zip",
        binary: "mine.exe",
        runner: "windows-latest",
    },
];
// END MINE PLATFORMS

pub fn lookup(os: &str, arch: &str) -> MineResult<Platform> {
    PLATFORMS
        .iter()
        .find(|p| p.os == os && p.arch == arch)
        .copied()
        .ok_or_else(|| MineError::ExternalDependency {
            detail: format!("unsupported platform: {os}/{arch}"),
        })
}

pub fn current() -> MineResult<Platform> {
    lookup(std::env::consts::OS, std::env::consts::ARCH)
}
