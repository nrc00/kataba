//! Highlight extraction: compile a `highlights.scm` query against a language,
//! run it over a tree, and map capture names to ratatui styles.
//!
//! This is demo-only glue (a throwaway viewer): the real query/highlight layer
//! belongs in `kataba-treesitter`/`kataba-syntax` (milestones M1/M2) and will
//! supersede this module.

use std::path::Path;

use ratatui::style::{Color, Modifier, Style};
use tree_sitter::{Language, Query, QueryCursor, StreamingIterator, Tree};

/// A styled byte range in the document.
#[derive(Clone, Copy, Debug)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

/// A compiled highlight query plus a per-capture style table.
pub struct Highlighter {
    query: Query,
    styles: Vec<Option<Style>>,
}

impl Highlighter {
    /// Compiles `query_path` (a highlights.scm) against `language`.
    pub fn load(
        language: &Language,
        query_path: &Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let source = std::fs::read_to_string(query_path)?;
        let query = Query::new(language, &source)?;
        let styles = query
            .capture_names()
            .iter()
            .map(|name| capture_style(name))
            .collect();
        Ok(Self { query, styles })
    }

    /// Collects every styled capture in the tree, in paint order.
    ///
    /// Earlier starts paint first; for equal starts the larger (outer) span
    /// paints first so nested spans, e.g. an escape inside a string, win.
    /// Exact ties are broken by pattern order: later patterns in the query
    /// file take precedence (the Helix convention that lets
    /// `(pair key: (_) @variable.other.member)` override `(string) @string`
    /// for object keys).
    pub fn spans(&self, tree: &Tree, text: &str) -> Vec<HighlightSpan> {
        let mut pending: Vec<(usize, usize, usize, Style)> = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, tree.root_node(), text.as_bytes());
        while let Some(query_match) = matches.next() {
            let pattern = query_match.pattern_index;
            for capture in query_match.captures {
                let Some(style) = self.styles.get(capture.index as usize).copied().flatten() else {
                    continue;
                };
                let node = capture.node;
                if node.start_byte() < node.end_byte() {
                    pending.push((node.start_byte(), node.end_byte(), pattern, style));
                }
            }
        }
        pending.sort_by_key(|&(start, end, pattern, _)| (start, std::cmp::Reverse(end), pattern));
        pending
            .into_iter()
            .map(|(start, end, _, style)| HighlightSpan { start, end, style })
            .collect()
    }
}

/// Maps a tree-sitter capture name to a style.
///
/// Understands both the modern convention (`@constant.numeric`) and Helix's
/// older names (`@variable.other.member`), because the demo reuses Helix's
/// query files. Non-visual captures (`@spell`, injections) get `None`.
pub fn capture_style(name: &str) -> Option<Style> {
    if name.starts_with("spell") || name.contains("injection") || name.contains("content") {
        return None;
    }
    let plain = |color: Color| Style::default().fg(color);
    let style = if name.contains("escape") {
        plain(Color::LightGreen)
    } else if name.contains("comment") {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC)
    } else if name.contains("string") {
        plain(Color::Green)
    } else if name.contains("number") || name.contains("float") {
        plain(Color::Cyan)
    } else if name.contains("boolean") || name.contains("constant") || name.contains("character") {
        plain(Color::Magenta)
    } else if name.contains("function") {
        plain(Color::Blue)
    } else if name.contains("constructor")
        || name.contains("type")
        || name.contains("class")
        || name.contains("struct")
        || name.contains("enum")
    {
        plain(Color::Yellow)
    } else if name.contains("keyword")
        || name.contains("storage")
        || name.contains("conditional")
        || name.contains("repeat")
    {
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD)
    } else if name.contains("operator") {
        plain(Color::LightRed)
    } else if name.contains("punctuation") || name.contains("delimiter") || name.contains("bracket")
    {
        plain(Color::Gray)
    } else if name.contains("label") {
        plain(Color::LightCyan)
    } else if name.contains("attribute") || name.contains("decorator") {
        plain(Color::LightYellow)
    } else if name.contains("tag") {
        plain(Color::Blue)
    } else if name.contains("property") || name.contains("member") || name.contains("field") {
        plain(Color::LightBlue)
    } else if name.contains("parameter") {
        plain(Color::LightCyan)
    } else if name.contains("builtin") {
        plain(Color::LightMagenta)
    } else if name.contains("namespace") || name.contains("module") {
        plain(Color::Cyan)
    } else {
        return None;
    };
    Some(style)
}

#[cfg(test)]
mod tests {
    use super::capture_style;

    #[test]
    fn known_captures_get_styles() {
        assert!(capture_style("string").is_some());
        assert!(capture_style("constant.numeric").is_some());
        assert!(capture_style("variable.other.member").is_some());
        assert!(capture_style("punctuation.bracket").is_some());
        assert!(capture_style("comment").is_some());
    }

    #[test]
    fn non_visual_captures_are_ignored() {
        assert!(capture_style("spell").is_none());
        assert!(capture_style("injection.content").is_none());
        assert!(capture_style("variable").is_none());
    }
}
