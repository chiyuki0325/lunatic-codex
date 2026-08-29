//! Approval, denial, and review-status transcript cells.

use super::*;

fn truncate_exec_snippet(full_cmd: &str) -> String {
    let mut snippet = match full_cmd.split_once('\n') {
        Some((first, _)) => format!("{first} ..."),
        None => full_cmd.to_string(),
    };
    snippet = truncate_text(&snippet, /*max_graphemes*/ 80);
    snippet
}

fn exec_snippet(command: &[String]) -> String {
    let full_cmd = strip_bash_lc_and_escape(command);
    truncate_exec_snippet(&full_cmd)
}

fn non_empty_exec_snippet(command: &[String]) -> Option<String> {
    let snippet = exec_snippet(command);
    (!snippet.is_empty()).then_some(snippet)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReviewDecision {
    Approved,
    ApprovedExecpolicyAmendment {
        proposed_execpolicy_amendment: ExecPolicyAmendment,
    },
    ApprovedForSession,
    NetworkPolicyAmendment {
        network_policy_amendment: NetworkPolicyAmendment,
    },
    Denied,
    TimedOut,
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ApprovalDecisionSubject {
    Command(Vec<String>),
    NetworkAccess { target: String },
}

pub fn new_approval_decision_cell(
    subject: ApprovalDecisionSubject,
    decision: ReviewDecision,
    actor: ApprovalDecisionActor,
) -> Box<dyn HistoryCell> {
    use ReviewDecision::*;
    use codex_protocol::approvals::NetworkPolicyRuleAction;

    let (symbol, summary): (Span<'static>, Vec<Span<'static>>) = match decision {
        Approved => match subject {
            ApprovalDecisionSubject::Command(command) => {
                let summary = if let Some(snippet) = non_empty_exec_snippet(&command) {
                    vec![
                        actor.subject().into(),
                        "已批准".bold(),
                        "允许 Codex 运行 ".into(),
                        Span::from(snippet).dim(),
                        "（仅本次）".bold(),
                    ]
                } else {
                    vec![
                        actor.subject().into(),
                        "已批准".bold(),
                        "此请求".into(),
                        "（仅本次）".bold(),
                    ]
                };
                ("✔ ".green(), summary)
            }
            ApprovalDecisionSubject::NetworkAccess { target } => (
                "✔ ".green(),
                vec![
                    actor.subject().into(),
                    "已批准".bold(),
                    "允许 Codex 访问网络：".into(),
                    Span::from(target).dim(),
                    "（仅本次）".bold(),
                ],
            ),
        },
        ApprovedExecpolicyAmendment {
            proposed_execpolicy_amendment,
        } => {
            let snippet = Span::from(exec_snippet(&proposed_execpolicy_amendment.command)).dim();
            (
                "✔ ".green(),
                vec![
                    actor.subject().into(),
                    "已批准".bold(),
                    "允许 Codex 始终运行以下开头的命令：".into(),
                    snippet,
                ],
            )
        }
        ApprovedForSession => match subject {
            ApprovalDecisionSubject::Command(command) => {
                let summary = if let Some(snippet) = non_empty_exec_snippet(&command) {
                    vec![
                        actor.subject().into(),
                        "已批准".bold(),
                        "允许 Codex 运行 ".into(),
                        Span::from(snippet).dim(),
                        "（本次会话始终允许）".bold(),
                    ]
                } else {
                    vec![
                        actor.subject().into(),
                        "已批准".bold(),
                        "此请求".into(),
                        "（本次会话始终允许）".bold(),
                    ]
                };
                ("✔ ".green(), summary)
            }
            ApprovalDecisionSubject::NetworkAccess { target } => (
                "✔ ".green(),
                vec![
                    actor.subject().into(),
                    "已批准".bold(),
                    "允许 Codex 访问网络：".into(),
                    Span::from(target).dim(),
                    "（本次会话始终允许）".bold(),
                ],
            ),
        },
        NetworkPolicyAmendment {
            network_policy_amendment,
        } => {
            let target = match subject {
                ApprovalDecisionSubject::NetworkAccess { target } => target,
                ApprovalDecisionSubject::Command(_) => network_policy_amendment.host,
            };
            match network_policy_amendment.action {
                NetworkPolicyRuleAction::Allow => (
                    "✔ ".green(),
                    vec![
                        actor.subject().into(),
                        "已永久保存".bold(),
                        " Codex 的网络访问权限：".into(),
                        Span::from(target).dim(),
                    ],
                ),
                NetworkPolicyRuleAction::Deny => (
                    "✗ ".red(),
                    vec![
                        actor.subject().into(),
                        "已拒绝 Codex 的网络访问：".bold(),
                        Span::from(target).dim(),
                        "，并已保存该规则".into(),
                    ],
                ),
            }
        }
        Denied => match subject {
            ApprovalDecisionSubject::Command(command) => {
                let summary = if let Some(snippet) = non_empty_exec_snippet(&command) {
                    let snippet = Span::from(snippet).dim();
                    match actor {
                        ApprovalDecisionActor::User => {
                            vec![actor.subject().into(), "未批准 Codex 运行 ".bold(), snippet]
                        }
                        ApprovalDecisionActor::Guardian => {
                            vec!["请求".into(), "已拒绝 Codex 运行 ".bold(), snippet]
                        }
                    }
                } else {
                    match actor {
                        ApprovalDecisionActor::User => {
                            vec![actor.subject().into(), "未批准此请求".bold()]
                        }
                        ApprovalDecisionActor::Guardian => {
                            vec!["请求".into(), "已拒绝".bold()]
                        }
                    }
                };
                ("✗ ".red(), summary)
            }
            ApprovalDecisionSubject::NetworkAccess { target } => (
                "✗ ".red(),
                vec![
                    actor.subject().into(),
                    "未批准 Codex 的网络访问：".bold(),
                    Span::from(target).dim(),
                ],
            ),
        },
        TimedOut => match subject {
            ApprovalDecisionSubject::Command(command) => {
                let summary = if let Some(snippet) = non_empty_exec_snippet(&command) {
                    vec![
                        "审核".into(),
                        "已超时，Codex 未能运行 ".bold(),
                        Span::from(snippet).dim(),
                    ]
                } else {
                    vec!["审核".into(), "已超时，此请求未获批准".bold()]
                };
                ("✗ ".red(), summary)
            }
            ApprovalDecisionSubject::NetworkAccess { target } => (
                "✗ ".red(),
                vec![
                    "审核".into(),
                    "已超时，Codex 未能访问 ".bold(),
                    Span::from(target).dim(),
                ],
            ),
        },
        Abort => match subject {
            ApprovalDecisionSubject::Command(command) => {
                let summary = if let Some(snippet) = non_empty_exec_snippet(&command) {
                    vec![
                        actor.subject().into(),
                        "已取消运行请求：".bold(),
                        Span::from(snippet).dim(),
                    ]
                } else {
                    vec![actor.subject().into(), "已取消此请求".bold()]
                };
                ("✗ ".red(), summary)
            }
            ApprovalDecisionSubject::NetworkAccess { target } => (
                "✗ ".red(),
                vec![
                    actor.subject().into(),
                    "已取消 Codex 的网络访问请求：".bold(),
                    Span::from(target).dim(),
                ],
            ),
        },
    };

    Box::new(PrefixedWrappedHistoryCell::new(
        Line::from(summary),
        symbol,
        "  ",
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecisionActor {
    User,
    Guardian,
}

impl ApprovalDecisionActor {
    fn subject(self) -> &'static str {
        match self {
            Self::User => "你",
            Self::Guardian => "自动审核器",
        }
    }
}

pub fn new_guardian_denied_patch_request(files: Vec<String>) -> Box<dyn HistoryCell> {
    let mut summary = vec!["请求".into(), "已拒绝 Codex 应用".bold()];
    if files.len() == 1 {
        summary.push("涉及以下文件的补丁：".into());
        summary.push(Span::from(files[0].clone()).dim());
    } else {
        summary.push("涉及以下文件的补丁：".into());
        summary.push(Span::from(files.len().to_string()).dim());
        summary.push(" 个文件".into());
    }

    Box::new(PrefixedWrappedHistoryCell::new(
        Line::from(summary),
        "✗ ".red(),
        "  ",
    ))
}

pub fn new_guardian_denied_action_request(summary: String) -> Box<dyn HistoryCell> {
    let line = Line::from(vec![
        "请求".into(),
        "已拒绝：".bold(),
        Span::from(summary).dim(),
    ]);
    Box::new(PrefixedWrappedHistoryCell::new(line, "✗ ".red(), "  "))
}

pub fn new_guardian_timed_out_patch_request(files: Vec<String>) -> Box<dyn HistoryCell> {
    let mut summary = vec!["审核".into(), "已超时，Codex 未能应用".bold()];
    if files.len() == 1 {
        summary.push("涉及以下文件的补丁：".into());
        summary.push(Span::from(files[0].clone()).dim());
    } else {
        summary.push("涉及以下文件的补丁：".into());
        summary.push(Span::from(files.len().to_string()).dim());
        summary.push(" 个文件".into());
    }

    Box::new(PrefixedWrappedHistoryCell::new(
        Line::from(summary),
        "✗ ".red(),
        "  ",
    ))
}

pub fn new_guardian_timed_out_action_request(summary: String) -> Box<dyn HistoryCell> {
    let line = Line::from(vec![
        "审核".into(),
        "已超时：".bold(),
        Span::from(summary).dim(),
    ]);
    Box::new(PrefixedWrappedHistoryCell::new(line, "✗ ".red(), "  "))
}

/// Cyan history cell line showing the current review status.
pub(crate) fn new_review_status_line(message: String) -> PlainHistoryCell {
    PlainHistoryCell {
        lines: vec![Line::from(message.cyan())],
    }
}
