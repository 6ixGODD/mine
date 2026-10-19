# MINE 用户指南

本指南介绍 MINE 的实际使用方式。

关于 Design、Plan、Execution Graph、Review、worktree、补偿 Plan 和 stable 的设计意图，参见[核心概念](concepts.zh-CN.md)。

---

## 1. 安装

0.2.0 交付 Windows x86_64、Linux x86_64。macOS 交付推迟到 dashboard 阶段，
没有 macOS 0.2.0 产物；这不是原生验证通过。受支持主机上的 Claude Code、Codex、
Pi、OpenCode 四种集成均保留。

### Windows

在 PowerShell 中执行：

```powershell
irm https://raw.githubusercontent.com/6ixGODD/mine/master/scripts/bootstrap.ps1 | iex
```

### Linux x86_64

在终端中执行：

```sh
curl -fsSL https://raw.githubusercontent.com/6ixGODD/mine/master/scripts/bootstrap.sh | sh
```

安装完成后，重新打开终端，验证安装是否成功：

```sh
mine --version
```

然后安装 Agent 集成：

```sh
mine setup
```

查看当前集成状态：

```sh
mine agent status
```

MINE 当前支持 Claude Code、Codex、Pi 和 OpenCode。

对于支持 MCP 的客户端，mine setup 会注册本地 mine mcp serve；当 MCP 不可用时，Skill 会自动回退到 JSON CLI 模式。Pi
的最小核心本身不包含 MCP，因此可以直接使用 Skills + CLI 完成全部操作。

### 更新

`mine update` 会替换二进制**并**用新版本的 embedded payload 刷新所有已安装 Agent 的 Skill，因此升级后无需重新运行
`mine setup`：

```sh
mine update
```

`mine setup` 用于**首次安装**以及添加或移除 Agent（例如 `mine setup --agents claude-code,codex`）。更新后用
`mine --version` 和 `mine agent status` 验证。

### Pi 共享 Skill

Pi 会在共享 Agent Skills 目录（`~/.agents/skills`，即 Codex 的安装位置）和自身目录（`~/.pi/agent/skills`）同时发现
Skill。为避免 Pi 加载两份（以及由此产生的冲突告警），当共享目录已有完整 MINE Skill 集时，MINE 会将 Pi 的 Skill 安装到
共享目录，并移除 `~/.pi/agent/skills` 下的遗留 MINE Skill。

### 安装指定版本

bootstrap 默认安装最新 Release。如需安装指定版本，请使用`MINE_REF`。

例如，安装 `v0.2.0`：

#### Windows

```powershell
$env:MINE_REF = "v0.2.0"
irm https://raw.githubusercontent.com/6ixGODD/mine/master/scripts/bootstrap.ps1 | iex
```

#### Linux x86_64

```sh
MINE_REF=v0.2.0 \
sh -c "$(curl -fsSL https://raw.githubusercontent.com/6ixGODD/mine/master/scripts/bootstrap.sh)"
```

---

## 2. 初始化仓库

在 Git 仓库根目录运行：

```sh
mine init
```

一个仓库通常只需要初始化一次。

`mine init` 会建立 MINE 所需的仓库状态和 `.mine/design/`，但不会替你设计系统或修改业务代码。

用户 `docs/` 不会被搬动。实际路径以 Root 配置为准；仍在执行的旧布局先用现有版本
完成，再进行单独授权、绑定真实源证据的终态切换，不能直接改配置冒充迁移完成。

### ADR 与架构基础

`mine-arch` 先把重大架构决策拆成独立 ADR，提出精确正文与摘要，等待 owner 明确批准，
再进入依赖这些决策的规划。批准后的正文不原地改写，变更通过新决策取代旧决策。
架构师可以在明确分配的普通 Plan 中实现接口、类型、不变量和最小真实消费者；
同样经过 prepare/start、独立审查与接受，没有 Root 写代码或自我接受的例外。

---

## 3. 开始一次工程变更

日常工作主要通过以下五个 Agent Skill 完成：

```text
mine-arch
mine-sync
mine-plan-create
mine-plan-exec
mine-plan-review
```

需要说明的是，这些名称指的是 **Agent Skill**，而非 `mine` CLI 的子命令。不同 Agent 的调用语法存在差异，请以所使用的 Agent
的实际调用方式为准。

以下列举几种常见 Agent 的调用形式：

**Codex：**

```text
$mine-arch ...
```

**Claude Code（通过 Marketplace 安装的插件）：**

```text
/mine:mine-arch ...
```

**Pi：**

```text
/skill:mine-arch ...
```

**其他 Agent：**

```text
/mine-arch ...
```

上述示例均以 `mine-arch` 为代表，其余四个 Skill 的调用方式遵循相同规则，只需将 Skill 名称替换为对应的 `mine-sync`、
`mine-plan-create`、`mine-plan-exec` 或 `mine-plan-review` 即可。

## 4. 创建 Plan

Design 明确后：

```text
mine-plan-create <需要规划的范围>
```

例如：

```text
mine-plan-create 将刚才确定的 Passkey 登录改动整理为执行计划。
```

明确的 scope 会作为本次规划边界。`mine-plan-create` 会围绕这一范围检查相关 Design、代码、测试、依赖和成熟实践，并创建一个或多个可执行
Plan。

如果直接调用：

```text
mine-plan-create
```

而不指定 scope，Skill 会自行从 Design、Execution Graph 和仓库状态中寻找下一批需要规划的工作，因此通常会进行范围更广的探索。

如果 Design 缺少重大决策，交由 `mine-arch` 完成对应 ADR 的 owner 批准后，
再继续依赖它的规划；职责域内不改变契约的文件拆分等局部选择不必反复请示。

查看当前可执行 Plan：

```sh
mine graph ready
```

查看某个 Plan 的详情：

```sh
mine plan show --id <id>
```

---

## 5. 执行和 Review

对处于 READY 状态的 Plan：

```text
mine-plan-exec <Plan ID 或登记路径>
```

执行完成后，Plan 进入：

```text
IMPLEMENTED
```

然后在 **独立的 Agent session** 中运行：

```text
mine-plan-review <Plan ID 或登记路径>
```

**不要**在刚刚执行 `mine-plan-exec` 的同一个上下文中继续 Review。

可以使用：

* 同一种 Agent 的两个独立 session；
* 两种不同 Agent。

例如：

```text
Codex session A
    → mine-plan-exec

Codex session B
    → mine-plan-review
```

或者：

```text
Claude Code
    → mine-plan-exec

Codex
    → mine-plan-review
```

Review 可能产生三种结果：

* **ACCEPTED**：实现被接受并集成；
* **局部修正后 ACCEPTED**：Reviewer 可以直接修复明确、局部且不改变工程契约的问题；
* **REJECTED**：当前 Plan 无法接受，通常会创建补偿 Plan，并更新受影响的后续依赖。

如果 REJECTED 的根因需要修改 Design，Reviewer 可以主动调用 `mine-arch`。用户通常无需手工修改 Execution Graph。

---

## 6. 并行执行

READY 只表示可以尝试执行，不是写权限。原生 wave/start 同时核对所有持有中的
IN_PROGRESS/IMPLEMENTED owner、候选职责域、worktree、可变输出和 pending 操作。
观察修订过期要重新读取；超时不允许另起一个重复执行者。

MINE 会使用独立 branch / worktree 隔离不同 Plan，例如：

```text
dev
├── plan/plan01-api       → worktree A
├── plan/plan02-storage   → worktree B
└── plan/plan03-ui        → worktree C
```

通常不需要手工：

* 创建 Plan branch；
* 创建 worktree；
* 修改 Execution Graph；
* 管理 graph revision；
* 管理 report 路径；
* 将 accepted Plan 手工集成到 `dev`。

这些状态由 MINE 和对应 Skill 自动管理。

工作区开放期间 Root 保持 integration（通常 dev）。Skill 显式携带观察修订、actor
和同一个 prepare operation ID；原生 checkpoint 只提交它自己的 graph pair。
响应不确定时先查询、恢复同一操作，不手工快进图、不重复 merge/cleanup。
审查者合入精确被审 tip，验证集成结果并清理干净、已合并的 worktree/本地分支后
才能接受；拒绝保留原分支，下一个同级补偿复用同一个谱系 worktree。

写域以职责/命名空间、排除项与物理映射为边界，不是穷举每个内部文件。
各种语言的可变 build/test/generated/coverage/package 输出使用
`<Root>/.mine/build/<精确PlanID>/<输出类别>/`；Rust 设置对应 CARGO_TARGET_DIR。
环境、下载与安全缓存复用 Root，不能在 worktree 再建 venv、target、node_modules。
测得磁盘代价异常时先与 owner 协商序列化/共享/清理边界，不静默共享并行可变产物。

重跑昂贵检查前，按当前源码、正文、policy、工具、环境、平台、features 与产物身份
检查既有证据是否适用，区分 RUN/REUSED/INVALIDATED/NON_REUSABLE/NOT_RUN。
capture/record/check 不代跑测试；exit0 也可能表示失效。独立、原生和变更输入的
检查不能被旧收据替代，首次失败与缺失证据必须保留。

---

## 7. 发布

所有计划工作完成后，先执行 Final Sync：

```text
mine-sync prepare this repository for stable release
```

它会根据最终代码重新核对 `.mine/design/`。

然后在独立 Review 上下文中执行：

```text
mine-plan-review complete release closure
```

Release Closure 会完成本地发布收口，包括：

* 验证 Execution Graph 和 release gates；
* 确认最终 Design 已同步；
* 构造并验证 stable candidate；
* 检查产品代码中是否残留当前开发周期的临时 Plan 引用；
* 移除 `.mine/plan/` 和执行报告；
* 将最终状态集成到 stable；
* 清理 MINE 管理的本地临时 branch / worktree。

MINE **不会**主动执行：

* push；
* Git tag；
* GitHub Release；
* package publish；
* 其他远程发布操作。

这些操作由用户决定。

---

## 8. 查看状态和诊断

### Agent 集成

```sh
mine agent status
mine setup
```

### 当前仓库

```sh
mine status
mine doctor
```

### Design

```sh
mine design status
mine design validate
```

### Execution Graph

```sh
mine graph status
mine graph ready
mine graph wave
mine graph validate
```

### Plan

```sh
mine plan show --id <id>
```

### 发布检查

```sh
mine release --format json
```

当命令执行失败时，优先查看 CLI 返回的 human 或 JSON 格式的诊断信息。

关于各机制的设计意图，参见[核心概念](concepts.zh-CN.md)。
