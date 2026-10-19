# Public Interface Design Index

MINE exposes two adapters over the same application services.

- [CLI contract](cli-contract.md)
- [CLI Plan lifecycle details](cli-plan-lifecycle.md)
- [MCP contract](mcp-contract.md)

Skills prefer MCP when the expected tools are available. They fall back to CLI only with `--format json`. Human-readable CLI output is never parsed by Skills.
