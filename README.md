# 增强 Multi-agent 默认体验

这是 Codex「月狂氛围」的特性开发分支，基于上游 **v0.149.1**。

无前置特性分支；本特性基于 `lunatic/template-v0.149.1` 独立开发，之后可与其他特性移摘组合。

> 对于任何编码智能体：当在 Linux arm64 平台上开发时，禁止本机编译（本机性能不足，编译 Codex 需要至少 32G 内存），必须参考 `building-codex-in-devbox.txt` 在远端开发机编译。

## 背景

当前代码同时保留两套 multi-agent 后端：

- V1 是稳定的旧版协作工具面，使用 agent UUID 管理子线程，默认特性 `multi_agent` 已开启。
- V2 是新版 task-path 协作工具面，使用 `/root/task_name` 标识 agent，提供 mailbox、agent 间通信、follow-up task 和按需卸载等能力；`multi_agent_v2` 当前默认关闭，但模型目录可以按模型选择 V2。

V2 已为子 agent 分配随机且唯一的人物 nickname，但持久 activity/status 目前只展示 task path。调用模型填写的 `task_name` 受协议路径约束，通常是 `research_codex_multi_agent` 一类 snake_case 标识，不适合作为中文界面的自然语言职责名称。

详细代码调查与差异见 [`INVESTIGATION.md`](INVESTIGATION.md)。

## 功能点

本分支包含三个相互独立的小功能点。

### 1. 新会话默认选择 V2

让**新建的普通用户会话**默认选择 Multi-agent V2，使模型获得新版 `spawn_agent`、`send_message`、`followup_task`、`wait_agent`、`interrupt_agent` 和 `list_agents` 工具面。

默认切换必须保持会话后端稳定：已经写入 rollout 的 V1、V2 或 Disabled 会话在 resume、fork 和继续运行时沿用原版本，不在历史会话中途更换工具协议。

### 2. 为 V2 agent 生成语义人物名称

每次 V2 `spawn_agent` 收到 `task_name` 和 `message` 后，发起一次独立、无工具、无父会话历史的短模型请求，根据任务内容生成指定语言的简短语义人物名称。例如：

```text
task_name:     research_codex_multi_agent
agent_path:    /root/research_codex_multi_agent
nickname:      Archimedes
semantic_name: Codex 多 Agent 调研员
```

`semantic_name` 是内部展示 metadata，不参与 agent 寻址。`task_name`、`AgentPath` 和 nickname 继续维持现有职责。

**Responses API 中现有 Multi-agent V2 工具定义必须逐字保持不变。** 这些工具属于 Responses API 的 reserved namespace；服务端会校验保留工具的完整定义，任何工具名称、description、输入 schema、required 字段或输出 schema 变化都会使请求返回 HTTP 400。这是 API 硬约束，不是本分支的兼容性偏好。语义名称由 Core 的独立辅助推理生成，不能要求调用 `spawn_agent` 的主模型填写新字段，也不能把结果追加到 tool result。

生成请求使用可配置的语言字段，本分支默认为简体中文 `zh-CN`。请求只携带经过硬上限约束的 `task_name`、`message`、目标语言和固定命名指令，不继承 parent/child Responses history，不写入双方的模型可见上下文。

语义名称生成不得阻塞 agent 启动：spawn 先按现有流程完成，初始展示使用 nickname 与 `task_name`；辅助请求异步完成后更新 metadata 和 TUI。请求失败、超时或返回空名称时继续使用 `task_name`，不影响 agent 工作与通信。

### 3. 底栏树形 agent 选择器

当 session tree 中存在其它 agent 时，在模型与 Context 底栏下显示 inline agent 选择器。普通模式最多预览四个 running agent；idle agent 不占预览行但仍计入总数并可选择，closed agent 不进入普通预览但沿用现有 picker 的可选弱化行为。

选择器按 `AgentPath` 的父子关系进行深度优先的树形展示。每深入一级增加缩进和类似 `└` 的连接符，使 subagent 自己 spawn 的孙级 agent 保持清晰归属。root 默认身份为 `Codex [default]`；其它 agent 优先显示 `nickname · semantic_name [role]`，缺失 role 时回退为 `[default]`。

空 composer 中按 `↓` 进入选择模式；`↑/↓` 循环移动，`Enter` 立即聚焦候选，`x` 停止候选当前正在执行的任务，`Esc` 不切换并返回输入。聚焦切换不得等待、暂停、中断或取消当前运行中的 agent。`/subagents` 打开同一 inline 选择器，现有 `Alt+←/→` 直接切换继续保留。

## 用户体验

- 用户新建 Codex 会话后，不需要在 `config.toml` 中手动开启 `features.multi_agent_v2`。
- V2 每个 session tree 默认提供 16 个并发槽，包含 root，因此最多可同时驻留 15 个直属或嵌套 subagent。
- 新会话的根 agent 使用 V2 task path `/root`；新建子 agent 使用稳定、可读的 `/root/<task_name>` 路径。
- 子 agent 的消息和完成通知沿用 V2 mailbox 机制，能够使用相对 task name 或 canonical task path 通信；继承到 V2 的子 agent 保留完整 V2 协作工具，可继续 spawn 孙级 agent。
- 语义名称生成后，已有 Started activity、状态卡和 inline 选择器显示 `nickname · semantic name`，并保留 task path 作为次级协议身份。
- 语义名称尚未生成或不可用时显示 `nickname · task_name`；nickname 也缺失时回退为纯 `task_name`，状态展示不能因此失败。
- nickname 与 semantic name 不互相覆盖；名称只有终端宽度不足时才按 Unicode 显示宽度截断。
- running 使用绿色实心圆 `●`；idle 使用终端默认前景色的空心圆 `○`，不显式刷白；closed 条目保持弱化。
- 选择模式用行首 `>` 标识候选，并将候选整行渲染为浅绿色粗体。
- 普通模式右侧提示 `按 ↓ 以聚焦其它 agent`；选择模式提示 `↑/↓ 选择 · Enter 以聚焦该 agent · x 以停止 · Esc 返回`。
- 用户明确禁用 agents 时，不暴露 V1 或 V2 multi-agent 工具，也不触发语义名称生成请求。

## 版本选择规则

实现后的选择顺序应清晰且可测试：

1. `agents.enabled = false` 是总开关，结果为 Disabled。
2. resume、fork 或父 agent 继承到已经确定的 `multi_agent_version` 时，沿用该版本。
3. 对尚未确定版本的新会话，显式配置继续具有优先级：开启 `multi_agent_v2` 选择 V2；关闭后允许模型目录声明或旧版 fallback 决定版本。
4. 未显式覆盖、模型目录也未提供选择时，新会话默认 V2。
5. Guardian review、compact、memory consolidation 等内部线程继续遵守现有的禁用或专用选择逻辑，不因为用户会话默认值改变而获得协作工具。

版本一旦在会话首个有效 turn 确定并写入 session metadata，该会话后续 turn 不因模型目录刷新或默认值变化而切换后端。

## 配置兼容

- 保留 `[agents]` 下的 `enabled`、并发限制、默认子 agent 模型/推理强度和自定义 roles。
- 保留 `features.multi_agent_v2` 的布尔与 table 两种写法，以及 V2 的 namespace、wait timeout、usage hints、spawn metadata 等配置。
- 在 V2 table 中增加语义名称语言配置，本分支默认 `zh-CN`；配置只决定辅助命名请求的输出语言，不进入 reserved tool definition。
- 保留 V1 实现和 `multi_agent` 兼容键，用于旧 rollout、显式回退和仍声明 V1 的集成环境。
- 不删除、重命名或改变 app-server/rollout 中的 `MultiAgentVersion::{Disabled,V1,V2}` 序列化值。
- 不把旧 V1 tool call 或 rollout item 重写成 V2 形态。

## 实现约束

### 默认选择 V2

- 不应只把 `Feature::MultiAgentV2.default_enabled` 从 `false` 改为 `true` 后结束：当前配置覆盖顺序会让这个改动同时覆盖已记录的 V1 会话和 `agents.enabled = false`，造成超出“新会话默认值”的行为变化。
- 默认选择逻辑应集中在现有版本解析入口，避免在 TUI、app-server、exec 等前端分别注入配置。
- 继续由 `TurnContext.multi_agent_version` 驱动工具注册、提示词片段、并发限制和 rollout metadata，不建立第二套布尔判断。
- 本分支将 V2 默认并发上限从上游的 4 提高到 16：每个 session tree 最多驻留 16 个 agent，包含 root；直属与嵌套 subagent 共享其余 15 个槽。
- 强制 V2 时必须覆盖模型目录声明 V1、Disabled 或未声明版本的新会话测试，同时验证 V2 子 agent 的工具暴露规则；不得让提示词宣称可递归委派而实际没有工具。

### 语义人物名称

- 不修改生成 V2 reserved tools 的实现或其任何 schema 文本；必须用回归测试锁定实际发往 Responses API 的工具定义，避免无意变化导致 HTTP 400。
- 在 agent metadata 中新增独立的可选语义名称字段，以 `AgentPath` 所属 metadata 作为唯一事实来源，不维护另一份容易失同步的全局 `task_name -> semantic_name` HashMap。
- 辅助命名请求与 parent/child 推理隔离：不带工具、不继承 `previous_response_id` 或对话历史、不产生 inter-agent message，也不写入 model-visible context。
- 不得通过向 Multi-agent V2 工作区、tool router 或 agent 可见工具集合添加命名工具来实现；命名能力只存在于 Core 内部的独立 Responses 请求中，V2 agent 的工作区和工具面保持原样。
- 输入和输出都必须有硬上限；输出去除首尾空白与包裹引号，只接受单行非空名称，超长结果按 Unicode 字符边界处理。
- 命名任务异步执行，不能占用 multi-agent 并发 slot，不能延迟 `spawn_agent` tool result，也不能因请求失败而关闭或回滚已经创建的 agent。
- 语义名称完成后通过现有 metadata/activity 更新路径刷新 TUI，并持久化到 rollout，使 resume 后不重复调用模型生成名称。
- 同一 agent 只生成一次语义名称；状态更新必须按 thread/agent identity 防止异步结果写入错误或已经被替换的条目。

## 验收标准

### 新会话

- 默认配置、模型未声明版本时，首次 turn 选择 V2 并公开 V2 工具面，不公开 `multi_agent_v1` namespace。
- 模型声明 V1 或 Disabled 时，月狂默认策略是否覆盖模型声明由配置规则一致决定，并有测试锁定；默认开启场景最终选择 V2。
- `agents.enabled = false` 时不公开任何协作工具。
- 显式关闭 V2 后可回到模型目录选择或 V1 fallback，不需要删除配置文件中的其他 V2 table 字段。

### 旧会话与继承

- 带 `multi_agent_version = v1` 的 rollout resume 后仍为 V1。
- 带 `multi_agent_version = v2` 的 rollout resume 后仍为 V2。
- 缺少版本 metadata 的旧 rollout 按现有兼容规则继续视为 V1。
- 从 V2 parent 创建的子 agent 继承 V2；从 V1 parent 创建的子 agent 不被默认值升级。
- fork 会话沿用来源会话已经确定的版本。

### 工具与持久化

- V2 根 agent 获得 `spawn_agent`、`send_message`、`followup_task`、`interrupt_agent`、`list_agents`，并按配置决定是否公开 `wait_agent`。
- 发往 Responses API 的每个 V2 reserved tool definition 与本分支基线逐字一致；不得因语义名称功能改变 description、schema 或 tool result 形状，否则服务端会返回 HTTP 400。
- V1 仍获得 `spawn_agent`、`send_input`、`resume_agent`、`wait_agent`、`close_agent`。
- 新会话选出的版本写入 `SessionMeta.multi_agent_version`，resume 后无需依赖当前默认值重算。
- 成功生成的 semantic name 随 agent metadata 持久化；resume 直接恢复，不能产生第二次辅助推理请求。
- app-server thread start/read/resume/fork 的版本字段和现有 schema 保持兼容。

### 语义名称生成

- 每次成功接受的 V2 spawn 最多触发一次辅助命名请求；V1、Disabled 和被拒绝的 spawn 不触发。
- 请求只包含有界的 `task_name`、`message`、固定命名指令和语言标识，不包含父会话历史或 multi-agent tools。
- 默认语言 `zh-CN` 能将 `research_codex_multi_agent` 与对应 message 生成为类似 `Codex 多 Agent 调研员` 的简短中文名称。
- 修改语言配置后，新 spawn 使用指定语言；已持久化的名称保持不变。
- 命名请求缓慢、失败、超时或输出无效时，agent 仍立即正常工作，展示稳定回退到 `task_name`。
- 辅助请求及其响应不出现在 parent 或 child 的模型上下文、mailbox 和 `spawn_agent` tool result 中。

### TUI 身份展示与选择器

- semantic name 可用时，V2 `Started`、`Interacted`、`Interrupted` 等持久 activity 和选择器显示 `nickname · semantic name [role]`，task path 作为次级协议身份保留。
- semantic name 尚未生成或缺失时回退为 `nickname · task_name [role]`；nickname 也缺失时精确回退为 `task_name [role]`，root 最终回退为 `Codex [default]`。
- nickname 和 semantic name 只改变展示文本，不替代 `AgentPath`，不进入 mailbox target，也不改变 reserved tool result。
- 普通模式最多显示四个 running agent；idle 和 closed 不进入预览。选择模式最多显示四个可滚动候选，包含 running、idle 和 retained closed 条目。
- running 条目使用绿色实心圆 `●`；idle 条目使用终端默认前景色的空心圆 `○`；当前候选以 `>` 开头，并将整行显示为浅绿色粗体。
- 嵌套 agent 按 `AgentPath` 层级深度优先排列，子级与孙级使用累积缩进及 `└` 树形前缀。
- 空 composer 的 `↓` 只在没有 popup、modal、history navigation 或 paste 冲突时进入选择模式；`Enter` 切换 thread 的过程中不得等待或中断任何 running agent；`x` 只停止候选当前正在执行的任务并保持选择模式；`Esc` 返回输入。
- 长 nickname、semantic name、task_name 和中英文混排仅在终端宽度不足时按 Unicode display width 截断，并保留右侧状态与交互提示。

## 测试要求

至少补充或调整以下覆盖：

- feature 默认值和显式覆盖解析测试；
- `Config` 的 agents 总开关、V2 选择和 V1 fallback 优先级测试；
- session 新建、resume、fork、parent-child 继承的版本稳定性测试；
- tool plan 对 V1/V2/Disabled 三种版本的完整工具集合测试；
- 一条 Core 集成测试，验证默认配置的新会话实际向 Responses API 发送 V2 工具定义；
- reserved V2 tool definitions 的精确回归测试，证明本功能没有改变基线 definition 或 tool result 形状；
- 辅助命名请求的输入裁剪、默认中文、可配置语言、单行输出规范化、失败回退、异步更新和单次生成测试；
- metadata 写入与 resume 恢复测试，证明恢复已有语义名称时不会再次请求模型；
- V2 activity 与 inline selector 的 `nickname · semantic name [role]` snapshots，包括生成前与失败时的 fallback、缺失 nickname 和长文本布局；
- selector 的 running `●`、idle `○` 默认前景色、候选整行浅绿色粗体、idle 汇总、closed 弱化、三级树形层级、四行滚动和窄终端截断 snapshots；
- selector 输入路由测试，覆盖空 composer 的 `↓`、冲突场景、循环选择、`Enter` 非阻塞聚焦、`x` 停止运行任务和 `Esc` 返回；
- 更新 config schema，并执行 `just write-config-schema`。

代码改动完成后按仓库规则在远端开发机运行 `just fmt`、`just fix -p codex-core`、相关 crate 测试和所需集成测试。本地仅进行不涉及编译的静态检查。

## 非目标

- 删除 V1 或迁移历史 V1 rollout。
- 修改 Responses API reserved Multi-agent V2 工具的任何定义内容；该行为会导致请求返回 HTTP 400。
- 重新设计 V2 task path、mailbox 或主模型协作提示词。
- 让语义名称替代 nickname、`AgentPath` 或 mailbox target。
- 改变模型主动委派策略、Ultra effort 行为或 multi-agent mode 文案。
- 引入独立全屏 agent dashboard；本特性仅增加底栏 inline selector，并复用现有 thread 切换机制。
- 改变 running、idle、closed 的后端生命周期，或在切换焦点时暂停、中断、取消 agent。
- 除将 V2 默认并发上限设为 16 外，改变并发槽的生命周期语义或 subagent 默认模型。
- 为 app-server 增加新的公开 API。

## 开发记录

- 2026-08-29：从 `lunatic/template-v0.149.1` 创建独立 worktree，随后将分支和工作树重命名为 `lunatic/enhance-multi-agent` 与 `lunatic-codex-enhance-multi-agent`。
- 2026-08-29：完成 V1/V2 选择链路、工具面、线程标识、通信、上下文继承、并发和 rollout 兼容性调查，记录于 [`INVESTIGATION.md`](INVESTIGATION.md)。
- 2026-08-29：用 `jq` 抽取本机 TraeX parent/child session 片段，追加闭源行为层的多 agent 协作对照，并将 V2 `nickname · task path` 持久状态展示纳入需求。
- 2026-08-29：检查 TraeX Responses request dump，对照其 `Agent`/后台任务工具面、提示词职责分配和 Codex V2 task-tree/mailbox 契约；保留 V2 协议边界，不引入单工具 facade。
- 2026-08-29：将需求明确拆分为“新会话默认选择 V2”和“异步生成默认中文的语义人物名称”两个功能点；后者不得改动 reserved tool definition，否则 Responses API 返回 HTTP 400。
- 2026-08-29：追加第三个功能点：底栏 inline agent selector，按 V2 `AgentPath` 展示可滚动的父子树，并支持不阻塞 running agent 的即时聚焦切换。
- 2026-08-29：开始代码实现；编译与交互验证尚未完成。
