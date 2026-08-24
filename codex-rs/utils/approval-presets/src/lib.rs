use codex_protocol::models::ActivePermissionProfile;
use codex_protocol::models::BUILT_IN_PERMISSION_PROFILE_DANGER_FULL_ACCESS;
use codex_protocol::models::BUILT_IN_PERMISSION_PROFILE_READ_ONLY;
use codex_protocol::models::BUILT_IN_PERMISSION_PROFILE_WORKSPACE;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::AskForApproval;

/// A simple preset pairing an approval policy with a permission profile.
#[derive(Debug, Clone)]
pub struct ApprovalPreset {
    /// Stable identifier for the preset.
    pub id: &'static str,
    /// Display label shown in UIs.
    pub label: &'static str,
    /// Short human description shown next to the label in UIs.
    pub description: &'static str,
    /// Approval policy to apply.
    pub approval: AskForApproval,
    /// Built-in permission profile selected by this preset.
    pub active_permission_profile: ActivePermissionProfile,
    /// Permission profile to apply.
    pub permission_profile: PermissionProfile,
}

/// Built-in list of approval presets that pair approval and permissions.
///
/// Keep this UI-agnostic so it can be reused by both TUI and MCP server.
pub fn builtin_approval_presets() -> Vec<ApprovalPreset> {
    vec![
        ApprovalPreset {
            id: "read-only",
            label: "只读",
            description: "Codex 可以读取当前工作区中的文件。编辑文件或访问互联网时需要审批。",
            approval: AskForApproval::OnRequest,
            active_permission_profile: ActivePermissionProfile::new(
                BUILT_IN_PERMISSION_PROFILE_READ_ONLY,
            ),
            permission_profile: PermissionProfile::read_only(),
        },
        ApprovalPreset {
            id: "auto",
            label: "默认",
            description: "Codex 可以读取和编辑当前工作区中的文件，并运行命令。访问互联网或编辑其他文件时需要审批。（与 Agent 模式相同）",
            approval: AskForApproval::OnRequest,
            active_permission_profile: ActivePermissionProfile::new(
                BUILT_IN_PERMISSION_PROFILE_WORKSPACE,
            ),
            permission_profile: PermissionProfile::workspace_write(),
        },
        ApprovalPreset {
            id: "full-access",
            label: "完全访问",
            description: "Codex 可以编辑此工作区外的文件并访问互联网，无需请求审批。使用时请谨慎。",
            approval: AskForApproval::Never,
            active_permission_profile: ActivePermissionProfile::new(
                BUILT_IN_PERMISSION_PROFILE_DANGER_FULL_ACCESS,
            ),
            permission_profile: PermissionProfile::Disabled,
        },
    ]
}

/// Return the concrete profile for one of the built-in active profile ids.
pub fn builtin_permission_profile_for_active_permission_profile(
    active_permission_profile: &ActivePermissionProfile,
) -> Option<PermissionProfile> {
    if active_permission_profile.extends.is_some() {
        return None;
    }

    match active_permission_profile.id.as_str() {
        BUILT_IN_PERMISSION_PROFILE_READ_ONLY => Some(PermissionProfile::read_only()),
        BUILT_IN_PERMISSION_PROFILE_WORKSPACE => Some(PermissionProfile::workspace_write()),
        BUILT_IN_PERMISSION_PROFILE_DANGER_FULL_ACCESS => Some(PermissionProfile::Disabled),
        _ => None,
    }
}
