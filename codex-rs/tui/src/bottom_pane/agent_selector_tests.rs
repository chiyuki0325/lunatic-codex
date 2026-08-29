use super::*;
use crate::app_event_sender::AppEventSender;
use crossterm::event::KeyModifiers;
use insta::assert_snapshot;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use tokio::sync::mpsc::unbounded_channel;
use unicode_width::UnicodeWidthStr;

fn thread_id(value: u128) -> ThreadId {
    ThreadId::from_string(&format!("{value:032x}")).expect("valid thread id")
}

fn entry(
    value: u128,
    label: &str,
    depth: usize,
    is_running: bool,
    is_closed: bool,
    status_changed_at: Instant,
) -> AgentSelectorEntry {
    AgentSelectorEntry {
        thread_id: thread_id(value),
        label: label.to_string(),
        depth,
        is_running,
        is_closed,
        status_changed_at,
    }
}

fn snapshot(selector: &AgentSelector, now: Instant, width: u16) -> String {
    let area = Rect::new(0, 0, width, selector.desired_height());
    let mut buffer = Buffer::empty(area);
    selector.render_at(area, &mut buffer, now);
    (0..area.height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < area.width {
                let symbol = buffer[(x, y)].symbol();
                line.push_str(symbol);
                x = x.saturating_add(symbol.width().max(1) as u16);
            }
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn normal_preview_omits_idle_and_closed_agents() {
    let now = Instant::now();
    let root = thread_id(1);
    let mut selector = AgentSelector::default();
    selector.update(
        vec![
            entry(1, "Codex [default]", 0, true, false, now),
            entry(
                2,
                "Curie · 调研协议 [explorer]",
                0,
                true,
                false,
                now - Duration::from_secs(30),
            ),
            entry(
                3,
                "Turing · 核对工具定义 [default]",
                1,
                false,
                false,
                now - Duration::from_secs(90),
            ),
            entry(
                4,
                "Hopper [worker]",
                0,
                false,
                true,
                now - Duration::from_secs(120),
            ),
        ],
        Some(root),
    );

    assert_snapshot!(snapshot(&selector, now, 86), @r###"
                                                                 按 ↓ 以聚焦其它 agent
      ● Codex [default]                                                    运行中 · 0s
      ● Curie · 调研协议 [explorer]                                       运行中 · 30s
    "###);
}

#[test]
fn selection_scrolls_to_nested_candidate_and_keeps_closed_entries() {
    let now = Instant::now();
    let root = thread_id(1);
    let mut selector = AgentSelector::default();
    selector.update(
        vec![
            entry(1, "Codex [default]", 0, true, false, now),
            entry(2, "Curie [explorer]", 0, true, false, now),
            entry(3, "Turing [default]", 1, false, false, now),
            entry(4, "Hopper [worker]", 2, true, false, now),
            entry(5, "Lovelace [reviewer]", 0, false, true, now),
            entry(6, "Shannon [default]", 0, false, false, now),
        ],
        Some(root),
    );
    assert!(selector.enter_selection());
    let (tx, _rx) = unbounded_channel();
    let tx = AppEventSender::new(tx);
    for _ in 0..5 {
        selector.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &tx);
    }

    assert_snapshot!(snapshot(&selector, now + Duration::from_secs(90), 86), @r###"
                                              ↑/↓ 选择 · Enter 以聚焦该 agent · Esc 返回
        └ ● Turing [default]                                              空闲 · 1min30s
          └ ● Hopper [worker]                                           运行中 · 1min30s
        ● Lovelace [reviewer]                                           已关闭 · 1min30s
      > ● Shannon [default]                                               空闲 · 1min30s
    "###);
}

#[test]
fn normal_preview_summarizes_idle_agents() {
    let now = Instant::now();
    let root = thread_id(1);
    let mut selector = AgentSelector::default();
    selector.update(
        vec![
            entry(1, "Codex [default]", 0, true, false, now),
            entry(2, "Curie [explorer]", 0, false, false, now),
            entry(3, "Turing [worker]", 0, false, false, now),
        ],
        Some(root),
    );

    assert_snapshot!(snapshot(&selector, now, 86), @r###"
                                             2 个 agent 空闲中 · 按 ↓ 以聚焦其它 agent
      ● Codex [default]                                                    运行中 · 0s
    "###);
}

#[test]
fn narrow_preview_truncates_only_the_agent_label() {
    let now = Instant::now();
    let root = thread_id(1);
    let mut selector = AgentSelector::default();
    selector.update(
        vec![
            entry(1, "Codex [default]", 0, true, false, now),
            entry(
                2,
                "Curie · 调研一个只有终端宽度不足时才折叠的语义名称 [explorer]",
                0,
                true,
                false,
                now - Duration::from_secs(30),
            ),
        ],
        Some(root),
    );

    assert_snapshot!(snapshot(&selector, now, 48), @r###"
                           按 ↓ 以聚焦其它 agent
      ● Codex [default]              运行中 · 0s
      ● Curie · 调研一个只有终端宽… 运行中 · 30s
    "###);
}

#[test]
fn idle_dot_uses_terminal_default_foreground() {
    let now = Instant::now();
    let root = thread_id(1);
    let mut selector = AgentSelector::default();
    selector.update(
        vec![
            entry(1, "Codex [default]", 0, true, false, now),
            entry(2, "Curie [default]", 0, false, false, now),
        ],
        Some(root),
    );
    selector.enter_selection();
    let area = Rect::new(0, 0, 60, selector.desired_height());
    let mut buffer = Buffer::empty(area);
    selector.render_at(area, &mut buffer, now);

    let idle_dot = (0..area.width)
        .find_map(|x| {
            let cell = &buffer[(x, 2)];
            (cell.symbol() == "●").then_some(cell)
        })
        .expect("idle status dot");
    assert_eq!(idle_dot.fg, Color::Reset);
}

#[test]
fn enter_emits_focus_event_and_escape_does_not() {
    let now = Instant::now();
    let root = thread_id(1);
    let child = thread_id(2);
    let mut selector = AgentSelector::default();
    selector.update(
        vec![
            entry(1, "Codex [default]", 0, true, false, now),
            entry(2, "Curie [default]", 0, true, false, now),
        ],
        Some(root),
    );
    let (tx, mut rx) = unbounded_channel();
    let tx = AppEventSender::new(tx);

    selector.enter_selection();
    selector.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &tx);
    selector.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &tx);
    assert!(matches!(rx.try_recv(), Ok(AppEvent::SelectAgentThread(id)) if id == child));
    assert!(rx.try_recv().is_err());

    selector.enter_selection();
    selector.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &tx);
    assert!(rx.try_recv().is_err());
}
