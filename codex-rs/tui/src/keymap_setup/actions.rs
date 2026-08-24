//! Catalog and accessors for keymap actions shown by `/keymap`.
//!
//! The descriptor table is the single UI-facing inventory of configurable
//! actions. Each descriptor ties together the config path segment, user-facing
//! context label, stable action name, and short description used by the picker
//! and action menu.
//!
//! Root-config accessors mirror the descriptor table, while runtime lookups
//! reuse the inventory owned by [`crate::keymap`]. A catalog action must remain
//! both writable in `TuiKeymap` and readable from the shared runtime inventory.

use std::collections::BTreeSet;

use codex_config::types::KeybindingsSpec;
use codex_config::types::TuiKeymap;
use crossterm::event::KeyEvent;

use crate::keymap::RuntimeKeymap;
use crate::keymap::bindings_for_action;

#[derive(Clone, Copy, Debug)]
pub(super) struct KeymapActionDescriptor {
    /// Config context segment, such as `composer` in `tui.keymap.composer.submit`.
    pub(super) context: &'static str,
    /// Human-readable group label shown in the picker.
    pub(super) context_label: &'static str,
    /// Config action segment, such as `submit` in `tui.keymap.composer.submit`.
    pub(super) action: &'static str,
    /// Short user-facing explanation of what the action does.
    pub(super) description: &'static str,
    /// Feature required before the action appears in `/keymap`.
    required_feature: Option<KeymapActionFeature>,
}

const fn action(
    context: &'static str,
    context_label: &'static str,
    action: &'static str,
    description: &'static str,
) -> KeymapActionDescriptor {
    KeymapActionDescriptor {
        context,
        context_label,
        action,
        description,
        required_feature: None,
    }
}

const fn gated_action(
    context: &'static str,
    context_label: &'static str,
    action: &'static str,
    description: &'static str,
    required_feature: KeymapActionFeature,
) -> KeymapActionDescriptor {
    KeymapActionDescriptor {
        context,
        context_label,
        action,
        description,
        required_feature: Some(required_feature),
    }
}

#[derive(Clone, Copy, Debug)]
enum KeymapActionFeature {
    FastMode,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeymapActionFilter {
    pub(crate) fast_mode_enabled: bool,
}

impl KeymapActionDescriptor {
    pub(super) fn is_visible(self, filter: KeymapActionFilter) -> bool {
        match self.required_feature {
            None => true,
            Some(KeymapActionFeature::FastMode) => filter.fast_mode_enabled,
        }
    }
}

#[rustfmt::skip]
pub(super) const KEYMAP_ACTIONS: &[KeymapActionDescriptor] = &[
    action("global", "全局", "open_agents", "打开共享的智能体会话概览。"),
    action("global", "全局", "open_transcript", "打开对话记录浮层。"),
    action("global", "全局", "open_external_editor", "在外部编辑器中打开当前草稿。"),
    action("global", "全局", "copy", "将上一条智能体回复复制到剪贴板。"),
    action("global", "全局", "clear_terminal", "清除终端界面。"),
    action("global", "全局", "toggle_vim_mode", "打开或关闭 Vim 编辑模式。"),
    gated_action("global", "全局", "toggle_fast_mode", "打开或关闭快速模式。", KeymapActionFeature::FastMode),
    action("global", "全局", "toggle_raw_output", "切换原始滚动缓冲区模式。"),
    action("global", "全局", "toggle_side_conversation", "在侧边对话与其父对话间切换。"),
    action("chat", "对话", "interrupt_turn", "中断当前轮次。"),
    action("chat", "对话", "decrease_reasoning_effort", "降低推理强度。"),
    action("chat", "对话", "increase_reasoning_effort", "提高推理强度。"),
    action("chat", "对话", "edit_queued_message", "编辑最近排队的消息。"),
    action("composer", "编辑框", "submit", "提交当前编辑框草稿。"),
    action("composer", "编辑框", "queue", "任务运行时将草稿加入队列。"),
    action("composer", "编辑框", "toggle_shortcuts", "显示或隐藏编辑框快捷键浮层。"),
    action("composer", "编辑框", "history_search_previous", "打开历史搜索或移至上一个匹配项。"),
    action("composer", "编辑框", "history_search_next", "移至下一个历史搜索匹配项。"),
    action("editor", "编辑器", "insert_newline", "在编辑器中插入换行。"),
    action("editor", "编辑器", "move_left", "将光标左移。"),
    action("editor", "编辑器", "move_right", "将光标右移。"),
    action("editor", "编辑器", "move_up", "将光标上移。"),
    action("editor", "编辑器", "move_down", "将光标下移。"),
    action("editor", "编辑器", "move_word_left", "移至上一个词的开头。"),
    action("editor", "编辑器", "move_word_right", "移至下一个词的结尾。"),
    action("editor", "编辑器", "move_line_start", "移至行首。"),
    action("editor", "编辑器", "move_line_end", "移至行尾。"),
    action("editor", "编辑器", "delete_backward", "删除左侧一个字素簇。"),
    action("editor", "编辑器", "delete_forward", "删除右侧一个字素簇。"),
    action("editor", "编辑器", "delete_backward_word", "删除上一个词。"),
    action("editor", "编辑器", "delete_forward_word", "删除下一个词。"),
    action("editor", "编辑器", "kill_line_start", "删除从光标到行首的内容。"),
    action("editor", "编辑器", "kill_whole_line", "删除当前行。"),
    action("editor", "编辑器", "kill_line_end", "删除从光标到行尾的内容。"),
    action("editor", "编辑器", "yank", "粘贴剪切缓冲区内容。"),
    action("vim_normal", "Vim 普通模式", "enter_insert", "在光标处进入插入模式。"),
    action("vim_normal", "Vim 普通模式", "append_after_cursor", "在光标后进入插入模式。"),
    action("vim_normal", "Vim 普通模式", "append_line_end", "在行尾进入插入模式。"),
    action("vim_normal", "Vim 普通模式", "insert_line_start", "在第一个非空白字符处进入插入模式。"),
    action("vim_normal", "Vim 普通模式", "open_line_below", "在下方新建一行并进入插入模式。"),
    action("vim_normal", "Vim 普通模式", "open_line_above", "在上方新建一行并进入插入模式。"),
    action("vim_normal", "Vim 普通模式", "move_left", "在 Vim 普通模式中左移。"),
    action("vim_normal", "Vim 普通模式", "move_right", "在 Vim 普通模式中右移。"),
    action("vim_normal", "Vim 普通模式", "move_up", "在 Vim 普通模式中上移或调出较早历史。"),
    action("vim_normal", "Vim 普通模式", "move_down", "在 Vim 普通模式中下移或调出较新历史。"),
    action("vim_normal", "Vim 普通模式", "move_word_forward", "移至下一个词的开头。"),
    action("vim_normal", "Vim 普通模式", "move_word_backward", "移至上一个词的开头。"),
    action("vim_normal", "Vim 普通模式", "move_word_end", "移至当前词或下一个词的结尾。"),
    action("vim_normal", "Vim 普通模式", "move_line_start", "移至行首。"),
    action("vim_normal", "Vim 普通模式", "move_line_end", "移至行尾。"),
    action("vim_normal", "Vim 普通模式", "delete_char", "删除光标下的字符。"),
    action("vim_normal", "Vim 普通模式", "replace_char", "替换光标下的字符。"),
    action("vim_normal", "Vim 普通模式", "substitute_char", "删除光标下的字符并进入插入模式。"),
    action("vim_normal", "Vim 普通模式", "delete_to_line_end", "删除从光标到行尾的内容。"),
    action("vim_normal", "Vim 普通模式", "change_to_line_end", "修改从光标到行尾的内容并进入插入模式。"),
    action("vim_normal", "Vim 普通模式", "yank_line", "复制整行。"),
    action("vim_normal", "Vim 普通模式", "paste_after", "在光标后粘贴。"),
    action("vim_normal", "Vim 普通模式", "start_delete_operator", "开始删除操作并等待移动命令。"),
    action("vim_normal", "Vim 普通模式", "start_yank_operator", "开始复制操作并等待移动命令。"),
    action("vim_normal", "Vim 普通模式", "start_change_operator", "开始修改操作并等待移动命令或文本对象。"),
    action("vim_normal", "Vim 普通模式", "cancel_operator", "取消待执行的 Vim 操作。"),
    action("vim_operator", "Vim 操作模式", "delete_line", "重复删除操作以删除整行。"),
    action("vim_operator", "Vim 操作模式", "yank_line", "重复复制操作以复制整行。"),
    action("vim_operator", "Vim 操作模式", "motion_left", "操作向左移动。"),
    action("vim_operator", "Vim 操作模式", "motion_right", "操作向右移动。"),
    action("vim_operator", "Vim 操作模式", "motion_up", "操作向上移动。"),
    action("vim_operator", "Vim 操作模式", "motion_down", "操作向下移动。"),
    action("vim_operator", "Vim 操作模式", "motion_word_forward", "操作移至下一个词的开头。"),
    action("vim_operator", "Vim 操作模式", "motion_word_backward", "操作移至上一个词的开头。"),
    action("vim_operator", "Vim 操作模式", "motion_word_end", "操作移至词尾。"),
    action("vim_operator", "Vim 操作模式", "motion_line_start", "操作移至行首。"),
    action("vim_operator", "Vim 操作模式", "motion_line_end", "操作移至行尾。"),
    action("vim_operator", "Vim 操作模式", "select_inner_text_object", "选择内部文本对象。"),
    action("vim_operator", "Vim 操作模式", "select_around_text_object", "选择包含文本对象。"),
    action("vim_operator", "Vim 操作模式", "cancel", "取消待执行的操作。"),
    action("vim_text_object", "Vim 文本对象", "word", "选定当前词。"),
    action("vim_text_object", "Vim 文本对象", "big_word", "选定当前 WORD。"),
    action("vim_text_object", "Vim 文本对象", "parentheses", "选定包围的圆括号。"),
    action("vim_text_object", "Vim 文本对象", "brackets", "选定包围的方括号。"),
    action("vim_text_object", "Vim 文本对象", "braces", "选定包围的花括号。"),
    action("vim_text_object", "Vim 文本对象", "double_quote", "选定包围的双引号。"),
    action("vim_text_object", "Vim 文本对象", "single_quote", "选定包围的单引号。"),
    action("vim_text_object", "Vim 文本对象", "backtick", "选定包围的反引号。"),
    action("vim_text_object", "Vim 文本对象", "cancel", "取消待选定的文本对象。"),
    action("pager", "分页器", "scroll_up", "向上滚动一行。"),
    action("pager", "分页器", "scroll_down", "向下滚动一行。"),
    action("pager", "分页器", "page_up", "向上滚动一页。"),
    action("pager", "分页器", "page_down", "向下滚动一页。"),
    action("pager", "分页器", "half_page_up", "向上滚动半页。"),
    action("pager", "分页器", "half_page_down", "向下滚动半页。"),
    action("pager", "分页器", "jump_top", "跳至开头。"),
    action("pager", "分页器", "jump_bottom", "跳至结尾。"),
    action("pager", "分页器", "close", "关闭分页器浮层。"),
    action("pager", "分页器", "close_transcript", "关闭对话记录浮层。"),
    action("list", "列表", "move_up", "将列表选择上移。"),
    action("list", "列表", "move_down", "将列表选择下移。"),
    action("list", "列表", "move_left", "在列表选择器中向左水平移动。"),
    action("list", "列表", "move_right", "在列表选择器中向右水平移动。"),
    action("list", "列表", "page_up", "将列表选择上移一页。"),
    action("list", "列表", "page_down", "将列表选择下移一页。"),
    action("list", "列表", "jump_top", "跳至第一个列表项。"),
    action("list", "列表", "jump_bottom", "跳至最后一个列表项。"),
    action("list", "列表", "accept", "接受当前列表选择。"),
    action("list", "列表", "cancel", "取消并关闭选择视图。"),
    action("agents", "智能体", "search", "搜索可用的智能体任务。"),
    action("agents", "智能体", "new_task", "开始编写新的智能体任务。"),
    action("agents", "智能体", "rename", "重命名选定任务。"),
    action("agents", "智能体", "stop", "停止选定的运行中任务。"),
    action("agents", "智能体", "toggle_grouping", "按状态或项目对任务分组。"),
    action("approval", "审批", "open_fullscreen", "全屏打开审批详情。"),
    action("approval", "审批", "open_thread", "可用时打开审批来源对话。"),
    action("approval", "审批", "approve", "批准主要选项。"),
    action("approval", "审批", "approve_for_session", "可用时批准本次会话。"),
    action("approval", "审批", "approve_for_prefix", "可用时按执行策略前缀批准。"),
    action("approval", "审批", "deny", "可用时选择明确拒绝选项。"),
    action("approval", "审批", "decline", "拒绝并提供修正指引。"),
    action("approval", "审批", "cancel", "取消信息征询请求。"),
];

/// Convert a stable action identifier into a display label.
///
/// This is intentionally presentation-only: the returned string must never be
/// parsed back into an action name, because underscores and casing are part of
/// the stable config contract.
pub(super) fn action_label(action: &str) -> String {
    let label = match action {
        "open_agents" => "打开智能体",
        "open_transcript" => "打开对话记录",
        "open_external_editor" => "打开外部编辑器",
        "copy" => "复制",
        "clear_terminal" => "清除终端",
        "toggle_vim_mode" => "切换 Vim 模式",
        "toggle_fast_mode" => "切换快速模式",
        "toggle_raw_output" => "切换原始输出",
        "toggle_side_conversation" => "切换侧边对话",
        "interrupt_turn" => "中断轮次",
        "decrease_reasoning_effort" => "降低推理强度",
        "increase_reasoning_effort" => "提高推理强度",
        "edit_queued_message" => "编辑排队消息",
        "submit" => "提交",
        "queue" => "加入队列",
        "toggle_shortcuts" => "切换快捷键",
        "history_search_previous" => "历史搜索上一个",
        "history_search_next" => "历史搜索下一个",
        "insert_newline" => "插入换行",
        "move_left" => "左移",
        "move_right" => "右移",
        "move_up" => "上移",
        "move_down" => "下移",
        "move_word_left" => "移至上一个词",
        "move_word_right" => "移至下一个词",
        "move_word_forward" => "移至下一个词",
        "move_word_backward" => "移至上一个词",
        "move_word_end" => "移至词尾",
        "move_line_start" => "移至行首",
        "move_line_end" => "移至行尾",
        "delete_backward" => "向后删除",
        "delete_forward" => "向前删除",
        "delete_backward_word" => "删除上一个词",
        "delete_forward_word" => "删除下一个词",
        "kill_line_start" => "删除至行首",
        "kill_whole_line" => "删除整行",
        "kill_line_end" => "删除至行尾",
        "yank" => "粘贴剪切缓冲区",
        "enter_insert" => "进入插入模式",
        "append_after_cursor" => "在光标后插入",
        "append_line_end" => "在行尾插入",
        "insert_line_start" => "在行首插入",
        "open_line_below" => "在下方新建行",
        "open_line_above" => "在上方新建行",
        "delete_char" => "删除字符",
        "replace_char" => "替换字符",
        "substitute_char" => "替换并进入插入模式",
        "delete_to_line_end" => "删除至行尾",
        "change_to_line_end" => "修改至行尾",
        "yank_line" => "复制整行",
        "paste_after" => "在光标后粘贴",
        "start_delete_operator" => "开始删除操作",
        "start_yank_operator" => "开始复制操作",
        "start_change_operator" => "开始修改操作",
        "cancel_operator" => "取消操作",
        "delete_line" => "删除整行",
        "motion_left" => "向左移动",
        "motion_right" => "向右移动",
        "motion_up" => "向上移动",
        "motion_down" => "向下移动",
        "motion_word_forward" => "移至下一个词",
        "motion_word_backward" => "移至上一个词",
        "motion_word_end" => "移至词尾",
        "motion_line_start" => "移至行首",
        "motion_line_end" => "移至行尾",
        "select_inner_text_object" => "选择内部文本对象",
        "select_around_text_object" => "选择包含文本对象",
        "cancel" => "取消",
        "word" => "词",
        "big_word" => "WORD",
        "parentheses" => "圆括号",
        "brackets" => "方括号",
        "braces" => "花括号",
        "double_quote" => "双引号",
        "single_quote" => "单引号",
        "backtick" => "反引号",
        "scroll_up" => "向上滚动",
        "scroll_down" => "向下滚动",
        "page_up" => "向上翻页",
        "page_down" => "向下翻页",
        "half_page_up" => "向上滚动半页",
        "half_page_down" => "向下滚动半页",
        "jump_top" => "跳至顶部",
        "jump_bottom" => "跳至底部",
        "close" => "关闭",
        "close_transcript" => "关闭对话记录",
        "accept" => "接受",
        "search" => "搜索",
        "new_task" => "新建任务",
        "rename" => "重命名",
        "stop" => "停止",
        "toggle_grouping" => "切换分组",
        "open_fullscreen" => "全屏打开",
        "open_thread" => "打开对话",
        "approve" => "批准",
        "approve_for_session" => "本次会话批准",
        "approve_for_prefix" => "按前缀批准",
        "deny" => "拒绝",
        "decline" => "谢绝",
        _ => action,
    };
    label.to_string()
}

#[rustfmt::skip]
/// Return the mutable root-config binding slot for one catalog action.
///
/// The returned `Option<KeybindingsSpec>` distinguishes three states that the
/// editor must preserve: absent means use fallback/default resolution, `Some`
/// with one or more keys is a custom binding, and `Some(Many([]))` is an
/// explicit unbind.
pub(super) fn binding_slot<'a>(
    keymap: &'a mut TuiKeymap,
    context: &str,
    action: &str,
) -> Option<&'a mut Option<KeybindingsSpec>> {
    match (context, action) {
        ("global", "open_agents") => Some(&mut keymap.global.open_agents),
        ("global", "open_transcript") => Some(&mut keymap.global.open_transcript),
        ("global", "open_external_editor") => Some(&mut keymap.global.open_external_editor),
        ("global", "copy") => Some(&mut keymap.global.copy),
        ("global", "clear_terminal") => Some(&mut keymap.global.clear_terminal),
        ("global", "toggle_vim_mode") => Some(&mut keymap.global.toggle_vim_mode),
        ("global", "toggle_fast_mode") => Some(&mut keymap.global.toggle_fast_mode),
        ("global", "toggle_raw_output") => Some(&mut keymap.global.toggle_raw_output),
        ("global", "toggle_side_conversation") => Some(&mut keymap.global.toggle_side_conversation),
        ("chat", "interrupt_turn") => Some(&mut keymap.chat.interrupt_turn),
        ("chat", "decrease_reasoning_effort") => Some(&mut keymap.chat.decrease_reasoning_effort),
        ("chat", "increase_reasoning_effort") => Some(&mut keymap.chat.increase_reasoning_effort),
        ("chat", "edit_queued_message") => Some(&mut keymap.chat.edit_queued_message),
        ("composer", "submit") => Some(&mut keymap.composer.submit),
        ("composer", "queue") => Some(&mut keymap.composer.queue),
        ("composer", "toggle_shortcuts") => Some(&mut keymap.composer.toggle_shortcuts),
        ("composer", "history_search_previous") => Some(&mut keymap.composer.history_search_previous),
        ("composer", "history_search_next") => Some(&mut keymap.composer.history_search_next),
        ("editor", "insert_newline") => Some(&mut keymap.editor.insert_newline),
        ("editor", "move_left") => Some(&mut keymap.editor.move_left),
        ("editor", "move_right") => Some(&mut keymap.editor.move_right),
        ("editor", "move_up") => Some(&mut keymap.editor.move_up),
        ("editor", "move_down") => Some(&mut keymap.editor.move_down),
        ("editor", "move_word_left") => Some(&mut keymap.editor.move_word_left),
        ("editor", "move_word_right") => Some(&mut keymap.editor.move_word_right),
        ("editor", "move_line_start") => Some(&mut keymap.editor.move_line_start),
        ("editor", "move_line_end") => Some(&mut keymap.editor.move_line_end),
        ("editor", "delete_backward") => Some(&mut keymap.editor.delete_backward),
        ("editor", "delete_forward") => Some(&mut keymap.editor.delete_forward),
        ("editor", "delete_backward_word") => Some(&mut keymap.editor.delete_backward_word),
        ("editor", "delete_forward_word") => Some(&mut keymap.editor.delete_forward_word),
        ("editor", "kill_line_start") => Some(&mut keymap.editor.kill_line_start),
        ("editor", "kill_whole_line") => Some(&mut keymap.editor.kill_whole_line),
        ("editor", "kill_line_end") => Some(&mut keymap.editor.kill_line_end),
        ("editor", "yank") => Some(&mut keymap.editor.yank),
        ("vim_normal", "enter_insert") => Some(&mut keymap.vim_normal.enter_insert),
        ("vim_normal", "append_after_cursor") => Some(&mut keymap.vim_normal.append_after_cursor),
        ("vim_normal", "append_line_end") => Some(&mut keymap.vim_normal.append_line_end),
        ("vim_normal", "insert_line_start") => Some(&mut keymap.vim_normal.insert_line_start),
        ("vim_normal", "open_line_below") => Some(&mut keymap.vim_normal.open_line_below),
        ("vim_normal", "open_line_above") => Some(&mut keymap.vim_normal.open_line_above),
        ("vim_normal", "move_left") => Some(&mut keymap.vim_normal.move_left),
        ("vim_normal", "move_right") => Some(&mut keymap.vim_normal.move_right),
        ("vim_normal", "move_up") => Some(&mut keymap.vim_normal.move_up),
        ("vim_normal", "move_down") => Some(&mut keymap.vim_normal.move_down),
        ("vim_normal", "move_word_forward") => Some(&mut keymap.vim_normal.move_word_forward),
        ("vim_normal", "move_word_backward") => Some(&mut keymap.vim_normal.move_word_backward),
        ("vim_normal", "move_word_end") => Some(&mut keymap.vim_normal.move_word_end),
        ("vim_normal", "move_line_start") => Some(&mut keymap.vim_normal.move_line_start),
        ("vim_normal", "move_line_end") => Some(&mut keymap.vim_normal.move_line_end),
        ("vim_normal", "delete_char") => Some(&mut keymap.vim_normal.delete_char),
        ("vim_normal", "replace_char") => Some(&mut keymap.vim_normal.replace_char),
        ("vim_normal", "substitute_char") => Some(&mut keymap.vim_normal.substitute_char),
        ("vim_normal", "delete_to_line_end") => Some(&mut keymap.vim_normal.delete_to_line_end),
        ("vim_normal", "change_to_line_end") => Some(&mut keymap.vim_normal.change_to_line_end),
        ("vim_normal", "yank_line") => Some(&mut keymap.vim_normal.yank_line),
        ("vim_normal", "paste_after") => Some(&mut keymap.vim_normal.paste_after),
        ("vim_normal", "start_delete_operator") => Some(&mut keymap.vim_normal.start_delete_operator),
        ("vim_normal", "start_yank_operator") => Some(&mut keymap.vim_normal.start_yank_operator),
        ("vim_normal", "start_change_operator") => Some(&mut keymap.vim_normal.start_change_operator),
        ("vim_normal", "cancel_operator") => Some(&mut keymap.vim_normal.cancel_operator),
        ("vim_operator", "delete_line") => Some(&mut keymap.vim_operator.delete_line),
        ("vim_operator", "yank_line") => Some(&mut keymap.vim_operator.yank_line),
        ("vim_operator", "motion_left") => Some(&mut keymap.vim_operator.motion_left),
        ("vim_operator", "motion_right") => Some(&mut keymap.vim_operator.motion_right),
        ("vim_operator", "motion_up") => Some(&mut keymap.vim_operator.motion_up),
        ("vim_operator", "motion_down") => Some(&mut keymap.vim_operator.motion_down),
        ("vim_operator", "motion_word_forward") => Some(&mut keymap.vim_operator.motion_word_forward),
        ("vim_operator", "motion_word_backward") => Some(&mut keymap.vim_operator.motion_word_backward),
        ("vim_operator", "motion_word_end") => Some(&mut keymap.vim_operator.motion_word_end),
        ("vim_operator", "motion_line_start") => Some(&mut keymap.vim_operator.motion_line_start),
        ("vim_operator", "motion_line_end") => Some(&mut keymap.vim_operator.motion_line_end),
        ("vim_operator", "select_inner_text_object") => Some(&mut keymap.vim_operator.select_inner_text_object),
        ("vim_operator", "select_around_text_object") => Some(&mut keymap.vim_operator.select_around_text_object),
        ("vim_operator", "cancel") => Some(&mut keymap.vim_operator.cancel),
        ("vim_text_object", "word") => Some(&mut keymap.vim_text_object.word),
        ("vim_text_object", "big_word") => Some(&mut keymap.vim_text_object.big_word),
        ("vim_text_object", "parentheses") => Some(&mut keymap.vim_text_object.parentheses),
        ("vim_text_object", "brackets") => Some(&mut keymap.vim_text_object.brackets),
        ("vim_text_object", "braces") => Some(&mut keymap.vim_text_object.braces),
        ("vim_text_object", "double_quote") => Some(&mut keymap.vim_text_object.double_quote),
        ("vim_text_object", "single_quote") => Some(&mut keymap.vim_text_object.single_quote),
        ("vim_text_object", "backtick") => Some(&mut keymap.vim_text_object.backtick),
        ("vim_text_object", "cancel") => Some(&mut keymap.vim_text_object.cancel),
        ("pager", "scroll_up") => Some(&mut keymap.pager.scroll_up),
        ("pager", "scroll_down") => Some(&mut keymap.pager.scroll_down),
        ("pager", "page_up") => Some(&mut keymap.pager.page_up),
        ("pager", "page_down") => Some(&mut keymap.pager.page_down),
        ("pager", "half_page_up") => Some(&mut keymap.pager.half_page_up),
        ("pager", "half_page_down") => Some(&mut keymap.pager.half_page_down),
        ("pager", "jump_top") => Some(&mut keymap.pager.jump_top),
        ("pager", "jump_bottom") => Some(&mut keymap.pager.jump_bottom),
        ("pager", "close") => Some(&mut keymap.pager.close),
        ("pager", "close_transcript") => Some(&mut keymap.pager.close_transcript),
        ("list", "move_up") => Some(&mut keymap.list.move_up),
        ("list", "move_down") => Some(&mut keymap.list.move_down),
        ("list", "move_left") => Some(&mut keymap.list.move_left),
        ("list", "move_right") => Some(&mut keymap.list.move_right),
        ("list", "page_up") => Some(&mut keymap.list.page_up),
        ("list", "page_down") => Some(&mut keymap.list.page_down),
        ("list", "jump_top") => Some(&mut keymap.list.jump_top),
        ("list", "jump_bottom") => Some(&mut keymap.list.jump_bottom),
        ("list", "accept") => Some(&mut keymap.list.accept),
        ("list", "cancel") => Some(&mut keymap.list.cancel),
        ("agents", "search") => Some(&mut keymap.agents.search),
        ("agents", "new_task") => Some(&mut keymap.agents.new_task),
        ("agents", "rename") => Some(&mut keymap.agents.rename),
        ("agents", "stop") => Some(&mut keymap.agents.stop),
        ("agents", "toggle_grouping") => Some(&mut keymap.agents.toggle_grouping),
        ("approval", "open_fullscreen") => Some(&mut keymap.approval.open_fullscreen),
        ("approval", "open_thread") => Some(&mut keymap.approval.open_thread),
        ("approval", "approve") => Some(&mut keymap.approval.approve),
        ("approval", "approve_for_session") => Some(&mut keymap.approval.approve_for_session),
        ("approval", "approve_for_prefix") => Some(&mut keymap.approval.approve_for_prefix),
        ("approval", "deny") => Some(&mut keymap.approval.deny),
        ("approval", "decline") => Some(&mut keymap.approval.decline),
        ("approval", "cancel") => Some(&mut keymap.approval.cancel),
        _ => None,
    }
}

/// Format an action's active single-key and chord alternatives in config order.
///
/// Duplicate runtime variants that normalize to the same config spec are shown
/// once so compatibility defaults do not appear as separate user choices.
pub(super) fn format_action_binding_summary(
    runtime_keymap: &RuntimeKeymap,
    context: &str,
    action: &str,
) -> String {
    let specs = super::active_binding_specs(runtime_keymap, context, action).unwrap_or_else(|_| {
        bindings_for_action(runtime_keymap, context, action)
            .unwrap_or_default()
            .iter()
            .filter_map(|binding| super::binding_to_config_key_spec(*binding).ok())
            .collect()
    });
    let mut seen = BTreeSet::new();
    let specs = specs
        .into_iter()
        .filter(|spec| seen.insert(spec.clone()))
        .collect::<Vec<_>>();
    if specs.is_empty() {
        "未绑定".to_string()
    } else {
        specs.join(", ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum KeymapDebugBindingSource {
    Custom,
    CustomGlobal,
    Default,
}

impl KeymapDebugBindingSource {
    pub(super) const fn label(&self) -> &'static str {
        match self {
            Self::Custom => "自定义",
            Self::CustomGlobal => "全局自定义",
            Self::Default => "默认",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct KeymapDebugActionMatch {
    pub(super) context: &'static str,
    pub(super) action: &'static str,
    pub(super) label: String,
    pub(super) description: &'static str,
    pub(super) source: KeymapDebugBindingSource,
}

pub(super) fn matching_actions_for_key_event(
    runtime_keymap: &RuntimeKeymap,
    keymap_config: &TuiKeymap,
    event: KeyEvent,
) -> Vec<KeymapDebugActionMatch> {
    KEYMAP_ACTIONS
        .iter()
        .filter_map(|descriptor| {
            let bindings =
                bindings_for_action(runtime_keymap, descriptor.context, descriptor.action)?;
            bindings
                .iter()
                .any(|binding| binding.is_press(event))
                .then(|| KeymapDebugActionMatch {
                    context: descriptor.context,
                    action: descriptor.action,
                    label: action_label(descriptor.action),
                    description: descriptor.description,
                    source: debug_binding_source(keymap_config, descriptor),
                })
        })
        .collect()
}

fn debug_binding_source(
    keymap_config: &TuiKeymap,
    descriptor: &KeymapActionDescriptor,
) -> KeymapDebugBindingSource {
    let mut keymap_config = keymap_config.clone();
    let Some(slot) = binding_slot(&mut keymap_config, descriptor.context, descriptor.action) else {
        return KeymapDebugBindingSource::Default;
    };
    if slot.is_some() {
        return KeymapDebugBindingSource::Custom;
    }

    let Some(global_slot) = global_fallback_slot(&mut keymap_config, descriptor) else {
        return KeymapDebugBindingSource::Default;
    };
    if global_slot.is_some() {
        KeymapDebugBindingSource::CustomGlobal
    } else {
        KeymapDebugBindingSource::Default
    }
}

fn global_fallback_slot<'a>(
    keymap: &'a mut TuiKeymap,
    descriptor: &KeymapActionDescriptor,
) -> Option<&'a mut Option<KeybindingsSpec>> {
    if descriptor.context != "composer" {
        return None;
    }

    match descriptor.action {
        "submit" => Some(&mut keymap.global.submit),
        "queue" => Some(&mut keymap.global.queue),
        "toggle_shortcuts" => Some(&mut keymap.global.toggle_shortcuts),
        _ => None,
    }
}
