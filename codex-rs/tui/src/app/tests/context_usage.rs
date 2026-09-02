use super::*;
use codex_app_server_protocol::ThreadContextUsageResponse;
use pretty_assertions::assert_eq;
use ratatui::layout::Size;

#[tokio::test]
async fn refresh_context_usage_without_thread_id_waits_for_started_thread() {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let app_server = crate::start_embedded_app_server_for_picker(app.chat_widget.config_ref())
        .await
        .expect("test app-server");

    app.refresh_context_usage(&app_server, /*request_id*/ 7, /*thread_id*/ None);

    assert_eq!(app.context_usage_requests.pending.len(), 1);
    assert!(app_event_rx.try_recv().is_err());

    app_server
        .shutdown()
        .await
        .expect("shutdown test app-server");
}

#[tokio::test]
async fn startup_thread_started_dispatches_pending_context_usage() -> Result<()> {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let mut app_server =
        crate::start_embedded_app_server_for_picker(app.chat_widget.config_ref()).await?;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    tui.terminal.last_known_screen_size = Size {
        width: 100,
        height: 30,
    };

    app.pending_startup_thread_start = true;
    app.refresh_context_usage(&app_server, /*request_id*/ 11, /*thread_id*/ None);
    assert_eq!(app.context_usage_requests.pending.len(), 1);

    let started = crate::app_server_session::start_thread_with_request_handle(
        app_server.request_handle(),
        app.chat_widget.config_ref().clone(),
        app_server.thread_params_mode(),
        /*remote_cwd_override*/ None,
    )
    .await?;
    let started_thread_id = started.session.thread_id;

    app.handle_event(
        &mut tui,
        &mut app_server,
        AppEvent::StartupThreadStarted {
            result: Ok(started),
        },
    )
    .await?;

    assert!(app.context_usage_requests.pending.is_empty());

    let event = loop {
        match app_event_rx.recv().await {
            Some(AppEvent::ContextUsageLoaded {
                request_id,
                thread_id,
                result,
            }) if request_id == 11 && thread_id == started_thread_id => break result,
            Some(_) => continue,
            None => panic!("app event channel closed before context usage completion"),
        }
    };
    let ThreadContextUsageResponse { snapshot, .. } =
        event.map_err(|message| color_eyre::eyre::eyre!(message))?;
    assert_eq!(snapshot.model, app.chat_widget.current_model().to_string());

    app_server.shutdown().await?;
    Ok(())
}
