//! Terminal precaution UI for misalignment policy violations.

use super::*;

const MISALIGNMENT_POLICY_TITLE: &str = "对话已作为预防措施停止";
const MISALIGNMENT_POLICY_DESCRIPTION: &str = "我们无法确认智能体正在安全地遵循你的指令。要继续工作，请新建或恢复其他对话。";

impl ChatWidget {
    pub(crate) fn has_misalignment_policy_violation(&self) -> bool {
        self.misalignment_policy_violation
    }

    pub(crate) fn rejects_misalignment_policy_op(&self, op: &AppCommand) -> bool {
        self.misalignment_policy_violation
            && !matches!(
                op,
                AppCommand::Interrupt
                    | AppCommand::CleanBackgroundTerminals
                    | AppCommand::OverrideTurnContext { .. }
                    | AppCommand::ReloadUserConfig
                    | AppCommand::ListSkills { .. }
                    | AppCommand::SetThreadName { .. }
            )
    }

    pub(crate) fn on_misalignment_policy_violation(&mut self) {
        if self.misalignment_policy_violation {
            return;
        }

        self.misalignment_policy_violation = true;
        self.input_queue.clear();
        self.finalize_turn();
        self.refresh_pending_input_preview();
        self.bottom_pane.drain_pending_submission_state();
        self.bottom_pane
            .set_composer_text(String::new(), Vec::new(), Vec::new());
        self.bottom_pane.set_composer_input_enabled(
            /*enabled*/ false,
            Some(MISALIGNMENT_POLICY_TITLE.to_string()),
        );

        self.show_misalignment_policy_precaution();
    }

    pub(crate) fn show_misalignment_policy_precaution(&mut self) {
        let mut items = vec![
            SelectionItem {
                name: "新建对话".to_string(),
                actions: vec![Box::new(|tx| tx.send(AppEvent::NewSession { name: None }))],
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "恢复其他对话".to_string(),
                actions: vec![Box::new(|tx| tx.send(AppEvent::OpenResumePicker))],
                ..Default::default()
            },
        ];
        if self.remote_connection.is_some() {
            items.insert(
                1,
                SelectionItem {
                    name: "智能体指挥中心".to_string(),
                    actions: vec![Box::new(|tx| tx.send(AppEvent::OpenAgentsOverview))],
                    ..Default::default()
                },
            );
        }
        self.bottom_pane.show_selection_view(SelectionViewParams {
            header: Box::new(
                Paragraph::new(vec![
                    Line::from(MISALIGNMENT_POLICY_TITLE).bold(),
                    Line::from(MISALIGNMENT_POLICY_DESCRIPTION).dim(),
                ])
                .wrap(Wrap { trim: false }),
            ),
            items,
            allow_cancel: false,
            ..Default::default()
        });
    }
}
