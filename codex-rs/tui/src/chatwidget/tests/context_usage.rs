use super::*;
use crate::history_cell::HistoryCell;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn context_command_shows_loading_card_and_sends_refresh_without_history() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Context);

    let Ok(AppEvent::RefreshContextUsage {
        request_id,
        thread_id,
    }) = rx.try_recv()
    else {
        panic!("expected RefreshContextUsage event");
    };
    assert_eq!(request_id, 0);
    assert_eq!(thread_id, chat.thread_id);
    assert_eq!(
        chat.pending_context_usage_output()
            .map(|cell| lines_to_single_string(&cell.display_lines(u16::MAX))),
        Some("正在统计上下文用量…\n".to_string()),
    );
    assert!(drain_insert_history(&mut rx).is_empty());
}

#[tokio::test]
async fn context_usage_sequential_responses_commit_in_invocation_order() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let thread_id = ThreadId::new();
    chat.thread_id = Some(thread_id);

    chat.start_context_usage_refresh(/*request_id*/ 10, Some(thread_id));
    chat.start_context_usage_refresh(/*request_id*/ 11, Some(thread_id));

    assert!(chat.finish_context_usage_refresh(
        /*request_id*/ 10,
        thread_id,
        Ok(context_usage_response("req-10", /*total_tokens*/ 1200)),
    ));
    assert!(chat.finish_context_usage_refresh(
        /*request_id*/ 11,
        thread_id,
        Ok(context_usage_response("req-11", /*total_tokens*/ 2200)),
    ));

    assert_matches!(rx.try_recv(), Ok(AppEvent::CommitPendingUsageOutput));
    assert_matches!(rx.try_recv(), Ok(AppEvent::CommitPendingUsageOutput));

    let first = chat
        .take_completed_context_usage_output()
        .expect("first context usage cell");
    let second = chat
        .take_completed_context_usage_output()
        .expect("second context usage cell");
    assert!(chat.take_completed_context_usage_output().is_none());

    let first_rendered = lines_to_single_string(&first.display_lines(/*width*/ 80));
    let second_rendered = lines_to_single_string(&second.display_lines(/*width*/ 80));
    assert!(first_rendered.contains("1.2K/100K Token"));
    assert!(second_rendered.contains("2.2K/100K Token"));
}

#[tokio::test]
async fn context_usage_parallel_out_of_order_keeps_completed_head_transient_until_gap_closes() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let thread_id = ThreadId::new();
    chat.thread_id = Some(thread_id);

    chat.start_context_usage_refresh(/*request_id*/ 20, Some(thread_id));
    chat.start_context_usage_refresh(/*request_id*/ 21, Some(thread_id));

    assert!(chat.finish_context_usage_refresh(
        /*request_id*/ 21,
        thread_id,
        Ok(context_usage_response("req-21", /*total_tokens*/ 2100)),
    ));
    assert!(chat.take_completed_context_usage_output().is_none());
    assert_eq!(
        chat.pending_context_usage_output()
            .map(|cell| lines_to_single_string(&cell.display_lines(u16::MAX))),
        Some("正在统计上下文用量…\n".to_string()),
    );

    assert!(chat.finish_context_usage_refresh(
        /*request_id*/ 20,
        thread_id,
        Ok(context_usage_response("req-20", /*total_tokens*/ 1100)),
    ));

    let first = chat
        .take_completed_context_usage_output()
        .expect("first committed cell after gap closed");
    let second = chat
        .take_completed_context_usage_output()
        .expect("second committed cell after gap closed");
    assert!(lines_to_single_string(&first.display_lines(/*width*/ 80)).contains("1.1K/100K Token"));
    assert!(
        lines_to_single_string(&second.display_lines(/*width*/ 80)).contains("2.1K/100K Token")
    );
}

#[tokio::test]
async fn context_usage_streaming_block_keeps_completed_card_visible_until_safe_boundary() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let thread_id = ThreadId::new();
    chat.thread_id = Some(thread_id);

    chat.start_context_usage_refresh(/*request_id*/ 30, Some(thread_id));
    chat.on_agent_message_delta("partial response".to_string());
    assert!(chat.usage_history_insertion_blocked());

    assert!(chat.finish_context_usage_refresh(
        /*request_id*/ 30,
        thread_id,
        Ok(context_usage_response("req-30", /*total_tokens*/ 3100)),
    ));
    assert_eq!(
        chat.pending_context_usage_output()
            .map(|cell| lines_to_single_string(&cell.display_lines(u16::MAX))),
        Some(render_context_usage_success("req-30", 3100)),
    );
    assert!(chat.take_completed_context_usage_output().is_none());

    chat.finalize_turn();
    assert!(
        std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::CommitPendingUsageOutputAfterStreamShutdown))
    );
}

#[tokio::test]
async fn context_usage_failure_keeps_composer_active() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let thread_id = ThreadId::new();
    chat.thread_id = Some(thread_id);

    chat.start_context_usage_refresh(/*request_id*/ 40, Some(thread_id));

    assert!(chat.finish_context_usage_refresh(
        /*request_id*/ 40,
        thread_id,
        Err("目标线程已不可用，暂时无法获取上下文用量。".to_string()),
    ));

    let cell = chat
        .take_completed_context_usage_output()
        .expect("failed context usage cell");
    assert_eq!(
        lines_to_single_string(&cell.display_lines(u16::MAX)),
        "■ 目标线程已不可用，暂时无法获取上下文用量\n",
    );
    assert!(!chat.blocks_direct_input);
}

#[tokio::test]
async fn late_result_for_previous_thread_is_discarded() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let old_thread = ThreadId::new();
    let new_thread = ThreadId::new();
    chat.thread_id = Some(old_thread);

    chat.start_context_usage_refresh(/*request_id*/ 50, Some(old_thread));
    chat.clear_pending_context_usage_refreshes();
    chat.thread_id = Some(new_thread);

    assert!(!chat.finish_context_usage_refresh(
        /*request_id*/ 50,
        old_thread,
        Ok(context_usage_response("req-50", /*total_tokens*/ 5000)),
    ));
    assert!(chat.pending_context_usage_output().is_none());
    assert!(chat.take_completed_context_usage_output().is_none());
}

fn context_usage_response(snapshot_id: &str, total_tokens: i64) -> ThreadContextUsageResponse {
    ThreadContextUsageResponse {
        snapshot: ContextUsageSnapshot {
            snapshot_id: snapshot_id.to_string(),
            request_sequence: 1,
            generated_at: 1,
            model: "gpt-5.6".to_string(),
            model_context_window: Some(100_000),
            auto_compact_threshold: Some(90_000),
            reserved_tokens: Some(10_000),
            categories: vec![ContextUsageCategory {
                kind: ContextUsageCategoryKind::Messages,
                estimated_tokens: total_tokens as u64,
            }],
            mcp_tool_details: Vec::new(),
            instruction_details: Vec::new(),
            skill_details: Vec::new(),
            estimated_total_tokens: total_tokens as u64,
            completeness: ContextUsageCompleteness::Complete,
            request_config_version: 1,
        },
        actual_usage: Some(codex_app_server_protocol::ContextUsageActualUsage {
            usage: codex_app_server_protocol::TokenUsageBreakdown {
                total_tokens,
                input_tokens: total_tokens,
                cached_input_tokens: 0,
                cache_write_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
            },
            snapshot_id: Some(snapshot_id.to_string()),
        }),
        actual_source: ContextUsageActualSource::CurrentRequest,
        last_completed_snapshot_id: Some(snapshot_id.to_string()),
    }
}

fn render_context_usage_success(snapshot_id: &str, total_tokens: i64) -> String {
    let cell = crate::history_cell::ContextUsageHistoryCell::success(
        context_usage_response(snapshot_id, total_tokens).snapshot,
        Some(codex_app_server_protocol::ContextUsageActualUsage {
            usage: codex_app_server_protocol::TokenUsageBreakdown {
                total_tokens,
                input_tokens: total_tokens,
                cached_input_tokens: 0,
                cache_write_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
            },
            snapshot_id: Some(snapshot_id.to_string()),
        }),
        ContextUsageActualSource::CurrentRequest,
        Some(snapshot_id.to_string()),
    );
    lines_to_single_string(&cell.display_lines(u16::MAX))
}
