# Configuration, Security, and Observability

## Project configuration

`.mine/config.toml` is long-lived and may exist on the stable branch.

```toml
schema_version = 1
repository_id = "<uuid>"
mine_code_version = "0.1.0"

[branches]
stable = "master"
integration = "dev"

[design]
root = "docs/design/index.md"
marker = "docs/design/.mine-design.toml"
language = "en"
index_soft_limit_lines = 250
leaf_soft_limit_lines = 400

[plan]
root = "docs/plan"
worktrees_root = ".mine/worktrees"
shared_environment_root = "."
ephemeral = true
purge_before_stable_release = true

[graph]
source = "docs/plan/execution-graph.toml"
rendered = "docs/plan/execution-graph.md"
lock_timeout_ms = 5000

[adr]
registry = "docs/design/decisions/adr-registry.toml"
rendered_index = "docs/design/decisions/index.md"
lock_timeout_ms = 5000
```

`mine init` detects the real stable branch; `master` is only this repository's current example. `worktrees_root` and `shared_environment_root` are repository-relative, normalized, and containment-validated; they default to `.mine/worktrees` and `.` respectively. Root-managed mutable outputs derive beneath `.mine/build/<plan-id>/` from the exact Plan node, while reusable toolchains, dependency downloads, virtual environments, and safe content-addressed caches remain shared. Compensation nodes reuse their lineage worktree but not a rejected node's mutable build/test namespace. Machine-local absolute paths and secrets never belong in project configuration.

## User-level state

Managed install state records binary/Skill versions and hashes, owned harness
entries, external-configuration backups, and last doctor result. The agent
installer's transactional model (preflight / staging / commit / rollback /
recovery with a durable pending-transaction record, mandatory exact-byte
configuration backup before mutation, and explicit `--config-root` isolation
that never honors real process environment overrides) is specified in
[`docs/design/integrations/distribution.md`](../integrations/distribution.md#mine-agent-install).
The pending-transaction record is local recovery material (under the injected
or real configuration root) and never contains secrets.

## Logging

- Human diagnostics go to stderr when stdout is structured output.
- MCP stdout is protocol-only.
- `.mine/runtime/sync/` contains local agent-authored synchronization reports,
  not a fact source. The current executable does not persist a general event
  log.
- Sensitive values and unrelated file contents are redacted.

## Security controls

- no shell string interpolation;
- no arbitrary MCP shell or filesystem tools;
- no writes outside validated repository or managed user directories;
- design ownership marker before rewrite;
- verified local design backup before synchronization;
- no broad deletion commands;
- bounded managed-branch Git actions only;
- linked worktrees only beneath the validated configured root, with no forced
  add/remove/prune and no Root branch switching during unreleased work;
- backups before external configuration changes (exact-byte, verified,
  recorded in the installation transaction; no mutation if backup fails);
- exact ownership for uninstall (never inferred from a filename);
- explicit `--config-root` isolation: real process environment overrides
  (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `PI_HOME`, `OPENCODE_CONFIG_DIR`) are not
  honored when an explicit configuration root is supplied;
- dependency audit and release checksums.

## Destructive-operation policy

Permitted destructive actions are narrowly defined:

- rewrite/delete MINE-managed design after successful local backup;
- purge ownership-marked `docs/plan/` after release gates;
- delete accepted and merged local MINE-managed temporary branches.

Everything else defaults to refusal.
