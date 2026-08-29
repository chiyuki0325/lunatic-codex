# Codex Multi-agent V1 / V2 调查报告

调查日期：2026-08-29
代码基线：`lunatic/template-v0.149.1`（Codex v0.149.1）

## 结论摘要

Codex 的 V1 与 V2 共用 `AgentControl`、`ThreadManager`、session/rollout 和 TUI 基础设施，但面向模型的是两套不同协议。

V1 以 `ThreadId` 为核心，提供传统的 spawn/input/wait/close 生命周期；V2 以稳定的 `AgentPath` 为核心，引入 task tree、mailbox、agent 间消息、follow-up turn、可中断但继续驻留的 agent，以及完成线程的 LRU 卸载。V2 不是 V1 的工具别名，切换会改变工具 schema、上下文继承参数、消息投递语义、并发限制和模型可见提示词。

当前选择链路支持模型目录按模型声明 V1/V2/Disabled，也允许 `features.multi_agent_v2` 强制 V2。V1 feature 默认开启，V2 feature 默认关闭。因此“默认开启新版 Subagent”不能只看工具注册；还必须处理已持久化会话、agents 总开关、模型选择器和 parent-child 继承的优先级。

建议仅让尚未确定版本的新用户会话默认 V2，保留旧会话已经记录的版本，并让 `agents.enabled = false` 保持硬禁用语义。

## 1. 特性与配置入口

### 1.1 Feature 定义

`codex-rs/features/src/lib.rs:1114-1125` 定义：

| Feature | 配置键 | Stage | 当前默认值 |
| --- | --- | --- | --- |
| `Feature::Collab` | `multi_agent` | Stable | `true` |
| `Feature::MultiAgentV2` | `multi_agent_v2` | Stable | `false` |

`collab` 只是 `multi_agent` 的 legacy alias，见 `codex-rs/features/src/tests.rs:401-402`。

V2 配置不只是开关。`codex-rs/features/src/feature_configs.rs:232-277` 还允许设置：

- session tree 并发数；
- `wait_agent` 的最小、最大和默认 timeout；
- root/subagent usage hint 与 developer instructions；
- V2 tool namespace；
- spawn metadata 和 model override 是否暴露；
- `wait_agent` 是否暴露；
- tool exposure 模式。

布尔 `multi_agent_v2 = true` 与 table `[features.multi_agent_v2] enabled = true` 都受支持，config merge 层专门保留二者的兼容合并语义。

### 1.2 Agents 总配置

`codex-rs/config/src/config_toml.rs:661-680` 的 `[agents]` 配置覆盖两套后端：

- `enabled`：是否启用 multi-agent；
- `max_concurrent_threads_per_session`：并发限制；
- `max_depth`：只对 V1 生效；
- `default_subagent_model` / `default_subagent_reasoning_effort`；
- `interrupt_message`。

自定义 agent roles 也位于 `[agents.<role>]`。

### 1.3 V2 运行时默认值

`codex-rs/core/src/config/mod.rs:219-229` 与 `1219-1263` 给出 V2 默认值：

- 每个 session tree 共 4 个并发 slot，包含 root；
- 默认可同时驻留 3 个子 agent；
- `wait_agent` 默认 30 秒，允许范围 10 秒至 1 小时；
- 默认 namespace 为 `collaboration`；
- 默认隐藏 spawn 的 agent metadata；
- 默认允许 spawn 时覆盖 model/reasoning effort；
- 默认公开 `wait_agent`；
- `non_code_mode_only` 默认开启。

V1 的默认 `DEFAULT_AGENT_MAX_THREADS` 是 6，`DEFAULT_AGENT_MAX_DEPTH` 是 1。两套数字的计数口径不同，切换默认后不能假定并发行为不变。

## 2. 当前版本选择链路

### 2.1 配置、模型目录和 fallback

核心逻辑位于 `codex-rs/core/src/config/mod.rs:1475-1502`：

1. `features.multi_agent_v2` 有效开启时，配置 override 返回 V2；
2. 否则 `agents_enabled == false` 时返回 Disabled；
3. 没有配置 override 时，使用模型目录的 `ModelInfo.multi_agent_version`；
4. 模型未声明时，`multi_agent` 开启则 fallback 到 V1，否则 Disabled。

这意味着 V2 feature 当前优先于 `agents.enabled = false`，也优先于模型声明的 V1/Disabled。

模型目录字段定义于 `codex-rs/protocol/src/openai_models.rs:260` 和 `480`，协议枚举位于 `codex-rs/protocol/src/protocol.rs:2828-2835`。未知的未来版本会按“未声明”处理，而不会反序列化失败。

`codex-rs/core/tests/suite/model_runtime_selectors.rs:353-408` 已有集成测试锁定当前优先级：配置可强制关闭 agents、选择 V1，或在模型声明 Disabled 时强制 V2。

### 2.2 会话首次选择与固定

Session 使用 `OnceLock<MultiAgentVersion>` 保存已经确定的版本，见 `codex-rs/core/src/session/session.rs:49-53`。首次有效 turn 根据当时选择的模型解析版本，随后固定，见 `codex-rs/core/src/session/mod.rs:3424-3443`。

`SessionMeta.multi_agent_version` 持久化到 rollout，字段位于 `codex-rs/protocol/src/protocol.rs:2893-2924`。

历史恢复规则位于 `codex-rs/core/src/session/mod.rs:439-454`：

- rollout 已记录版本时沿用该值；
- parent/inherited version 次之；
- 没有版本 metadata 的旧 resume/fork 会话按 V1 处理；
- inherited Disabled 始终先返回 Disabled。

但在 turn 解析阶段，`Config::multi_agent_version_for_model()` 会再次应用配置 override。若直接把 V2 feature 默认值改为 true，已恢复到 `OnceLock` 的 V1 仍可能被配置 override 改成 V2。这是简单翻转 feature 默认值的首要兼容风险。

### 2.3 模型切换

版本在首个有效 turn 才最终确定，因此首 turn 前的模型设置会影响选择。`codex-rs/core/tests/suite/model_runtime_selectors.rs:411-485` 验证：启动后、首 turn 前选择声明 V2 的模型，会让该 session 固定为 V2 并发送 V2 工具。

固定后不应随下一次模型切换更换后端，否则同一 rollout 会混入两套工具调用和通信语义。

## 3. V1 工具面

入口模块是 `codex-rs/core/src/tools/handlers/multi_agents.rs:1-84`，工具 schema 集中在 `multi_agents_spec.rs`。

V1 使用 Responses API namespace `multi_agent_v1`。模型看到 5 个工具：

| 工具 | 目标与语义 |
| --- | --- |
| `spawn_agent` | 创建子 thread，返回 `agent_id` 和 nickname |
| `send_input` | 按 agent UUID 发送结构化或文本输入，可用 `interrupt=true` 立即重定向 |
| `resume_agent` | 按 UUID 重新加载已经 close 的 agent |
| `wait_agent` | 等待指定的一组 agent 到达 final status，并返回完成内容 |
| `close_agent` | 关闭目标及其开放 descendants，释放并发名额 |

工具注册见 `codex-rs/core/src/tools/spec_plan.rs:1202-1227`。当 search tool 可用时，V1 namespace 可以 deferred loading；否则直接暴露。

### 3.1 标识与拓扑

V1 的外部标识是 `ThreadId` UUID，nickname 仅用于展示。通信、wait、resume 和 close 都以 UUID 为权威目标。

V1 通过 `agents.max_depth` 限制递归 spawn。`collab_tools_enabled()` 在 V1 下根据当前 `SessionSource` 的下一层深度决定是否继续公开工具，见 `codex-rs/core/src/tools/spec_plan.rs:601-607`。

### 3.2 上下文继承

V1 `spawn_agent` 接受：

- `message` 或结构化 `items`；
- `agent_type`；
- `fork_context: bool`；
- `model`、`reasoning_effort`、`service_tier`。

schema 位于 `codex-rs/core/src/tools/handlers/multi_agents_spec.rs:545-628`。

`fork_context=true` 是完整历史 fork；false/省略则只给初始任务。它不能像 V2 一样选择最近 N 个 turns。

### 3.3 生命周期特征

V1 将 completed agent 保持为 open，仍计入上限，直到调用 `close_agent`。close 后如果要继续使用，需要 `resume_agent`。这套显式 close/resume 生命周期也是 V1 tool surface 保留这两个工具的原因。

### 3.4 典型交互形态

一次 V1 协作在模型侧大致如下：

```text
用户 -> root：调查 A 和 B
root -> multi_agent_v1.spawn_agent(message="调查 A")
工具 -> root：{ agent_id: "<uuid>", nickname: "Curie" }
root -> multi_agent_v1.spawn_agent(message="调查 B")
工具 -> root：{ agent_id: "<uuid>", nickname: "Kepler" }

root -> multi_agent_v1.send_input(target="<uuid>", message="补查 C")
root -> multi_agent_v1.wait_agent(targets=["<uuid>", "<uuid>"])
子 agent 完成 -> root 收到带最终状态和结果的通知
root -> multi_agent_v1.close_agent(target="<uuid>")
```

它的交互中心是“root 管理一组按 UUID 寻址的子线程”：

- `spawn_agent` 立即返回 UUID 和人类可读 nickname；
- 后续输入、等待、关闭和恢复都使用 UUID，nickname 主要出现在 TUI 和结果展示中；
- `send_input(interrupt=false)` 排队，`interrupt=true` 立即打断并重定向当前工作；
- parent 可以显式等待指定 agent，完成通知也会注入 parent 会话；
- completed agent 仍需 close，之后若想继续交互再 resume。

用户在 TUI 中看到的是各自带 nickname/role 的 agent 行，例如 `Curie [worker]`；底层工具寻址仍是不可读的 UUID。

## 4. V2 工具面

入口模块是 `codex-rs/core/src/tools/handlers/multi_agents_v2.rs:1-45`。V2 默认放在可配置 namespace `collaboration` 下，工具注册位于 `codex-rs/core/src/tools/spec_plan.rs:1143-1201`。

模型看到最多 6 个工具：

| 工具 | 目标与语义 |
| --- | --- |
| `spawn_agent` | 用局部 `task_name` 创建子 agent，返回 canonical task name |
| `send_message` | 投递消息但不触发新 turn |
| `followup_task` | 给既有非 root agent 新任务，空闲时触发 turn |
| `wait_agent` | 等任意 live agent 的 mailbox 更新、用户 steer 或 timeout，不直接返回消息正文 |
| `interrupt_agent` | 中断当前 turn，agent 保持可继续接收消息/任务 |
| `list_agents` | 列出当前 root tree 中可见的 live agents，可按 path prefix 过滤 |

`wait_agent` 可由配置隐藏，其余工具成组公开。

### 4.1 Task path

V2 使用层级化 `AgentPath`：root 是 `/root`，子任务如 `/root/search_docs`，孙任务如 `/root/search_docs/check_tests`。

`spawn_agent` 强制要求 `task_name` 和 `message`，可使用局部 task name；跨 sibling tree 通信时使用 canonical path。schema 与说明位于 `codex-rs/core/src/tools/handlers/multi_agents_spec.rs:102-146`、`631-672` 和 `749-779`。

相比 UUID，task path 同时承担稳定身份、路由和树结构表达。rollout metadata 仍保存底层 `ThreadId`，另有 `agent_path` 字段，所以 V2 并未取消 thread/session 基础设施。

### 4.2 Mailbox 与通信

`send_message` 只投递消息，不启动新 turn；`followup_task` 才表达“投递新任务并在空闲时启动”。二者拆分避免 V1 `send_input` 的 interrupt/queue 布尔参数承载多种语义。

V2 消息以 `InterAgentCommunication` 进入 mailbox。常规 tool call 的 payload 可使用加密字段；直接 plaintext message source 会渲染结构化 `InterAgentMessage`。转换逻辑位于 `codex-rs/core/src/tools/handlers/multi_agents_v2.rs:57-84`。

父 agent 不靠 `wait_agent` 返回正文读取结果；mailbox 更新会作为单独的 model-visible inter-agent message 交付。`wait_agent` 只是等待新的 mailbox activity 或 steer 信号，schema 说明见 `codex-rs/core/src/tools/handlers/multi_agents_spec.rs:285-315`。

### 4.3 上下文继承

V2 `spawn_agent` 使用 `fork_turns`：

- `all` 或省略：完整历史；
- `none`：不继承 turn；
- 正整数字符串：只继承最近 N 个 turns。

V2 可对有限历史 fork 覆盖 model/reasoning effort；完整历史 fork 默认继承 parent model 与 effort。usage hint 会向模型说明该约束，见 `codex-rs/core/src/session/multi_agents.rs:50-58` 和 `116-128`。

V2 的初始输入只接受加密的 plain-text `message`，没有 V1 的结构化 `items` 参数。

### 4.4 递归能力与模型支持

V2 root 在版本为 V2 时可以获得协作工具；V2 subagent 只有在当前模型自身声明支持 V2 时才继续获得协作工具。条件位于 `codex-rs/core/src/tools/spec_plan.rs:601-612`。

因此，全局强制 V2 可以让一个声明 V1/Disabled 的模型在 root 获得 V2 工具，但其 child 未必获得递归 spawn 工具。默认 V2 的测试必须覆盖这个组合，避免 bundled hint 宣称 child 可以 spawn，而实际 tool plan 不提供工具。

### 4.5 Residency 与并发

V2 把 root 外的 resident subagents 纳入 LRU residency。容量满时，系统可以自动卸载已 Completed、Errored 或 Interrupted、且没有 active turn/待处理 mailbox 的 agent，再为新 agent 腾出 slot。

实现位于 `codex-rs/core/src/agent/control/residency.rs:17-239`。卸载前会 materialize rollout、保存环境选择并从 ThreadManager 移除；后续按需重新加载。这与 V1 要求模型显式 close completed agent 的策略不同。

### 4.6 典型交互形态

一次 V2 协作在模型侧大致如下：

```text
用户 -> /root：调查 A 和 B
/root -> spawn_agent(task_name="research_a", message="调查 A")
工具 -> /root：{ task_name: "/root/research_a" }
/root -> spawn_agent(task_name="research_b", message="调查 B")
工具 -> /root：{ task_name: "/root/research_b" }

/root -> send_message(target="research_a", message="顺便告诉 research_b 发现")
/root -> followup_task(target="research_b", message="再核对 C")
/root -> wait_agent(timeout_ms=...)
mailbox -> /root：MESSAGE 或 FINAL_ANSWER，带 Sender、Task name 和 Payload
/root -> interrupt_agent(target="research_b")
```

它的交互中心是“多个按 task path 寻址的 agent 通过 mailbox 协作”：

- spawn 时由调用者选择局部 `task_name`，系统组成 canonical path；
- 同一父节点下可以用短名，跨分支通信使用完整路径；
- `send_message` 只通信，不启动 turn；`followup_task` 表示新的工作并在可运行时触发 turn；
- agent 可以直接给 parent 或其他 agent 发消息，完成结果以 `FINAL_ANSWER` mailbox 消息到达；
- `wait_agent` 等待整棵 live tree 的新事件，不要求 root 预先列出 UUID，也不在 tool result 中复制消息正文；
- `interrupt_agent` 只停止当前 turn，不销毁 agent，后续可继续派任务；completed/idle agent 可由 residency 自动卸载。

Core 为 V2 子 agent 分配 nickname，部分 picker/footer 数据路径能够取得该 metadata；但当前持久 activity/status 界面没有接入 nickname：`Started` 行和 `/subagents` 状态卡都只渲染 `/root/...` task path。用户实际看到的典型输出是 `Started /root/research_a`，不能据此知道该 agent 的人名。

### 4.7 “每个 agent 都有自己的名字”属于哪一版

**当前代码里两版都会为每个 thread-spawn agent 分配唯一的人类可读 nickname。** 这是共用 `AgentControl::prepare_agent_metadata()` 的行为，候选名来自内置名字表或 role 的 `nickname_candidates`，见 `codex-rs/core/src/agent/control.rs:604-626` 和 `codex-rs/core/src/agent/control/spawn.rs:31-50`。

两版的区别在“哪个名字是协议身份”：

- **V1**：协议身份是 UUID，但 `spawn_agent` 默认把 `nickname` 与 `agent_id` 一起返回。因此如果所说的是 spawn 后直接看到每个 agent 获得类似 `Curie`、`Kepler` 的独立名字，V1 的工具交互最明显。
- **V2**：协议身份是调用者指定的 `task_name` / canonical task path。底层仍分配 nickname，部分 picker/footer 数据链可以取得它；不过持久 activity/status 当前没有显示。V2 默认 `hide_spawn_agent_metadata = true`，所以 `spawn_agent` 的默认 tool result 也只返回 task path，不把 nickname 告诉模型。关闭该配置后，V2 result 可同时返回 `task_name` 和 `nickname`，仍不会自动改变持久状态卡。

因此：**人名式随机 nickname 并非 V1 独有，两版都有；以可读名字作为 agent 间寻址方式的是 V2，但它使用任务名和路径，而非随机人名。**

### 4.8 让 V2 持久状态显示人名

`hide_spawn_agent_metadata = false` 只会让 V2 `spawn_agent` 的模型工具结果附带 nickname，不会改变 TUI 的持久状态输出。当前缺口位于两条独立的 UI 数据链：

- `codex-rs/tui/src/multi_agents.rs:316-320` 的 activity summary 只接收 `agent_path`；
- `codex-rs/tui/src/app/agent_status_feed.rs:65-115` 的状态预览只保存 `agent_path` 和 activity。

推荐把 nickname 作为展示 metadata 传入这两条链，统一显示为：

```text
Started Curie · /root/research_a

Sub-agents running
  • Curie · /root/research_a
```

task path 继续作为协议身份、路由目标和唯一性来源；nickname 只增强辨识度。历史事件、缺失 metadata 或 root 等无法取得 nickname 的场景回退为纯 path，不能阻止状态卡渲染。这个改动需要覆盖 Started/Interacted/Interrupted 等 activity snapshots，以及 running/completed/error 状态卡 snapshots。

## 5. 两套后端共用的基础设施

两套 handler 最终都调用同一个 `AgentControl`：

- spawn/config 派生集中在 `codex-rs/core/src/agent/control/spawn.rs`；
- thread 存取由 `ThreadManager` 管理；
- agent status、rollout、session source、TUI events 和 protocol item 大量共用；
- tool result 仍使用统一的 `CollabAgentToolCallItem` 等协议 item；
- TUI 的 `multi_agents` 模块和 agent dashboard 消费统一状态，而非分别运行两套 agent runtime。

V2 主要新增 task-path resolver、mailbox 通信和 residency，而没有复制完整 session runtime。

## 6. 模型可见上下文差异

V2 会额外注入 role-specific usage hint。默认 root/subagent 文本位于 `codex-rs/core/src/session/multi_agents.rs:11-59`，包含：

- 当前 agent 在 team 中的身份；
- 可使用的 V2 工具；
- inter-agent message envelope；
- 共享 filesystem/cwd；
- 并发 slot 数；
- `wait_agent` 和 model override 指引。

提示词可由本地 V2 config 或 model catalog 覆盖/抑制。解析优先级位于同文件 `67-143`。

V2 还根据 reasoning effort 生成 multi-agent mode：Ultra 默认为 Proactive，其他 effort 默认为 ExplicitRequestOnly；model catalog 或配置可提供自定义 hint。逻辑位于 `codex-rs/core/src/session/multi_agents.rs:145-185`。

所以默认切换会增加稳定的模型可见上下文，也可能改变模型是否主动委派。实现与验收必须把“工具后端默认值”和“主动委派策略”分开，避免顺手修改 mode 文案。

## 7. 直接翻转 V2 Feature 默认值的影响

把 `Feature::MultiAgentV2.default_enabled` 改成 `true` 能让新会话强制选择 V2，但同时会产生以下行为：

1. **旧 rollout 被覆盖**：配置 override 可能把 resume 得到的 V1 改成 V2。
2. **agents 总开关失效**：当前 override 先检查 V2 feature，再检查 `agents_enabled`；默认开启后，单独 `agents.enabled=false` 不再禁用工具。
3. **模型声明被覆盖**：声明 V1 或 Disabled 的模型也会在 root 选择 V2。
4. **递归能力不一致**：被强制 V2、但模型未声明 V2 时，root 和 child 的 tool exposure 可能不同。
5. **并发语义变化**：默认从 V1 上限/深度切到 V2 的 4-slot tree 与 LRU residency。
6. **提示词与 cache key 变化**：V2 role/mode hints 进入模型上下文。
7. **测试和 schema 预期变化**：feature 默认集合、配置解析、tool plan、session resume/fork 和 Responses request fixture 都可能受影响。

这些都应作为有意识的产品决定，而不能当作一行默认值修改的附带结果。

## 8. 推荐实现边界

### 8.1 推荐选择语义

推荐把后端选择区分为“总开关”“会话已确定版本”“新会话默认值”：

1. `agents.enabled=false` 返回 Disabled；
2. session/rollout/parent 已经提供版本时原样沿用；
3. 新会话的显式 V2 配置覆盖模型目录；
4. 未显式选择的新会话默认 V2；
5. 显式关闭 V2 时保留模型目录与 V1 fallback 路径。

这样实现 feature 名称所表达的“default new”，同时避免历史协议迁移。

### 8.2 不建议的方案

- 只改 TUI 启动参数：exec、MCP、app-server 等入口会继续得到不同默认值。
- 在 config 文件首次运行时写入 `multi_agent_v2=true`：污染用户配置，也无法正确处理 profile/managed config。
- 删除 V1：旧 rollout、特定模型和兼容测试仍依赖它。
- resume 时把 V1 rollout 转换为 V2：两套 tool call、target identity 和消息语义不兼容。

### 8.3 预计改动点

实现阶段应优先检查：

- `codex-rs/features/src/lib.rs`：默认 feature 策略；
- `codex-rs/core/src/config/mod.rs`：配置优先级与 fallback；
- `codex-rs/core/src/session/mod.rs`：persisted/inherited version 的稳定性；
- `codex-rs/core/src/tools/spec_plan.rs`：确认 V2/V1/Disabled 工具集合；
- `codex-rs/core/src/config/config_tests.rs`、`session/tests.rs`、`tools/spec_plan_tests.rs`：单元覆盖；
- `codex-rs/core/tests/suite/model_runtime_selectors.rs`：端到端请求工具面；
- app-server resume/fork tests：旧 rollout 兼容。

如果 `ConfigToml` 或嵌套 config schema 形态没有变化，只调整默认值通常无需新增字段；仍应检查生成 schema 是否编码了 feature 默认信息。

## 9. 验证建议

本分支运行在 Linux arm64，本机不编译。实现后按 `building-codex-in-devbox.txt` 在远端验证：

1. `just fmt`；
2. `just fix -p codex-core`；
3. 与 feature/config 相关的 crate 测试；
4. `just test -p codex-core`；
5. app-server protocol 或 app-server 被实际改动时运行对应 crate 测试；
6. 若 config schema 变化，运行 `just write-config-schema` 并检查 diff；
7. 静态检查新会话、V1 resume、V2 resume、legacy-no-metadata resume、V1/V2 parent-child 组合。

编译通过只能证明 Rust 构建正确；Responses request 中的实际工具集合、rollout resume 版本和 V2 child 工具暴露需要由测试分别证明。

## 10. TraeX 多 agent 协作的行为调查

### 10.1 调查边界与样本

TraeX 是闭源软件，本节只记录本机 `~/.trae/cli` 中真实会话产生的可观察行为，不推断内部源码、类名或实现算法。调查使用 `jq` 提取会话 JSONL 的 session metadata、agent 工具调用、activity 事件和 inter-agent message，并用本地 thread index 关联 parent/child rollout。

抽取了三类样本：

- 一个并行中文 TUI 审查会话：先后创建 22 个 child session，包含后台启动、列出 agent、定向消息、停止和完成回传；
- 一个跨 Core/app-server/TUI 的实现会话：先后创建 21 个 child session，包含多轮“实现—复核—修复”委派；
- 两个 child rollout：用于确认子 agent 的独立上下文、task label、person-like path 和完成回传形态。

这些是 2026-08-28 至 2026-08-29 的本机样本，不应外推为所有 TraeX 版本的稳定协议。尤其是工具参数在样本之间存在迁移痕迹，报告只把成功事件或多处一致记录视为已确认行为。

### 10.2 两层可读身份

TraeX 的会话记录同时出现两种可读标签：

| 标签 | 样例 | 可观察用途 |
| --- | --- | --- |
| task label | `扫描底栏选择文案` | spawn 的 description、child session 标题和任务语义 |
| person-like agent path | `/root/archimedes` | activity、agent 列表、消息作者/收件人和生命周期工具目标 |

因此 TraeX 展示“每个 agent 都有人名”的直接来源是 **agent path 的最后一段本身就是人名**。它没有把 `Archimedes` 与 `/root/scan_footer` 并排显示，而是使用 `/root/archimedes` 作为运行期地址，再用独立 task label 说明工作内容。

同一个 parent 的样本中，`/root/euclid`、`/root/avicenna` 等 path 曾先后绑定不同 child thread 和不同 task label。这说明人名 path 可在前一个任务结束后复用，不是跨任务永久身份。报告不能把它描述成稳定的全局 agent ID；其稳定范围至多是一次活跃 child session。

### 10.3 Spawn 与前后台交互

可观察的 spawn 参数至少包括：

- 简短 description，成为 task label；
- 完整 task prompt；
- model 与 reasoning effort；
- `run_in_background`。

后台模式立即向 parent 返回一个 `/root/<person>` 地址，parent 可以继续启动其他 agent 或处理自己的任务。前台模式会占住当前 tool call，直到 child 返回、失败或被用户中止。样本中大规模并行审查均使用后台模式；少数前台委派在完成前被中止，没有被误记为成功结果。

启动后，root rollout 会产生 path-only activity：

```text
started  /root/archimedes
started  /root/avicenna
```

这与 Codex V2 当前 `Started /root/<task_name>` 的持久 activity 形态相近，但 path 的命名责任不同：Codex V2 path 表达任务，TraeX 样本 path 表达人名，任务语义另存为 task label。

### 10.4 子 agent 上下文与共享工作区

每个 TraeX child 都有独立 rollout，session metadata 标记为 subagent，并记录自己的 task label 与 agent path。抽样 child 的初始上下文包含：

- 通用运行规则和适用的仓库指令；
- “这是新创建 team agent”的协作角色说明；
- parent 在 spawn 时提供的专用任务 prompt。

未观察到 parent 完整聊天历史被原样复制给 child。协作依赖 parent 写出自足任务说明，并让 agent 读取共享工作树中的权威文件。多个 child 直接操作同一 cwd，改动对其他 agent 和 root 立即可见；因此 parent 需要事先划分不重叠范围，发生重叠时再调停，而不是依赖隔离 worktree 自动消解冲突。

### 10.5 状态、完成回传与唤醒

`ListAgents` 的样本输出把两类对象分开：

```text
Subagents
  /root
  /root/archimedes  running
  /root/avicenna    running

Peer sessions
  /peers/<session-address>  local
```

这说明 TraeX 除 parent-child agent tree 外，还把其他本地会话暴露为 peer address。两者使用不同命名空间；完整 peer address 可避免与 in-process agent 同名时歧义。样本没有验证跨 peer 的成功消息往返，因此这里只确认“可发现、可寻址的界面存在”。

child 完成后，root rollout 收到独立的 inter-agent communication，关键语义为：

```text
kind: final_answer
author: /root/archimedes
recipient: /root
trigger_turn: true
```

也就是说，后台 child 的最终结果不是要求 root 主动轮询后才出现；完成消息会持久化并触发 parent 继续运行。多名 child 可以独立完成，root 按到达顺序消费结果，再决定汇总、补派或修复。

停止操作以 `/root/<person>` 为目标，并明确返回已停止的 agent task。样本中的 thread edge 状态没有随每个完成事件及时变成 closed，因此外部分析不应只看 index 的 edge status 判断运行态；应结合 activity、停止结果和 `final_answer`。

### 10.6 定向通信的证据边界

TraeX 的协作工具面提供向运行中 agent 发消息的入口，目标是 `/root/<person>`；当前运行环境也把它定义为保留 agent 上下文的继续通信。选取的历史样本确实记录了多次定向消息尝试，但这些调用因当时的参数字段不匹配而在 schema 校验阶段失败，没有产生成功投递证据。

因此，本报告只确认：

- agent path 是定向通信的地址形态；
- 接口意图支持在不创建新 child 的情况下继续联系后台 agent；
- 所选历史片段不能证明那几次消息实际送达，也不能据此断言 interrupt/queue 的具体语义。

这类失败也提示 Codex 若借鉴其交互，不应让同一“补充消息”动作同时存在多个易混淆目标字段；工具 schema、模型 usage hint 和实际 handler 必须同步测试。

### 10.7 与 Codex V1/V2 的交互对照

| 维度 | Codex V1 | Codex V2 | TraeX 会话样本 |
| --- | --- | --- | --- |
| 主要寻址 | UUID | `/root/<task_name>` | `/root/<person>` |
| 人名显示 | 独立 nickname，spawn result 明显 | Core 有 nickname，但持久状态默认只显示 task path | 人名直接编码进 path |
| 任务语义 | 初始 message/role | task name + initial message | 独立 task label + prompt |
| 后台并行 | agent thread，可 wait | task tree + mailbox | background spawn 立即返回，完成异步回传 |
| 完成通知 | status/result | `FINAL_ANSWER` mailbox | `final_answer` inter-agent message，并触发 parent |
| 继续协作 | `send_input` / resume | `send_message` / `followup_task` | path-targeted message interface；所选发送样本未成功 |
| 可见范围 | 当前 agent tree | 当前 V2 live tree | subagent tree 与 peer sessions 分栏 |
| 工作区 | 共享 cwd | 共享 cwd | 共享 cwd |

TraeX 最值得借鉴的是“**人名与任务同时可见**”，不必照搬“人名即地址”。对 Codex V2 更合适的形态仍是 `Curie · /root/research_a`：

- `Curie` 提供短、稳定于当前 thread 的视觉辨识；
- `/root/research_a` 保留调用者选择的任务语义和现有 mailbox 路由；
- 两者并列避免 TraeX 样本中 person path 被复用后，单看历史 `/root/euclid` 无法知道当时任务的问题。

### 10.8 对 default-new-subagent 的结论

这次行为调查支持把 V2 nickname 展示纳入本特性的用户体验，但不支持替换 V2 task-path 协议：

1. 新会话仍默认使用 V2 `/root/<task_name>` 作为权威身份；
2. Started activity 与 `/subagents` 持久状态卡同时显示 nickname 和 path；
3. picker、footer、activity 与 status card 使用同一展示格式和 fallback；
4. nickname 缺失或历史 metadata 不完整时只显示 path；
5. 模型工具参数、mailbox target 和 rollout wire format 不因展示增强而改变；
6. snapshot 测试覆盖 running/completed/error、长 nickname、长 path 和缺失 nickname。

该范围是现有 TUI 身份 metadata 的展示接线，不引入 peer sessions、不改变并发模型，也不照搬闭源产品的内部结构。

### 10.9 TraeX request dump 与 Codex V2 的模型契约对照

进一步检查了 `/home/chiyuki/Downloads/traex-request-dump.json`。该文件是一份 Responses `response.create` 请求快照，包含本次请求的 `instructions`、工具 definitions 和 wire flags。它带有 `previous_response_id`，且 `input` 只有本轮增量 tool output，因此它不是完整会话上下文：早先 response 中已经持久化的 developer/user items 不会在当前 JSON 里重复出现。以下“提示词中没有”只表示当前 `instructions` 字段没有，不能证明 server-side previous-response history 中从未注入。

#### 10.9.1 工具形态

TraeX 在这份请求里暴露五个与多 agent 直接相关的顶层工具：`Agent`、`ListAgents`、`SendMessage`、`TaskStop`、`TaskOutput`。它们不是独立 namespace，也没有 Codex V2 的六工具一一映射。

| 维度 | TraeX request dump | Codex V2 |
| --- | --- | --- |
| 创建工具 | `Agent(description, prompt, ...)` | `spawn_agent(task_name, message, ...)` |
| agent 名称 | runtime 自动分配 person-like name/path；description 是任务标签 | caller 选择局部 `task_name`，runtime 组成 canonical path；另有 nickname metadata |
| 启动模式 | `run_in_background` 可选；默认 foreground 等结果 | spawn 本身返回 canonical task name，后续通过 mailbox/通知协作 |
| 上下文继承 | 每次 `Agent` 从 fresh context 开始，要求 prompt 自足 | `fork_turns=all|none|N` 显式选择继承范围 |
| 隔离 | `isolation="worktree"` 可为单个 agent 建隔离工作树 | V2 schema 无 per-spawn worktree isolation；默认共享 cwd/filesystem |
| agent 类型 | `general-purpose`、`Explore`、`Plan` 等专用类型，可有不同工具权限 | `agent_type` 来自 Codex role 配置；bundled V2 hint 假定 team agent 能力对等，实际暴露还受模型支持约束 |
| 模型覆盖 | `model_provider` + 固定枚举 `model` | 可配置暴露 `model`、`reasoning_effort`、`service_tier`；完整历史 fork 禁止覆盖 |
| 继续任务 | `SendMessage(to, message)` 同时承担联系后台 agent 和恢复其上下文的用途 | `send_message` 只投递且不触发 turn；`followup_task` 明确触发 idle agent 的新 turn |
| 目标范围 | in-process agent、agent name/ID，以及 `/peers/...` 本地会话 | 当前 root thread tree 中的相对或 canonical task path |
| 列表 | `ListAgents` 同时列 subagents 与 peer sessions；当前 build 的过滤参数不可用 | `list_agents(path_prefix?)` 只列当前 V2 live tree，可按 path prefix 过滤 |
| 等待 | 无专用 multi-agent wait；foreground `Agent` 可阻塞，通用 `TaskOutput` 也可 block/poll，但文案已标记 deprecated | `wait_agent` 等任意 mailbox activity、用户 steer 或 timeout，不返回消息正文 |
| 中断 | 通用 `TaskStop` 同时处理 shell 和 agent；interrupted agent 可由 `SendMessage` 恢复 | `interrupt_agent` 只中断当前 turn，agent 明确保留给 message/follow-up |
| 完成结果 | foreground 直接成为 `Agent` result；background 自动通知，root 再向用户总结 | `FINAL_ANSWER` 作为 inter-agent message 投递 parent mailbox |
| schema 输出 | dump 中只有 input parameter schema，没有 per-tool output schema | spawn、wait、list、interrupt 均定义结构化 output schema |
| message 字段 | 普通 JSON string | `message`/task payload 使用 encrypted schema 标记 |

最本质的区别是：TraeX 把“创建一个有类型的独立 worker”做成一个高层 `Agent` 工具，再复用通用后台任务设施；Codex V2 把协作拆成 task tree、mailbox、follow-up、wait 和 interrupt 六个领域动作。TraeX 的表面积更小、产品导向更强，Codex V2 的状态机和消息语义更显式。

#### 10.9.2 前后台与并行调用

TraeX 的 `Agent` description 明确要求多个独立 agent 在同一 assistant message 中批量调用，并通过 `run_in_background=true` 立即返回；但这份 wire request 同时设置了：

```json
"parallel_tool_calls": false
```

这是当前快照中的直接矛盾：模型可见 prose 鼓励同轮多个 Agent calls，Responses request flag 却关闭 parallel tool calls。历史 session 证明多个后台 agent 最终可以并发运行，但不能由此断言模型在这一个 request 上会稳定地产生同轮并行 tool calls。

Codex 当前 `build_prompt()` 把 `parallel_tool_calls` 设为 true，普通 Responses 请求会把它传到 wire；只有 Responses Lite 路径另行关闭。因而 Codex V2 的“同轮 spawn 多个独立 agent”在 tool guidance 和普通 Responses wire flag 上更一致，见 `codex-rs/core/src/session/turn.rs:1313-1329`、`codex-rs/core/src/client.rs:930`。

#### 10.9.3 系统提示词与工具描述的职责分配

TraeX 当前 dump 的 multi-agent 策略主要分布在两处：

1. 通用 `instructions` 的 session-specific guidance：要求在适合专用 agent 时使用，宽泛代码搜索超过约三轮查询时优先 Explore，并说明后台 agent 完成会自动通知；
2. `Agent` 工具 description：约 7.5k 字符，集中定义 agent types、工具权限、何时使用/不使用、foreground/background、prompt 写法、并行模式、worktree isolation、model catalog 和示例。

当前 `instructions` 不含 `/root` team identity、agent 间消息 envelope、共享 filesystem 或 slot 数；但因为请求使用 `previous_response_id`，这里只能确认这些内容没有在本轮 `instructions` 重发。child session 样本另行证明 TraeX 会给新 child 注入专用 spawned-agent context。

Codex V2 的职责分配不同：

- `spawn_agent` description 主要解释 task path、递归 spawn、`fork_turns` 和完成回传，见 `codex-rs/core/src/tools/handlers/multi_agents_spec.rs:749-778`；
- root/subagent 各有 role-specific usage hint，明确 `/root` 身份、能力对等、`spawn_agent`/`followup_task`/`send_message` 分工、消息 envelope 和 final-channel 回传，见 `codex-rs/core/src/session/multi_agents.rs:11-49`；
- shared hint 另行声明共享 cwd/filesystem、直接 tool-call namespace、并发 slot、wait 和 model override 规则，见 `codex-rs/core/src/session/multi_agents.rs:50-59`、`110-128`；
- proactive/explicit policy 由 `MultiAgentMode` 独立决定，默认非 Ultra effort 为 ExplicitRequestOnly，见 `codex-rs/core/src/session/multi_agents.rs:145-184`。

因此 TraeX 更依赖一个很长、始终与 `Agent` tool schema 一起发送的工具说明；Codex V2 把稳定 team contract 放进按 root/subagent 选择的上下文 fragment，把单工具语义留在各自 description。后者分层更清楚，但 V2 role/shared hints 会成为每轮固定上下文成本，默认切换时必须继续关注 cache 稳定性。

#### 10.9.4 策略差异与产品取舍

- **主动性**：TraeX base guidance 倾向在匹配 agent 类型、广泛搜索或描述要求时主动委派；Codex V2 默认通过 ExplicitRequestOnly mode 收紧主动 spawn，Ultra 才默认 Proactive。
- **任务切分**：TraeX description 强调 foreground 用于阻塞性研究、background 用于真正独立工作；Codex V2 bundled spawn description 统一要求 bounded、可独立并且 parent 同时有本地工作。
- **上下文成本**：TraeX `Agent` description 单项约 7.5k 字符，另有约 2.7k 字符的通用 `TaskOutput` description；Codex V2 使用更多短工具 schema，加一份 role/shared hint。不能只按工具数量判断 Token 成本。
- **能力异质性**：TraeX 把 Explore/Plan 的只读和禁用 spawn 权限直接写进 Agent description；Codex V2 的默认文案强调能力对等，更适合通用协作 tree，但必须保证 child tool exposure 与文案一致。
- **通信抽象**：TraeX 一个 `SendMessage` 既可联系 agent 又可联系 peer，并隐含恢复/触发行为；Codex V2 将不触发 turn 的 message 与触发新 turn 的 follow-up 分开，语义更精确。
- **身份与展示**：TraeX 的人名是运行期地址，task label 独立；Codex V2 保留 task path 路由并并列 nickname，能同时保留任务语义和人物辨识度。

#### 10.9.5 对本特性的影响

这份 dump 没有改变 `default-new-subagent` 的协议选择：仍应默认 Codex V2，而不是把 V2 改造成 TraeX 的 `Agent` 单工具 facade。可以吸收的交互原则仅限于：

1. activity/status 同时显示短人名和任务路径；
2. spawn usage hint 清楚说明 child 是否继承上下文、是否共享工作区；
3. 工具 prose 与 `parallel_tool_calls`、实际 child tool exposure 保持一致；
4. 不把 `send_message` 与 `followup_task` 重新合并成含糊动作；
5. 保持 V2 role/shared hints 稳定，避免把会频繁变化的 model list 或 agent catalog 塞进每轮固定提示词。
