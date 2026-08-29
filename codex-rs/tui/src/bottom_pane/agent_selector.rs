use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::render::renderable::Renderable;
use codex_protocol::ThreadId;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Widget;
use std::time::Duration;
use std::time::Instant;

const MAX_VISIBLE_AGENTS: usize = 4;
const NORMAL_HINT: &str = "按 ↓ 以聚焦其它 agent";
const SELECTING_HINT: &str = "↑/↓ 选择 · Enter 以聚焦该 agent · Esc 返回";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentSelectorEntry {
    pub(crate) thread_id: ThreadId,
    pub(crate) label: String,
    pub(crate) depth: usize,
    pub(crate) is_running: bool,
    pub(crate) is_closed: bool,
    pub(crate) status_changed_at: Instant,
}

#[derive(Debug, Default)]
pub(super) struct AgentSelector {
    entries: Vec<AgentSelectorEntry>,
    current_thread_id: Option<ThreadId>,
    selected_thread_id: Option<ThreadId>,
    selecting: bool,
}

impl AgentSelector {
    pub(super) fn update(
        &mut self,
        entries: Vec<AgentSelectorEntry>,
        current_thread_id: Option<ThreadId>,
    ) -> bool {
        let previous_entries = std::mem::replace(&mut self.entries, entries);
        let previous_current = self.current_thread_id;
        self.current_thread_id = current_thread_id;

        if self
            .selected_thread_id
            .is_none_or(|selected| !self.entries.iter().any(|entry| entry.thread_id == selected))
        {
            self.selected_thread_id = current_thread_id
                .filter(|current| self.entries.iter().any(|entry| entry.thread_id == *current))
                .or_else(|| self.entries.first().map(|entry| entry.thread_id));
        }
        if !self.has_other_agent() {
            self.selecting = false;
        }

        previous_entries != self.entries || previous_current != self.current_thread_id
    }

    pub(super) fn enter_selection(&mut self) -> bool {
        if !self.has_other_agent() {
            return false;
        }
        self.selecting = true;
        self.selected_thread_id = self
            .current_thread_id
            .filter(|current| self.entries.iter().any(|entry| entry.thread_id == *current))
            .or_else(|| self.entries.first().map(|entry| entry.thread_id));
        true
    }

    pub(super) fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        app_event_tx: &AppEventSender,
    ) -> bool {
        if !self.selecting || !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            return false;
        }

        match key_event.code {
            KeyCode::Up => self.move_selection(/*forward*/ false),
            KeyCode::Down => self.move_selection(/*forward*/ true),
            KeyCode::Enter => {
                if let Some(thread_id) = self.selected_thread_id {
                    app_event_tx.send(AppEvent::SelectAgentThread(thread_id));
                }
                self.selecting = false;
            }
            KeyCode::Esc => self.selecting = false,
            _ => return true,
        }
        true
    }

    pub(super) fn is_visible(&self) -> bool {
        self.has_other_agent()
    }

    fn has_other_agent(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| Some(entry.thread_id) != self.current_thread_id)
    }

    fn move_selection(&mut self, forward: bool) {
        if self.entries.is_empty() {
            return;
        }
        let current_index = self
            .selected_thread_id
            .and_then(|selected| {
                self.entries
                    .iter()
                    .position(|entry| entry.thread_id == selected)
            })
            .unwrap_or(0);
        let next_index = if forward {
            (current_index + 1) % self.entries.len()
        } else if current_index == 0 {
            self.entries.len() - 1
        } else {
            current_index - 1
        };
        self.selected_thread_id = Some(self.entries[next_index].thread_id);
    }

    fn visible_entries(&self) -> Vec<&AgentSelectorEntry> {
        let candidates = self
            .entries
            .iter()
            .filter(|entry| {
                self.selecting
                    || Some(entry.thread_id) == self.current_thread_id
                    || (entry.is_running && !entry.is_closed)
            })
            .collect::<Vec<_>>();
        let anchor = if self.selecting {
            self.selected_thread_id
        } else {
            self.current_thread_id
        };
        let anchor_index = anchor
            .and_then(|thread_id| {
                candidates
                    .iter()
                    .position(|entry| entry.thread_id == thread_id)
            })
            .unwrap_or(0);
        let max_start = candidates.len().saturating_sub(MAX_VISIBLE_AGENTS);
        let start = anchor_index
            .saturating_sub(MAX_VISIBLE_AGENTS - 1)
            .min(max_start);
        candidates
            .into_iter()
            .skip(start)
            .take(MAX_VISIBLE_AGENTS)
            .collect()
    }

    fn desired_height(&self) -> u16 {
        if !self.is_visible() {
            return 0;
        }
        1_u16.saturating_add(self.visible_entries().len() as u16)
    }

    fn render_at(&self, area: Rect, buf: &mut Buffer, now: Instant) {
        if area.is_empty() || !self.is_visible() {
            return;
        }
        let content = Rect {
            x: area.x.saturating_add(2),
            y: area.y,
            width: area.width.saturating_sub(4),
            height: area.height,
        };
        if content.is_empty() {
            return;
        }

        let hint = if self.selecting {
            SELECTING_HINT.to_string()
        } else {
            let idle_count = self
                .entries
                .iter()
                .filter(|entry| {
                    !entry.is_running
                        && !entry.is_closed
                        && Some(entry.thread_id) != self.current_thread_id
                })
                .count();
            let other_running = self.entries.iter().any(|entry| {
                entry.is_running
                    && !entry.is_closed
                    && Some(entry.thread_id) != self.current_thread_id
            });
            if !other_running && idle_count > 0 {
                format!("{idle_count} 个 agent 空闲中 · {NORMAL_HINT}")
            } else {
                NORMAL_HINT.to_string()
            }
        };
        Line::from(hint)
            .dim()
            .right_aligned()
            .render(Rect::new(content.x, content.y, content.width, 1), buf);

        for (row, entry) in self.visible_entries().into_iter().enumerate() {
            let y = content.y.saturating_add(1 + row as u16);
            if y >= content.bottom() {
                break;
            }
            let selected = self.selecting && self.selected_thread_id == Some(entry.thread_id);
            let current = self.current_thread_id == Some(entry.thread_id);
            render_split_line(
                Rect::new(content.x, y, content.width, 1),
                buf,
                entry_line(entry, selected, current),
                status_line(entry, now),
            );
        }
    }
}

impl Renderable for AgentSelector {
    fn desired_height(&self, _width: u16) -> u16 {
        self.desired_height()
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_at(area, buf, Instant::now());
    }
}

fn entry_line(entry: &AgentSelectorEntry, selected: bool, current: bool) -> Line<'static> {
    let mut spans = vec![if selected { "> ".bold() } else { "  ".into() }];
    let tree_prefix = tree_prefix(entry.depth);
    if !tree_prefix.is_empty() {
        spans.push(tree_prefix.dim());
    }
    let dot = if entry.is_running && !entry.is_closed {
        "●".green()
    } else {
        "●".into()
    };
    spans.push(dot);
    spans.push(" ".into());
    let label = if current {
        entry.label.clone().bold()
    } else if entry.is_closed {
        entry.label.clone().dim()
    } else {
        entry.label.clone().into()
    };
    spans.push(label);
    Line::from(spans)
}

fn tree_prefix(depth: usize) -> String {
    if depth == 0 {
        String::new()
    } else {
        format!("{}└ ", "  ".repeat(depth.saturating_sub(1)))
    }
}

fn status_line(entry: &AgentSelectorEntry, now: Instant) -> Line<'static> {
    let state = if entry.is_closed {
        "已关闭"
    } else if entry.is_running {
        "运行中"
    } else {
        "空闲"
    };
    let elapsed = now.saturating_duration_since(entry.status_changed_at);
    Line::from(format!("{state} · {}", format_duration(elapsed))).dim()
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else if seconds == 0 {
        format!("{minutes}min")
    } else {
        format!("{minutes}min{seconds}s")
    }
}

fn render_split_line(area: Rect, buf: &mut Buffer, left: Line<'static>, right: Line<'static>) {
    if area.is_empty() {
        return;
    }
    let right_width = right.width().min(area.width as usize);
    let left_width = area.width as usize - right_width;
    let left = truncate_line_with_ellipsis_if_overflow(left, left_width.saturating_sub(1));
    left.render(Rect::new(area.x, area.y, left_width as u16, 1), buf);
    right.right_aligned().render(
        Rect::new(
            area.x.saturating_add(left_width as u16),
            area.y,
            right_width as u16,
            1,
        ),
        buf,
    );
}

#[cfg(test)]
#[path = "agent_selector_tests.rs"]
mod tests;
