//! App-level `/context` RPC lifecycle.

use super::*;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadContextUsageParams;
use codex_app_server_protocol::ThreadContextUsageResponse;
use std::collections::VecDeque;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingContextUsageRequest {
    pub(crate) request_id: u64,
    pub(crate) thread_id: Option<ThreadId>,
}

#[derive(Debug, Default)]
pub(crate) struct ContextUsageRequests {
    pub(crate) pending: VecDeque<PendingContextUsageRequest>,
}

impl ContextUsageRequests {
    pub(crate) fn clear(&mut self) {
        self.pending.clear();
    }

    pub(crate) fn enqueue(&mut self, request: PendingContextUsageRequest) {
        self.pending.push_back(request);
    }

    pub(crate) fn dispatch_started_thread(
        &mut self,
        app_server: &AppServerSession,
        app_event_tx: AppEventSender,
        thread_id: ThreadId,
    ) {
        let mut dispatched = Vec::new();
        self.pending.retain(|pending| {
            let matches_started_thread = pending.thread_id.is_none();
            if matches_started_thread {
                dispatched.push(PendingContextUsageRequest {
                    request_id: pending.request_id,
                    thread_id: Some(thread_id),
                });
            }
            !matches_started_thread
        });
        for request in dispatched {
            spawn_context_usage_request(
                app_server.request_handle(),
                app_event_tx.clone(),
                request.request_id,
                thread_id,
            );
        }
    }
}

impl App {
    pub(super) fn refresh_context_usage(
        &mut self,
        app_server: &AppServerSession,
        request_id: u64,
        thread_id: Option<ThreadId>,
    ) {
        match thread_id {
            Some(thread_id) => {
                spawn_context_usage_request(
                    app_server.request_handle(),
                    self.app_event_tx.clone(),
                    request_id,
                    thread_id,
                );
            }
            None => {
                self.context_usage_requests
                    .enqueue(PendingContextUsageRequest {
                        request_id,
                        thread_id: None,
                    });
            }
        }
    }

    pub(super) fn finish_context_usage_refresh(
        &mut self,
        tui: &mut tui::Tui,
        request_id: u64,
        thread_id: ThreadId,
        result: Result<ThreadContextUsageResponse, String>,
    ) {
        let accepted =
            self.chat_widget
                .finish_context_usage_refresh(request_id, thread_id, result.clone());
        if accepted {
            self.insert_pending_usage_output_if_ready(tui);
            return;
        }
        match result {
            Ok(_) => {
                tracing::debug!(request_id, %thread_id, "ignored stale context usage response");
            }
            Err(err) => {
                tracing::debug!(request_id, %thread_id, error = %err, "ignored stale context usage failure");
            }
        }
    }
}

fn spawn_context_usage_request(
    request_handle: AppServerRequestHandle,
    app_event_tx: AppEventSender,
    request_id: u64,
    thread_id: ThreadId,
) {
    tokio::spawn(async move {
        let result = fetch_context_usage(request_handle, thread_id)
            .await
            .map_err(map_context_usage_error);
        app_event_tx.send(AppEvent::ContextUsageLoaded {
            request_id,
            thread_id,
            result,
        });
    });
}

async fn fetch_context_usage(
    request_handle: AppServerRequestHandle,
    thread_id: ThreadId,
) -> Result<ThreadContextUsageResponse> {
    let request_id = RequestId::String(format!("thread-context-usage-{}", Uuid::new_v4()));
    request_handle
        .request_typed(ClientRequest::ThreadContextUsage {
            request_id,
            params: ThreadContextUsageParams {
                thread_id: thread_id.to_string(),
            },
        })
        .await
        .wrap_err("thread/contextUsage failed in TUI")
}

fn map_context_usage_error(err: color_eyre::Report) -> String {
    let message = err.to_string();
    if message.contains("timed out") {
        "获取上下文用量超时，请稍后重试。".to_string()
    } else if message.contains("not found") || message.contains("closed") {
        "目标线程已不可用，暂时无法获取上下文用量。".to_string()
    } else {
        "暂时无法获取上下文用量。".to_string()
    }
}
