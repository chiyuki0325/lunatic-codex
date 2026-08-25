# Codex「月狂氛围」

本仓库为我个人自用的 Coding Agent，fork 自 [openai/codex](https://github.com/openai/codex)，比起上游 Codex CLI，追加了我个人的一些偏好改动。

目前基于上游版本 **v0.149.1**。

## 当前特性：异步命令

本分支实现斜杠命令在 agent 工作期间的非阻塞交互。命令是否需要等待，应由其实际语义决定，而不是统一等待当前 turn 完成。

### 需求

- `/ps` 等只读查看命令在 agent 工作期间按 Enter 后立即执行，不等待当前 turn 或 tool call 完成。
- `/model` 在 agent 工作期间立即打开模型列表；用户确认选择后，新设置从下一 turn 开始生效，当前 turn 继续使用启动时捕获的模型和推理设置。
- `/permissions`、`/personality` 和 service tier 等下一 turn 设置采用与 `/model` 相同的行为：选择界面立即打开，设置更新不得阻塞 TUI，且不改变当前 turn。
- `/resume` 菜单在 agent 工作期间立即打开，不等待当前 turn 完成；用户选定会话后再按既有会话切换流程执行，不冻结当前 TUI。
- 会话启动和 MCP 初始化期间采用与 agent 工作期间相同的命令可用性规则，不再统一屏蔽输入。只读命令和菜单应立即可用，下一 turn 设置可先选择，并在当前会话完成配置后应用。
- 审计其他实现上会阻塞、但语义上不需要等待 agent 的命令。纯读取、纯 TUI 操作或只影响后续 turn 的命令应允许立即执行，首批包括 `/export`、`/keymap`、`/vim`、`/theme` 和 `/pets`。
- 会启动模型工作或改变当前会话生命周期、且无法安全异步处理的命令继续在任务运行期间禁用，包括 `/init`、`/compact`、`/review`、`/new`、`/clear`、`/fork` 和 `/cd`。这里的 `/init` 指创建 `AGENTS.md` 的斜杠命令，不是会话启动初始化阶段。
- 保持现有按键语义：Enter 立即提交或 steer，Tab 显式排队到当前 turn 结束。
- 保留现有 `/model` 命令名，不新增 `/models` 别名。

### 非目标

- 不在当前 turn 的 tool call 边界切换模型、推理强度、权限或 personality。
- 不改变普通用户消息的 steer 和排队协议。
- 不放宽会与当前任务竞争或破坏会话状态的命令。

### 验收条件

- agent 工作期间执行 `/ps`，结果立即显示。
- agent 工作期间执行 `/model`，模型列表立即显示，选择操作不冻结 TUI，新模型仅用于下一 turn。
- agent 工作期间执行 `/resume`，会话菜单立即显示，选择和切换过程不冻结 TUI。
- 会话启动或 MCP 初始化尚未完成时，所有在 agent 工作期间允许的命令保持可用，并遵循相同的即时执行或异步应用语义。
- 其他被判定为非阻塞的命令在 agent 工作期间可立即使用。
- Tab 排队行为和当前禁止命令的错误提示保持不变。
- 设置更新失败时仍能向用户显示错误，且不会卡住后续输入处理。
- 所有用户可见变化均有对应的 TUI snapshot 或行为测试。

## 维护方式和特性列表

由于 Codex 上游仓库采用 vibe coding，代码变更速度和规模都前所无比地大，所以 Codex「月狂氛围」不采用传统的补丁集维护，而采用分多个特性分支的维护方式：

- 每个特性建立一个分支（`lunatic/feature-id`，一般基于当前版本的模板分支 `lunatic/template-v0.149.1`，或前置特性分支）
- 人类在该分支 README 写好需求，agent 在该分支单独实现功能，并记录开发过程中的 plan、spec 等中间产物
- 单独把该特性的代码实现移摘到「月狂氛围」分支（当前版本 `lunatic/v0.149.1`）

每次上游版本更新之后，传统 pick 方式必然导致大规模冲突。以这种方式维护，就不需要大规模地重新 explore 然后重写所有补丁，而可以快速移植。

以下为特性列表，`[ ]` 为在此版本未集成的特性，`[x]` 为已集成的特性。分支位于 `lunatic/feature-id`。集成完毕后更新状态。

- [ ] 界面全部中文化（`chinese-translation`）
- [ ] 输入框、工作中分行、更新屏蔽等界面定制和改善（`ui-customization`）
- [ ] /ps /model 等命令改为即时查看、异步提交（`async-commands`）
- [ ] /raw 裸 markdown 渲染模式（`cc-raw`）
- [ ] /context 上下文窗口占用查看（`cc-context`）
- [ ] /rewind 回滚当前会话到先前位点（`cc-rewind`）
- [ ] Worktree 工具调用强制隔离（`worktree-isolation`）
- [ ] 输入框 Tab 键进入 raw 模式（`input-box-raw`）
- [ ] 默认开启新版 Subagent（`default-new-subagent`）
- [ ] User Answer Questions 不需要必须 plan 模式 + 功能改善（`survey-while-working`）
- [ ] 批准工具调用后可快捷提权（`yes-then-yolo`）
- [ ] 提供多套工具定义，可切换 Claude、DSH 风格（`multi-tool-shapes`）
- [ ] 自带 MCP 提供的代码智能支持，像 CC 一样提示安装 LSP（`mcp-lsp-bridge`）
- [ ] 上下文压缩抑制尝试（`suppress-autocompact`）

## 命名来源

GPT-5.6 Luna 模型和 K-forest 的乐曲 Lunatic Vibes。

---


<p align="center"><strong>Codex CLI</strong> is a coding agent from OpenAI that runs locally on your computer.
<p align="center">
  <img src=".github/codex-cli-splash.png" alt="Codex CLI splash" width="80%" />
</p>
</br>
If you want Codex in your code editor (VS Code, Cursor, Windsurf), <a href="https://developers.openai.com/codex/ide">install in your IDE.</a>
</br>If you want the desktop app experience, run <code>codex app</code> or visit <a href="https://chatgpt.com/codex?app-landing-page=true">the Codex App page</a>.
</br>If you are looking for the <em>cloud-based agent</em> from OpenAI, <strong>Codex Web</strong>, go to <a href="https://chatgpt.com/codex">chatgpt.com/codex</a>.</p>


---

## Quickstart

### Installing and running Codex CLI

Run the following on Mac or Linux to install Codex CLI:

```shell
curl -fsSL https://chatgpt.com/codex/install.sh | sh
```

Run the following on Windows to install Codex CLI:

```shell
powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"
```

The standalone installers download from `https://releases.openai.com/codex` by default and fall back to GitHub Releases if a metadata or asset download is unavailable. To force GitHub Releases, set `CODEX_INSTALLER_USE_RELEASES_OPENAI_COM` to `false` (`0` and `no` are also accepted):

```shell
curl -fsSL https://chatgpt.com/codex/install.sh | CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false sh
```

```powershell
$env:CODEX_INSTALLER_USE_RELEASES_OPENAI_COM='false'; irm https://chatgpt.com/codex/install.ps1 | iex
```

Codex CLI can also be installed via the following package managers:

```shell
# Install using npm
npm install -g @openai/codex
```

```shell
# Install using Homebrew
brew install --cask codex
```

Then simply run `codex` to get started.

<details>
<summary>You can also go to the <a href="https://github.com/openai/codex/releases/latest">latest GitHub Release</a> and download the appropriate binary for your platform.</summary>

Each GitHub Release contains many executables, but in practice, you likely want one of these:

- macOS
  - Apple Silicon/arm64: `codex-aarch64-apple-darwin.tar.gz`
  - x86_64 (older Mac hardware): `codex-x86_64-apple-darwin.tar.gz`
- Linux
  - x86_64: `codex-x86_64-unknown-linux-musl.tar.gz`
  - arm64: `codex-aarch64-unknown-linux-musl.tar.gz`

Each archive contains a single entry with the platform baked into the name (e.g., `codex-x86_64-unknown-linux-musl`), so you likely want to rename it to `codex` after extracting it.

</details>

### Using Codex with your ChatGPT plan

Run `codex` and select **Sign in with ChatGPT**. We recommend signing into your ChatGPT account to use Codex as part of your Plus, Pro, Business, Edu, or Enterprise plan. [Learn more about what's included in your ChatGPT plan](https://help.openai.com/en/articles/11369540-codex-in-chatgpt).

You can also use Codex with an API key, but this requires [additional setup](https://developers.openai.com/codex/auth#sign-in-with-an-api-key).

## Docs

- [**Codex Documentation**](https://developers.openai.com/codex)
- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Open source fund**](./docs/open-source-fund.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
