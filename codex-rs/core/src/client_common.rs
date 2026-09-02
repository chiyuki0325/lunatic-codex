pub use codex_api::ResponseEvent;
use codex_protocol::error::Result;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ResponseItem;
use codex_tools::ToolSpec;
use futures::Stream;
use serde_json::Value;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptInputUsageCategory {
    Instructions,
    Skills,
    Messages,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromptInputUsageDetail {
    pub(crate) label: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) category: PromptInputUsageCategory,
    pub(crate) input_index: usize,
    pub(crate) content_index: usize,
    pub(crate) weight_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromptToolUsageOrigin {
    BuiltIn,
    Mcp {
        public_label: String,
        source_identity: String,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptToolLoadState {
    Available,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromptUnloadedMcpTool {
    pub(crate) public_label: String,
    pub(crate) source_identity: String,
    pub(crate) load_state: PromptToolLoadState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PromptUsageSidecar {
    /// One category vector per original prompt item. Message vectors align with content items;
    /// every other item has exactly one category.
    pub(crate) input_categories: Vec<Vec<PromptInputUsageCategory>>,
    pub(crate) input_details: Vec<PromptInputUsageDetail>,
    pub(crate) tool_origins: Vec<PromptToolUsageOrigin>,
    pub(crate) unloaded_mcp_tools: Vec<PromptUnloadedMcpTool>,
    pub(crate) complete: bool,
}

/// API request payload for a single model turn
#[derive(Debug, Clone)]
pub struct Prompt {
    /// Conversation context input items.
    pub input: Vec<ResponseItem>,

    /// Tools available to the model, including additional tools sourced from
    /// external MCP servers.
    pub(crate) tools: Arc<[ToolSpec]>,

    /// Whether parallel tool calls are permitted for this prompt.
    pub(crate) parallel_tool_calls: bool,

    pub base_instructions: BaseInstructions,

    /// Optional the output schema for the model's response.
    pub output_schema: Option<Value>,

    /// Whether the Responses API should strictly validate `output_schema`.
    pub output_schema_strict: bool,

    /// Non-serialized usage provenance captured alongside the model-visible prompt.
    pub(crate) usage_sidecar: PromptUsageSidecar,
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            input: Vec::new(),
            tools: Arc::default(),
            parallel_tool_calls: false,
            base_instructions: BaseInstructions::default(),
            output_schema: None,
            output_schema_strict: true,
            usage_sidecar: PromptUsageSidecar::default(),
        }
    }
}

impl Prompt {
    pub(crate) fn get_formatted_input_for_request(
        &self,
        use_responses_lite: bool,
    ) -> Vec<ResponseItem> {
        let mut input = self.input.clone();
        if use_responses_lite {
            strip_image_details(&mut input);
        }
        input
    }
}

fn strip_image_details(items: &mut [ResponseItem]) {
    for item in items {
        match item {
            ResponseItem::Message { content, .. } => {
                for content_item in content {
                    if let ContentItem::InputImage { detail, .. } = content_item {
                        *detail = None;
                    }
                }
            }
            ResponseItem::FunctionCallOutput { output, .. }
            | ResponseItem::CustomToolCallOutput { output, .. } => {
                if let Some(content) = output.content_items_mut() {
                    for content_item in content {
                        if let FunctionCallOutputContentItem::InputImage { detail, .. } =
                            content_item
                        {
                            *detail = None;
                        }
                    }
                }
            }
            ResponseItem::AdditionalTools { .. }
            | ResponseItem::Reasoning { .. }
            | ResponseItem::AgentMessage { .. }
            | ResponseItem::LocalShellCall { .. }
            | ResponseItem::FunctionCall { .. }
            | ResponseItem::ToolSearchCall { .. }
            | ResponseItem::CustomToolCall { .. }
            | ResponseItem::ToolSearchOutput { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::ImageGenerationCall { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::CompactionTrigger { .. }
            | ResponseItem::ContextCompaction { .. }
            | ResponseItem::Other => {}
        }
    }
}

pub struct ResponseStream {
    pub(crate) rx_event: mpsc::Receiver<Result<ResponseEvent>>,
    /// Signals the mapper task that the consumer stopped polling before the
    /// provider stream reached its own terminal event.
    pub(crate) consumer_dropped: CancellationToken,
    pub(crate) context_usage_snapshot_id: Option<String>,
}

impl ResponseStream {
    pub(crate) fn context_usage_snapshot_id(&self) -> Option<&str> {
        self.context_usage_snapshot_id.as_deref()
    }
}

impl Stream for ResponseStream {
    type Item = Result<ResponseEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx_event.poll_recv(cx)
    }
}

impl Drop for ResponseStream {
    fn drop(&mut self) {
        self.consumer_dropped.cancel();
    }
}

#[cfg(test)]
#[path = "client_common_tests.rs"]
mod tests;
