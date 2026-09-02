use super::ContextUsageSuccessData;
use codex_app_server_protocol::ContextUsageActualUsage;
use codex_app_server_protocol::ContextUsageCategory;
use codex_app_server_protocol::ContextUsageCategoryKind;
use std::cmp::Reverse;

pub(super) const DEFAULT_GRID_COLUMNS: usize = 10;
pub(super) const DEFAULT_GRID_ROWS: usize = 10;
pub(super) const TOTAL_GRID_SLOTS: usize = DEFAULT_GRID_COLUMNS * DEFAULT_GRID_ROWS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GridAllocation {
    pub(super) slots: Vec<AllocatedSlot>,
    pub(super) grid_columns: usize,
    pub(super) grid_rows: usize,
    pub(super) used_slots: usize,
    pub(super) reserve_slots: usize,
    pub(super) free_slots: usize,
    pub(super) normalized_overestimate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AllocatedSlot {
    pub(super) kind: LegendKind,
    pub(super) label: String,
    pub(super) legend_glyph: &'static str,
    pub(super) grid_glyph: &'static str,
    pub(super) slots: usize,
    pub(super) estimated_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum LegendKind {
    SystemPrompt,
    BuiltInTools,
    McpTools,
    Instructions,
    Skills,
    Messages,
    Other,
    Unattributed,
    Reserve,
    Free,
}

impl LegendKind {
    pub(super) fn fixed_order(self) -> usize {
        match self {
            Self::SystemPrompt => 0,
            Self::BuiltInTools => 1,
            Self::McpTools => 2,
            Self::Instructions => 3,
            Self::Skills => 4,
            Self::Messages => 5,
            Self::Other => 6,
            Self::Unattributed => 7,
            Self::Reserve => 8,
            Self::Free => 9,
        }
    }

    pub(super) fn from_category(kind: ContextUsageCategoryKind) -> Self {
        match kind {
            ContextUsageCategoryKind::SystemPrompt => Self::SystemPrompt,
            ContextUsageCategoryKind::BuiltInTools => Self::BuiltInTools,
            ContextUsageCategoryKind::McpTools => Self::McpTools,
            ContextUsageCategoryKind::Instructions => Self::Instructions,
            ContextUsageCategoryKind::Skills => Self::Skills,
            ContextUsageCategoryKind::Messages => Self::Messages,
            ContextUsageCategoryKind::Other => Self::Other,
            ContextUsageCategoryKind::Unattributed => Self::Unattributed,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::SystemPrompt => "系统提示词",
            Self::BuiltInTools => "内置工具",
            Self::McpTools => "MCP 工具",
            Self::Instructions => "指令",
            Self::Skills => "Skills",
            Self::Messages => "消息",
            Self::Other => "其他",
            Self::Unattributed => "未归因",
            Self::Reserve => "自动压缩预留区",
            Self::Free => "剩余空间",
        }
    }

    pub(super) fn legend_glyph(self) -> &'static str {
        match self {
            Self::Messages => "■",
            Self::Reserve => "⛝",
            Self::Free => "⛶",
            _ => "⛁",
        }
    }

    pub(super) fn partial_glyph(self) -> &'static str {
        match self {
            Self::Messages => "■",
            Self::Reserve => "⛝",
            Self::Free => "⛶",
            _ => "⛀",
        }
    }
}

pub(super) fn allocate_grid(success: &ContextUsageSuccessData, width: usize) -> GridAllocation {
    let snapshot = &success.snapshot;
    let window = snapshot.model_context_window.unwrap_or_default();
    let actual_total = actual_total(&success.actual_usage);
    let authoritative_used = actual_total
        .unwrap_or(snapshot.estimated_total_tokens)
        .min(window);
    let estimated_sum: u64 = snapshot
        .categories
        .iter()
        .map(|category| category.estimated_tokens)
        .sum();

    let reserve_tokens = snapshot
        .reserved_tokens
        .or_else(|| {
            snapshot
                .auto_compact_threshold
                .map(|threshold| window.saturating_sub(threshold))
        })
        .unwrap_or(0);
    let clipped_reserve = reserve_tokens.min(window.saturating_sub(authoritative_used));
    let free_tokens = window
        .saturating_sub(authoritative_used)
        .saturating_sub(clipped_reserve);

    let mut slots = ordered_categories(&snapshot.categories)
        .into_iter()
        .filter(|category| category.estimated_tokens > 0)
        .map(category_slot)
        .collect::<Vec<_>>();

    if let Some(actual_total) = actual_total
        && estimated_sum < actual_total
    {
        let unattributed = actual_total - estimated_sum;
        if unattributed > 0 {
            slots.push(AllocatedSlot {
                kind: LegendKind::Unattributed,
                label: LegendKind::Unattributed.label().to_string(),
                legend_glyph: LegendKind::Unattributed.legend_glyph(),
                grid_glyph: LegendKind::Unattributed.legend_glyph(),
                slots: 0,
                estimated_tokens: unattributed,
            });
        }
    }

    let estimated_used_tokens: u64 = slots.iter().map(|slot| slot.estimated_tokens).sum();
    let normalized_overestimate =
        actual_total.is_some() && estimated_used_tokens > authoritative_used;
    let used_slots_budget = percent_slots(authoritative_used, window);
    assign_slots(&mut slots, used_slots_budget, estimated_used_tokens);

    let mut reserve_slot = reserved_slot(clipped_reserve);
    let mut free_slot = free_slot(free_tokens);

    let used_slots = slots.iter().map(|slot| slot.slots).sum::<usize>();
    let remaining_slots = TOTAL_GRID_SLOTS.saturating_sub(used_slots);
    assign_slots_pair(
        &mut reserve_slot,
        &mut free_slot,
        remaining_slots,
        clipped_reserve,
        free_tokens,
    );

    if reserve_slot.slots > 0 {
        slots.push(reserve_slot);
    }
    if free_slot.slots > 0 {
        slots.push(free_slot);
    }

    let used_slots = slots
        .iter()
        .filter(|slot| slot.kind != LegendKind::Reserve && slot.kind != LegendKind::Free)
        .map(|slot| slot.slots)
        .sum();
    let reserve_slots = slots
        .iter()
        .find(|slot| slot.kind == LegendKind::Reserve)
        .map_or(0, |slot| slot.slots);
    let free_slots = slots
        .iter()
        .find(|slot| slot.kind == LegendKind::Free)
        .map_or(0, |slot| slot.slots);

    let grid_columns = choose_grid_columns(width);
    let grid_rows = TOTAL_GRID_SLOTS.div_ceil(grid_columns);

    GridAllocation {
        slots,
        grid_columns,
        grid_rows,
        used_slots,
        reserve_slots,
        free_slots,
        normalized_overestimate,
    }
}

pub(super) fn ordered_categories(categories: &[ContextUsageCategory]) -> Vec<ContextUsageCategory> {
    let mut ordered = categories.to_vec();
    ordered.sort_by_key(|category| LegendKind::from_category(category.kind).fixed_order());
    ordered
}

fn actual_total(actual_usage: &Option<ContextUsageActualUsage>) -> Option<u64> {
    actual_usage
        .as_ref()
        .and_then(|usage| u64::try_from(usage.usage.total_tokens).ok())
}

fn category_slot(category: ContextUsageCategory) -> AllocatedSlot {
    let kind = LegendKind::from_category(category.kind);
    AllocatedSlot {
        kind,
        label: kind.label().to_string(),
        legend_glyph: kind.legend_glyph(),
        grid_glyph: kind.legend_glyph(),
        slots: 0,
        estimated_tokens: category.estimated_tokens,
    }
}

fn reserved_slot(estimated_tokens: u64) -> AllocatedSlot {
    AllocatedSlot {
        kind: LegendKind::Reserve,
        label: LegendKind::Reserve.label().to_string(),
        legend_glyph: LegendKind::Reserve.legend_glyph(),
        grid_glyph: LegendKind::Reserve.legend_glyph(),
        slots: 0,
        estimated_tokens,
    }
}

fn free_slot(estimated_tokens: u64) -> AllocatedSlot {
    AllocatedSlot {
        kind: LegendKind::Free,
        label: LegendKind::Free.label().to_string(),
        legend_glyph: LegendKind::Free.legend_glyph(),
        grid_glyph: LegendKind::Free.legend_glyph(),
        slots: 0,
        estimated_tokens,
    }
}

fn assign_slots(slots: &mut [AllocatedSlot], total_slots: usize, total_tokens: u64) {
    if slots.is_empty() || total_slots == 0 || total_tokens == 0 {
        return;
    }

    let total_slots_u128 = total_slots as u128;
    let total_tokens_u128 = u128::from(total_tokens);
    let mut exacts = slots
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let numerator = u128::from(slot.estimated_tokens) * total_slots_u128;
            let floor = usize::try_from(numerator / total_tokens_u128).unwrap_or(total_slots);
            let remainder = numerator % total_tokens_u128;
            let positive = slot.estimated_tokens > 0;
            (index, floor, remainder, positive)
        })
        .collect::<Vec<_>>();

    let mut assigned = exacts.iter().map(|(_, floor, _, _)| *floor).sum::<usize>();
    let visible_candidates = exacts
        .iter()
        .filter(|(_, floor, _, positive)| *positive && *floor == 0)
        .map(|(index, _, remainder, _)| (*index, *remainder))
        .collect::<Vec<_>>();
    let available_for_visibility = total_slots.saturating_sub(assigned);
    let visibility_count = visible_candidates.len().min(available_for_visibility);
    let mut ranked_visible = visible_candidates;
    ranked_visible
        .sort_by_key(|(index, remainder)| (Reverse(*remainder), slots[*index].kind.fixed_order()));
    let mut visible_indices = ranked_visible
        .into_iter()
        .take(visibility_count)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    visible_indices.sort_unstable();

    for index in visible_indices {
        exacts[index].1 += 1;
        exacts[index].2 = 0;
        assigned += 1;
    }

    if assigned < total_slots {
        let remaining = total_slots - assigned;
        let mut ranked = exacts
            .iter()
            .enumerate()
            .map(|(position, (index, floor, remainder, _))| {
                let boosted = if *floor > 0 { *remainder } else { 0 };
                (position, *index, *floor, boosted)
            })
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(_, index, floor, remainder)| {
            (
                Reverse(*remainder),
                Reverse(*floor),
                slots[*index].kind.fixed_order(),
            )
        });
        for (position, _, _, _) in ranked.into_iter().take(remaining) {
            exacts[position].1 += 1;
        }
    } else if assigned > total_slots {
        let overflow = assigned - total_slots;
        let mut ranked = exacts
            .iter()
            .enumerate()
            .filter(|(_, (_, floor, _, _))| *floor > 0)
            .map(|(position, (index, floor, remainder, _))| (position, *index, *floor, *remainder))
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(_, index, floor, remainder)| {
            (
                *remainder,
                *floor,
                Reverse(slots[*index].kind.fixed_order()),
            )
        });
        for (position, _, _, _) in ranked.into_iter().take(overflow) {
            exacts[position].1 = exacts[position].1.saturating_sub(1);
        }
    }

    let positive_count = exacts
        .iter()
        .filter(|(_, _, _, positive)| *positive)
        .count();
    if total_slots >= positive_count {
        let invisible_positions = exacts
            .iter()
            .enumerate()
            .filter(|(_, (_, assigned_slots, _, positive))| *positive && *assigned_slots == 0)
            .map(|(position, _)| position)
            .collect::<Vec<_>>();
        for recipient_position in invisible_positions {
            let donor_position = exacts
                .iter()
                .enumerate()
                .filter(|(position, (_, assigned_slots, _, _))| {
                    *position != recipient_position && *assigned_slots > 1
                })
                .max_by_key(|(_, (index, assigned_slots, _, _))| {
                    (*assigned_slots, Reverse(slots[*index].kind.fixed_order()))
                })
                .map(|(position, _)| position);
            if let Some(donor_position) = donor_position {
                exacts[donor_position].1 -= 1;
                exacts[recipient_position].1 = 1;
            }
        }
    }

    for (index, assigned_slots, remainder, _) in exacts {
        let kind = slots[index].kind;
        let exact_numerator = u128::from(slots[index].estimated_tokens) * total_slots_u128;
        let partial = !exact_numerator.is_multiple_of(total_tokens_u128);
        slots[index].slots = assigned_slots;
        slots[index].grid_glyph = if partial && assigned_slots > 0 {
            kind.partial_glyph()
        } else {
            kind.legend_glyph()
        };
        if remainder == 0 && assigned_slots == 0 {
            slots[index].grid_glyph = kind.legend_glyph();
        }
    }
}

fn assign_slots_pair(
    reserve_slot: &mut AllocatedSlot,
    free_slot: &mut AllocatedSlot,
    total_slots: usize,
    reserve_tokens: u64,
    free_tokens: u64,
) {
    let total = reserve_tokens + free_tokens;
    if total_slots == 0 || total == 0 {
        return;
    }

    let mut pair = vec![reserve_slot.clone(), free_slot.clone()];
    assign_slots(&mut pair, total_slots, total);
    *reserve_slot = pair.remove(0);
    *free_slot = pair.remove(0);
}

fn percent_slots(numerator: u64, denominator: u64) -> usize {
    if denominator == 0 {
        return 0;
    }

    usize::try_from((u128::from(numerator) * TOTAL_GRID_SLOTS as u128) / u128::from(denominator))
        .unwrap_or(TOTAL_GRID_SLOTS)
        .min(TOTAL_GRID_SLOTS)
}

fn choose_grid_columns(width: usize) -> usize {
    let columns = if width >= 80 {
        DEFAULT_GRID_COLUMNS
    } else if width >= 60 {
        5
    } else if width >= 40 {
        4
    } else if width >= 20 {
        2
    } else {
        1
    };
    TOTAL_GRID_SLOTS / (TOTAL_GRID_SLOTS / columns)
}
