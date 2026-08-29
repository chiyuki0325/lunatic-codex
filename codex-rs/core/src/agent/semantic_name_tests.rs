use pretty_assertions::assert_eq;

use super::normalize_semantic_name;
use super::truncate_chars;

#[test]
fn normalizes_single_line_semantic_names() {
    assert_eq!(
        normalize_semantic_name("  “Codex 多 Agent 调研员”  "),
        Some("Codex 多 Agent 调研员".to_string())
    );
    assert_eq!(
        normalize_semantic_name("后端实现调研员"),
        Some("后端实现调研员".to_string())
    );
}

#[test]
fn rejects_empty_and_multiline_semantic_names() {
    assert_eq!(normalize_semantic_name("  "), None);
    assert_eq!(normalize_semantic_name("后端调研员\n额外解释"), None);
}

#[test]
fn truncates_on_unicode_scalar_boundaries() {
    assert_eq!(truncate_chars("甲乙丙丁", 3), "甲乙丙");
    assert_eq!(
        normalize_semantic_name(&"调".repeat(40)),
        Some("调".repeat(32))
    );
}
