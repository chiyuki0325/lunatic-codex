# Caffeine：工作期间阻止系统空闲睡眠

这是 Codex「月狂氛围」的特性开发分支，基于上游 **v0.149.1**。

> 对于任何编码智能体：当在 Linux arm64 平台上开发时，禁止本机编译（本机性能不足，编译 Codex 需要至少 32G 内存），必须参考 `building-codex-in-devbox.txt` 在远端开发机编译。

## 现有机制

上游已经完整实现 `prevent_idle_sleep` 功能，并由 TUI 的 agent turn 生命周期驱动：turn 开始或恢复时取得系统空闲抑制，turn 结束、切换会话或 TUI 退出时释放。

- macOS 使用 IOKit `PreventUserIdleSystemSleep` 电源断言。
- Linux 优先运行 `systemd-inhibit --what=idle`，不可用时回退到 `gnome-session-inhibit --inhibit idle`。
- Windows 已有对应实现，但本特性分支暂不改变其上游实验状态。
- 后端不可用或启动失败时仅记录警告，不影响 Codex turn 正常运行。

上游将该功能标记为实验性并默认关闭，因此用户未配置时不会取得空闲抑制。

## 详细需求

### 需求

- 在 Linux 和 macOS 上将现有 `prevent_idle_sleep` 功能视为稳定功能并默认开启。
- 仅在 agent turn 实际运行期间阻止系统因空闲进入睡眠；空闲输入态不持有抑制。
- 保留 `features.prevent_idle_sleep = false` 配置覆盖，使用户可以显式关闭默认行为。
- 保留现有平台实现、生命周期接线、失败降级和资源释放行为。

### 非目标

- 不阻止用户主动睡眠、关机、合盖或手动锁屏。
- 不保证绕过桌面环境或组织策略强制触发的锁屏；Linux 的 idle inhibitor 是否同时抑制自动锁屏取决于桌面环境。
- 不改变 Windows 和其他平台的默认状态。
- 不新增命令、设置项、状态提示或新的系统依赖。

### 验收标准

- Linux 和 macOS 上，未配置功能开关时 `Feature::PreventIdleSleep` 默认启用。
- Linux 和 macOS 上，该功能不再出现在 `/experimental` 列表中。
- 显式配置 `features.prevent_idle_sleep = false` 后仍可关闭功能。
- agent turn 开始、结束和恢复时沿用现有逻辑正确取得或释放空闲抑制。
- Windows 仍保持实验性且默认关闭；其他平台仍处于开发中且默认关闭。

## 开发记录

- `codex-rs/features/src/lib.rs`：按目标平台调整 `PreventIdleSleep` 的阶段和默认值。
- 现有平台后端和 TUI 生命周期实现无需修改。
