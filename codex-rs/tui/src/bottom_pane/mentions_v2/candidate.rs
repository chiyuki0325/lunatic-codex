use std::path::PathBuf;

use ratatui::style::Style;
use ratatui::style::Styled;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

const TAG_WIDTH: usize = 6;

#[derive(Clone, Debug)]
pub(crate) enum Selection {
    File(PathBuf),
    Tool {
        insert_text: String,
        path: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MentionType {
    Plugin,
    Skill,
    File,
    Directory,
}

impl MentionType {
    pub(super) fn is_filesystem(self) -> bool {
        matches!(self, Self::File | Self::Directory)
    }

    pub(super) fn span(self, base_style: Style) -> Span<'static> {
        let style = match self {
            Self::Plugin => base_style.magenta(),
            Self::Skill => base_style.dim(),
            Self::File => base_style.cyan(),
            Self::Directory => base_style,
        };
        let label = self.label();
        let padding = TAG_WIDTH.saturating_sub(UnicodeWidthStr::width(label));
        format!("{label}{}", " ".repeat(padding)).set_style(style)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Plugin => "插件",
            Self::Skill => "Skill",
            Self::File => "文件",
            Self::Directory => "目录",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(super) display_name: String,
    pub(super) description: Option<String>,
    pub(super) search_terms: Vec<String>,
    pub(super) mention_type: MentionType,
    pub(super) selection: Selection,
}

#[derive(Clone, Debug)]
pub(super) struct SearchResult {
    pub(super) display_name: String,
    pub(super) description: Option<String>,
    pub(super) mention_type: MentionType,
    pub(super) selection: Selection,
    pub(super) match_indices: Option<Vec<usize>>,
    pub(super) score: i32,
}

impl Candidate {
    pub(super) fn to_result(&self, match_indices: Option<Vec<usize>>, score: i32) -> SearchResult {
        SearchResult {
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            mention_type: self.mention_type,
            selection: self.selection.clone(),
            match_indices,
            score,
        }
    }
}
