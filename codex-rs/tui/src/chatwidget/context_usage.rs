//! Coordinates asynchronous `/context` cards in invocation order.

use super::*;
use crate::history_cell::ContextUsageHistoryCell;
use codex_app_server_protocol::ThreadContextUsageResponse;

#[derive(Debug)]
pub(super) enum ContextUsageQueueState {
    Loading(ContextUsageHistoryCell),
    Ready(ContextUsageHistoryCell),
}

#[derive(Debug)]
pub(super) struct PendingContextUsageOutput {
    request_id: u64,
    thread_id: Option<ThreadId>,
    pub(super) state: ContextUsageQueueState,
}

impl PendingContextUsageOutput {
    fn loading(request_id: u64, thread_id: Option<ThreadId>) -> Self {
        Self {
            request_id,
            thread_id,
            state: ContextUsageQueueState::Loading(ContextUsageHistoryCell::loading()),
        }
    }

    fn as_history_cell(&self) -> &ContextUsageHistoryCell {
        match &self.state {
            ContextUsageQueueState::Loading(cell) | ContextUsageQueueState::Ready(cell) => cell,
        }
    }
}

impl ChatWidget {
    pub(crate) fn start_context_usage_refresh(
        &mut self,
        request_id: u64,
        thread_id: Option<ThreadId>,
    ) {
        self.pending_context_usage_outputs
            .push_back(PendingContextUsageOutput::loading(request_id, thread_id));
        self.bump_active_cell_revision();
        self.request_redraw();
    }

    pub(super) fn pending_context_usage_output(&self) -> Option<&ContextUsageHistoryCell> {
        self.pending_context_usage_outputs
            .iter()
            .find_map(|output| match output.state {
                ContextUsageQueueState::Loading(_) => Some(output.as_history_cell()),
                ContextUsageQueueState::Ready(_) => None,
            })
            .or_else(|| {
                self.pending_context_usage_outputs
                    .iter()
                    .find_map(|output| match output.state {
                        ContextUsageQueueState::Ready(_) => Some(output.as_history_cell()),
                        ContextUsageQueueState::Loading(_) => None,
                    })
            })
    }

    pub(crate) fn finish_context_usage_refresh(
        &mut self,
        request_id: u64,
        thread_id: ThreadId,
        result: Result<ThreadContextUsageResponse, String>,
    ) -> bool {
        let Some(output) = self
            .pending_context_usage_outputs
            .iter_mut()
            .find(|output| output.request_id == request_id)
        else {
            return false;
        };

        if output.thread_id != Some(thread_id) {
            return false;
        }

        output.state = ContextUsageQueueState::Ready(match result {
            Ok(response) => ContextUsageHistoryCell::success(
                response.snapshot,
                response.actual_usage,
                response.actual_source,
                response.last_completed_snapshot_id,
            ),
            Err(message) => ContextUsageHistoryCell::error(normalize_context_usage_error(message)),
        });
        self.bump_active_cell_revision();
        self.request_redraw();
        self.request_pending_usage_output_insertion();
        true
    }

    pub(crate) fn take_completed_context_usage_output(
        &mut self,
    ) -> Option<ContextUsageHistoryCell> {
        if self.usage_history_insertion_blocked() {
            return None;
        }
        let ready_count = self
            .pending_context_usage_outputs
            .iter()
            .take_while(|output| matches!(output.state, ContextUsageQueueState::Ready(_)))
            .count();
        if ready_count == 0 {
            return None;
        }
        let output = self.pending_context_usage_outputs.pop_front()?;
        self.bump_active_cell_revision();
        match output.state {
            ContextUsageQueueState::Ready(cell) => Some(cell),
            ContextUsageQueueState::Loading(_) => None,
        }
    }

    pub(crate) fn clear_pending_context_usage_refreshes(&mut self) {
        if self.pending_context_usage_outputs.is_empty() {
            return;
        }
        self.pending_context_usage_outputs.clear();
        self.bump_active_cell_revision();
        self.request_redraw();
    }

    pub(crate) fn bind_pending_context_usage_to_thread(&mut self, thread_id: ThreadId) {
        let mut changed = false;
        for output in &mut self.pending_context_usage_outputs {
            if output.thread_id.is_none() {
                output.thread_id = Some(thread_id);
                changed = true;
            }
        }
        if changed {
            self.request_redraw();
        }
    }
}

fn normalize_context_usage_error(message: String) -> String {
    let message = message.trim().trim_end_matches('。');
    if message.is_empty() {
        "暂时无法获取上下文用量".to_string()
    } else {
        message.to_string()
    }
}
