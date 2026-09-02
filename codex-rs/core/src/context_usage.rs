use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use arc_swap::ArcSwap;
use chrono::DateTime;
use chrono::Utc;
use codex_api::ResponsesApiRequest;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use codex_utils_string::approx_token_count;
use uuid::Uuid;

use crate::client_common::Prompt;
use crate::client_common::PromptInputUsageCategory;
use crate::client_common::PromptToolLoadState;
use crate::client_common::PromptToolUsageOrigin;
use crate::context_manager::estimate_item_token_count;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContextUsageCategoryKind {
    SystemPrompt,
    BuiltInTools,
    McpTools,
    Instructions,
    Skills,
    Messages,
    Other,
    Unattributed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextUsageDetailLoadState {
    Loaded,
    Available,
    Deferred,
}

impl From<crate::tools::registry::ToolExposure> for ContextUsageDetailLoadState {
    fn from(value: crate::tools::registry::ToolExposure) -> Self {
        match value {
            crate::tools::registry::ToolExposure::Direct
            | crate::tools::registry::ToolExposure::DirectModelOnly => Self::Loaded,
            crate::tools::registry::ToolExposure::Deferred
            | crate::tools::registry::ToolExposure::DeferredModelOnly => Self::Deferred,
            crate::tools::registry::ToolExposure::CodeModeOnly
            | crate::tools::registry::ToolExposure::Hidden => Self::Available,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextUsageDetail {
    pub label: String,
    pub path: Option<PathBuf>,
    pub load_state: ContextUsageDetailLoadState,
    pub estimated_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextUsageCategory {
    pub kind: ContextUsageCategoryKind,
    pub estimated_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextUsageActualSource {
    CurrentRequest,
    PreviousCompletedRequest,
    LocalEstimate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextUsageCompleteness {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextUsageSnapshot {
    pub snapshot_id: String,
    pub request_sequence: u64,
    pub generated_at: DateTime<Utc>,
    pub model: String,
    pub model_context_window: Option<u64>,
    pub auto_compact_threshold: Option<u64>,
    pub reserved_tokens: Option<u64>,
    pub categories: Vec<ContextUsageCategory>,
    pub mcp_tool_details: Vec<ContextUsageDetail>,
    pub instruction_details: Vec<ContextUsageDetail>,
    pub skill_details: Vec<ContextUsageDetail>,
    pub estimated_total_tokens: u64,
    pub completeness: ContextUsageCompleteness,
    pub request_config_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextUsageReadSnapshot {
    pub latest_snapshot: Arc<ContextUsageSnapshot>,
    pub last_completed_snapshot_id: Option<String>,
    pub actual_source: ContextUsageActualSource,
    pub actual_usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletedRequestUsage {
    snapshot_id: String,
    usage: TokenUsage,
}

#[derive(Debug, Clone, Default)]
struct ContextUsageStoreState {
    latest_snapshot: Option<Arc<ContextUsageSnapshot>>,
    completed_request: Option<CompletedRequestUsage>,
}

#[derive(Debug, Default)]
pub(crate) struct ContextUsageStore {
    state: ArcSwap<ContextUsageStoreState>,
}

impl ContextUsageStore {
    pub(crate) fn publish_unavailable(
        &self,
        model_info: &codex_protocol::openai_models::ModelInfo,
    ) {
        let model_context_window = model_info
            .resolved_context_window()
            .and_then(|value| u64::try_from(value).ok());
        let auto_compact_threshold = model_info
            .auto_compact_token_limit()
            .and_then(|value| u64::try_from(value).ok());
        self.publish(Arc::new(ContextUsageSnapshot {
            snapshot_id: Uuid::new_v4().to_string(),
            request_sequence: 0,
            generated_at: Utc::now(),
            model: model_info.slug.clone(),
            model_context_window,
            auto_compact_threshold,
            reserved_tokens: reserved_tokens(model_context_window, auto_compact_threshold),
            categories: Vec::new(),
            mcp_tool_details: Vec::new(),
            instruction_details: Vec::new(),
            skill_details: Vec::new(),
            estimated_total_tokens: 0,
            completeness: ContextUsageCompleteness::Unavailable,
            request_config_version: 0,
        }));
    }

    pub(crate) fn publish(&self, snapshot: Arc<ContextUsageSnapshot>) {
        self.state.rcu(|state| {
            Arc::new(ContextUsageStoreState {
                latest_snapshot: Some(Arc::clone(&snapshot)),
                completed_request: state.completed_request.clone(),
            })
        });
    }

    pub(crate) fn mark_completed(&self, snapshot_id: String, usage: TokenUsage) {
        self.state.rcu(|state| {
            Arc::new(ContextUsageStoreState {
                latest_snapshot: state.latest_snapshot.clone(),
                completed_request: Some(CompletedRequestUsage {
                    snapshot_id: snapshot_id.clone(),
                    usage: usage.clone(),
                }),
            })
        });
    }

    pub(crate) fn read(&self) -> Option<ContextUsageReadSnapshot> {
        let state = self.state.load_full();
        let latest_snapshot = state.latest_snapshot.clone()?;
        let (last_completed_snapshot_id, actual_source, actual_usage) =
            match &state.completed_request {
                Some(completed) => (
                    Some(completed.snapshot_id.clone()),
                    if completed.snapshot_id == latest_snapshot.snapshot_id {
                        ContextUsageActualSource::CurrentRequest
                    } else {
                        ContextUsageActualSource::PreviousCompletedRequest
                    },
                    Some(completed.usage.clone()),
                ),
                None => (None, ContextUsageActualSource::LocalEstimate, None),
            };
        Some(ContextUsageReadSnapshot {
            latest_snapshot,
            last_completed_snapshot_id,
            actual_source,
            actual_usage,
        })
    }
}

pub(crate) struct ContextUsageRequestCapture<'a> {
    pub(crate) store: &'a ContextUsageStore,
    pub(crate) next_request_sequence: &'a AtomicU64,
    pub(crate) model_context_window: Option<i64>,
    pub(crate) auto_compact_threshold: Option<i64>,
}

impl ContextUsageRequestCapture<'_> {
    pub(crate) fn publish(&self, prompt: &Prompt, wire_request: &ResponsesApiRequest) -> String {
        let request_sequence = self
            .next_request_sequence
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        let snapshot = Arc::new(estimate_request_snapshot(
            prompt,
            wire_request,
            request_sequence,
            self.model_context_window,
            self.auto_compact_threshold,
        ));
        let snapshot_id = snapshot.snapshot_id.clone();
        self.store.publish(snapshot);
        snapshot_id
    }
}

fn estimate_request_snapshot(
    prompt: &Prompt,
    wire_request: &ResponsesApiRequest,
    request_sequence: u64,
    model_context_window: Option<i64>,
    auto_compact_threshold: Option<i64>,
) -> ContextUsageSnapshot {
    let mut category_totals = BTreeMap::new();
    let mut completeness = if prompt.usage_sidecar.complete {
        ContextUsageCompleteness::Complete
    } else {
        ContextUsageCompleteness::Partial
    };

    let responses_lite_prefix_len = wire_request.input.len().saturating_sub(prompt.input.len());
    if responses_lite_prefix_len == 2 {
        add_tokens(
            &mut category_totals,
            ContextUsageCategoryKind::SystemPrompt,
            estimate_item_tokens(&wire_request.input[1]),
        );
    } else {
        add_tokens(
            &mut category_totals,
            ContextUsageCategoryKind::SystemPrompt,
            estimate_string_tokens(&wire_request.instructions),
        );
        if responses_lite_prefix_len != 0 {
            completeness = ContextUsageCompleteness::Partial;
        }
    }

    let mut input_content_tokens = BTreeMap::new();
    if wire_request.input.len() >= responses_lite_prefix_len
        && wire_request.input.len() - responses_lite_prefix_len
            == prompt.usage_sidecar.input_categories.len()
    {
        for (input_index, (item, categories)) in wire_request.input[responses_lite_prefix_len..]
            .iter()
            .zip(&prompt.usage_sidecar.input_categories)
            .enumerate()
        {
            estimate_input_item(
                item,
                categories,
                input_index,
                &mut category_totals,
                &mut input_content_tokens,
                &mut completeness,
            );
        }
    } else {
        completeness = ContextUsageCompleteness::Partial;
        for item in &wire_request.input[responses_lite_prefix_len..] {
            add_tokens(
                &mut category_totals,
                ContextUsageCategoryKind::Other,
                estimate_item_tokens(item),
            );
        }
    }

    let (mut mcp_tool_details, tool_complete) = estimate_tools(
        prompt,
        wire_request,
        responses_lite_prefix_len,
        &mut category_totals,
    );
    if !tool_complete {
        completeness = ContextUsageCompleteness::Partial;
    }
    mcp_tool_details.extend(prompt.usage_sidecar.unloaded_mcp_tools.iter().map(|tool| {
        ContextUsageDetail {
            label: tool.public_label.clone(),
            path: None,
            load_state: match tool.load_state {
                PromptToolLoadState::Available => ContextUsageDetailLoadState::Available,
                PromptToolLoadState::Deferred => ContextUsageDetailLoadState::Deferred,
            },
            estimated_tokens: 0,
        }
    }));

    add_tokens(
        &mut category_totals,
        ContextUsageCategoryKind::Other,
        estimate_output_schema_tokens(wire_request),
    );

    let instruction_details = estimate_details(
        &prompt.usage_sidecar.input_details,
        PromptInputUsageCategory::Instructions,
        &input_content_tokens,
    );
    let skill_details = estimate_details(
        &prompt.usage_sidecar.input_details,
        PromptInputUsageCategory::Skills,
        &input_content_tokens,
    );
    let categories = categories_from_totals(category_totals);
    let estimated_total_tokens = categories
        .iter()
        .map(|category| category.estimated_tokens)
        .sum();
    let model_context_window = model_context_window.and_then(|value| u64::try_from(value).ok());
    let auto_compact_threshold = auto_compact_threshold.and_then(|value| u64::try_from(value).ok());
    let reserved_tokens = reserved_tokens(model_context_window, auto_compact_threshold);

    ContextUsageSnapshot {
        snapshot_id: Uuid::new_v4().to_string(),
        request_sequence,
        generated_at: Utc::now(),
        model: wire_request.model.clone(),
        model_context_window,
        auto_compact_threshold,
        reserved_tokens,
        categories,
        mcp_tool_details,
        instruction_details,
        skill_details,
        estimated_total_tokens,
        completeness,
        // Request sequence is the only configuration revision available at this boundary.
        request_config_version: request_sequence,
    }
}

fn estimate_input_item(
    item: &ResponseItem,
    categories: &[PromptInputUsageCategory],
    input_index: usize,
    category_totals: &mut BTreeMap<ContextUsageCategoryKind, u64>,
    input_content_tokens: &mut BTreeMap<(usize, usize), u64>,
    completeness: &mut ContextUsageCompleteness,
) {
    let total_tokens = estimate_item_tokens(item);
    let ResponseItem::Message { content, .. } = item else {
        if let [category] = categories {
            add_tokens(category_totals, category_kind(*category), total_tokens);
        } else {
            *completeness = ContextUsageCompleteness::Partial;
            add_tokens(
                category_totals,
                ContextUsageCategoryKind::Other,
                total_tokens,
            );
        }
        return;
    };
    if categories.len() != content.len() {
        *completeness = ContextUsageCompleteness::Partial;
        add_tokens(
            category_totals,
            ContextUsageCategoryKind::Other,
            total_tokens,
        );
        return;
    }

    let weights = content
        .iter()
        .map(estimate_content_item_tokens)
        .collect::<Vec<_>>();
    let allocations = allocate_tokens(total_tokens, &weights);
    for (content_index, (category, tokens)) in categories.iter().zip(allocations).enumerate() {
        add_tokens(category_totals, category_kind(*category), tokens);
        input_content_tokens.insert((input_index, content_index), tokens);
    }
}

fn estimate_tools(
    prompt: &Prompt,
    wire_request: &ResponsesApiRequest,
    responses_lite_prefix_len: usize,
    category_totals: &mut BTreeMap<ContextUsageCategoryKind, u64>,
) -> (Vec<ContextUsageDetail>, bool) {
    let total_tokens = if responses_lite_prefix_len == 2 {
        estimate_item_tokens(&wire_request.input[0])
    } else {
        wire_request
            .tools
            .as_ref()
            .and_then(|tools| serde_json::to_string(tools).ok())
            .map(|tools| estimate_string_tokens(&tools))
            .unwrap_or(0)
    };
    if total_tokens == 0 && prompt.tools.is_empty() {
        return (Vec::new(), true);
    }
    if prompt.tools.len() != prompt.usage_sidecar.tool_origins.len() {
        add_tokens(
            category_totals,
            ContextUsageCategoryKind::Other,
            total_tokens,
        );
        return (Vec::new(), false);
    }

    let weights = prompt
        .tools
        .iter()
        .map(|tool| {
            serde_json::to_string(tool)
                .ok()
                .map(|value| estimate_string_tokens(&value))
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let allocations = allocate_tokens(total_tokens, &weights);
    let mut details = Vec::new();
    let mut complete = true;
    for (origin, estimated_tokens) in prompt.usage_sidecar.tool_origins.iter().zip(allocations) {
        match origin {
            PromptToolUsageOrigin::BuiltIn => add_tokens(
                category_totals,
                ContextUsageCategoryKind::BuiltInTools,
                estimated_tokens,
            ),
            PromptToolUsageOrigin::Mcp {
                public_label,
                source_identity: _source_identity,
            } => {
                add_tokens(
                    category_totals,
                    ContextUsageCategoryKind::McpTools,
                    estimated_tokens,
                );
                details.push(ContextUsageDetail {
                    label: public_label.clone(),
                    path: None,
                    load_state: ContextUsageDetailLoadState::Loaded,
                    estimated_tokens,
                });
            }
            PromptToolUsageOrigin::Unknown => {
                complete = false;
                add_tokens(
                    category_totals,
                    ContextUsageCategoryKind::Other,
                    estimated_tokens,
                );
            }
        }
    }
    (details, complete)
}

fn estimate_details(
    details: &[crate::client_common::PromptInputUsageDetail],
    category: PromptInputUsageCategory,
    input_content_tokens: &BTreeMap<(usize, usize), u64>,
) -> Vec<ContextUsageDetail> {
    let mut grouped = BTreeMap::<(usize, usize), Vec<_>>::new();
    for detail in details.iter().filter(|detail| detail.category == category) {
        grouped
            .entry((detail.input_index, detail.content_index))
            .or_default()
            .push(detail);
    }

    let mut estimated = Vec::new();
    for (position, details) in grouped {
        let Some(total_tokens) = input_content_tokens.get(&position).copied() else {
            continue;
        };
        let weights = details
            .iter()
            .map(|detail| detail.weight_tokens)
            .collect::<Vec<_>>();
        for (detail, estimated_tokens) in details
            .into_iter()
            .zip(allocate_tokens(total_tokens, &weights))
        {
            estimated.push(ContextUsageDetail {
                label: detail.label.clone(),
                path: detail.path.clone(),
                load_state: ContextUsageDetailLoadState::Loaded,
                estimated_tokens,
            });
        }
    }
    estimated
}

fn estimate_content_item_tokens(content: &ContentItem) -> u64 {
    serde_json::to_string(content)
        .ok()
        .map(|value| estimate_string_tokens(&value))
        .unwrap_or(0)
}

fn allocate_tokens(total: u64, weights: &[u64]) -> Vec<u64> {
    if weights.is_empty() {
        return Vec::new();
    }
    let weight_total = weights
        .iter()
        .map(|weight| u128::from(*weight))
        .sum::<u128>();
    if weight_total == 0 {
        let mut allocations = vec![0; weights.len()];
        allocations[0] = total;
        return allocations;
    }

    let mut allocations = Vec::with_capacity(weights.len());
    let mut remainders = Vec::with_capacity(weights.len());
    let mut allocated = 0_u64;
    for (index, weight) in weights.iter().copied().enumerate() {
        let numerator = u128::from(total) * u128::from(weight);
        let quotient = numerator / weight_total;
        let allocation = u64::try_from(quotient).unwrap_or(u64::MAX);
        allocations.push(allocation);
        allocated = allocated.saturating_add(allocation);
        remainders.push((numerator % weight_total, index));
    }
    remainders.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    for (_, index) in remainders
        .into_iter()
        .take(usize::try_from(total.saturating_sub(allocated)).unwrap_or(usize::MAX))
    {
        allocations[index] = allocations[index].saturating_add(1);
    }
    allocations
}

fn categories_from_totals(
    category_totals: BTreeMap<ContextUsageCategoryKind, u64>,
) -> Vec<ContextUsageCategory> {
    [
        ContextUsageCategoryKind::SystemPrompt,
        ContextUsageCategoryKind::BuiltInTools,
        ContextUsageCategoryKind::McpTools,
        ContextUsageCategoryKind::Instructions,
        ContextUsageCategoryKind::Skills,
        ContextUsageCategoryKind::Messages,
        ContextUsageCategoryKind::Other,
        ContextUsageCategoryKind::Unattributed,
    ]
    .into_iter()
    .filter_map(|kind| {
        let estimated_tokens = category_totals.get(&kind).copied().unwrap_or(0);
        (estimated_tokens > 0).then_some(ContextUsageCategory {
            kind,
            estimated_tokens,
        })
    })
    .collect()
}

fn add_tokens(
    category_totals: &mut BTreeMap<ContextUsageCategoryKind, u64>,
    kind: ContextUsageCategoryKind,
    tokens: u64,
) {
    if tokens > 0 {
        let total = category_totals.entry(kind).or_default();
        *total = total.saturating_add(tokens);
    }
}

fn category_kind(category: PromptInputUsageCategory) -> ContextUsageCategoryKind {
    match category {
        PromptInputUsageCategory::Instructions => ContextUsageCategoryKind::Instructions,
        PromptInputUsageCategory::Skills => ContextUsageCategoryKind::Skills,
        PromptInputUsageCategory::Messages => ContextUsageCategoryKind::Messages,
        PromptInputUsageCategory::Other => ContextUsageCategoryKind::Other,
    }
}

fn estimate_item_tokens(item: &ResponseItem) -> u64 {
    u64::try_from(estimate_item_token_count(item).max(0)).unwrap_or(0)
}

fn estimate_string_tokens(value: &str) -> u64 {
    u64::try_from(approx_token_count(value)).unwrap_or(u64::MAX)
}

fn estimate_output_schema_tokens(wire_request: &ResponsesApiRequest) -> u64 {
    serde_json::to_string(&wire_request.text)
        .ok()
        .map(|value| estimate_string_tokens(&value))
        .unwrap_or(0)
}

fn reserved_tokens(
    model_context_window: Option<u64>,
    auto_compact_threshold: Option<u64>,
) -> Option<u64> {
    let reserved = model_context_window?.checked_sub(auto_compact_threshold?)?;
    (reserved > 0).then_some(reserved)
}

impl crate::session::session::Session {
    pub(crate) fn context_usage_request_capture(
        &self,
        turn_context: &crate::session::turn_context::TurnContext,
    ) -> ContextUsageRequestCapture<'_> {
        ContextUsageRequestCapture {
            store: &self.context_usage_store,
            next_request_sequence: &self.next_context_usage_request_sequence,
            model_context_window: turn_context.model_context_window(),
            auto_compact_threshold: turn_context.model_info.auto_compact_token_limit(),
        }
    }

    pub(crate) fn mark_context_usage_request_completed(
        &self,
        snapshot_id: &str,
        usage: &TokenUsage,
    ) {
        self.context_usage_store
            .mark_completed(snapshot_id.to_string(), usage.clone());
    }
}

pub(crate) fn preview_context_usage_for_model(
    mut read: ContextUsageReadSnapshot,
    model: String,
    model_context_window: Option<u64>,
    auto_compact_threshold: Option<u64>,
) -> ContextUsageReadSnapshot {
    if read.latest_snapshot.model == model {
        return read;
    }

    let mut snapshot = (*read.latest_snapshot).clone();
    snapshot.snapshot_id = Uuid::new_v4().to_string();
    snapshot.generated_at = Utc::now();
    snapshot.model = model;
    snapshot.model_context_window = model_context_window;
    snapshot.auto_compact_threshold = auto_compact_threshold;
    snapshot.reserved_tokens = reserved_tokens(model_context_window, auto_compact_threshold);
    snapshot.completeness = ContextUsageCompleteness::Partial;
    snapshot.request_config_version = snapshot.request_config_version.saturating_add(1);
    read.latest_snapshot = Arc::new(snapshot);
    read.actual_source = if read.actual_usage.is_some() {
        ContextUsageActualSource::PreviousCompletedRequest
    } else {
        ContextUsageActualSource::LocalEstimate
    };
    read
}

#[cfg(test)]
#[path = "context_usage_tests.rs"]
mod tests;
