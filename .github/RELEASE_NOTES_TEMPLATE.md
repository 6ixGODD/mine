## Install

One command (no Rust toolchain required):

- **Windows x86_64 (PowerShell):** `irm https://raw.githubusercontent.com/6ixGODD/mine/master/scripts/bootstrap.ps1 | iex`
- **Linux x86_64:** `curl -fsSL https://raw.githubusercontent.com/6ixGODD/mine/master/scripts/bootstrap.sh | sh`

macOS delivery is owner-DEFERRED until the dashboard phase. There is no macOS
0.2.0 archive; preserved source/history is not a native verification pass.

This downloads the prebuilt `mine` binary for your platform, puts it on PATH,
and runs `mine setup`, which detects installed coding agents and installs MINE
(Skills + MCP server) into the ones you choose.

## Upgrade

If `mine` is already installed: `mine update`.

## Assets

For 0.2.0 only, owner-approved local equivalent verification replaces hosted
execution while retaining every quality and exact native-delivery obligation.
GitHub workflows: NOT_RUN, not PASS. Publication requires the independently
reviewed local evidence join; metadata or fixture success alone is insufficient.

Each platform archive ships `mine` (or `mine.exe`) plus README and LICENSE,
with an adjacent `.sha256` sidecar naming that archive. The complete release
also includes `SHA256SUMS.txt`. Bootstrap and `mine update` verify checksums
before installation. Checksums provide transfer integrity, not independent
publisher authentication or reproducible-build proof.

Supported archives: `mine-linux-x86_64.tar.gz`, `mine-windows-x86_64.zip`.
Intel macOS is unsupported; ARM64 macOS is deferred for this release.
