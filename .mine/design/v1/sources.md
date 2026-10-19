# Research and Implementation Reality — 2026-10-16

## Evidence boundaries

Current core is Rust, TOML graph/config/ADR persistence, CLI/MCP and five Skills.
`src/domain/graph.rs` Plan owner/run and plan_service start are attribution, not
authenticated principals/session leases. Existing operation/checkpoint/receipt
contracts supply recovery concepts to preserve. No present SQLite control plane,
global Project UI or three-provider session supervisor is claimed implemented.
Root graph90 records19/21 IN_PROGRESS and24 BLOCKED; these are current0.2 work,
not prototype1.0 evidence. REQUIREMENTS.md was not found.

Sources were actually opened during bounded main/subagent research2026-10-16.
Current upstream SDKs are moving targets: implementation must pin versions,
generate/validate their actual schemas and repeat adapter conformance probes.
Names below are provider APIs, not new installed MINE command promises.

| Primary source | Applicable version / verified claim | Design consequence |
|---|---|---|
| [Codex SDK / OpenAI](https://learn.chatgpt.com/docs/codex-sdk) | Current docs; TypeScript startThread/resumeThread | Official managed-session route; exact stored native ID |
| [Codex app-server / OpenAI](https://learn.chatgpt.com/docs/app-server) | Current versioned JSON-RPC, model/list capabilities, thread/name/set, turn events | Pin schema; project context and capability discovery; no assumed desktop visibility |
| [Codex configuration / OpenAI](https://learn.chatgpt.com/docs/config-file/config-reference) | service_tier fast maps to request priority; effort and feature/availability are separate | Per-session supported selection, not global default mutation |
| [Claude sessions / Anthropic](https://code.claude.com/docs/en/agent-sdk/sessions) | Current session_id/resume and rename | Persist binding, not most-recent resume |
| [Claude TS reference / Anthropic](https://code.claude.com/docs/en/agent-sdk/typescript.md) | Current Query.supportedModels, selection capabilities; projectConfigRoot requires Code2.1.275+ | Requested/effective model tuple; configuration Root distinct from cwd. Main Web parser rejects Markdown; research lane fetched official text |
| [Claude settings / Anthropic](https://code.claude.com/docs/en/agent-sdk/claude-code-features) | Current settingSources and cwd discovery; managed settings still apply | Explicit project configuration, not global static list or assumed isolation |
| [Claude fast / Anthropic](https://code.claude.com/docs/en/fast-mode) | Current research-preview fast differs from effort; unsupported toggle may change model | Validate tuple and actual state, avoid blind slash toggles; show cost/availability |
| [Claude permissions / Anthropic](https://code.claude.com/docs/en/agent-sdk/permissions) | Allow rules and callbacks have different precedence | MINE authority enforced at service boundary, not callback-only claims |
| [Pi SDK / upstream](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/sdk.md) | Current @earendil-works/pi-coding-agent factory/runtime | Explicit cwd/settings/resources and persistent sessions |
| [Pi RPC / upstream](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/rpc.md) | JSONL correlation, stream backpressure and completion distinction | Drain events; prompt accepted is not work complete |
| [Pi commands / upstream](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/rpc-commands.md) | get_available_models, set_model, get_state | Actual runtime model identity, not hardcoded catalog |
| [Pi RPC types / upstream](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/src/modes/rpc/rpc-types.ts) | Current thinking-level capability methods; no verified universal fast field | Provider-specific capability and unsupported/unknown states |
| [Pi config / upstream](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/configuration.md) | Project cwd and user agentDir resources differ | Verify Root-to-worktree config mapping; no invented root flag |
| [SQLite WAL](https://www.sqlite.org/wal.html) | Local shared-memory constraints, single writer, current WAL-reset fix guidance | Local stores, short writer transactions, pinned patched library |
| [SQLite transaction](https://www.sqlite.org/lang_transaction.html) and [pragmas](https://www.sqlite.org/pragma.html#pragma_synchronous) | Transactions/locking and synchronous durability controls | Bound write queues, verified durability settings |
| [SQLite foreign keys](https://www.sqlite.org/foreignkeys.html) | Referential enforcement must be enabled | Validate connections and migration integrity |
| [SQLite ATTACH](https://www.sqlite.org/lang_attach.html) | Multi-DB WAL commits not collectively atomic | Init/deinit and cross-project messages need durable multi-step reconciliation |
| [SQLite backup](https://www.sqlite.org/backup.html) | Consistent live backup API | No main-db-only copies or Git-tracked live WAL |
| [Temporal workflows](https://docs.temporal.io/workflow-execution) | Persistent history/awaitable recovery model | Compare durable continuation design; no Temporal dependency |
| [RabbitMQ confirms](https://www.rabbitmq.com/docs/confirms) | Publisher/consumer acknowledgments concern different stages | Transport acceptance differs from completed commitment; no broker dependency |
| [shadcn CLI](https://ui.shadcn.com/docs/cli) and [Vite](https://ui.shadcn.com/docs/installation/vite) | Preset-driven local React generation | b2jI retained, pinned generation/visual verification still required |
| [WHATWG SSE](https://html.spec.whatwg.org/multipage/server-sent-events.html) | Event IDs/reconnection | Revisioned replay/reset; not authority from browser cache |
| [OWASP CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html) | Browser mutation protections require explicit controls | Loopback is not sufficient authentication/CSRF isolation |

## Unknowns and required post-approval proofs

- No SDK runtime was installed/upgraded or billable session created by research.
  Exact supported versions, authentication terms and native client inspection
  behavior require a version-pinned three-provider compatibility matrix.
- Claude general HTML TS page fetch exceeded Web size and Markdown parsing failed
  for the main fetcher; official text was opened by the SDK research lane. Do not
  replace it with an unofficial mirror. Project settings remain security-sensitive.
- The initial research could not access the b2jI preview. On 2026-10-16 the
  foundation used the actual pinned shadcn CLI to generate b2jI and inspected
  the resulting local dashboard in a browser; see [foundation](foundation.md)
  and `v1/dashboard/README.md`. This is demonstrator evidence, not production UI acceptance.
- No runtime model list or pricing from the actual user's projects was queried.
  Example model IDs are requirements examples, never a promised available option.
- OS-enforced isolation and native app visibility are not inferred from SDK calls.
  Same-OS-user filesystem access is a declared trust limit.
- UI production packages/schema/API names and provider minimum versions need exact
  post-approval design and tests. This proposal is not execution-ready code.

Alternatives rejected: one global DB duplicating all project state; global static
model dropdown; full-transcript broadcast; repeated LLM polling; elapsed-time-only
writer reassignment; auto-approved ownership/security expansion; independent UI
and CLI state machines; silent1.0 self-bootstrap while0.2 is active.
