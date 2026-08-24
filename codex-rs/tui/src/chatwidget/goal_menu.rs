//! Goal summary for the bare `/goal` command.

use super::*;
use crate::goal_display::format_goal_elapsed_seconds;
use crate::goal_files;
use crate::status::format_tokens_compact;

impl ChatWidget {
    pub(crate) fn show_goal_summary(&mut self, goal: AppThreadGoal) {
        self.add_plain_history_lines(goal_summary_lines(&goal));
    }

    pub(crate) fn show_goal_edit_prompt(&mut self, thread_id: ThreadId, goal: AppThreadGoal) {
        let tx = self.app_event_tx.clone();
        let status = edited_goal_status(goal.status);
        let token_budget = goal.token_budget;
        let view = CustomPromptView::new(
            "编辑目标".to_string(),
            "输入目标并按下 Enter".to_string(),
            goal.objective,
            /*context_label*/ None,
            Box::new(move |objective: String| {
                tx.send(AppEvent::SetThreadGoalDraft {
                    thread_id,
                    draft: goal_files::GoalDraft {
                        objective,
                        ..Default::default()
                    },
                    mode: crate::app_event::ThreadGoalSetMode::UpdateExisting {
                        status,
                        token_budget,
                    },
                });
            }),
        );
        self.bottom_pane.show_text_prompt(view);
    }

    pub(crate) fn show_resume_paused_goal_prompt(
        &mut self,
        thread_id: ThreadId,
        objective: String,
    ) {
        let resume_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::SetThreadGoalStatus {
                thread_id,
                status: AppThreadGoalStatus::Active,
            });
        })];
        self.show_selection_view(SelectionViewParams {
            title: Some("恢复已暂停的目标？".to_string()),
            subtitle: Some(format!("目标：{objective}")),
            footer_hint: Some(standard_popup_hint_line()),
            initial_selected_idx: Some(0),
            items: vec![
                SelectionItem {
                    name: "恢复目标".to_string(),
                    description: Some("将其标记为活动状态并在空闲时继续".to_string()),
                    actions: resume_actions,
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "保持暂停".to_string(),
                    description: Some("保持暂停；稍后使用 /goal resume".to_string()),
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
    }

    pub(crate) fn on_thread_goal_cleared(&mut self, thread_id: &str) {
        if self
            .thread_id
            .is_some_and(|active_thread_id| active_thread_id.to_string() == thread_id)
        {
            self.current_goal_status = None;
            self.update_collaboration_mode_indicator();
        }
    }
}

fn goal_summary_lines(goal: &AppThreadGoal) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from("目标".bold()),
        Line::from(vec![
            "状态：".dim(),
            goal_status_label(goal.status).to_string().into(),
        ]),
        Line::from(vec!["目标内容：".dim(), goal.objective.clone().into()]),
        Line::from(vec![
            "已用时间：".dim(),
            format_goal_elapsed_seconds(goal.time_used_seconds).into(),
        ]),
        Line::from(vec![
            "已用 Token：".dim(),
            format_tokens_compact(goal.tokens_used).into(),
        ]),
    ];
    if let Some(token_budget) = goal.token_budget {
        lines.push(Line::from(vec![
            "Token 预算：".dim(),
            format_tokens_compact(token_budget).into(),
        ]));
    }
    let command_hint = match goal.status {
        AppThreadGoalStatus::Active => "命令：/goal edit、/goal pause、/goal clear",
        AppThreadGoalStatus::Paused
        | AppThreadGoalStatus::Blocked
        | AppThreadGoalStatus::UsageLimited => "命令：/goal edit、/goal resume、/goal clear",
        AppThreadGoalStatus::BudgetLimited | AppThreadGoalStatus::Complete => {
            "命令：/goal edit、/goal clear"
        }
    };
    lines.push(Line::default());
    lines.push(Line::from(command_hint.dim()));
    lines
}

fn goal_status_label(status: AppThreadGoalStatus) -> &'static str {
    match status {
        AppThreadGoalStatus::Active => "进行中",
        AppThreadGoalStatus::Paused => "已暂停",
        AppThreadGoalStatus::Blocked => "已阻塞",
        AppThreadGoalStatus::UsageLimited => "受使用额度限制",
        AppThreadGoalStatus::BudgetLimited => "受 Token 预算限制",
        AppThreadGoalStatus::Complete => "已完成",
    }
}

fn edited_goal_status(status: AppThreadGoalStatus) -> AppThreadGoalStatus {
    match status {
        AppThreadGoalStatus::Active => AppThreadGoalStatus::Active,
        AppThreadGoalStatus::Paused
        | AppThreadGoalStatus::Blocked
        | AppThreadGoalStatus::UsageLimited => status,
        AppThreadGoalStatus::BudgetLimited | AppThreadGoalStatus::Complete => {
            AppThreadGoalStatus::Active
        }
    }
}
