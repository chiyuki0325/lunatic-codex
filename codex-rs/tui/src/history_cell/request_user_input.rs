//! Completed request-user-input transcript rendering.

use super::*;

/// Renders a completed (or interrupted) request_user_input exchange in history.
#[derive(Debug)]
pub(crate) struct RequestUserInputResultCell {
    pub(crate) questions: Vec<ToolRequestUserInputQuestion>,
    pub(crate) answers: HashMap<String, ToolRequestUserInputAnswer>,
    pub(crate) interrupted: bool,
}

impl HistoryCell for RequestUserInputResultCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let width = width.max(1) as usize;
        let total = self.questions.len();
        let answered = self
            .questions
            .iter()
            .filter(|question| {
                self.answers
                    .get(&question.id)
                    .is_some_and(|answer| !answer.answers.is_empty())
            })
            .count();
        let unanswered = total.saturating_sub(answered);

        let mut header = vec!["•".dim(), " ".into(), "问题".bold()];
        header.push(format!(" 已回答 {answered}/{total}").dim());
        if self.interrupted {
            header.push("（已中断）".cyan());
        }

        let mut lines: Vec<Line<'static>> = vec![header.into()];

        for question in &self.questions {
            let answer = self.answers.get(&question.id);
            let answer_missing = match answer {
                Some(answer) => answer.answers.is_empty(),
                None => true,
            };
            let mut question_lines = wrap_with_prefix(
                &question.question,
                width,
                "  • ".into(),
                "    ".into(),
                Style::default(),
            );
            if answer_missing && let Some(last) = question_lines.last_mut() {
                last.spans.push("（未回答）".dim());
            }
            lines.extend(question_lines);

            let Some(answer) = answer.filter(|answer| !answer.answers.is_empty()) else {
                continue;
            };
            if question.is_secret {
                lines.extend(wrap_with_prefix(
                    "••••••",
                    width,
                    "    答案：".dim(),
                    "          ".dim(),
                    Style::default().fg(Color::Cyan),
                ));
                continue;
            }

            let (options, note) = split_request_user_input_answer(answer);

            for option in options {
                lines.extend(wrap_with_prefix(
                    &option,
                    width,
                    "    答案：".dim(),
                    "          ".dim(),
                    Style::default().fg(Color::Cyan),
                ));
            }
            if let Some(note) = note {
                let (label, continuation, style) = if question.options.is_some() {
                    (
                        "    备注：".dim(),
                        "          ".dim(),
                        Style::default().fg(Color::Cyan),
                    )
                } else {
                    (
                        "    答案：".dim(),
                        "          ".dim(),
                        Style::default().fg(Color::Cyan),
                    )
                };
                lines.extend(wrap_with_prefix(&note, width, label, continuation, style));
            }
        }

        if self.interrupted && unanswered > 0 {
            let summary = format!("有 {unanswered} 个问题未回答，已中断");
            lines.extend(wrap_with_prefix(
                &summary,
                width,
                "  ↳ ".cyan().dim(),
                "    ".dim(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
            ));
        }

        lines
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        let total = self.questions.len();
        let answered = self
            .questions
            .iter()
            .filter(|question| {
                self.answers
                    .get(&question.id)
                    .is_some_and(|answer| !answer.answers.is_empty())
            })
            .count();
        let mut lines = vec![Line::from(format!("问题：已回答 {answered}/{total}"))];
        if self.interrupted {
            lines.push(Line::from("（已中断）"));
        }
        for question in &self.questions {
            lines.push(Line::from(question.question.clone()));
            if let Some(answer) = self
                .answers
                .get(&question.id)
                .filter(|answer| !answer.answers.is_empty())
            {
                if question.is_secret {
                    lines.push(Line::from("答案：******"));
                } else {
                    let (options, note) = split_request_user_input_answer(answer);
                    lines.extend(
                        options
                            .into_iter()
                            .map(|option| Line::from(format!("答案：{option}"))),
                    );
                    if let Some(note) = note {
                        lines.push(Line::from(format!("备注：{note}")));
                    }
                }
            } else {
                lines.push(Line::from("（未回答）"));
            }
        }
        lines
    }
}

/// Wrap a plain string with textwrap and prefix each line, while applying a style to the content.
fn wrap_with_prefix(
    text: &str,
    width: usize,
    initial_prefix: Span<'static>,
    subsequent_prefix: Span<'static>,
    style: Style,
) -> Vec<Line<'static>> {
    let line = Line::from(vec![Span::from(text.to_string()).set_style(style)]);
    let opts = RtOptions::new(width.max(1))
        .initial_indent(Line::from(vec![initial_prefix]))
        .subsequent_indent(Line::from(vec![subsequent_prefix]));
    let wrapped = adaptive_wrap_line(&line, opts);
    let mut out = Vec::new();
    push_owned_lines(&wrapped, &mut out);
    out
}

/// Split a request_user_input answer into option labels and an optional freeform note.
/// Notes are encoded as "user_note: <text>" entries in the answers list.
fn split_request_user_input_answer(
    answer: &ToolRequestUserInputAnswer,
) -> (Vec<String>, Option<String>) {
    let mut options = Vec::new();
    let mut note = None;
    for entry in &answer.answers {
        if let Some(note_text) = entry.strip_prefix("user_note: ") {
            note = Some(note_text.to_string());
        } else {
            options.push(entry.clone());
        }
    }
    (options, note)
}
