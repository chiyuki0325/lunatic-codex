use crate::JsonSchema;
use crate::TS;
use serde::Deserialize;
use serde::Serialize;

use super::TokenUsageBreakdown;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadContextUsageParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadContextUsageResponse {
    pub snapshot: ContextUsageSnapshot,
    pub actual_usage: Option<ContextUsageActualUsage>,
    pub actual_source: ContextUsageActualSource,
    pub last_completed_snapshot_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ContextUsageSnapshot {
    pub snapshot_id: String,
    #[ts(type = "number")]
    pub request_sequence: u64,
    #[ts(type = "number")]
    pub generated_at: i64,
    pub model: String,
    #[ts(type = "number | null")]
    pub model_context_window: Option<u64>,
    #[ts(type = "number | null")]
    pub auto_compact_threshold: Option<u64>,
    #[ts(type = "number | null")]
    pub reserved_tokens: Option<u64>,
    pub categories: Vec<ContextUsageCategory>,
    pub mcp_tool_details: Vec<ContextUsageDetail>,
    pub instruction_details: Vec<ContextUsageDetail>,
    pub skill_details: Vec<ContextUsageDetail>,
    #[ts(type = "number")]
    pub estimated_total_tokens: u64,
    pub completeness: ContextUsageCompleteness,
    #[ts(type = "number")]
    pub request_config_version: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ContextUsageCategory {
    pub kind: ContextUsageCategoryKind,
    #[ts(type = "number")]
    pub estimated_tokens: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ContextUsageDetail {
    pub label: String,
    pub path: Option<String>,
    pub load_state: ContextUsageDetailLoadState,
    #[ts(type = "number")]
    pub estimated_tokens: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ContextUsageActualUsage {
    pub usage: TokenUsageBreakdown,
    pub snapshot_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
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

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum ContextUsageDetailLoadState {
    Loaded,
    Available,
    Deferred,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum ContextUsageActualSource {
    CurrentRequest,
    PreviousCompletedRequest,
    LocalEstimate,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum ContextUsageCompleteness {
    Complete,
    Partial,
    Unavailable,
}
