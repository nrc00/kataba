//! Fuzzy finder: fast file/buffer/symbol lookup with fuzzy matching.
//!
//! As you type, candidates are filtered using a fuzzy match algorithm.
//! Selecting a candidate opens the corresponding file or buffer.

use std::path::{Path, PathBuf};

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::buffer_tab::BufferTab;
use crate::ui;

/// A fuzzy match result: matched character indices and score.
#[derive(Debug, Clone)]
struct FuzzyMatch {
    indices: Vec<usize>,
    score: i32,
}

/// Fuzzy match `pattern` against `text`. Returns None if no match.
///
/// Uses a simple scoring algorithm: consecutive matches get higher scores,
    /// matches at word boundaries get bonuses.
fn fuzzy_match(pattern: &str, text: &str) -> Option<FuzzyMatch> {
    if pattern.is_empty() {
        return Some(FuzzyMatch {
            indices: Vec::new(),
            score: 0,
        });
    }

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let mut pi = 0;
    let mut indices = Vec::new();
    let mut score: i32 = 0;
    let mut prev_match_idx: Option<usize> = None;

    for (ti, &tc) in text_chars.iter().enumerate() {
        if pi >= pattern_chars.len() {
            break;
        }
        if tc.eq_ignore_ascii_case(&pattern_chars[pi]) {
            indices.push(ti);
            score += 10;

            // Consecutive match bonus.
            if let Some(prev) = prev_match_idx {
                if ti == prev + 1 {
                    score += 5;
                }
            }

            // Word boundary bonus (match after separator or at start).
            if ti == 0 || text_chars[ti.saturating_sub(1)] == ' ' || text_chars[ti.saturating_sub(1)] == '/' || text_chars[ti.saturating_sub(1)] == '_' || text_chars[ti.saturating_sub(1)] == '-' {
                score += 5;
            }

            prev_match_idx = Some(ti);
            pi += 1;
        }
    }

    if pi == pattern_chars.len() {
        Some(FuzzyMatch { indices, score })
    } else {
        None
    }
}

/// A candidate item in the fuzzy finder.
#[derive(Debug, Clone)]
enum CandidateKind {
    File(PathBuf),
    Buffer(usize), // index into BufferTab
}

/// One fuzzy finder candidate.
#[derive(Debug, Clone)]
struct Candidate {
    kind: CandidateKind,
    display: String,
    score: i32,
}

/// Fuzzy finder state: candidates, filter text, selected index.
pub struct FuzzyFinder {
    candidates: Vec<Candidate>,
    filtered: Vec<usize>, // indices into candidates
    filter: String,
    selected: usize,
    list_state: ListState,
}

impl FuzzyFinder {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            filtered: Vec::new(),
            filter: String::new(),
            selected: 0,
            list_state: ListState::default(),
        }
    }

    /// Set candidates from files in a directory (recursive, limited depth).
    pub fn set_files(&mut self, root: &Path) {
        self.candidates = Self::scan_files(root);
        self.filtered = (0..self.candidates.len()).collect();
        self.selected = 0;
        self.list_state.select(Some(0));
        self.apply_filter();
    }

    /// Set candidates from open buffers.
    pub fn set_buffers(&mut self, buffers: &BufferTab) {
        self.candidates = (0..buffers.len())
            .map(|i| Candidate {
                kind: CandidateKind::Buffer(i),
                display: buffers.buffer(i).map(|b| b.name.clone()).unwrap_or_default(),
                score: 0,
            })
            .collect();
        self.filtered = (0..self.candidates.len()).collect();
        self.selected = 0;
        self.list_state.select(Some(0));
        self.apply_filter();
    }

    /// Scan files recursively up to depth 3, skipping common ignore dirs.
    fn scan_files(root: &Path) -> Vec<Candidate> {
        let mut candidates = Vec::new();
        Self::scan_dir(root, root, 0, &mut candidates);
        candidates
    }

    fn scan_dir(dir: &Path, root: &Path, depth: usize, out: &mut Vec<Candidate>) {
        if depth > 3 {
            return;
        }
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = read_dir
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|a| a.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
        }
            if path.is_dir() {
                Self::scan_dir(&path, root, depth + 1, out);
            } else {
                let display = path
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| name.clone());
                out.push(Candidate {
                    kind: CandidateKind::File(path),
                    display,
                    score: 0,
                });
            }
        }
    }

    /// Apply the current filter text to candidates.
    ///
    /// Every candidate is scored once here; the score is cached on the
    /// candidate so sorting does not re-run the matcher.
    pub fn apply_filter(&mut self) {
        self.filtered.clear();
        for (i, candidate) in self.candidates.iter_mut().enumerate() {
            match fuzzy_match(&self.filter, &candidate.display) {
                Some(m) => {
                    candidate.score = m.score;
                    self.filtered.push(i);
                }
                None => candidate.score = 0,
            }
        }
        // Best score first, ties broken by display name.
        self.filtered.sort_by(|&a, &b| {
            self.candidates[b]
                .score
                .cmp(&self.candidates[a].score)
                .then_with(|| self.candidates[a].display.cmp(&self.candidates[b].display))
        });
        if self.filtered.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
        } else if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len() - 1;
            self.list_state.select(Some(self.selected));
        } else {
            self.list_state.select(Some(self.selected));
        }
    }

    /// Append a character to the filter.
    pub fn push_char(&mut self, c: char) {
        self.filter.push(c);
        self.apply_filter();
    }

    /// Remove the last character from the filter.
    pub fn pop_char(&mut self) {
        self.filter.pop();
        self.apply_filter();
    }

    /// Clear the filter.
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.apply_filter();
    }

    /// Move selection up.
    pub fn select_up(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.list_state.select(Some(self.selected));
    }

    /// Move selection down.
    pub fn select_down(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.filtered.len() - 1);
        self.list_state.select(Some(self.selected));
    }

    /// Currently selected candidate, if any.
    fn selected_candidate(&self) -> Option<&Candidate> {
        self.filtered.get(self.selected).and_then(|&i| self.candidates.get(i))
    }

    /// Selected candidate's path (for File candidates).
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected_candidate().and_then(|c| match &c.kind {
            CandidateKind::File(path) => Some(path.clone()),
            CandidateKind::Buffer(_) => None,
        })
    }

    /// Selected candidate's buffer index (for Buffer candidates).
    pub fn selected_buffer_index(&self) -> Option<usize> {
        self.selected_candidate().and_then(|c| match &c.kind {
            CandidateKind::Buffer(idx) => Some(*idx),
            CandidateKind::File(_) => None,
        })
    }

    /// Current filter text.
    pub fn filter_text(&self) -> &str {
        &self.filter
    }

    /// Render the fuzzy finder into `frame` over `area`.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!("fuzzy finder ({})", self.filtered.len());
        let block = ui::block(&title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // Filter input line.
        let filter_line = Line::from(Span::styled(
            format!(" /{} ", self.filter),
            Style::default()
                .fg(ui::STATUSLINE_FG)
                .add_modifier(Modifier::BOLD),
        ));
        let filter_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            ratatui::widgets::Paragraph::new(filter_line),
            filter_area,
        );

        let list_inner = Rect {
            x: inner.x,
            y: inner.y + 1,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        };

        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .enumerate()
            .map(|(list_i, &cand_i)| {
                let selected = list_i == self.selected;
                let candidate = &self.candidates[cand_i];
                let icon = match &candidate.kind {
                    CandidateKind::File(_) => "📄 ",
                    CandidateKind::Buffer(_) => "📑 ",
                };
                let style = if selected {
                    ui::menu_selected_style()
                } else {
                    Style::default().fg(ui::TEXT)
                };
                // Emphasise the characters that the filter matched.
                let matched = fuzzy_match(&self.filter, &candidate.display)
                    .map(|m| m.indices)
                    .unwrap_or_default();
                let match_style = if selected {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(ui::STATUSLINE_FG)
                        .add_modifier(Modifier::BOLD)
                };

                let mut spans = vec![Span::styled(icon, style)];
                let mut run = String::new();
                let mut run_matched = false;
                for (idx, ch) in candidate.display.chars().enumerate() {
                    let is_match = matched.contains(&idx);
                    if is_match != run_matched && !run.is_empty() {
                        let span_style = if run_matched { match_style } else { style };
                        spans.push(Span::styled(std::mem::take(&mut run), span_style));
                    }
                    run_matched = is_match;
                    run.push(ch);
                }
                if !run.is_empty() {
                    let span_style = if run_matched { match_style } else { style };
                    spans.push(Span::styled(run, span_style));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default())
            .style(Style::default().fg(ui::TEXT))
            .highlight_style(ui::menu_selected_style());
        frame.render_stateful_widget(list, list_inner, &mut self.list_state);
    }
}

impl Default for FuzzyFinder {
    fn default() -> Self {
        Self::new()
    }
}
