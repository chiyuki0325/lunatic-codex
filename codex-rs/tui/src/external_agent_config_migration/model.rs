use codex_app_server_protocol::ExternalAgentConfigMigrationItem;
use codex_app_server_protocol::ExternalAgentConfigMigrationItemType;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(crate) struct ExternalAgentConfigMigrationGroupModel {
    pub(crate) label: String,
    pub(crate) description: &'static str,
    pub(crate) item_indices: Vec<usize>,
}

pub(crate) fn external_agent_config_migration_groups(
    items: &[ExternalAgentConfigMigrationItem],
) -> Vec<ExternalAgentConfigMigrationGroupModel> {
    let tools_and_setup = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            (item.cwd.is_none() && item.item_type != ExternalAgentConfigMigrationItemType::Sessions)
                .then_some(idx)
        })
        .collect::<Vec<_>>();
    let projects = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            (item.cwd.is_some() && item.item_type != ExternalAgentConfigMigrationItemType::Sessions)
                .then_some(idx)
        })
        .collect::<Vec<_>>();
    let chat_sessions = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            (item.item_type == ExternalAgentConfigMigrationItemType::Sessions).then_some(idx)
        })
        .collect::<Vec<_>>();

    let mut groups = Vec::new();
    if !tools_and_setup.is_empty() {
        groups.push(ExternalAgentConfigMigrationGroupModel {
            label: "工具与设置".to_string(),
            description: "设置、说明、集成、agent、命令和 Skill",
            item_indices: tools_and_setup,
        });
    }
    if !projects.is_empty() {
        let project_count = projects
            .iter()
            .filter_map(|idx| items[*idx].cwd.as_deref())
            .collect::<BTreeSet<_>>()
            .len();
        groups.push(ExternalAgentConfigMigrationGroupModel {
            label: if project_count == 1 {
                "当前项目".to_string()
            } else {
                format!("项目（{project_count}）")
            },
            description: "将 Codex 文件添加到现有项目文件旁",
            item_indices: projects,
        });
    }
    if !chat_sessions.is_empty() {
        let session_count = chat_sessions
            .iter()
            .filter_map(|idx| items[*idx].details.as_ref())
            .map(|details| details.sessions.len())
            .sum::<usize>();
        groups.push(ExternalAgentConfigMigrationGroupModel {
            label: format!("聊天会话（{session_count}）"),
            description: "最近 30 天的聊天记录",
            item_indices: chat_sessions,
        });
    }
    groups
}

pub(crate) fn external_agent_config_migration_item_label(
    item: &ExternalAgentConfigMigrationItem,
) -> &'static str {
    match item.item_type {
        ExternalAgentConfigMigrationItemType::AgentsMd => "说明",
        ExternalAgentConfigMigrationItemType::Config => "设置",
        ExternalAgentConfigMigrationItemType::Skills => "Skill",
        ExternalAgentConfigMigrationItemType::Plugins => "插件",
        ExternalAgentConfigMigrationItemType::McpServerConfig => "MCP 服务器",
        ExternalAgentConfigMigrationItemType::Subagents => "agent",
        ExternalAgentConfigMigrationItemType::Hooks => "Hook",
        ExternalAgentConfigMigrationItemType::Commands => "斜杠命令",
        ExternalAgentConfigMigrationItemType::Memory => "记忆",
        ExternalAgentConfigMigrationItemType::Sessions => "最近的聊天会话",
    }
}

pub(crate) fn external_agent_config_migration_type_label(
    item_type: ExternalAgentConfigMigrationItemType,
) -> &'static str {
    match item_type {
        ExternalAgentConfigMigrationItemType::AgentsMd => "说明",
        ExternalAgentConfigMigrationItemType::Config => "设置",
        ExternalAgentConfigMigrationItemType::Skills => "Skill",
        ExternalAgentConfigMigrationItemType::Plugins => "插件",
        ExternalAgentConfigMigrationItemType::McpServerConfig => "MCP 服务器",
        ExternalAgentConfigMigrationItemType::Subagents => "agent",
        ExternalAgentConfigMigrationItemType::Hooks => "Hook",
        ExternalAgentConfigMigrationItemType::Commands => "斜杠命令",
        ExternalAgentConfigMigrationItemType::Memory => "记忆",
        ExternalAgentConfigMigrationItemType::Sessions => "聊天会话",
    }
}

/// Summarizes the concrete objects represented by selected migration items.
///
/// Most detected item types carry the objects they will import in `details`; types without
/// details represent one importable file or source directory per migration item.
pub(crate) fn external_agent_config_migration_count_summary<'a>(
    items: impl IntoIterator<Item = &'a ExternalAgentConfigMigrationItem>,
) -> String {
    let mut counts = Vec::<(ExternalAgentConfigMigrationItemType, usize)>::new();
    for item in items {
        let count = external_agent_config_migration_item_count(item);
        if let Some((_, type_count)) = counts
            .iter_mut()
            .find(|(item_type, _)| *item_type == item.item_type)
        {
            *type_count += count;
        } else {
            counts.push((item.item_type, count));
        }
    }

    counts
        .into_iter()
        .map(|(item_type, count)| {
            format!(
                "{count} 个{}",
                external_agent_config_migration_type_label(item_type)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn external_agent_config_migration_item_count(
    item: &ExternalAgentConfigMigrationItem,
) -> usize {
    match item.item_type {
        ExternalAgentConfigMigrationItemType::Plugins => {
            item.details.as_ref().map_or(1, |details| {
                details
                    .plugins
                    .iter()
                    .map(|plugin_group| plugin_group.plugin_names.len())
                    .sum()
            })
        }
        ExternalAgentConfigMigrationItemType::McpServerConfig => item
            .details
            .as_ref()
            .map_or(1, |details| details.mcp_servers.len()),
        ExternalAgentConfigMigrationItemType::Subagents => item
            .details
            .as_ref()
            .map_or(1, |details| details.subagents.len()),
        ExternalAgentConfigMigrationItemType::Hooks => item
            .details
            .as_ref()
            .map_or(1, |details| details.hooks.len()),
        ExternalAgentConfigMigrationItemType::Commands => item
            .details
            .as_ref()
            .map_or(1, |details| details.commands.len()),
        ExternalAgentConfigMigrationItemType::Memory => item
            .details
            .as_ref()
            .map_or(0, |details| details.memory.len()),
        ExternalAgentConfigMigrationItemType::Sessions => item
            .details
            .as_ref()
            .map_or(1, |details| details.sessions.len()),
        ExternalAgentConfigMigrationItemType::Skills => item
            .details
            .as_ref()
            .map_or(1, |details| details.skills.len()),
        ExternalAgentConfigMigrationItemType::AgentsMd
        | ExternalAgentConfigMigrationItemType::Config => 1,
    }
}

pub(crate) fn external_agent_config_migration_item_detail(
    item: &ExternalAgentConfigMigrationItem,
) -> Option<String> {
    let details = item.details.as_ref()?;
    match item.item_type {
        ExternalAgentConfigMigrationItemType::Plugins => None,
        ExternalAgentConfigMigrationItemType::Skills => Some(format_counted_details(
            "Skill",
            details.skills.len(),
            details.skills.iter().map(|skill| skill.name.as_str()),
        )),
        ExternalAgentConfigMigrationItemType::McpServerConfig => Some(format_counted_details(
            "MCP 服务器",
            details.mcp_servers.len(),
            details
                .mcp_servers
                .iter()
                .map(|server| server.name.as_str()),
        )),
        ExternalAgentConfigMigrationItemType::Subagents => Some(format_counted_details(
            "agent",
            details.subagents.len(),
            details.subagents.iter().map(|agent| agent.name.as_str()),
        )),
        ExternalAgentConfigMigrationItemType::Hooks => Some(format_counted_details(
            "Hook",
            details.hooks.len(),
            details.hooks.iter().map(|hook| hook.name.as_str()),
        )),
        ExternalAgentConfigMigrationItemType::Commands => Some(format_counted_details(
            "斜杠命令",
            details.commands.len(),
            details.commands.iter().map(|command| command.name.as_str()),
        )),
        ExternalAgentConfigMigrationItemType::Memory => Some(format_counted_details(
            "记忆",
            details.memory.len(),
            details.memory.iter().map(String::as_str),
        )),
        ExternalAgentConfigMigrationItemType::Sessions => Some(format_counted_details(
            "聊天会话",
            details.sessions.len(),
            details
                .sessions
                .iter()
                .filter_map(|session| session.title.as_deref()),
        )),
        ExternalAgentConfigMigrationItemType::AgentsMd
        | ExternalAgentConfigMigrationItemType::Config => None,
    }
}

fn format_counted_details<'a>(
    noun: &str,
    count: usize,
    names: impl Iterator<Item = &'a str>,
) -> String {
    match names.take(4).collect::<Vec<_>>() {
        names if names.is_empty() => format!("{count} 个{noun}"),
        names => format!("{count} 个{noun}：{}", names.join("、")),
    }
}
