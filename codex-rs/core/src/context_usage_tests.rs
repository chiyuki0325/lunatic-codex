use std::path::PathBuf;
use std::sync::Arc;

use chrono::TimeZone;
use chrono::Utc;
use codex_protocol::protocol::TokenUsage;
use codex_tools::ToolExposure;
use pretty_assertions::assert_eq;

use super::ContextUsageActualSource;
use super::ContextUsageCategory;
use super::ContextUsageCategoryKind;
use super::ContextUsageDetail;
use super::ContextUsageDetailLoadState;
use super::ContextUsageReadSnapshot;
use super::ContextUsageSnapshot;
use super::ContextUsageStore;
use super::preview_context_usage_for_model;

#[test]
fn tool_exposure_maps_to_detail_load_state() {
    let actual = vec![
        ContextUsageDetailLoadState::from(ToolExposure::Direct),
        ContextUsageDetailLoadState::from(ToolExposure::DirectModelOnly),
        ContextUsageDetailLoadState::from(ToolExposure::Deferred),
        ContextUsageDetailLoadState::from(ToolExposure::DeferredModelOnly),
        ContextUsageDetailLoadState::from(ToolExposure::CodeModeOnly),
        ContextUsageDetailLoadState::from(ToolExposure::Hidden),
    ];

    let expected = vec![
        ContextUsageDetailLoadState::Loaded,
        ContextUsageDetailLoadState::Loaded,
        ContextUsageDetailLoadState::Deferred,
        ContextUsageDetailLoadState::Deferred,
        ContextUsageDetailLoadState::Available,
        ContextUsageDetailLoadState::Available,
    ];

    assert_eq!(actual, expected);
}

#[test]
fn store_uses_current_request_source_when_completed_snapshot_matches_latest() {
    let generated_at = Utc.with_ymd_and_hms(2026, 8, 29, 1, 2, 3).unwrap();
    let snapshot = Arc::new(ContextUsageSnapshot {
        snapshot_id: "snapshot-3".into(),
        request_sequence: 3,
        generated_at,
        model: "gpt-5.6".into(),
        model_context_window: Some(272_000),
        auto_compact_threshold: Some(260_000),
        reserved_tokens: Some(12_000),
        categories: vec![
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::BuiltInTools,
                estimated_tokens: 500,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::Messages,
                estimated_tokens: 1000,
            },
        ],
        mcp_tool_details: Vec::new(),
        instruction_details: Vec::new(),
        skill_details: Vec::new(),
        estimated_total_tokens: 1500,
        completeness: super::ContextUsageCompleteness::Complete,
        request_config_version: 3,
    });
    let usage = TokenUsage {
        input_tokens: 1400,
        cached_input_tokens: 0,
        cache_write_input_tokens: 0,
        output_tokens: 200,
        reasoning_output_tokens: 50,
        total_tokens: 1650,
        codex_rollout_budget_units: None,
    };
    let store = ContextUsageStore::default();

    store.publish(Arc::clone(&snapshot));
    store.mark_completed("snapshot-3".into(), usage.clone());

    assert_eq!(
        store.read(),
        Some(ContextUsageReadSnapshot {
            latest_snapshot: snapshot,
            last_completed_snapshot_id: Some("snapshot-3".into()),
            actual_source: ContextUsageActualSource::CurrentRequest,
            actual_usage: Some(usage),
        })
    );
}

#[test]
fn store_uses_previous_completed_request_source_when_latest_snapshot_is_newer() {
    let generated_at = Utc.with_ymd_and_hms(2026, 8, 29, 3, 4, 5).unwrap();
    let snapshot = Arc::new(ContextUsageSnapshot {
        snapshot_id: "snapshot-7".into(),
        request_sequence: 7,
        generated_at,
        model: "gpt-5.6".into(),
        model_context_window: Some(272_000),
        auto_compact_threshold: Some(260_000),
        reserved_tokens: Some(12_000),
        categories: vec![ContextUsageCategory {
            kind: ContextUsageCategoryKind::SystemPrompt,
            estimated_tokens: 1200,
        }],
        mcp_tool_details: Vec::new(),
        instruction_details: Vec::new(),
        skill_details: Vec::new(),
        estimated_total_tokens: 1200,
        completeness: super::ContextUsageCompleteness::Partial,
        request_config_version: 7,
    });
    let usage = TokenUsage {
        input_tokens: 900,
        cached_input_tokens: 0,
        cache_write_input_tokens: 0,
        output_tokens: 100,
        reasoning_output_tokens: 20,
        total_tokens: 1020,
        codex_rollout_budget_units: None,
    };
    let store = ContextUsageStore::default();

    store.mark_completed("snapshot-5".into(), usage.clone());
    store.publish(Arc::clone(&snapshot));

    assert_eq!(
        store.read(),
        Some(ContextUsageReadSnapshot {
            latest_snapshot: snapshot,
            last_completed_snapshot_id: Some("snapshot-5".into()),
            actual_source: ContextUsageActualSource::PreviousCompletedRequest,
            actual_usage: Some(usage),
        })
    );
}

#[test]
fn model_preview_preserves_cached_categories_and_marks_actual_as_previous() {
    let snapshot = Arc::new(ContextUsageSnapshot {
        snapshot_id: "snapshot-7".into(),
        request_sequence: 7,
        generated_at: Utc.with_ymd_and_hms(2026, 8, 29, 3, 4, 5).unwrap(),
        model: "gpt-5.6-sol".into(),
        model_context_window: Some(258_000),
        auto_compact_threshold: Some(244_400),
        reserved_tokens: Some(13_600),
        categories: vec![ContextUsageCategory {
            kind: ContextUsageCategoryKind::Messages,
            estimated_tokens: 1000,
        }],
        mcp_tool_details: Vec::new(),
        instruction_details: Vec::new(),
        skill_details: Vec::new(),
        estimated_total_tokens: 1000,
        completeness: super::ContextUsageCompleteness::Complete,
        request_config_version: 7,
    });
    let usage = TokenUsage {
        total_tokens: 1200,
        ..Default::default()
    };
    let read = ContextUsageReadSnapshot {
        latest_snapshot: Arc::clone(&snapshot),
        last_completed_snapshot_id: Some("snapshot-7".into()),
        actual_source: ContextUsageActualSource::CurrentRequest,
        actual_usage: Some(usage.clone()),
    };

    let preview =
        preview_context_usage_for_model(read, "gpt-5.6-terra".into(), Some(258_000), Some(244_400));
    let mut expected_snapshot = (*snapshot).clone();
    expected_snapshot.snapshot_id = preview.latest_snapshot.snapshot_id.clone();
    expected_snapshot.generated_at = preview.latest_snapshot.generated_at;
    expected_snapshot.model = "gpt-5.6-terra".into();
    expected_snapshot.completeness = super::ContextUsageCompleteness::Partial;
    expected_snapshot.request_config_version = 8;

    assert_eq!(
        preview,
        ContextUsageReadSnapshot {
            latest_snapshot: Arc::new(expected_snapshot),
            last_completed_snapshot_id: Some("snapshot-7".into()),
            actual_source: ContextUsageActualSource::PreviousCompletedRequest,
            actual_usage: Some(usage),
        }
    );
}

#[test]
fn store_returns_local_estimate_when_no_completed_usage_exists() {
    let snapshot = Arc::new(ContextUsageSnapshot {
        snapshot_id: "snapshot-1".into(),
        request_sequence: 1,
        generated_at: Utc.with_ymd_and_hms(2026, 8, 29, 6, 7, 8).unwrap(),
        model: "gpt-5.6".into(),
        model_context_window: None,
        auto_compact_threshold: None,
        reserved_tokens: None,
        categories: vec![ContextUsageCategory {
            kind: ContextUsageCategoryKind::Messages,
            estimated_tokens: 42,
        }],
        mcp_tool_details: vec![ContextUsageDetail {
            label: "calendar".into(),
            path: None,
            load_state: ContextUsageDetailLoadState::Available,
            estimated_tokens: 0,
        }],
        instruction_details: vec![ContextUsageDetail {
            label: "/repo/AGENTS.md".into(),
            path: Some(PathBuf::from("/repo/AGENTS.md")),
            load_state: ContextUsageDetailLoadState::Loaded,
            estimated_tokens: 20,
        }],
        skill_details: vec![ContextUsageDetail {
            label: "review-pr".into(),
            path: None,
            load_state: ContextUsageDetailLoadState::Loaded,
            estimated_tokens: 22,
        }],
        estimated_total_tokens: 42,
        completeness: super::ContextUsageCompleteness::Partial,
        request_config_version: 1,
    });
    let store = ContextUsageStore::default();

    store.publish(Arc::clone(&snapshot));

    assert_eq!(
        store.read(),
        Some(ContextUsageReadSnapshot {
            latest_snapshot: snapshot,
            last_completed_snapshot_id: None,
            actual_source: ContextUsageActualSource::LocalEstimate,
            actual_usage: None,
        })
    );
}

#[test]
fn store_returns_none_when_no_snapshot_has_been_published() {
    let store = ContextUsageStore::default();

    assert_eq!(store.read(), None);
}
