use std::sync::Arc;
use std::time::Duration;

use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::AgentSemanticNameUpdatedEvent;
use codex_protocol::protocol::EventMsg;
use codex_rollout_trace::InferenceTraceContext;
use futures::StreamExt;
use tokio::time::timeout;

use crate::Prompt;
use crate::ResponseEvent;
use crate::content_items_to_text;
use crate::responses_metadata::CodexResponsesRequestKind;
use crate::session::session::Session;
use crate::session::step_context::StepContext;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_TASK_NAME_CHARS: usize = 128;
const MAX_MESSAGE_CHARS: usize = 2_000;
const MAX_LANGUAGE_CHARS: usize = 32;
const MAX_RAW_OUTPUT_CHARS: usize = 256;
const MAX_SEMANTIC_NAME_CHARS: usize = 32;
const NAMING_INSTRUCTIONS: &str = "Generate one concise human-readable role name for a software subagent. Follow the requested language. Return only the name on one line, without explanation, markdown, quotes, or punctuation wrappers.";

pub(crate) fn schedule_agent_semantic_name(
    session: Arc<Session>,
    step_context: Arc<StepContext>,
    agent_thread_id: ThreadId,
    agent_path: AgentPath,
    task_name: String,
    message: String,
    language: String,
) {
    tokio::spawn(async move {
        let result = timeout(
            REQUEST_TIMEOUT,
            generate_agent_semantic_name(&session, &step_context, task_name, message, language),
        )
        .await;
        let Ok(Ok(Some(semantic_name))) = result else {
            return;
        };
        match session
            .services
            .agent_control
            .set_agent_semantic_name(agent_thread_id, &agent_path, semantic_name.clone())
            .await
        {
            Ok(true) => {
                session
                    .send_ephemeral_event(
                        &step_context.turn,
                        EventMsg::AgentSemanticNameUpdated(AgentSemanticNameUpdatedEvent {
                            agent_thread_id,
                            semantic_name,
                        }),
                    )
                    .await;
            }
            Ok(false) => {}
            Err(err) => {
                tracing::debug!(%agent_thread_id, %agent_path, %err, "failed to save agent semantic name");
            }
        }
    });
}

async fn generate_agent_semantic_name(
    session: &Arc<Session>,
    step_context: &Arc<StepContext>,
    task_name: String,
    message: String,
    language: String,
) -> codex_protocol::error::Result<Option<String>> {
    let turn = &step_context.turn;
    let input = format!(
        "language: {}\ntask_name: {}\nmessage: {}",
        truncate_chars(&language, MAX_LANGUAGE_CHARS),
        truncate_chars(&task_name, MAX_TASK_NAME_CHARS),
        truncate_chars(&message, MAX_MESSAGE_CHARS),
    );
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: input }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }],
        tools: Arc::default(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: NAMING_INSTRUCTIONS.to_string(),
            provenance: None,
        },
        output_schema: None,
        output_schema_strict: true,
    };
    let window_id = session.current_window_id().await;
    let responses_metadata = turn.turn_metadata_state.to_responses_metadata(
        session.installation_id.clone(),
        window_id,
        CodexResponsesRequestKind::AgentName,
    );
    let mut client_session = session.services.model_client.new_session();
    let mut stream = client_session
        .stream(
            &prompt,
            &turn.model_info,
            &turn.session_telemetry,
            Some(ReasoningEffort::Low),
            turn.reasoning_summary,
            turn.config.service_tier.clone(),
            &responses_metadata,
            &InferenceTraceContext::disabled(),
        )
        .await?;
    let mut output = String::new();
    while let Some(event) = stream.next().await {
        match event? {
            ResponseEvent::OutputTextDelta(delta) => {
                let remaining = MAX_RAW_OUTPUT_CHARS.saturating_sub(output.chars().count());
                output.extend(delta.chars().take(remaining));
            }
            ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. })
                if output.is_empty() =>
            {
                if let Some(text) = content_items_to_text(&content) {
                    output.extend(text.chars().take(MAX_RAW_OUTPUT_CHARS));
                }
            }
            ResponseEvent::Completed { .. } => return Ok(normalize_semantic_name(&output)),
            _ => {}
        }
    }
    Ok(None)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn normalize_semantic_name(output: &str) -> Option<String> {
    let mut name = output.trim();
    if name.contains(['\n', '\r']) {
        return None;
    }
    for (opening, closing) in [
        ('"', '"'),
        ('\'', '\''),
        ('`', '`'),
        ('“', '”'),
        ('「', '」'),
        ('『', '』'),
    ] {
        if name.starts_with(opening) && name.ends_with(closing) && name.chars().count() >= 2 {
            name = &name[opening.len_utf8()..name.len() - closing.len_utf8()];
            name = name.trim();
            break;
        }
    }
    if name.is_empty() {
        return None;
    }
    Some(truncate_chars(name, MAX_SEMANTIC_NAME_CHARS))
}

#[cfg(test)]
#[path = "semantic_name_tests.rs"]
mod tests;
