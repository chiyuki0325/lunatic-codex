use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ContextUsageCompleteness;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadContextUsageParams;
use codex_app_server_protocol::ThreadContextUsageResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use pretty_assertions::assert_eq;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[cfg(windows)]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(25);
#[cfg(not(windows))]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn thread_context_usage_reads_live_thread_without_materializing_history() -> Result<()> {
    let codex_home = TempDir::new()?;
    let server = app_test_support::create_mock_responses_server_repeating_assistant("Done").await;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;

    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;

    let ThreadStartResponse { thread, .. } = app
        .start_thread(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let rollout_path = thread.path.clone().expect("thread path");
    assert!(
        !rollout_path.exists(),
        "thread start alone should not write rollout"
    );

    let request_id = app
        .send_thread_context_usage_request(ThreadContextUsageParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let response: ThreadContextUsageResponse =
        timeout(DEFAULT_TIMEOUT, app.read_response(request_id)).await??;

    assert_eq!(response.snapshot.model, "mock-model");
    match response.snapshot.completeness {
        ContextUsageCompleteness::Unavailable => {
            assert_eq!(response.snapshot.categories, Vec::new());
            assert_eq!(response.snapshot.estimated_total_tokens, 0);
        }
        ContextUsageCompleteness::Partial | ContextUsageCompleteness::Complete => {
            assert!(!response.snapshot.categories.is_empty());
            assert!(response.snapshot.estimated_total_tokens > 0);
        }
    }
    assert_eq!(response.snapshot.mcp_tool_details, Vec::new());
    assert_eq!(response.snapshot.instruction_details, Vec::new());
    assert_eq!(response.snapshot.skill_details, Vec::new());
    assert_eq!(response.actual_usage, None);
    assert_eq!(response.last_completed_snapshot_id, None);
    assert!(
        !rollout_path.exists(),
        "context usage read should stay read-only"
    );

    Ok(())
}

#[tokio::test]
async fn thread_context_usage_returns_last_token_usage_when_available() -> Result<()> {
    let codex_home = TempDir::new()?;
    let server = app_test_support::create_mock_responses_server_repeating_assistant("Done").await;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;

    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;

    let ThreadStartResponse { thread, .. } = app
        .start_thread(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;

    let _ = app
        .start_turn_and_wait_for_completion(codex_app_server_protocol::TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![codex_app_server_protocol::UserInput::Text {
                text: "hello".to_string(),
                text_elements: Vec::new(),
            }],
            client_user_message_id: None,
            responsesapi_client_metadata: None,
            additional_context: None,
            environments: None,
            cwd: None,
            runtime_workspace_roots: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            permissions: None,
            model: None,
            service_tier: None,
            effort: None,
            summary: None,
            personality: None,
            output_schema: None,
            collaboration_mode: None,
            multi_agent_mode: None,
        })
        .await?;

    let request_id = app
        .send_thread_context_usage_request(ThreadContextUsageParams {
            thread_id: thread.id.clone(),
        })
        .await?;
    let response: ThreadContextUsageResponse =
        timeout(DEFAULT_TIMEOUT, app.read_response(request_id)).await??;

    let actual_usage = response
        .actual_usage
        .expect("actual usage should be present");
    assert_eq!(
        actual_usage.snapshot_id,
        response.last_completed_snapshot_id
    );
    assert!(actual_usage.usage.total_tokens >= 0);
    assert_eq!(
        response.snapshot.completeness,
        ContextUsageCompleteness::Partial
    );

    Ok(())
}

#[tokio::test]
async fn thread_context_usage_rejects_not_loaded_threads() -> Result<()> {
    let codex_home = TempDir::new()?;
    let server = app_test_support::create_mock_responses_server_repeating_assistant("Done").await;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;

    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build_initialized()
        .await?;

    let request_id = app
        .send_thread_context_usage_request(ThreadContextUsageParams {
            thread_id: "bfd12a78-5900-467b-9bc5-d3d35df08191".to_string(),
        })
        .await?;
    let error: JSONRPCError = timeout(
        DEFAULT_TIMEOUT,
        app.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;

    assert_eq!(
        error.error.message,
        "thread not loaded: bfd12a78-5900-467b-9bc5-d3d35df08191"
    );

    Ok(())
}

#[tokio::test]
async fn thread_context_usage_requires_experimental_api_capability() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build()
        .await?;

    let init = app
        .initialize_with_capabilities(
            ClientInfo {
                name: "codex-app-server-tests".to_string(),
                title: None,
                version: "0.1.0".to_string(),
            },
            Some(InitializeCapabilities {
                experimental_api: false,
                request_attestation: false,
                opt_out_notification_methods: None,
                mcp_server_openai_form_elicitation: false,
                extensions: None,
            }),
        )
        .await?;
    let JSONRPCMessage::Response(_) = init else {
        anyhow::bail!("expected initialize response, got {init:?}");
    };

    let request_id = app
        .send_thread_context_usage_request(ThreadContextUsageParams {
            thread_id: "bfd12a78-5900-467b-9bc5-d3d35df08191".to_string(),
        })
        .await?;
    let error = timeout(
        DEFAULT_TIMEOUT,
        app.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;

    assert_eq!(
        error.error.message,
        "thread/contextUsage requires experimentalApi capability"
    );

    Ok(())
}
