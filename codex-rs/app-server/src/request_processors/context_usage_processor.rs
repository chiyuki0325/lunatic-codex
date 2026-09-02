use crate::error_code::invalid_request;
use codex_app_server_protocol::ClientResponsePayload;
use codex_app_server_protocol::ContextUsageActualSource as ApiContextUsageActualSource;
use codex_app_server_protocol::ContextUsageActualUsage as ApiContextUsageActualUsage;
use codex_app_server_protocol::ContextUsageCategory as ApiContextUsageCategory;
use codex_app_server_protocol::ContextUsageCategoryKind as ApiContextUsageCategoryKind;
use codex_app_server_protocol::ContextUsageCompleteness as ApiContextUsageCompleteness;
use codex_app_server_protocol::ContextUsageDetail as ApiContextUsageDetail;
use codex_app_server_protocol::ContextUsageDetailLoadState as ApiContextUsageDetailLoadState;
use codex_app_server_protocol::ContextUsageSnapshot as ApiContextUsageSnapshot;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ThreadContextUsageParams;
use codex_app_server_protocol::ThreadContextUsageResponse;
use codex_core::ContextUsageActualSource;
use codex_core::ContextUsageCategoryKind;
use codex_core::ContextUsageCompleteness;
use codex_core::ContextUsageDetailLoadState;
use codex_core::ContextUsageReadSnapshot;
use codex_core::ThreadManager;
use codex_protocol::ThreadId;
use std::sync::Arc;

use crate::error_code::internal_error;

#[derive(Clone)]
pub(crate) struct ContextUsageRequestProcessor {
    thread_manager: Arc<ThreadManager>,
}

impl ContextUsageRequestProcessor {
    pub(crate) fn new(thread_manager: Arc<ThreadManager>) -> Self {
        Self { thread_manager }
    }

    pub(crate) async fn thread_context_usage(
        &self,
        params: ThreadContextUsageParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        self.thread_context_usage_response_inner(params)
            .await
            .map(|response| Some(response.into()))
    }

    async fn thread_context_usage_response_inner(
        &self,
        params: ThreadContextUsageParams,
    ) -> Result<ThreadContextUsageResponse, JSONRPCErrorError> {
        let thread_id = ThreadId::from_string(&params.thread_id)
            .map_err(|err| invalid_request(format!("invalid thread id: {err}")))?;
        let thread = self
            .thread_manager
            .get_thread(thread_id)
            .await
            .map_err(|_| invalid_request(format!("thread not loaded: {thread_id}")))?;
        let snapshot = thread.context_usage().await.ok_or_else(|| {
            internal_error(format!("context usage unavailable for thread: {thread_id}"))
        })?;
        Ok(map_thread_context_usage(&snapshot))
    }
}

fn map_thread_context_usage(snapshot: &ContextUsageReadSnapshot) -> ThreadContextUsageResponse {
    let latest_snapshot = snapshot.latest_snapshot.as_ref();
    ThreadContextUsageResponse {
        snapshot: ApiContextUsageSnapshot {
            snapshot_id: latest_snapshot.snapshot_id.clone(),
            request_sequence: latest_snapshot.request_sequence,
            generated_at: latest_snapshot.generated_at.timestamp(),
            model: latest_snapshot.model.clone(),
            model_context_window: latest_snapshot.model_context_window,
            auto_compact_threshold: latest_snapshot.auto_compact_threshold,
            reserved_tokens: latest_snapshot.reserved_tokens,
            categories: latest_snapshot
                .categories
                .iter()
                .map(|category| ApiContextUsageCategory {
                    kind: map_category_kind(category.kind),
                    estimated_tokens: category.estimated_tokens,
                })
                .collect(),
            mcp_tool_details: latest_snapshot
                .mcp_tool_details
                .iter()
                .map(map_detail)
                .collect(),
            instruction_details: latest_snapshot
                .instruction_details
                .iter()
                .map(map_detail)
                .collect(),
            skill_details: latest_snapshot
                .skill_details
                .iter()
                .map(map_detail)
                .collect(),
            estimated_total_tokens: latest_snapshot.estimated_total_tokens,
            completeness: map_completeness(latest_snapshot.completeness),
            request_config_version: latest_snapshot.request_config_version,
        },
        actual_usage: snapshot
            .actual_usage
            .clone()
            .map(|usage| ApiContextUsageActualUsage {
                usage: usage.into(),
                snapshot_id: snapshot.last_completed_snapshot_id.clone(),
            }),
        actual_source: map_actual_source(snapshot.actual_source),
        last_completed_snapshot_id: snapshot.last_completed_snapshot_id.clone(),
    }
}

fn map_detail(detail: &codex_core::ContextUsageDetail) -> ApiContextUsageDetail {
    ApiContextUsageDetail {
        label: detail.label.clone(),
        path: detail.path.as_ref().map(|path| path.display().to_string()),
        load_state: map_detail_load_state(detail.load_state),
        estimated_tokens: detail.estimated_tokens,
    }
}

fn map_category_kind(kind: ContextUsageCategoryKind) -> ApiContextUsageCategoryKind {
    match kind {
        ContextUsageCategoryKind::SystemPrompt => ApiContextUsageCategoryKind::SystemPrompt,
        ContextUsageCategoryKind::BuiltInTools => ApiContextUsageCategoryKind::BuiltInTools,
        ContextUsageCategoryKind::McpTools => ApiContextUsageCategoryKind::McpTools,
        ContextUsageCategoryKind::Instructions => ApiContextUsageCategoryKind::Instructions,
        ContextUsageCategoryKind::Skills => ApiContextUsageCategoryKind::Skills,
        ContextUsageCategoryKind::Messages => ApiContextUsageCategoryKind::Messages,
        ContextUsageCategoryKind::Other => ApiContextUsageCategoryKind::Other,
        ContextUsageCategoryKind::Unattributed => ApiContextUsageCategoryKind::Unattributed,
    }
}

fn map_detail_load_state(
    load_state: ContextUsageDetailLoadState,
) -> ApiContextUsageDetailLoadState {
    match load_state {
        ContextUsageDetailLoadState::Loaded => ApiContextUsageDetailLoadState::Loaded,
        ContextUsageDetailLoadState::Available => ApiContextUsageDetailLoadState::Available,
        ContextUsageDetailLoadState::Deferred => ApiContextUsageDetailLoadState::Deferred,
    }
}

fn map_actual_source(source: ContextUsageActualSource) -> ApiContextUsageActualSource {
    match source {
        ContextUsageActualSource::CurrentRequest => ApiContextUsageActualSource::CurrentRequest,
        ContextUsageActualSource::PreviousCompletedRequest => {
            ApiContextUsageActualSource::PreviousCompletedRequest
        }
        ContextUsageActualSource::LocalEstimate => ApiContextUsageActualSource::LocalEstimate,
    }
}

fn map_completeness(completeness: ContextUsageCompleteness) -> ApiContextUsageCompleteness {
    match completeness {
        ContextUsageCompleteness::Complete => ApiContextUsageCompleteness::Complete,
        ContextUsageCompleteness::Partial => ApiContextUsageCompleteness::Partial,
        ContextUsageCompleteness::Unavailable => ApiContextUsageCompleteness::Unavailable,
    }
}
