# Codex「月狂氛围」

本仓库为我个人自用的 Coding Agent，fork 自 [openai/codex](https://github.com/openai/codex)，比起上游 Codex CLI，追加了我个人的一些偏好改动。

目前基于上游版本 **v0.149.1**。

## 维护方式和特性列表

由于 Codex 上游仓库采用 vibe coding，代码变更速度和规模都前所无比地大，所以 Codex「月狂氛围」不采用传统的补丁集维护，而采用分多个特性分支的维护方式：

- 每个特性建立一个分支（`lunatic/feature-id`，一般基于当前版本的模板分支 `lunatic/template-v0.149.1`，或前置特性分支）
- 人类在该分支 README 写好需求，agent 在该分支单独实现功能，并记录开发过程中的 plan、spec 等中间产物
- 单独把该特性的代码实现移摘到「月狂氛围」分支（当前版本 `lunatic/v0.149.1`）

每次上游版本更新之后，传统 pick 方式必然导致大规模冲突。以这种方式维护，就不需要大规模地重新 explore 然后重写所有补丁，而可以快速移植。

以下为特性列表，`[ ]` 为在此版本未集成的特性，`[x]` 为已集成的特性。分支位于 `lunatic/feature-id`。集成完毕后更新状态。

- [x] 界面全部中文化（`chinese-translation`）
- [ ] 输入框、工作中分行、更新屏蔽等界面定制和改善（`ui-customization`）
- [x] /ps /model 等命令改为即时查看、异步提交（`async-commands`）
- [x] /context 上下文窗口占用查看（`cc-context`）
- [ ] /rewind 回滚当前会话到先前位点（`cc-rewind`）
- [ ] Worktree 工具调用强制隔离（`worktree-isolation`）
- [ ] 输入框 Ctrl+Tab 键进入 raw 模式（`input-box-raw`）
- [ ] 改善 multi-agent 交互体验（`enhance-multi-agent`）
- [ ] User Answer Questions 不需要必须 plan 模式 + 功能改善（`survey-while-working`）
- [ ] 批准工具调用后可快捷提权（`yes-then-yolo`）
- [ ] 自带 MCP 提供的代码智能支持，像 CC 一样提示安装 LSP（`mcp-lsp-bridge`）
- [ ] 上下文压缩抑制尝试（`suppress-autocompact`）
- [x] 工作运行时阻止自动睡眠和锁屏（平台相关  Linux / macOS）（`caffeine`）
- [ ] 维护缓存率，底栏显示，大规模 miss 时弹信息可观测（`cache-observability`）
- [ ] 系统提示词模型身份感知（`system-prompt-model-id`）
- [ ] 思考时 Esc 结束思考直接回到输入态（`interrupt-back-to-input`）
- [ ] cd 命令，带参数切换文件夹，不带参数则刷新文件夹和分支（`cd-update-path`）

对于任何编码智能体：当在 Linux arm64 平台上开发时，禁止本机编译（本机性能不足，编译 Codex 需要至少 32G 内存），必须参考 `building-codex-in-devbox.txt` 在远端开发机编译。

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
