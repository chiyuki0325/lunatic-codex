use super::*;
use crate::status::format_tokens_compact;
use crate::width::display_width;
use codex_app_server_protocol::ContextUsageActualSource;
use codex_app_server_protocol::ContextUsageActualUsage;
use codex_app_server_protocol::ContextUsageCategory;
use codex_app_server_protocol::ContextUsageCompleteness;
use codex_app_server_protocol::ContextUsageDetail;
use codex_app_server_protocol::ContextUsageDetailLoadState;
use codex_app_server_protocol::ContextUsageSnapshot;
use ratatui::text::Text;
use textwrap::wrap;

#[path = "context_usage_grid.rs"]
mod context_usage_grid;

use context_usage_grid::AllocatedSlot;
use context_usage_grid::GridAllocation;
use context_usage_grid::LegendKind;
use context_usage_grid::allocate_grid;
use context_usage_grid::ordered_categories;

const MIN_LEGEND_WIDTH: usize = 28;
const GRID_LEGEND_GAP: usize = 2;

fn styled_glyph(kind: LegendKind, glyph: &'static str) -> Span<'static> {
    match kind {
        LegendKind::SystemPrompt => glyph.cyan(),
        LegendKind::BuiltInTools => glyph.green(),
        LegendKind::McpTools => glyph.magenta(),
        LegendKind::Instructions => glyph.cyan().italic(),
        LegendKind::Skills => glyph.blue(),
        LegendKind::Messages => glyph.bold(),
        LegendKind::Other => glyph.dim(),
        LegendKind::Unattributed => glyph.red(),
        LegendKind::Reserve => glyph.magenta().dim(),
        LegendKind::Free => glyph.dim(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextUsageCellState {
    Loading,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextUsageHistoryCell {
    state: ContextUsageCellState,
    loading_message: Option<String>,
    error_message: Option<String>,
    success: Option<ContextUsageSuccessData>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContextUsageSuccessData {
    snapshot: ContextUsageSnapshot,
    actual_usage: Option<ContextUsageActualUsage>,
    actual_source: ContextUsageActualSource,
    last_completed_snapshot_id: Option<String>,
}

impl ContextUsageHistoryCell {
    pub(crate) fn loading() -> Self {
        Self {
            state: ContextUsageCellState::Loading,
            loading_message: Some("正在统计上下文用量…".to_string()),
            error_message: None,
            success: None,
        }
    }

    pub(crate) fn error(message: impl Into<String>) -> Self {
        Self {
            state: ContextUsageCellState::Error,
            loading_message: None,
            error_message: Some(message.into()),
            success: None,
        }
    }

    pub(crate) fn success(
        snapshot: ContextUsageSnapshot,
        actual_usage: Option<ContextUsageActualUsage>,
        actual_source: ContextUsageActualSource,
        last_completed_snapshot_id: Option<String>,
    ) -> Self {
        Self {
            state: ContextUsageCellState::Success,
            loading_message: None,
            error_message: None,
            success: Some(ContextUsageSuccessData {
                snapshot,
                actual_usage,
                actual_source,
                last_completed_snapshot_id,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn state(&self) -> ContextUsageCellState {
        self.state
    }
}

impl HistoryCell for ContextUsageHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        match self.state {
            ContextUsageCellState::Loading => vec![Line::from(
                self.loading_message
                    .clone()
                    .unwrap_or_else(|| "正在统计上下文用量…".to_string())
                    .cyan(),
            )],
            ContextUsageCellState::Error => vec![Line::from(
                format!(
                    "■ {}",
                    self.error_message
                        .clone()
                        .unwrap_or_else(|| "暂时无法获取上下文用量".to_string())
                )
                .red(),
            )],
            ContextUsageCellState::Success => self
                .success
                .as_ref()
                .map_or_else(Vec::new, |success| render_success_cell(success, width)),
        }
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        match self.state {
            ContextUsageCellState::Loading => {
                vec![Line::from(self.loading_message.clone().unwrap_or_default())]
            }
            ContextUsageCellState::Error => vec![Line::from(
                self.error_message
                    .clone()
                    .unwrap_or_else(|| "暂时无法获取上下文用量".to_string()),
            )],
            ContextUsageCellState::Success => self
                .success
                .as_ref()
                .map_or_else(Vec::new, |_| plain_lines(self.display_lines(u16::MAX))),
        }
    }
}

fn render_success_cell(success: &ContextUsageSuccessData, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width.max(1));
    let mut lines = vec![Line::from("上下文用量".bold()), Line::from("")];

    let body = if success.snapshot.model_context_window.is_some() {
        render_known_window(success, width)
    } else {
        render_unknown_window(success, width)
    };
    lines.extend(body);
    lines
}

fn render_known_window(success: &ContextUsageSuccessData, width: usize) -> Vec<Line<'static>> {
    let allocation = allocate_grid(success, width);
    let grid_lines = render_grid_lines(&allocation);
    let legend_lines = render_legend_lines(success, &allocation);

    let mut lines = if can_render_side_by_side(width, &grid_lines, &legend_lines) {
        render_side_by_side(grid_lines, legend_lines, width)
    } else {
        let mut stacked = Vec::new();
        stacked.extend(grid_lines);
        stacked.push(Line::from(""));
        stacked.extend(legend_lines);
        stacked
    };

    append_data_freshness(&mut lines, success, /*window_known*/ true);
    append_detail_section(
        &mut lines,
        "MCP 工具 · /mcp",
        &success.snapshot.mcp_tool_details,
        width,
    );
    append_detail_section(
        &mut lines,
        "指令文件",
        &success.snapshot.instruction_details,
        width,
    );
    append_detail_section(
        &mut lines,
        "Skills · /skills",
        &success.snapshot.skill_details,
        width,
    );
    lines
}

fn render_unknown_window(success: &ContextUsageSuccessData, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        success.snapshot.model.clone().bold(),
        " · 上下文窗口未知".dim(),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from("各分类估算用量".bold()));

    lines.extend(
        ordered_categories(&success.snapshot.categories)
            .into_iter()
            .filter(|category| category.estimated_tokens > 0)
            .map(category_line_without_percent),
    );

    match success.snapshot.completeness {
        ContextUsageCompleteness::Partial => {
            lines.push(Line::from(""));
            lines.push(Line::from("当前快照不完整，仅展示可靠统计的部分。".dim()));
        }
        ContextUsageCompleteness::Unavailable => {
            lines.push(Line::from(""));
            lines.push(Line::from("当前线程暂时无法提供完整上下文用量。".red()));
        }
        ContextUsageCompleteness::Complete => {}
    }

    append_data_freshness(&mut lines, success, /*window_known*/ false);
    append_detail_section(
        &mut lines,
        "MCP 工具 · /mcp",
        &success.snapshot.mcp_tool_details,
        width,
    );
    append_detail_section(
        &mut lines,
        "指令文件",
        &success.snapshot.instruction_details,
        width,
    );
    append_detail_section(
        &mut lines,
        "Skills · /skills",
        &success.snapshot.skill_details,
        width,
    );
    lines
}

fn render_grid_lines(allocation: &GridAllocation) -> Vec<Line<'static>> {
    let glyphs = allocation
        .slots
        .iter()
        .flat_map(|slot| {
            std::iter::repeat_n(slot.kind, slot.slots).map(move |kind| (kind, slot.grid_glyph))
        })
        .collect::<Vec<_>>();

    glyphs
        .chunks(allocation.grid_columns)
        .map(|row| {
            let spans = row
                .iter()
                .map(|(kind, glyph)| styled_glyph(*kind, glyph))
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

fn render_legend_lines(
    success: &ContextUsageSuccessData,
    allocation: &GridAllocation,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(
        summary_line(success).unwrap_or_else(|| Line::from(success.snapshot.model.clone().bold())),
    );
    lines.push(Line::from("各分类估算用量".bold()));

    let window = success.snapshot.model_context_window.unwrap_or_default();
    for slot in &allocation.slots {
        lines.push(category_line(slot, window));
    }

    if allocation.normalized_overestimate {
        lines.push(Line::from(""));
        lines.push(Line::from(
            "注：分类估算总量高于最近 API 实际值，网格已按实际已用区域归一化。".dim(),
        ));
    }

    match success.snapshot.completeness {
        ContextUsageCompleteness::Partial => {
            lines.push(Line::from(""));
            lines.push(Line::from("当前快照不完整，仅展示可靠统计的部分。".dim()));
        }
        ContextUsageCompleteness::Unavailable => {
            lines.push(Line::from(""));
            lines.push(Line::from("当前线程暂时无法提供完整上下文用量。".red()));
        }
        ContextUsageCompleteness::Complete => {}
    }

    lines
}

fn summary_line(success: &ContextUsageSuccessData) -> Option<Line<'static>> {
    let window = i64::try_from(success.snapshot.model_context_window?).ok()?;
    let actual_total = success
        .actual_usage
        .as_ref()
        .map(|usage| usage.usage.total_tokens)
        .unwrap_or_else(|| {
            i64::try_from(success.snapshot.estimated_total_tokens).unwrap_or(i64::MAX)
        });
    let used = actual_total.clamp(0, window.max(0));
    let remaining = (window - used).max(0);
    let used_percent = percent_tenths(used as u64, window as u64);
    let remaining_percent = percent_tenths(remaining as u64, window as u64);
    let source_label = match success.actual_source {
        ContextUsageActualSource::CurrentRequest => "实际",
        ContextUsageActualSource::PreviousCompletedRequest => "实际（上一请求）",
        ContextUsageActualSource::LocalEstimate => "估算",
    };

    Some(Line::from(vec![
        success.snapshot.model.clone().bold(),
        " · ".into(),
        format!(
            "{}/{} Token（{source_label}已用 {}%，剩余 {}%）",
            format_tokens_compact(used),
            format_tokens_compact(window),
            format_percent_tenths(used_percent),
            format_percent_tenths(remaining_percent)
        )
        .into(),
    ]))
}

fn category_line(slot: &AllocatedSlot, window: u64) -> Line<'static> {
    let percent = percent_tenths(slot.estimated_tokens, window);
    Line::from(vec![
        styled_glyph(slot.kind, slot.legend_glyph),
        " ".into(),
        slot.label.clone().into(),
        "：".into(),
        format_tokens_compact(i64::try_from(slot.estimated_tokens).unwrap_or(i64::MAX)).into(),
        " Token（".dim(),
        format!("{}%", format_percent_tenths(percent)).dim(),
        "）".dim(),
    ])
}

fn category_line_without_percent(category: ContextUsageCategory) -> Line<'static> {
    let kind = LegendKind::from_category(category.kind);
    Line::from(vec![
        styled_glyph(kind, kind.legend_glyph()),
        " ".into(),
        kind.label().into(),
        "：".into(),
        format_tokens_compact(i64::try_from(category.estimated_tokens).unwrap_or(i64::MAX)).into(),
        " Token".dim(),
    ])
}

fn append_data_freshness(
    lines: &mut Vec<Line<'static>>,
    success: &ContextUsageSuccessData,
    window_known: bool,
) {
    let freshness = match success.actual_source {
        ContextUsageActualSource::CurrentRequest => {
            "数据说明：顶部实际值与分类明细来自同一份当前快照。"
        }
        ContextUsageActualSource::PreviousCompletedRequest => {
            if let Some(snapshot_id) = &success.last_completed_snapshot_id {
                if Some(snapshot_id)
                    == success
                        .actual_usage
                        .as_ref()
                        .and_then(|usage| usage.snapshot_id.as_ref())
                {
                    "数据说明：顶部实际值来自最近一次已完成请求，分类明细来自本次 /context 快照。"
                } else {
                    "数据说明：顶部实际值来自上一份已完成请求，分类明细来自本次 /context 快照。"
                }
            } else {
                "数据说明：顶部实际值来自上一份已完成请求，分类明细来自本次 /context 快照。"
            }
        }
        ContextUsageActualSource::LocalEstimate => {
            "数据说明：当前线程尚无 API 实际值，顶部与分类均为本地估算。"
        }
    };

    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(freshness.dim()));
    if !window_known {
        lines.push(Line::from(
            "窗口未知时不显示百分比、网格、预留区和剩余空间。".dim(),
        ));
    }
}

fn append_detail_section(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    details: &[ContextUsageDetail],
    width: usize,
) {
    if details.is_empty() {
        return;
    }

    lines.push(Line::from(""));
    lines.push(Line::from(title.to_string().bold()));
    let wrap_width = width.max(1);
    for detail in details {
        let label = detail.path.as_deref().unwrap_or(&detail.label);
        let mut body = format!(
            "{label}：{} Token",
            format_tokens_compact(i64::try_from(detail.estimated_tokens).unwrap_or(i64::MAX))
        );
        match detail.load_state {
            ContextUsageDetailLoadState::Loaded => {}
            ContextUsageDetailLoadState::Available => body.push_str(" （可用）"),
            ContextUsageDetailLoadState::Deferred => body.push_str(" （延迟加载）"),
        }
        let wrapped = wrap(
            &body,
            textwrap::Options::new(wrap_width)
                .initial_indent("└ ")
                .subsequent_indent("  "),
        );
        lines.extend(
            wrapped
                .into_iter()
                .map(|line| Line::from(line.into_owned())),
        );
    }
}

fn can_render_side_by_side(
    width: usize,
    grid_lines: &[Line<'static>],
    legend_lines: &[Line<'static>],
) -> bool {
    let grid_width = grid_lines.iter().map(line_display_width).max().unwrap_or(0);
    let legend_width = legend_lines
        .iter()
        .map(line_display_width)
        .max()
        .unwrap_or(0);
    grid_width + GRID_LEGEND_GAP + legend_width <= width && legend_width >= MIN_LEGEND_WIDTH
}

fn render_side_by_side(
    grid_lines: Vec<Line<'static>>,
    legend_lines: Vec<Line<'static>>,
    width: usize,
) -> Vec<Line<'static>> {
    let grid_width = grid_lines.iter().map(line_display_width).max().unwrap_or(0);
    let available_right = width.saturating_sub(grid_width + GRID_LEGEND_GAP).max(1);
    let row_count = grid_lines.len().max(legend_lines.len());
    let continuation_prefix = " ".repeat(grid_width + GRID_LEGEND_GAP);
    let mut lines = Vec::new();

    for index in 0..row_count {
        let mut left_spans = Vec::new();
        if let Some(line) = grid_lines.get(index) {
            left_spans.extend(line.spans.clone());
            let padding = grid_width.saturating_sub(line_display_width(line)) + GRID_LEGEND_GAP;
            if padding > 0 {
                left_spans.push(" ".repeat(padding).into());
            }
        } else {
            left_spans.push(continuation_prefix.clone().into());
        }

        if let Some(line) = legend_lines.get(index) {
            let wrapped = adaptive_wrap_lines(
                Text::from(vec![line.clone()]),
                RtOptions::new(available_right),
            );
            let mut wrapped_iter = wrapped.into_iter();
            if let Some(first) = wrapped_iter.next() {
                let mut first_line = left_spans.clone();
                first_line.extend(first.spans);
                lines.push(Line::from(first_line));
                for continuation in wrapped_iter {
                    let mut continuation_spans = vec![continuation_prefix.clone().into()];
                    continuation_spans.extend(continuation.spans);
                    lines.push(Line::from(continuation_spans));
                }
            } else {
                lines.push(Line::from(left_spans));
            }
        } else {
            lines.push(Line::from(left_spans));
        }
    }

    lines
}

fn line_display_width(line: &Line<'static>) -> usize {
    line.spans
        .iter()
        .map(|span| display_width(span.content.as_ref()))
        .sum()
}

fn percent_tenths(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return 0;
    }
    ((u128::from(numerator) * 1000) + (u128::from(denominator) / 2))
        .checked_div(u128::from(denominator))
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or(0)
}

fn format_percent_tenths(tenths: u64) -> String {
    format!("{}.{:01}", tenths / 10, tenths % 10)
}

#[cfg(test)]
#[path = "context_usage_tests.rs"]
mod tests;
