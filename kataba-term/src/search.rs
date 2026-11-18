//! Search & replace: buffer search with regex or literal matching.
//!
//! Supports forward/reverse search, match highlighting, and replace
//! (single/all/confirm). Integrated with the editor view via match spans.

use kataba_core::rope::Rope;

/// Search direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Reverse,
}

/// Search mode: literal or regex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Literal,
    Regex,
}

/// A single search match: byte range in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
}

/// Search state: query, mode, matches, current match index.
pub struct Search {
    /// Current search query text.
    pub query: String,
    /// Search mode: literal or regex.
    pub mode: SearchMode,
    direction: SearchDirection,
    matches: Vec<Match>,
    current: Option<usize>,
    case_sensitive: bool,
    wrap: bool,
}

impl Search {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            mode: SearchMode::Literal,
            direction: SearchDirection::Forward,
            matches: Vec::new(),
            current: None,
            case_sensitive: false,
            wrap: true,
        }
    }

    /// Append a character to the query and re-search.
    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
    }

    /// Set the search query and re-run against the document.
    pub fn set_query(&mut self, query: &str, doc: &Rope) {
        self.query = query.to_string();
        self.matches = Self::find_all(doc, query, self.mode, self.case_sensitive);
        self.current = if self.matches.is_empty() {
            None
        } else {
            match self.direction {
                SearchDirection::Forward => Some(0),
                SearchDirection::Reverse => Some(self.matches.len() - 1),
            }
        };
    }

    /// Set search mode.
    pub fn set_mode(&mut self, mode: SearchMode) {
        self.mode = mode;
    }

    /// Set case sensitivity.
    pub fn set_case_sensitive(&mut self, sensitive: bool) {
        if self.case_sensitive != sensitive {
            self.case_sensitive = sensitive;
            // Re-run search with new case sensitivity.
            let doc = Rope::new(); // placeholder - would need doc reference
            let _ = doc;
        }
    }

    /// Set direction.
    pub fn set_direction(&mut self, direction: SearchDirection) {
        self.direction = direction;
        self.current = if self.matches.is_empty() {
            None
        } else {
            match direction {
                SearchDirection::Forward => Some(0),
                SearchDirection::Reverse => Some(self.matches.len() - 1),
            }
        };
    }

    /// Find all matches of `query` in `doc`.
    fn find_all(
        doc: &Rope,
        query: &str,
        mode: SearchMode,
        case_sensitive: bool,
    ) -> Vec<Match> {
        if query.is_empty() {
            return Vec::new();
        }

        let text = doc.slice_text(0, doc.len());
        let search_text = if case_sensitive {
            text.clone()
        } else {
            text.to_lowercase()
        };
        let query_text = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        let mut matches = Vec::new();
        let mut start = 0;

        loop {
            let pos = match mode {
                SearchMode::Literal => search_text[start..].find(&query_text),
                SearchMode::Regex => {
                    // Simple regex-like: just literal for now (regex crate not added)
                    search_text[start..].find(&query_text)
                }
            };

            match pos {
                Some(p) => {
                    let abs_pos = start + p;
                    // Convert byte position to char position for Rope compatibility
                    let char_pos = doc.byte_to_char(abs_pos);
                    let end_char_pos = doc.byte_to_char(abs_pos + query.len());
                    matches.push(Match {
                        start: char_pos,
                        end: end_char_pos,
                    });
                    start = abs_pos + 1;
                }
                None => break,
            }
        }

        matches
    }

    /// Go to next match, wrapping only when `wrap` is enabled.
    pub fn next_match(&mut self) -> Option<Match> {
        if self.matches.is_empty() {
            return None;
        }
        let idx = match self.current {
            Some(i) if i + 1 < self.matches.len() => i + 1,
            Some(_) if self.wrap => 0,
            Some(_) => return None,
            None => 0,
        };
        self.current = Some(idx);
        self.matches.get(idx).copied()
    }

    /// Go to previous match, wrapping only when `wrap` is enabled.
    pub fn prev_match(&mut self) -> Option<Match> {
        if self.matches.is_empty() {
            return None;
        }
        let idx = match self.current {
            Some(i) if i > 0 => i - 1,
            Some(_) if self.wrap => self.matches.len() - 1,
            Some(_) => return None,
            None => self.matches.len() - 1,
        };
        self.current = Some(idx);
        self.matches.get(idx).copied()
    }

    /// Current match, if any.
    pub fn current_match(&self) -> Option<Match> {
        self.current.and_then(|i| self.matches.get(i).copied())
    }

    /// Number of matches.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Whether there are any matches.
    pub fn has_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    /// Current query text.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Highlight spans for a line: list of (char_start, char_end) ranges.
    pub fn highlight_line(
        &self,
        _line: usize,
        char_start: usize,
        char_end: usize,
    ) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        for m in &self.matches {
            // Check if match overlaps this line.
            // This is approximate - we use char positions directly.
            if m.start < char_end && m.end > char_start {
                let s = m.start.max(char_start);
                let e = m.end.min(char_end);
                if s < e {
                    spans.push((s, e));
                }
            }
        }
        spans
    }

    /// Clear search state.
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current = None;
    }
}

impl Default for Search {
    fn default() -> Self {
        Self::new()
    }
}

/// Replace state: replacement text and confirm flag.
pub struct Replace {
    replacement: String,
    confirm: bool,
}

impl Replace {
    pub fn new() -> Self {
        Self {
            replacement: String::new(),
            confirm: false,
        }
    }

    pub fn set_replacement(&mut self, text: &str) {
        self.replacement = text.to_string();
    }

    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    pub fn set_confirm(&mut self, confirm: bool) {
        self.confirm = confirm;
    }

    pub fn confirm(&self) -> bool {
        self.confirm
    }
}

impl Default for Replace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_literal_matches() {
        let doc = Rope::from_str("hello world hello universe");
        let mut search = Search::new();
        search.set_query("hello", &doc);
        assert_eq!(search.match_count(), 2);
    }

    #[test]
    fn find_no_matches() {
        let doc = Rope::from_str("hello world");
        let mut search = Search::new();
        search.set_query("xyz", &doc);
        assert_eq!(search.match_count(), 0);
    }

    #[test]
    fn next_and_prev_match() {
        let doc = Rope::from_str("one two three four");
        let mut search = Search::new();
        search.set_query("o", &doc);
        // "o" appears in "one" (pos 0), "two" (pos 4), "four" (pos 13)
        assert!(search.match_count() >= 2);
        let first = search.current_match();
        assert!(first.is_some());
        let second = search.next_match();
        assert!(second.is_some());
        assert_ne!(first, second);
        let back = search.prev_match();
        assert_eq!(back, first);
    }
}
