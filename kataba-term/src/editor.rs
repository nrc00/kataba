//! Editor view: a `kataba-core` buffer rendered in the terminal.
//!
//! The layout follows Helix: no border, a gutter with (relative) line numbers
//! and diff signs, and the cursor drawn where the terminal cursor sits. The
//! `Rope`, multi-cursor `Selection`, `LinearHistory` and tree-sitter state all
//! live here; input messages are applied by `App::update` in `app.rs`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use kataba_core::change::{delete_range, insert_at, ChangeSet};
use kataba_core::direction::Direction;
use kataba_core::history::State;
use kataba_core::linear_history::LinearHistory;
use kataba_core::motion;
use kataba_core::range::Range;
use kataba_core::rope::Rope;
use kataba_core::selection::Selection;
use kataba_core::transaction::Transaction;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::Mode;
use crate::syntax::SyntaxState;
use crate::ui;

pub struct EditorView {
    pub doc: Rope,
    pub sel: Selection,
    pub history: LinearHistory,
    pub mode: Mode,
    pub scroll_y: usize,
    pub scroll_x: usize,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
    pub status_message: Option<String>,
    pub syntax: SyntaxState,
    /// Diff signs keyed by zero-based document line (`+`, `~`, `-`).
    pub diff_signs: BTreeMap<usize, char>,
}

impl Default for EditorView {
    fn default() -> Self {
        Self::new("")
    }
}

impl EditorView {
    pub fn new(text: &str) -> Self {
        let doc = Rope::from_str(text);
        let sel = Selection::point(0);
        Self {
            doc,
            sel,
            history: LinearHistory::new(),
            mode: Mode::Normal,
            scroll_y: 0,
            scroll_x: 0,
            file_path: None,
            dirty: false,
            status_message: None,
            syntax: SyntaxState::new(),
            diff_signs: BTreeMap::new(),
        }
    }

    /// Primary cursor head as a document char offset.
    pub fn head(&self) -> usize {
        self.sel.primary().head
    }

    /// Number of lines in the document.
    pub fn line_count(&self) -> usize {
        self.doc.lines()
    }

    /// Text of line `line` (0-indexed), without the trailing newline.
    pub fn line_text(&self, line: usize) -> Option<String> {
        if line >= self.line_count() {
            return None;
        }
        let start = self.doc.line_to_char(line);
        let end = self.doc.line_to_char(line + 1).min(self.doc.len());
        let mut text = self.doc.slice_text(start, end);
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        Some(text)
    }

    /// Apply an insert/delete changeset, commit to history, update selection.
    fn apply_change(&mut self, changeset: ChangeSet) {
        let original = State {
            doc: self.doc.clone(),
            selection: self.sel.clone(),
        };
        let txn = Transaction::change(&self.doc, changeset.changes_iter());
        self.history.commit(&txn, &original);
        txn.apply(&mut self.doc, &mut self.sel);
        let _ = self.syntax.reparse(&self.doc, &changeset);
        self.dirty = true;
    }

    /// Insert text at the primary cursor head.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let head = self.head();
        let changeset = insert_at(head, text);
        self.apply_change(changeset);
    }

    /// Insert a newline at the primary cursor head.
    pub fn insert_newline(&mut self) {
        self.insert_text("\n");
    }

    /// Delete the character before the primary cursor head (backspace).
    pub fn backspace(&mut self) {
        let head = self.head();
        if head == 0 {
            return;
        }
        let changeset = delete_range(
            Range::new(head - 1, head).with_direction(Direction::Forward),
            &self.doc,
        );
        self.apply_change(changeset);
    }

    /// Delete the character at the primary cursor head.
    pub fn delete_char(&mut self) {
        let head = self.head();
        if head >= self.doc.len() {
            return;
        }
        let changeset = delete_range(
            Range::new(head, head + 1).with_direction(Direction::Forward),
            &self.doc,
        );
        self.apply_change(changeset);
    }

    /// Undo the last change.
    pub fn undo(&mut self) {
        if let Some(inv) = self.history.undo() {
            inv.apply(&mut self.doc, &mut self.sel);
            let _ = self.syntax.reparse_full(&self.doc);
        }
    }

    /// Redo the last undone change.
    pub fn redo(&mut self) {
        if let Some(txn) = self.history.redo() {
            txn.apply(&mut self.doc, &mut self.sel);
            let _ = self.syntax.reparse_full(&self.doc);
        }
    }

    /// Move primary cursor left one grapheme.
    pub fn move_left(&mut self) {
        let pos = motion::move_left(&self.doc, self.head());
        self.sel = Selection::point(pos);
    }

    /// Move primary cursor right one grapheme.
    pub fn move_right(&mut self) {
        let pos = motion::move_right(&self.doc, self.head());
        self.sel = Selection::point(pos);
    }

    /// Move primary cursor up one line.
    pub fn move_up(&mut self) {
        let pos = motion::move_up(&self.doc, self.head());
        self.sel = Selection::point(pos);
    }

    /// Move primary cursor down one line.
    pub fn move_down(&mut self) {
        let pos = motion::move_down(&self.doc, self.head());
        self.sel = Selection::point(pos);
    }

    /// Move to start of current line.
    pub fn move_line_start(&mut self) {
        let line = self.doc.char_to_line(self.head());
        let pos = self.doc.line_to_char(line);
        self.sel = Selection::point(pos);
    }

    /// Move to end of current line.
    pub fn move_line_end(&mut self) {
        let line = self.doc.char_to_line(self.head());
        let mut pos = self.doc.line_to_char(line + 1).min(self.doc.len());
        // `line_to_char(line + 1)` points past the newline; sit before it.
        if pos > 0 && self.doc.char(pos - 1) == Some('\n') {
            pos -= 1;
            if pos > 0 && self.doc.char(pos - 1) == Some('\r') {
                pos -= 1;
            }
        }
        self.sel = Selection::point(pos);
    }

    /// Toggle selection mode (visual) at current cursor.
    pub fn toggle_selection(&mut self) {
        let head = self.head();
        if self.sel.primary().from() == self.sel.primary().to() {
            self.sel = Selection::single(head, head);
        } else {
            let pos = self.sel.primary().head;
            self.sel = Selection::point(pos);
        }
    }

    /// Replace the diff-sign gutter map.
    pub fn set_diff_signs(&mut self, signs: BTreeMap<usize, char>) {
        self.diff_signs = signs;
    }

    /// Width of the whole gutter: diagnostics, spacer, numbers, spacer, diff.
    fn gutter_width(&self) -> usize {
        let digits = self.line_count().max(1).to_string().len().max(2);
        digits + 4
    }

    /// Scroll so the primary cursor stays inside a `height` x `width` viewport.
    fn ensure_visible(&mut self, height: usize, width: usize) {
        if height == 0 {
            return;
        }
        let line = self.doc.char_to_line(self.head());
        if line < self.scroll_y {
            self.scroll_y = line;
        }
        if line >= self.scroll_y.saturating_add(height) {
            self.scroll_y = line + 1 - height;
        }

        let line_start = self.doc.line_to_char(line);
        let col = self.head().saturating_sub(line_start);
        if col < self.scroll_x {
            self.scroll_x = col;
        }
        if width > 0 && col >= self.scroll_x.saturating_add(width) {
            self.scroll_x = col + 1 - width;
        }
    }

    /// Render the editor into `frame` over `area`.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let height = area.height as usize;
        let gutter = self.gutter_width();
        let text_width = (area.width as usize).saturating_sub(gutter);
        self.ensure_visible(height, text_width);

        let total_lines = self.line_count();
        let cursor_line = self.doc.char_to_line(self.head());
        let start_line = self.scroll_y;

        let mut lines: Vec<Line> = Vec::with_capacity(height);
        for row in 0..height {
            let line = start_line + row;
            if line < total_lines {
                lines.push(self.render_line(line, cursor_line, text_width));
            } else {
                lines.push(self.render_filler());
            }
        }

        let paragraph = Paragraph::new(lines).style(ui::base_style());
        frame.render_widget(paragraph, area);

        // Place the terminal cursor at the primary head.
        if cursor_line >= start_line && cursor_line < start_line + height {
            let line_start = self.doc.line_to_char(cursor_line);
            let col = self.head().saturating_sub(line_start);
            if col >= self.scroll_x {
                let x = area.x as usize + gutter + (col - self.scroll_x);
                let y = area.y as usize + (cursor_line - start_line);
                if x < area.right() as usize && y < area.bottom() as usize {
                    frame.set_cursor(x as u16, y as u16);
                }
            }
        }
    }

    /// Render one document line: gutter followed by highlighted text.
    fn render_line(&self, line: usize, cursor_line: usize, text_width: usize) -> Line<'static> {
        let text = self.line_text(line).unwrap_or_default();
        let line_start = self.doc.line_to_char(line);
        let highlights = self.syntax.highlight_line(line_start, &text);

        let mut spans = self.gutter_spans(line, cursor_line);
        spans.extend(self.text_spans(&text, line_start, &highlights, text_width));

        // Helix highlights the cursor line (`ui.cursorline.primary`), without
        // disturbing spans that already carry a background (e.g. selection).
        if line == cursor_line {
            let cursor_bg = Style::default().bg(ui::CURSORLINE);
            spans = spans
                .into_iter()
                .map(|span| {
                    if span.style.bg.is_none() {
                        span.patch_style(cursor_bg)
                    } else {
                        span
                    }
                })
                .collect();
        }

        Line::from(spans)
    }

    /// Gutter for a real document line.
    fn gutter_spans(&self, line: usize, cursor_line: usize) -> Vec<Span<'static>> {
        let digits = self.gutter_width() - 4;
        let current = line == cursor_line;
        let relative = matches!(self.mode, Mode::Normal);
        let number = if relative && !current {
            line.abs_diff(cursor_line)
        } else {
            line + 1
        };

        let blank = Style::default().fg(ui::MUTED);
        let (sign, sign_style) = match self.diff_signs.get(&line) {
            Some('+') => ("+", Style::default().fg(ui::DIFF_PLUS)),
            Some('-') => ("-", Style::default().fg(ui::DIFF_MINUS)),
            Some('~') => ("~", Style::default().fg(ui::DIFF_DELTA)),
            _ => (" ", blank),
        };

        vec![
            // diagnostics sign column (reserved for the LSP layer)
            Span::styled(" ", blank),
            Span::styled(" ", blank),
            Span::styled(
                format!("{number:>digits$}"),
                ui::line_number_style(current),
            ),
            Span::styled(" ", blank),
            Span::styled(sign.to_string(), sign_style),
        ]
    }

    /// Gutter for a line past the end of the document (the `~` filler).
    fn render_filler(&self) -> Line<'static> {
        let digits = self.gutter_width() - 4;
        let blank = Style::default().fg(ui::MUTED);
        Line::from(vec![
            Span::styled(" ", blank),
            Span::styled(" ", blank),
            Span::styled(format!("{:>digits$}", "~"), blank),
            Span::styled(" ", blank),
            Span::styled(" ", blank),
        ])
    }

    /// Build the text spans for a line, applying selection and syntax colours.
    fn text_spans(
        &self,
        text: &str,
        line_start: usize,
        highlights: &[(usize, usize, Style)],
        text_width: usize,
    ) -> Vec<Span<'static>> {
        let chars: Vec<char> = text.chars().collect();
        let total = chars.len();
        let skip = self.scroll_x.min(total);
        let end = skip.saturating_add(text_width).min(total);

        let primary = self.sel.primary();
        let sel_from = primary.from().min(primary.to()).saturating_sub(line_start);
        let sel_to = primary.from().max(primary.to()).saturating_sub(line_start);

        let mut spans: Vec<Span<'static>> = Vec::with_capacity(8);
        let mut run = String::new();
        let mut run_style: Option<Style> = None;

        for (idx, ch) in chars.iter().enumerate().take(end).skip(skip) {
            let style = if idx >= sel_from && idx < sel_to {
                ui::selected_style()
            } else {
                self.highlight_style_at(idx, highlights)
            };

            if run_style == Some(style) {
                run.push(*ch);
            } else {
                if let Some(previous) = run_style.take() {
                    if !run.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut run), previous));
                    }
                }
                run.push(*ch);
                run_style = Some(style);
            }
        }

        if let Some(previous) = run_style {
            if !run.is_empty() {
                spans.push(Span::styled(run, previous));
            }
        }

        spans
    }

    /// Pick the most specific highlight covering `idx` (later spans win).
    fn highlight_style_at(&self, idx: usize, highlights: &[(usize, usize, Style)]) -> Style {
        highlights
            .iter()
            .rev()
            .find(|(start, end, _)| idx >= *start && idx < *end)
            .map(|(_, _, style)| *style)
            .unwrap_or_else(|| Style::default().fg(ui::TEXT))
    }
}
