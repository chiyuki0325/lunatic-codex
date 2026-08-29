# Multi-agent enhancement implementation plan

## Context

Codex Multi-agent V2 allows a subagent to spawn its own subagents. Nested agents are represented by hierarchical `AgentPath` values such as `/root/parent/child` and share the session tree's concurrency limit. The TUI selector must preserve this relationship while allowing immediate focus changes without waiting for, pausing, interrupting, or cancelling running agents.

This branch contains three independent features:

1. Make V2 the default for new sessions.
2. Generate configurable-language semantic agent names asynchronously without changing the reserved Responses API tool definitions.
3. Replace the popup-oriented subagent picker with an inline hierarchical selector below the footer.

The Multi-agent V2 reserved tool names, descriptions, input/output schemas, and required fields must remain byte-for-byte unchanged; changing their shape causes the Responses API to reject requests with HTTP 400.

## Selector requirements

- Build a stable session tree from existing `AgentPath` parent-child relationships.
- Use `Codex [default]` as the root fallback identity.
- Prefer `nickname · semantic_name [role]` for subagents, falling back cleanly when metadata is absent and using `[default]` when role is unavailable.
- Keep the existing running, idle, and closed lifecycle. Track elapsed time from the latest TUI-observed running/idle transition; after resume, begin at the first observation when historical timing is unavailable.
- Render the selector below the existing model/context footer only while other agents exist.
- In normal mode, show the current identity in bold, a right-aligned focus hint, and at most four running agents. Idle and closed agents do not consume preview rows.
- If no other agent is running but idle agents exist, show `x 个 agent 空闲中 · 按 ↓ 以聚焦其它 agent`.
- In selection mode, show at most four scrollable candidates including running, idle, and retained closed entries. Keep the selected candidate visible.
- Render nested agents depth-first with cumulative tree indentation and a `└`-style branch prefix.
- Use a green `●` for running and the terminal's default foreground for idle. Keep closed entries dimmed. Never force white.
- Mark the selected candidate with a leftmost `>` independently of bolding the currently focused agent.
- Preserve full semantic names whenever space permits; truncate only when the terminal is too narrow, using Unicode display width and reserving room for right-aligned status text.
- Enter selection mode with `↓` only when the composer is empty and no popup, modal, history-navigation, or paste interaction conflicts.
- In selection mode, `↑/↓` cycles, `Enter` focuses and exits, and `Esc` exits without switching.
- `/subagents` refreshes state and enters the inline selector. Existing `Alt+←/→` direct navigation remains available.

## Implementation boundaries

- Add selector rendering and local interaction state in a dedicated `codex-rs/tui/src/bottom_pane/agent_selector.rs` module rather than growing `chat_composer.rs`.
- Extend `codex-rs/tui/src/app/agent_navigation.rs` for stable hierarchical ordering and observed lifecycle timing.
- Reuse the existing thread attach/switch path in `app/event_dispatch.rs`, `app/session_lifecycle.rs`, and `app/thread_routing.rs` so switching remains non-blocking.
- Make only the layout delegation needed in `bottom_pane/chat_composer.rs` and the input routing needed in `app/input.rs`.
- Do not modify `codex-rs/core/src/tools/handlers/multi_agents_spec.rs`.

## Verification

- Add focused tests for hierarchy, stable ordering, identity fallback, lifecycle timing, idle counts, scrolling, and input routing.
- Add `insta` snapshots for normal preview, idle summary, selection state, root and role labels, three-level nesting, scrolling, narrow widths, and default-foreground idle dots.
- Verify that focusing another thread while an agent runs does not emit interrupt or cancel behavior.
- Run `just fmt` locally after all changes.
- Run `just test -p codex-tui`, review and accept intended snapshots, and run `just fix -p codex-tui` on the remote devbox described by `building-codex-in-devbox.txt`.
- Interactively verify running, idle, nested spawn, scrolling beyond four entries, non-blocking focus changes, narrow terminals, and a light terminal theme on the remote devbox.
