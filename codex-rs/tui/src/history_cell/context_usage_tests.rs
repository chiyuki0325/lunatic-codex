use super::*;
use codex_app_server_protocol::ContextUsageActualSource;
use codex_app_server_protocol::ContextUsageActualUsage;
use codex_app_server_protocol::ContextUsageCategory;
use codex_app_server_protocol::ContextUsageCategoryKind;
use codex_app_server_protocol::ContextUsageCompleteness;
use codex_app_server_protocol::ContextUsageDetail;
use codex_app_server_protocol::ContextUsageDetailLoadState;
use codex_app_server_protocol::ContextUsageSnapshot;
use codex_app_server_protocol::TokenUsageBreakdown;
use pretty_assertions::assert_eq;

fn render(cell: &ContextUsageHistoryCell, width: u16) -> String {
    cell.display_lines(width)
        .into_iter()
        .map(|line| {
            line.spans
                .into_iter()
                .map(|span| span.content.into_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn grid_glyph_count(allocation: &GridAllocation) -> usize {
    allocation.slots.iter().map(|slot| slot.slots).sum()
}

fn grid_row_widths(rendered: &str) -> Vec<usize> {
    rendered
        .lines()
        .filter(|line| {
            !line.is_empty()
                && line
                    .chars()
                    .all(|ch| matches!(ch, '⛁' | '⛀' | '■' | '⛝' | '⛶'))
        })
        .map(display_width)
        .collect()
}

fn test_snapshot(window: Option<u64>) -> ContextUsageSnapshot {
    ContextUsageSnapshot {
        snapshot_id: "snap-1".to_string(),
        request_sequence: 42,
        generated_at: 1_756_000_000,
        model: "gpt-5.6".to_string(),
        model_context_window: window,
        auto_compact_threshold: Some(260_000),
        reserved_tokens: Some(12_000),
        categories: vec![
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::SystemPrompt,
                estimated_tokens: 8_200,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::BuiltInTools,
                estimated_tokens: 12_400,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::McpTools,
                estimated_tokens: 4_100,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::Instructions,
                estimated_tokens: 6_300,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::Skills,
                estimated_tokens: 2_000,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::Messages,
                estimated_tokens: 53_000,
            },
        ],
        mcp_tool_details: vec![ContextUsageDetail {
            label: "server.tool".to_string(),
            path: None,
            load_state: ContextUsageDetailLoadState::Loaded,
            estimated_tokens: 1_200,
        }],
        instruction_details: vec![ContextUsageDetail {
            label: "AGENTS.md".to_string(),
            path: Some("/path/to/AGENTS.md".to_string()),
            load_state: ContextUsageDetailLoadState::Loaded,
            estimated_tokens: 2_400,
        }],
        skill_details: vec![ContextUsageDetail {
            label: "skill-name".to_string(),
            path: None,
            load_state: ContextUsageDetailLoadState::Loaded,
            estimated_tokens: 620,
        }],
        estimated_total_tokens: 86_000,
        completeness: ContextUsageCompleteness::Complete,
        request_config_version: 7,
    }
}

fn test_usage(total_tokens: i64) -> ContextUsageActualUsage {
    ContextUsageActualUsage {
        usage: TokenUsageBreakdown {
            total_tokens,
            input_tokens: total_tokens,
            cached_input_tokens: 0,
            cache_write_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
        },
        snapshot_id: Some("snap-1".to_string()),
    }
}

#[test]
fn loading_cell_uses_chinese_message() {
    let cell = ContextUsageHistoryCell::loading();
    assert_eq!(cell.state(), ContextUsageCellState::Loading);
    assert_eq!(render(&cell, 80), "正在统计上下文用量…");
}

#[test]
fn error_cell_uses_chinese_message() {
    let cell = ContextUsageHistoryCell::error("暂时无法获取上下文用量");
    assert_eq!(cell.state(), ContextUsageCellState::Error);
    assert_eq!(render(&cell, 80), "■ 暂时无法获取上下文用量");
}

#[test]
fn success_cell_snapshot_normal_width() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        Some("snap-1".to_string()),
    );

    let rendered = render(&cell, 120);
    insta::assert_snapshot!("context_usage_success_normal", rendered);
}

#[test]
fn success_cell_snapshot_narrow_width() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        Some("snap-1".to_string()),
    );

    let rendered = render(&cell, 36);
    insta::assert_snapshot!("context_usage_success_narrow", rendered);
}

#[test]
fn unknown_window_hides_grid_and_percentages() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(None),
        None,
        ContextUsageActualSource::LocalEstimate,
        None,
    );

    let rendered = render(&cell, 80);
    assert!(rendered.contains("上下文窗口未知"));
    assert!(rendered.contains("窗口未知时不显示百分比、网格、预留区和剩余空间。"));
    assert!(!rendered.contains("⛝"));
    assert!(!rendered.contains("⛶"));
}

#[test]
fn grid_allocator_always_emits_exactly_100_slots() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 80,
    );
    assert_eq!(grid_glyph_count(&allocation), 100);
    assert_eq!(
        allocation.used_slots + allocation.reserve_slots + allocation.free_slots,
        100
    );
}

#[test]
fn category_allocation_uses_only_authoritative_used_share() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 80,
    );
    assert_eq!(allocation.used_slots, 31);
    assert_eq!(allocation.reserve_slots, 4);
    assert_eq!(allocation.free_slots, 65);
}

#[test]
fn undercount_adds_unattributed_category() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(90_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 120,
    );
    let unattributed = allocation
        .slots
        .iter()
        .find(|slot| slot.kind == LegendKind::Unattributed)
        .expect("unattributed slot");
    assert_eq!(unattributed.estimated_tokens, 4_000);
    assert!(unattributed.slots > 0);

    let rendered = render(&cell, 120);
    assert!(rendered.contains("未归因：4K Token"));
}

#[test]
fn overcount_shows_normalization_note() {
    let mut snapshot = test_snapshot(Some(272_000));
    snapshot.estimated_total_tokens = 120_000;
    snapshot.categories.push(ContextUsageCategory {
        kind: ContextUsageCategoryKind::Other,
        estimated_tokens: 34_000,
    });
    let cell = ContextUsageHistoryCell::success(
        snapshot,
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 120,
    );
    assert!(allocation.normalized_overestimate);
    assert_eq!(allocation.used_slots, 31);
    assert_eq!(grid_glyph_count(&allocation), 100);

    let rendered = render(&cell, 120);
    assert!(rendered.contains("网格已按实际已用区域归一化"));
}

#[test]
fn reserve_is_clipped_when_actual_usage_overlaps_it() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(100_000)),
        Some(test_usage(96_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 80,
    );
    assert_eq!(
        allocation.reserve_slots + allocation.free_slots + allocation.used_slots,
        100
    );
    assert_eq!(allocation.used_slots, 96);
    assert_eq!(allocation.reserve_slots, 4);
    assert_eq!(allocation.free_slots, 0);
}

#[test]
fn tiny_nonzero_categories_get_visibility_when_possible() {
    let snapshot = ContextUsageSnapshot {
        snapshot_id: "tiny".to_string(),
        request_sequence: 1,
        generated_at: 0,
        model: "gpt-5.6".to_string(),
        model_context_window: Some(10_000),
        auto_compact_threshold: None,
        reserved_tokens: None,
        categories: vec![
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::SystemPrompt,
                estimated_tokens: 1,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::BuiltInTools,
                estimated_tokens: 1,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::McpTools,
                estimated_tokens: 1,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::Instructions,
                estimated_tokens: 9_997,
            },
        ],
        mcp_tool_details: Vec::new(),
        instruction_details: Vec::new(),
        skill_details: Vec::new(),
        estimated_total_tokens: 10_000,
        completeness: ContextUsageCompleteness::Complete,
        request_config_version: 0,
    };
    let cell = ContextUsageHistoryCell::success(
        snapshot,
        None,
        ContextUsageActualSource::LocalEstimate,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 120,
    );
    let visible = allocation
        .slots
        .iter()
        .filter(|slot| slot.kind != LegendKind::Free && slot.kind != LegendKind::Reserve)
        .filter(|slot| slot.slots > 0)
        .count();
    assert_eq!(visible, 4);
    assert!(
        allocation
            .slots
            .iter()
            .filter(|slot| slot.kind == LegendKind::SystemPrompt)
            .all(|slot| slot.slots >= 1)
    );
}

#[test]
fn hamilton_tie_break_uses_fixed_category_order() {
    let snapshot = ContextUsageSnapshot {
        snapshot_id: "ties".to_string(),
        request_sequence: 1,
        generated_at: 0,
        model: "gpt-5.6".to_string(),
        model_context_window: Some(100),
        auto_compact_threshold: None,
        reserved_tokens: None,
        categories: vec![
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::BuiltInTools,
                estimated_tokens: 1,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::McpTools,
                estimated_tokens: 1,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::Instructions,
                estimated_tokens: 1,
            },
        ],
        mcp_tool_details: Vec::new(),
        instruction_details: Vec::new(),
        skill_details: Vec::new(),
        estimated_total_tokens: 3,
        completeness: ContextUsageCompleteness::Complete,
        request_config_version: 0,
    };
    let cell = ContextUsageHistoryCell::success(
        snapshot,
        Some(test_usage(2)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 120,
    );
    assert_eq!(allocation.used_slots, 2);
    assert_eq!(
        allocation
            .slots
            .iter()
            .find(|slot| slot.kind == LegendKind::BuiltInTools)
            .expect("built-in slot")
            .slots,
        1
    );
    assert_eq!(
        allocation
            .slots
            .iter()
            .find(|slot| slot.kind == LegendKind::McpTools)
            .expect("mcp slot")
            .slots,
        1
    );
    assert_eq!(
        allocation
            .slots
            .iter()
            .find(|slot| slot.kind == LegendKind::Instructions)
            .expect("instructions slot")
            .slots,
        0
    );
}

#[test]
fn narrow_width_changes_columns_but_not_slot_count() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 36,
    );
    assert_eq!(allocation.grid_columns, 2);
    assert_eq!(allocation.grid_rows, 50);
    assert_eq!(
        allocation.reserve_slots + allocation.free_slots + allocation.used_slots,
        100
    );
}

#[test]
fn grid_rows_round_up_when_columns_do_not_divide_100() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 55,
    );
    assert_eq!(allocation.grid_columns, 4);
    assert_eq!(allocation.grid_rows, 25);
    assert_eq!(grid_glyph_count(&allocation), 100);
}

#[test]
fn detail_sections_render_load_state_and_wrap_paths() {
    let mut snapshot = test_snapshot(Some(272_000));
    snapshot.mcp_tool_details = vec![ContextUsageDetail {
        label: "server.tool".to_string(),
        path: None,
        load_state: ContextUsageDetailLoadState::Deferred,
        estimated_tokens: 0,
    }];
    let cell = ContextUsageHistoryCell::success(
        snapshot,
        Some(test_usage(86_000)),
        ContextUsageActualSource::PreviousCompletedRequest,
        Some("snap-1".to_string()),
    );

    let rendered = render(&cell, 28);
    let compact = rendered.replace(['\n', ' '], "");
    assert!(compact.contains("（延迟加载）"));
    assert!(compact.contains("顶部实际值来自最近一次已完成请求"));
    assert!(rendered.contains("└ server.tool：0 Token"));
}

#[test]
fn detail_sections_indent_wrapped_lines() {
    let mut snapshot = test_snapshot(Some(272_000));
    snapshot.instruction_details = vec![ContextUsageDetail {
        label: "AGENTS.md".to_string(),
        path: Some("/very/long/path/to/a/context/file/that-needs-wrap/AGENTS.md".to_string()),
        load_state: ContextUsageDetailLoadState::Loaded,
        estimated_tokens: 2_400,
    }];
    let cell = ContextUsageHistoryCell::success(
        snapshot,
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        Some("snap-1".to_string()),
    );

    let rendered = render(&cell, 28);
    assert!(rendered.contains("└ /very/long/path/to/a/"));
    assert!(rendered.contains("  context/file/that-"));
}

#[test]
fn partial_glyph_marks_fractional_regular_categories() {
    let snapshot = ContextUsageSnapshot {
        snapshot_id: "partial".to_string(),
        request_sequence: 1,
        generated_at: 0,
        model: "gpt-5.6".to_string(),
        model_context_window: Some(100),
        auto_compact_threshold: None,
        reserved_tokens: None,
        categories: vec![
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::SystemPrompt,
                estimated_tokens: 1,
            },
            ContextUsageCategory {
                kind: ContextUsageCategoryKind::Messages,
                estimated_tokens: 32,
            },
        ],
        mcp_tool_details: Vec::new(),
        instruction_details: Vec::new(),
        skill_details: Vec::new(),
        estimated_total_tokens: 33,
        completeness: ContextUsageCompleteness::Complete,
        request_config_version: 0,
    };
    let cell = ContextUsageHistoryCell::success(
        snapshot,
        Some(test_usage(30)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let allocation = allocate_grid(
        cell.success.as_ref().expect("success data"),
        /*width*/ 120,
    );
    let system = allocation
        .slots
        .iter()
        .find(|slot| slot.kind == LegendKind::SystemPrompt)
        .expect("system slot");
    let messages = allocation
        .slots
        .iter()
        .find(|slot| slot.kind == LegendKind::Messages)
        .expect("message slot");
    assert_eq!(system.grid_glyph, "⛀");
    assert_eq!(messages.grid_glyph, "■");
}

#[test]
fn raw_lines_follow_plain_history_cell_convention() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let raw_lines = cell.raw_lines();
    let display_plain = plain_lines(cell.display_lines(u16::MAX));
    assert_eq!(raw_lines, display_plain);
}

#[test]
fn rendered_grid_rows_use_unicode_width_limits() {
    let cell = ContextUsageHistoryCell::success(
        test_snapshot(Some(272_000)),
        Some(test_usage(86_000)),
        ContextUsageActualSource::CurrentRequest,
        None,
    );

    let rendered = render(&cell, 36);
    let row_widths = grid_row_widths(&rendered);
    assert_eq!(row_widths.len(), 50);
    assert!(row_widths.iter().all(|width| (2..=4).contains(width)));
}
