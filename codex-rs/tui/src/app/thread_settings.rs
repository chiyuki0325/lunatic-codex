//! Thread settings sync between TUI-local state and app-server thread state.

use super::App;
use crate::app_command::AppCommand;
use crate::app_event::AppEvent;
use crate::app_server_session::AppServerSession;
use crate::chatwidget::cyber_model_approval_reviewer;
use crate::session_state::ThreadSessionState;
use codex_app_server_protocol::ApprovalsReviewer as AppServerApprovalsReviewer;
use codex_app_server_protocol::AskForApproval as AppServerAskForApproval;
use codex_app_server_protocol::ConfigEdit;
use codex_app_server_protocol::ThreadSettings;
use codex_app_server_protocol::ThreadSettingsUpdateParams;
use codex_config::types::ApprovalsReviewer;
use codex_protocol::ThreadId;
use codex_protocol::config_types::ModeKind;
use codex_protocol::models::BUILT_IN_PERMISSION_PROFILE_WORKSPACE;
use codex_protocol::models::PermissionProfile;
use codex_protocol::openai_models::MODEL_SPECIALTY_CYBER;
use std::collections::VecDeque;

pub(super) enum BackgroundSettingsUpdate {
    Thread {
        params: ThreadSettingsUpdateParams,
        cyber_model_auto_review_notice: bool,
    },
    Config {
        edits: Vec<ConfigEdit>,
        success_message: Option<String>,
        error_prefix: String,
    },
}

#[derive(Default)]
pub(super) struct SettingsUpdateState {
    queue: VecDeque<BackgroundSettingsUpdate>,
    in_flight: bool,
    selection_closed: bool,
    thread_settings_supported: bool,
}

impl SettingsUpdateState {
    pub(super) fn new() -> Self {
        Self {
            thread_settings_supported: true,
            ..Self::default()
        }
    }

    fn is_idle(&self) -> bool {
        !self.in_flight && self.queue.is_empty()
    }
}

impl App {
    pub(super) fn queue_config_settings_update(
        &mut self,
        app_server: &AppServerSession,
        edits: Vec<ConfigEdit>,
        success_message: Option<String>,
        error_prefix: String,
    ) {
        self.enqueue_background_settings_update(
            app_server,
            BackgroundSettingsUpdate::Config {
                edits,
                success_message,
                error_prefix,
            },
        );
    }

    pub(super) fn queue_thread_settings_update(
        &mut self,
        app_server: &AppServerSession,
        params: ThreadSettingsUpdateParams,
        cyber_model_auto_review_notice: bool,
    ) -> bool {
        if !thread_settings_update_has_changes(&params) {
            return false;
        }
        self.enqueue_background_settings_update(
            app_server,
            BackgroundSettingsUpdate::Thread {
                params,
                cyber_model_auto_review_notice,
            },
        );
        true
    }

    fn enqueue_background_settings_update(
        &mut self,
        app_server: &AppServerSession,
        update: BackgroundSettingsUpdate,
    ) {
        self.settings_updates.queue.push_back(update);
        self.start_next_background_settings_update(app_server);
    }

    pub(super) fn start_next_background_settings_update(&mut self, app_server: &AppServerSession) {
        if self.settings_updates.in_flight {
            return;
        }
        if matches!(
            self.settings_updates.queue.front(),
            Some(BackgroundSettingsUpdate::Thread { .. })
        ) && self.chat_widget.is_agent_turn_running()
        {
            return;
        }
        let Some(update) = self.settings_updates.queue.pop_front() else {
            return;
        };
        self.settings_updates.in_flight = true;
        let request_handle = app_server.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        let thread_settings_supported = self.settings_updates.thread_settings_supported;
        tokio::spawn(async move {
            let (
                success_message,
                error_message,
                cyber_model_auto_review_notice,
                thread_settings_supported,
            ) = match update {
                BackgroundSettingsUpdate::Thread {
                    params,
                    cyber_model_auto_review_notice,
                } if thread_settings_supported => {
                    match crate::app_server_session::thread_settings_update_with_request_handle(
                        request_handle,
                        params,
                    )
                    .await
                    {
                        Ok(settings_updated) => (
                            None,
                            None,
                            cyber_model_auto_review_notice && settings_updated,
                            settings_updated,
                        ),
                        Err(err) => (None, Some(format!("更新会话设置失败：{err}")), false, true),
                    }
                }
                BackgroundSettingsUpdate::Thread { .. } => (None, None, false, false),
                BackgroundSettingsUpdate::Config {
                    edits,
                    success_message,
                    error_prefix,
                } => match crate::config_update::write_config_batch(request_handle, edits).await {
                    Ok(_) => (success_message, None, false, thread_settings_supported),
                    Err(err) => (
                        None,
                        Some(format!("{error_prefix}: {err}")),
                        false,
                        thread_settings_supported,
                    ),
                },
            };
            app_event_tx.send(AppEvent::SettingsUpdateCompleted {
                success_message,
                error_message,
                cyber_model_auto_review_notice,
                thread_settings_supported,
            });
        });
    }

    pub(super) fn handle_settings_update_completed(
        &mut self,
        app_server: &AppServerSession,
        success_message: Option<String>,
        error_message: Option<String>,
        cyber_model_auto_review_notice: bool,
        thread_settings_supported: bool,
    ) {
        self.settings_updates.in_flight = false;
        self.settings_updates.thread_settings_supported = thread_settings_supported;
        if let Some(message) = success_message {
            self.chat_widget.add_info_message(message, /*hint*/ None);
        }
        if let Some(message) = error_message {
            tracing::warn!("background settings update failed: {message}");
            self.chat_widget.add_error_message(message);
        }
        if cyber_model_auto_review_notice {
            self.app_event_tx.send(AppEvent::CyberModelAutoReviewNotice);
        }
        self.start_next_background_settings_update(app_server);
        self.maybe_settle_settings_selection();
    }

    pub(super) fn handle_settings_selection_closed(&mut self) {
        self.settings_updates.selection_closed = true;
        self.maybe_settle_settings_selection();
    }

    fn maybe_settle_settings_selection(&mut self) {
        if self.settings_updates.selection_closed && self.settings_updates.is_idle() {
            self.settings_updates.selection_closed = false;
            self.app_event_tx.send(AppEvent::SettingsSelectionSettled);
        }
    }

    pub(super) fn queue_active_thread_model_setting(
        &mut self,
        app_server: &AppServerSession,
        model: String,
        effort: Option<codex_protocol::openai_models::ReasoningEffort>,
    ) {
        let Some(mut params) = self.active_thread_model_setting_update_params(model) else {
            return;
        };
        params.effort = effort;
        let defaulted_to_auto_review = params.approvals_reviewer
            == Some(AppServerApprovalsReviewer::AutoReview)
            && (self.chat_widget.config_ref().approvals_reviewer != ApprovalsReviewer::AutoReview
                || AppServerAskForApproval::from(
                    self.chat_widget
                        .config_ref()
                        .permissions
                        .approval_policy
                        .value(),
                ) != AppServerAskForApproval::OnRequest);
        self.queue_thread_settings_update(app_server, params, defaulted_to_auto_review);
    }

    pub(super) fn active_thread_model_setting_update_params(
        &self,
        model: String,
    ) -> Option<ThreadSettingsUpdateParams> {
        let thread_id = self.active_thread_id?;
        let is_cyber_model = self.model_catalog.try_list_models().is_ok_and(|models| {
            models.iter().any(|preset| {
                preset.model == model
                    && preset.model_specialty.as_deref() == Some(MODEL_SPECIALTY_CYBER)
            })
        });

        let mut params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            model: Some(model),
            collaboration_mode: Some(self.chat_widget.effective_collaboration_mode()),
            ..ThreadSettingsUpdateParams::default()
        };

        if is_cyber_model {
            let workspace_profile = PermissionProfile::workspace_write();
            let workspace_allowed = self
                .config
                .permissions
                .can_set_permission_profile(&workspace_profile)
                .is_ok()
                && self.config.is_permission_profile_allowed(
                    BUILT_IN_PERMISSION_PROFILE_WORKSPACE,
                    &workspace_profile,
                );

            if workspace_allowed && let Some(reviewer) = cyber_model_approval_reviewer(&self.config)
            {
                params.permissions = Some(BUILT_IN_PERMISSION_PROFILE_WORKSPACE.to_string());
                params.approval_policy = Some(AppServerAskForApproval::OnRequest);
                params.approvals_reviewer = Some(reviewer.into());
            }
        }

        Some(params)
    }

    pub(super) fn queue_active_thread_reasoning_setting(
        &mut self,
        app_server: &AppServerSession,
        effort: Option<codex_protocol::openai_models::ReasoningEffort>,
    ) {
        let Some(params) = self.active_thread_reasoning_setting_update_params(effort) else {
            return;
        };
        self.queue_thread_settings_update(
            app_server, params, /*cyber_model_auto_review_notice*/ false,
        );
    }

    pub(super) fn active_thread_reasoning_setting_update_params(
        &self,
        effort: Option<codex_protocol::openai_models::ReasoningEffort>,
    ) -> Option<ThreadSettingsUpdateParams> {
        let thread_id = self.active_thread_id?;
        Some(ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            effort,
            collaboration_mode: Some(self.chat_widget.current_collaboration_mode().clone()),
            ..ThreadSettingsUpdateParams::default()
        })
    }

    pub(super) fn queue_active_thread_plan_mode_reasoning_setting(
        &mut self,
        app_server: &AppServerSession,
    ) {
        let Some(thread_id) = self.active_thread_id else {
            return;
        };
        let params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            collaboration_mode: Some(self.chat_widget.effective_collaboration_mode()),
            ..ThreadSettingsUpdateParams::default()
        };
        self.queue_thread_settings_update(
            app_server, params, /*cyber_model_auto_review_notice*/ false,
        );
    }

    pub(super) fn queue_active_thread_personality_setting(
        &mut self,
        app_server: &AppServerSession,
        personality: codex_protocol::config_types::Personality,
    ) {
        let Some(thread_id) = self.active_thread_id else {
            return;
        };
        let params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            personality: Some(personality),
            ..ThreadSettingsUpdateParams::default()
        };
        self.queue_thread_settings_update(
            app_server, params, /*cyber_model_auto_review_notice*/ false,
        );
    }

    pub(super) fn queue_override_turn_context_settings(
        &mut self,
        app_server: &AppServerSession,
        thread_id: ThreadId,
        op: &AppCommand,
    ) {
        let AppCommand::OverrideTurnContext {
            cwd,
            approval_policy,
            approvals_reviewer,
            permission_profile: _,
            active_permission_profile,
            windows_sandbox_level: _,
            model,
            effort,
            summary,
            service_tier,
            collaboration_mode,
            personality,
        } = op
        else {
            return;
        };

        let params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            cwd: cwd.clone(),
            approval_policy: *approval_policy,
            approvals_reviewer: approvals_reviewer.map(AppServerApprovalsReviewer::from),
            permissions: active_permission_profile
                .as_ref()
                .map(|profile| profile.id.clone()),
            model: model.clone(),
            effort: effort.clone().unwrap_or_default(),
            summary: *summary,
            service_tier: service_tier.clone(),
            collaboration_mode: collaboration_mode.clone(),
            personality: *personality,
            ..ThreadSettingsUpdateParams::default()
        };
        self.queue_thread_settings_update(
            app_server, params, /*cyber_model_auto_review_notice*/ false,
        );
    }

    pub(super) async fn apply_thread_settings_to_cached_session(
        &mut self,
        thread_id: ThreadId,
        settings: &ThreadSettings,
    ) {
        if self.primary_thread_id == Some(thread_id)
            && let Some(session) = self.primary_session_configured.as_mut()
        {
            apply_thread_settings_to_session(session, settings);
        }

        if let Some(channel) = self.thread_event_channels.get(&thread_id) {
            let mut store = channel.store.lock().await;
            if let Some(session) = store.session.as_mut() {
                apply_thread_settings_to_session(session, settings);
            }
        }
    }
}

fn apply_thread_settings_to_session(session: &mut ThreadSessionState, settings: &ThreadSettings) {
    if settings.collaboration_mode.mode == ModeKind::Default {
        session.model = settings.model.clone();
        session.reasoning_effort = settings.effort.clone();
    }
    session.model_provider_id = settings.model_provider.clone();
    session.service_tier = settings.service_tier.clone();
    session.approval_policy = settings.approval_policy;
    session.approvals_reviewer = settings.approvals_reviewer.to_core();
    session.permission_profile = PermissionProfile::from_legacy_sandbox_policy_for_cwd(
        &settings.sandbox_policy.to_core(),
        settings.cwd.as_path(),
    );
    session.active_permission_profile = settings.active_permission_profile.clone().map(Into::into);
    session.set_cwd_retargeting_implicit_runtime_workspace_root(settings.cwd.clone());
    session.personality = settings.personality;
    let mut collaboration_mode = settings.collaboration_mode.clone();
    collaboration_mode
        .settings
        .model
        .clone_from(&settings.model);
    collaboration_mode.settings.reasoning_effort = settings.effort.clone();
    session.collaboration_mode = Some(Box::new(collaboration_mode));
}

fn thread_settings_update_has_changes(params: &ThreadSettingsUpdateParams) -> bool {
    params.cwd.is_some()
        || params.approval_policy.is_some()
        || params.approvals_reviewer.is_some()
        || params.sandbox_policy.is_some()
        || params.permissions.is_some()
        || params.model.is_some()
        || params.service_tier.is_some()
        || params.effort.is_some()
        || params.summary.is_some()
        || params.collaboration_mode.is_some()
        || params.personality.is_some()
}
