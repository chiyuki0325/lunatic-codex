# `/context` 实施计划

`README.md` 是需求、非目标和验收条件的权威说明；本文只记录实现批次、数据契约、并发规则、测试矩阵和验证流程。

本仓库的文档、代码、测试、快照和提交信息不得包含外部实现或调查来源信息。除非用户另行要求，不自动创建 Git commit。

## 批次一：Core 请求级上下文用量

### 数据模型

新增 `codex-rs/core/src/context_usage.rs` 及独立测试文件，定义内部只读模型：

- `ContextUsageCategoryKind`：系统提示词、内置工具、MCP 工具、指令、Skills、消息、其他、未归因。
- `ContextUsageDetail`：只保存名称、路径、加载状态和估算 Token，不保存提示词正文、工具 schema 或历史消息。
- `ContextUsageRequestSnapshot`：保存唯一快照 ID、请求序号、生成时间、模型名、模型窗口、自动压缩阈值、可用预留区、互斥分类、MCP/指令/Skills 明细、分类估算总量、数据完整度和请求配置版本。
- `ContextUsageActualSource`：区分当前快照 API 实际值、上一份已完成请求的实际值、首次响应前的本地估算。
- `ContextUsageStore`：以 `ArcSwapOption` 或等价方式原子发布不可变最新快照；只保存最近完成 API 用量对应的快照 ID，Token 数值继续读取现有 `TokenUsageInfo.last_token_usage.total_tokens`，不建立第二套累计器。

`codex-rs/core/src/lib.rs` 只导出 app-server 所需的最小只读类型和访问入口。

### 最终请求边界

扩展 `codex-rs/core/src/client_common.rs` 的 `Prompt`，加入不参与模型请求序列化的统计 sidecar：

- sidecar 与最终 `Prompt.input` 索引一一对应，记录互斥分类及可公开来源标识。
- 工具来源由注册和计划阶段明确记录，不通过工具名推断内置或 MCP。
- sidecar 不修改 `ResponseItem`、rollout 或模型可见内容。

调整 `codex-rs/core/src/session/turn.rs`：

- 在 `build_prompt` 附近基于最终 `ContextManager::for_prompt(...)` 构造 sidecar。
- 复用 `estimate_item_token_count(&ResponseItem)` 统计消息、工具结果、图片和压缩摘要。
- 使用现有结构化 fragment 区分 Skills 与上下文指令；未知 fragment 归入“其他”。
- 用 `StepContext.loaded_agents_md` 记录当前适用指令文件路径和独立估算明细。
- Skill 元数据或正文只有实际进入最终 prompt 时才计入；仅可用但未注入的不计入。
- 每次重试或 follow-up 生成新的不可变请求快照。

调整 `codex-rs/core/src/tools/router.rs` 及工具计划组装：

- `ToolRouter` 在生成 `model_visible_specs()` 时保留等长来源描述。
- 内置与 MCP 来源在注册阶段明确写入。
- deferred、hidden、code-mode-only 等未暴露工具不计入窗口；MCP 明细可显示“可用”或“延迟加载”，Token 为 0。
- 不改变工具暴露策略和发送顺序。

调整 `codex-rs/core/src/client.rs`：

- Responses API、Responses Lite 和已有其他 wire request 完成 provider-specific 转换并清理最终输入 ID 后发布快照。
- Responses Lite 的 instructions 和 additional-tools 分别计入系统提示词和工具，避免作为普通消息重复计算。
- 工具总量按最终完整序列化 payload 估算；逐工具估算用于分类和明细，包装差额计入“其他”。
- output schema 等其他模型可见请求内容计入“其他”。
- 在现有请求构造中发布快照，避免为 `/context` 再序列化完整请求。
- API usage 到达时把现有 `TokenUsageInfo` 与产生该响应的快照 ID 关联，不假定最新快照就是该响应对应请求。

### 首次响应、启动和设置变化

复用 `codex-rs/core/src/session_startup_prewarm.rs`：

- 在启动预热已构造 `StepContext`、工具路由和 `Prompt` 的位置发布首次估算快照。
- 首次 API usage 前明确标识“估算”，不得伪造 0 Token 实际值。
- 启动快照不完整时携带完整度状态，只展示可靠分类。

提供活动中和空闲查询入口：

- `codex-rs/core/src/session/mod.rs` 在 `capture_step_context` 完成时缓存最新不可变 `StepContext` 视图。
- active turn 已生成 wire request 时优先返回请求快照；尚未生成时从已捕获配置和克隆的模型可见历史在锁外生成估算。
- 空闲且模型或线程设置版本未变化时复用最新请求视图。
- `/model` 等设置变化后通过 cached-only 预览读取当前模型配置、环境就绪状态、缓存 MCP binding、指令和 Skills；不得触发 MCP 重连、文件发现、工具调用或模型请求。
- 将刷新动态依赖与从现有 snapshot 组装 `StepContext` 分离；普通 turn 保持刷新，`/context` 只能走无副作用路径。
- 缓存不足时返回结构化 unavailable/partial，不得 panic。

在 `codex-rs/core/src/session/context_window.rs` 复用模型窗口和自动压缩阈值：

- 固定预留量为模型窗口减去实际自动压缩阈值。
- 实际用量侵入预留区时，可见预留裁剪到 `window - actual`，剩余空间最小为 0。
- 自动压缩关闭、窗口未知或无固定阈值时不制造预留值。

在 `codex-rs/core/src/codex_thread.rs` 增加窄的公开异步 accessor，只返回组合后的结构化 snapshot。

## 批次二：app-server v2 只读接口

新增 `codex-rs/app-server-protocol/src/protocol/v2/context_usage.rs`：

- `ThreadContextUsageParams { thread_id }`。
- `ThreadContextUsageResponse` 及 snapshot、分类、明细、实际值来源、完整度 DTO。
- wire 字段和字符串枚举使用 camelCase。
- v2 类型使用 `#[ts(export_to = "v2/")]`。
- 请求可选字段若存在则使用 `#[ts(optional = nullable)]`；响应字段不使用 `skip_serializing_if`。
- 只返回计数和标识，不返回 prompt 文本、工具 schema 或 `ResponseItem`。

修改 `codex-rs/app-server-protocol/src/protocol/common.rs`：

- 注册实验性 `thread/contextUsage`。
- 使用 `#[experimental("thread/contextUsage")]`。
- 资源名保持单数 `thread`。

app-server 使用独立 processor：

- 新增 `codex-rs/app-server/src/request_processors/context_usage_processor.rs`。
- 从 live thread manager 获取 `CodexThread`，调用只读 accessor 并映射 DTO。
- 未加载、已关闭或不可统计线程返回明确 JSON-RPC 错误或结构化 unavailable。
- 不从 rollout 重建，不启动或恢复线程，不进入 thread serialization queue，不生成 thread item。
- `codex-rs/app-server/src/message_processor.rs` 只增加薄路由。
- 更新 `codex-rs/app-server/README.md` 和生成的 schema fixtures。

## 批次三：TUI 命令、并发和渲染

### 命令与异步请求

修改 `codex-rs/tui/src/slash_command.rs`：

- 增加 `SlashCommand::Context` 和 `/context`。
- 中文描述固定为 `查看当前上下文窗口占用`。
- `available_during_task()` 返回 true。
- 启动及 MCP 初始化期间同样可执行。

修改 `codex-rs/tui/src/chatwidget/slash_dispatch.rs`：

- 增加薄 dispatch，只发起本地异步查询。
- 不提交 user turn，不 steer，不向 rollout 或模型历史加入 `/context`。

新增 `codex-rs/tui/src/app/context_usage.rs`：

- 通过 `AppServerRequestHandle::request_typed` 调用 `thread/contextUsage`。
- 每次调用生成独立本地 request ID，并携带目标 thread ID。
- 尚无 thread ID 时保留请求，`AppServerStartedThread` 到达后立即发送。
- 后台完成只发送 TUI `AppEvent`，不直接修改 widget。
- RPC 失败映射为简体中文局部错误。

修改 `codex-rs/tui/src/app_event.rs` 及 app event dispatch：

- 增加请求和完成事件。
- 响应携带 request ID、目标 thread ID 和结果。
- 线程切换后的迟到结果不得插入新线程。

新增 `codex-rs/tui/src/chatwidget/context_usage.rs`：

- 用 `VecDeque` 保存多次调用，不覆盖上一项。
- 调用后立即显示 transient loading cell：`正在统计上下文用量…`。
- 完成后替换为不可变成功或错误 cell。
- 乱序响应可先完成显示，但只有队首连续完成项按调用顺序进入 history。
- active streaming cell 阻止 history 插入时保留 transient card，不清空或改写 transcript。
- reset、resume、fork、切换线程时清理或隔离旧线程 pending 状态。
- 失败显示 `暂时无法获取上下文用量`，composer 保持可输入。

### 中文网格与明细

新增 `codex-rs/tui/src/history_cell/context_usage.rs` 及独立测试文件：

- 标题 `上下文用量`。
- 顶部显示模型、实际或估算 Token、窗口、已用和剩余百分比。
- 分类标题 `各分类估算用量`。
- MCP、指令文件、Skills 明细只在非空时显示。
- 新增 UI 全部使用简体中文；`/context`、模型名、路径、工具名、协议标识和 `Token` 保持原文。
- 复用 `status::format_tokens_compact`、现有 wrapping helpers、`textwrap::wrap` 和 Unicode display width。
- 使用 Ratatui `Stylize`，不硬编码白色。

网格分配器必须是纯函数并满足：

1. 默认严格为 `10 × 10 = 100` 格，每格表示窗口 1%。
2. 有 API 实际值时用实际值确定已用区域，否则用分类估算总量。
3. 分类估算低于 API 实际值时增加“未归因”。
4. 分类估算高于 API 实际值时保留图例原始估算、显示中文偏差提示，并将分类权重归一化到权威已用区域。
5. 预留裁剪到尚未被实际用量占据的窗口，空闲最小为 0。
6. 用 `u128` 中间值和 Hamilton 最大余数法分配 100 个槽位，固定分类顺序作为稳定 tie-break。
7. 非零小分类在数学允许时至少获得一格，总数始终为 100。
8. 普通完整格 `⛁`，普通部分格 `⛀`，消息始终 `■`，预留 `⛝`，空闲 `⛶`。
9. 常规宽度为十行、每行十格；窄终端只改变网格与图例布局和每行格数，仍保留 100 格。
10. 模型窗口未知时隐藏百分比、网格、预留和剩余，只显示分类 Token 和明细。

## 并发规则

- Core 只原子发布不可变快照；查询只克隆快照，不长期持有 `SessionState` 锁。
- 每次模型重试、follow-up 和 `/context` 调用拥有独立 ID；禁止用“当前最新”代替请求关联。
- API actual 必须关联产生响应的请求快照。
- `/context` 的 cached-only 预览不得触发模型请求、MCP 重连、工具调用、文件扫描、压缩或 rollout/history 写入。
- TUI 后台任务只能通过 `AppEvent` 回到主状态机。
- 多次查询可乱序完成，但历史插入保持调用顺序。
- 响应必须同时匹配 request ID 和 thread ID；线程切换后的迟到响应丢弃或隔离。
- streaming 阻止持久 history 插入时，完成结果继续以 transient card 存在，直到可按顺序落盘。

## 变更规模

按三个独立可审查批次推进：

1. Core：统计模型、provenance sidecar、wire snapshot、首次估算、actual usage 关联和测试。
2. 协议：实验性 v2 API、独立 processor、schema 和 JSON-RPC 集成测试。
3. TUI：命令、异步生命周期、10×10 渲染、中文 UI、并发测试和 snapshots。

每批尽量控制在约 500 行；非机械变更预计超过 800 行时，先拆分纯统计模型和接线改动。批次边界不得缩减最终验收范围。

## 测试矩阵

### Core

- 分类互斥与总和。
- Responses API 与 Responses Lite 最终形态。
- MCP loaded/deferred 区分。
- Skill metadata/body 与指令文件明细。
- 压缩后只统计保留历史。
- 首次 API 响应前估算。
- 当前请求和上一已完成请求的 usage 关联。
- 分类估算低于或高于 API actual 的差值行为。
- 自动压缩关闭、阈值未知和实际用量侵入预留区。
- active turn、空闲预览、设置变化和并发只读查询。
- 查询不产生模型请求、MCP 重连、历史写入或压缩。

### app-server

- `thread/contextUsage` 公开 JSON-RPC 集成测试。
- camelCase 和 TypeScript schema。
- live、未加载、关闭和 unavailable thread。
- 查询前后 rollout item 数量和模型请求数不变。

### TUI 与 snapshots

- loading、成功、失败和窗口未知。
- 首次响应前“估算”和上一请求实际值来源说明。
- 10×10 常规宽度和窄终端恰好 100 格。
- Unicode display width、长路径和工具名换行。
- 空历史、无 MCP、无 Skills。
- “未归因”和估算偏差提示。
- 连续调用、乱序完成、streaming 阻塞插入和线程切换迟到响应。
- `/context` 在任务进行中和启动阶段可用。

## 本地验证

本机 Linux arm64 不运行 Rust 编译：

- 修改完成后在 `codex-rs` 运行 `just fmt`。
- 运行 `git diff --check`。
- 检查仓库产物不含禁止出现的外部路径、名称或实现信息。
- 检查默认网格常量明确为 10 行、10 列、100 格。
- 审阅所有新增中文文案及 `⛁`、`⛀`、`■`、`⛝`、`⛶`。

## 远程验证

遵循 `building-codex-in-devbox.txt`：

- 先检查远程仓库状态并建立独立远程工作树；不得 reset、stash、clean 或覆盖已有改动。
- 使用 `lunatic/devbox-build-fix-v0.149.1` 所需补丁和文档中的交叉编译环境变量。
- 运行：
  - `just write-app-server-schema` 并审阅差异。
  - `just fix -p codex-core`。
  - `just fix -p codex-app-server-protocol`。
  - `just fix -p codex-app-server`。
  - `just fix -p codex-tui`。
  - `just test -p codex-core`。
  - `just test -p codex-app-server-protocol`。
  - `just test -p codex-app-server`。
  - `just test -p codex-tui`。
  - `cargo insta pending-snapshots -p codex-tui`，逐个审阅后才接受预期 snapshots。
  - 文档规定的 aarch64 release build。
- 修改 Core 后，完整 `just test` 只在获得单独许可后运行。
- 在远程 PTY 实测 `/context`：空会话、模型流式输出中、工具调用中、连续调用、`/model` 后、`/compact` 后和窄终端。
- 分别报告格式化、schema、Clippy、各 crate 测试、snapshots、release build和人工 TUI 行为；任何一项不得替代另一项。
