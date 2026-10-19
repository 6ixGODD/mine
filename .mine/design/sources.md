# External Source Register

Verified for this design revision on 2026-08-24. Implementation plans must re-check current official documentation before relying on unstable platform details.

## Approved improvement sources

The 2026-10-09 additions were opened during the ADR audit. Exact source claims,
applicability and implications are linked in each detailed leaf and ADR.

| Area | Opened primary source | Concrete implication |
|---|---|---|
| Git object batching | https://git-scm.com/docs/git-cat-file | Validate requested exact object identities/types in one bounded process. |
| Ref inventory | https://git-scm.com/docs/git-for-each-ref | Reuse structured request-local inventory instead of per-node subprocesses. |
| Semantic model boundaries | https://martinfowler.com/bliki/BoundedContext.html | Declare coherent responsibilities; do not infer safe shared-file writes. |
| Input-keyed evidence | https://bazel.build/remote/caching | Reuse depends on complete inputs/command; undeclared environment prevents reliable reuse. |
| Publication joins | https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds | Mandatory quality failures must block publication dependencies. |
| HTTP limit diagnosis | https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api | Distinguish rate limits from missing release metadata. |
| Release entry points | https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases | Resolve latest once and pin archive/checksum to one release identity. |

## Existing baseline sources

| Area | Primary source | Design implication |
|---|---|---|
| Claude Code marketplaces | https://code.claude.com/docs/en/plugin-marketplaces | Marketplace root, self-contained plugin directory, namespaced Skills, validation commands |
| Claude Code plugins | https://code.claude.com/docs/en/plugins | Standalone versus plugin behavior and Skill namespace |
| Codex plugin creator | https://github.com/openai/codex/blob/main/codex-rs/skills/src/assets/samples/plugin-creator/SKILL.md | Current plugin manifest and personal marketplace scaffold; implementation must validate against installed stable Codex |
| Codex Skills | https://github.com/openai/skills | Agent Skills model and distribution expectations |
| Pi Skills | https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/skills.md | Skill locations, `/skill:name`, package discovery |
| Pi packages | https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/packages.md | Git/npm/local package installation and manifest |
| OpenCode Skills | https://opencode.ai/docs/skills | Native, Claude-compatible, and Agent-compatible discovery roots |
| OpenCode MCP | https://opencode.ai/docs/mcp-servers/ | Local stdio MCP configuration and context-cost warning |
| MCP transport | https://modelcontextprotocol.io/specification/2025-11-25/basic/transports | stdio process and protocol separation |
| MCP Rust SDK | https://github.com/modelcontextprotocol/rust-sdk | Official Rust implementation basis |
| Git linked worktrees | https://git-scm.com/docs/git-worktree | Use porcelain `-z`; linked worktrees isolate HEAD/index; never force branch/path reuse or dirty cleanup |
| Git repository path discovery | https://git-scm.com/docs/git-rev-parse | Resolve current top-level/common storage without mistaking a linked checkout for Root |
| Git commit ancestry | https://git-scm.com/docs/git-merge-base | `--is-ancestor` validates a persisted Plan anchor against its tip; sibling Root/Plan descendants are not ancestors of one another |
| Git merge and recovery | https://git-scm.com/docs/git-merge | Start integration clean; complete or safely abort conflicts before graph acceptance |
| Architecture Decision Records | https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions | Significant decisions are numbered, proposed/accepted, retained, and superseded rather than erased |
| MADR 4.0 | https://adr.github.io/madr/ | ADRs record status, decision makers, alternatives, consequences, and confirmation |
| adr-tools | https://github.com/npryce/adr-tools/blob/master/README.md | Monotonic decision log and linked supersession behavior |
