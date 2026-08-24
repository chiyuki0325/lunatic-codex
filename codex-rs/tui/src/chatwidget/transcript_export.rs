//! Destination and filename prompts for on-demand transcript exports.

use super::*;
use crate::app_event::TranscriptExportDestination;

impl ChatWidget {
    pub(crate) fn copy_transcript_to_clipboard(&mut self, markdown: &str) {
        match crate::clipboard_copy::copy_to_clipboard(markdown) {
            Ok(lease) => {
                self.clipboard_lease = lease;
                self.add_info_message(
                    "已将对话复制到剪贴板".to_string(),
                    /*hint*/ None,
                );
            }
            Err(error) => self.add_error_message(format!("复制失败：{error}")),
        }
    }

    pub(super) fn show_transcript_export_popup(&mut self) {
        self.show_selection_view(SelectionViewParams {
            title: Some("导出对话".to_string()),
            subtitle: Some("将完整对话保存为 Markdown".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items: vec![
                SelectionItem {
                    name: "复制到剪贴板".to_string(),
                    description: Some("复制完整的 Markdown 对话记录".to_string()),
                    is_disabled: cfg!(target_os = "android"),
                    actions: vec![Box::new(|tx| {
                        tx.send(AppEvent::ExportTranscript {
                            destination: TranscriptExportDestination::Clipboard,
                        });
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "保存到文件".to_string(),
                    description: Some("选择 Markdown 文件名".to_string()),
                    actions: vec![Box::new(|tx| {
                        tx.send(AppEvent::OpenTranscriptExportFilePrompt);
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        self.defer_input_until_settings_applied();
        self.request_redraw();
    }

    pub(crate) fn show_transcript_export_file_prompt(&mut self) {
        let tx = self.app_event_tx.clone();
        let filename = self.thread_id().map_or_else(
            || "codex-session.md".to_string(),
            |thread_id| format!("codex-session-{thread_id}.md"),
        );
        let view = CustomPromptView::new(
            "保存对话".to_string(),
            "Type a filename and press Enter".to_string(),
            filename,
            /*context_label*/ None,
            Box::new(move |filename| {
                tx.send(AppEvent::ExportTranscript {
                    destination: TranscriptExportDestination::File(PathBuf::from(filename)),
                });
            }),
        );
        self.bottom_pane.show_text_prompt(view);
        self.request_redraw();
    }
}
