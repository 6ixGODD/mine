# MINE Plugin Distribution

This directory (`plugins/mine/`) is a **generated distribution artifact** for
Claude Code and Codex plugin discovery. The `skills/` subdirectory here is a
byte-for-byte copy of the authoritative repository-root `skills/` directory.

## Source of truth

The repository-root `skills/` directory is the **only hand-edited Skill
source** (per `docs/design/integrations/distribution.md`). Never edit files
under `plugins/mine/skills/` directly - edit the root `skills/` and re-run the
synchronization script:

```bash
python scripts/sync-plugin-assets.py        # write (sync)
python scripts/sync-plugin-assets.py --check # verify (detect drift)
```

The sync is deterministic, idempotent, and removes stale MINE-owned generated
files while preserving unrelated content.

## MCP server registration

The `.mcp.json` file registers the MINE stdio MCP server:

```text
mine mcp serve
```

The server resolves the repository root from the current working directory
(the project root where the agent runs), or via the `--repo <path>` global
flag override. This is the approved mechanism for passing the repository
root to the MCP server.

## Version

The plugin version (`0.1.12`) is sourced from the MINE repository version
(`.mine/config.toml` `mine_code_version`), the single version source of truth.
