//! Read-only viewer state and rendering.

use std::io;
use std::time::Duration;

use kataba_core::Rope;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};

use crate::highlight::HighlightSpan;

/// Scrolling step for PageUp/PageDown and the space bar.
const PAGE: usize = 20;

pub struct Viewer {
    path: String,
    rope: Rope,
    spans: Vec<HighlightSpan>,
    has_highlighter: bool,
    highlight_enabled: bool,
    lang: Option<String>,
    abi: Option<usize>,
    note: String,
    scroll: usize,
}

impl Viewer {
    pub fn new(
        path: String,
        rope: Rope,
        spans: Vec<HighlightSpan>,
        has_highlighter: bool,
        lang: Option<String>,
        abi: Option<usize>,
        note: String,
    ) -> Self {
        Self {
            path,
            rope,
            spans,
            has_highlighter,
            highlight_enabled: true,
            lang,
            abi,
            note,
            scroll: 0,
        }
    }

    /// Runs the event loop until the user quits.
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && self.handle_key(key)
            {
                return Ok(());
            }
        }
    }

    /// Returns `true` when the viewer should quit.
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return true;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('h') if self.has_highlighter => {
                self.highlight_enabled = !self.highlight_enabled;
            }
            KeyCode::Down | KeyCode::Char('j') => self.scroll_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_by(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_by(PAGE as isize),
            KeyCode::PageUp => self.scroll_by(-(PAGE as isize)),
            KeyCode::Home | KeyCode::Char('g') => self.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.scroll = self.rope.len_lines().saturating_sub(1);
            }
            _ => {}
        }
        false
    }

    fn scroll_by(&mut self, delta: isize) {
        let next = self.scroll as isize + delta;
        self.scroll = next.max(0) as usize;
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let [body, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);

        let total_lines = self.rope.len_lines();
        let height = body.height as usize;
        let first = self.scroll.min(total_lines.saturating_sub(1));
        let last = (first + height).min(total_lines);

        let first_byte = self.rope.char_to_byte(self.rope.line_to_char(first));
        let last_byte = self.rope.char_to_byte(self.rope.line_to_char(last));
        let buckets = self.line_buckets(first, last, first_byte, last_byte);

        let gutter_width = total_lines.to_string().len().max(1);
        let gutter_style = Style::default().fg(Color::DarkGray);
        let texts: Vec<String> = (first..last).map(|index| self.line_text(index)).collect();
        let mut lines = Vec::with_capacity(texts.len());
        for (offset, text) in texts.iter().enumerate() {
            let mut spans = vec![Span::styled(
                format!("{:>gutter_width$} ", first + offset + 1),
                gutter_style,
            )];
            spans.extend(styled_line(text, &buckets[offset]));
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), body);

        frame.render_widget(
            Paragraph::new(self.status_line(first, total_lines, status.width as usize))
                .style(Style::default().bg(Color::DarkGray).fg(Color::White)),
            status,
        );
    }

    /// Groups highlight spans into per-visible-line, line-relative ranges.
    fn line_buckets(
        &self,
        first: usize,
        last: usize,
        first_byte: usize,
        last_byte: usize,
    ) -> Vec<Vec<(usize, usize, Style)>> {
        let mut buckets: Vec<Vec<(usize, usize, Style)>> = vec![Vec::new(); last - first];
        if !self.highlight_enabled || buckets.is_empty() {
            return buckets;
        }
        for span in &self.spans {
            if span.end <= first_byte {
                continue;
            }
            if span.start >= last_byte {
                break; // spans are sorted by start
            }
            let start_line = self.line_of_byte(span.start).max(first);
            let end_line = self.line_of_byte(span.end.saturating_sub(1)).min(last - 1);
            for line in start_line..=end_line {
                let line_start = self.rope.char_to_byte(self.rope.line_to_char(line));
                let relative_start = span.start.saturating_sub(line_start);
                let relative_end = span.end.saturating_sub(line_start);
                buckets[line - first].push((relative_start, relative_end, span.style));
            }
        }
        buckets
    }

    fn line_of_byte(&self, byte: usize) -> usize {
        let clamped = byte.min(self.rope.byte_len());
        self.rope.char_to_line(self.rope.byte_to_char(clamped))
    }

    fn line_text(&self, line: usize) -> String {
        let start = self.rope.line_to_char(line);
        let end = self.rope.line_to_char(line + 1);
        let mut text = self.rope.slice_text(start, end);
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        text
    }

    fn status_line(&self, first_line: usize, total_lines: usize, width: usize) -> String {
        let mut left = format!(" treesitter-viz  {}", self.path);
        if let Some(lang) = &self.lang {
            left.push_str(&format!("  [{lang}]"));
        }
        if let Some(abi) = self.abi {
            left.push_str(&format!(" ABI {abi}"));
        }
        left.push_str(&format!("  L{}/{}", first_line + 1, total_lines));
        if !self.note.is_empty() {
            left.push_str(&format!("  ! {note}", note = self.note));
        }
        let right = format!(
            "[h] highlights: {}  [q]uit ",
            if self.highlight_enabled && self.has_highlighter {
                "on"
            } else {
                "off"
            }
        );
        let left_len = left.chars().count();
        let right_len = right.chars().count();
        if left_len + right_len < width {
            format!("{left}{:>padding$}", right, padding = width - left_len)
        } else {
            format!("{left} {right}")
        }
    }
}

/// Turns one line of text plus its overlapping ranges into styled spans.
///
/// Ranges are painted in order (later wins), then runs of equal style are
/// emitted. Range edges are snapped to char boundaries so slices stay valid.
fn styled_line<'a>(line: &'a str, ranges: &[(usize, usize, Style)]) -> Vec<Span<'a>> {
    if ranges.is_empty() {
        return if line.is_empty() {
            Vec::new()
        } else {
            vec![Span::raw(line)]
        };
    }
    let mut styles: Vec<Option<Style>> = vec![None; line.len()];
    for &(mut start, mut end, style) in ranges {
        start = start.min(line.len());
        end = end.min(line.len());
        while start > 0 && !line.is_char_boundary(start) {
            start -= 1;
        }
        while end < line.len() && !line.is_char_boundary(end) {
            end += 1;
        }
        if start >= end {
            continue;
        }
        styles[start..end].fill(Some(style));
    }
    let mut spans = Vec::new();
    let mut run_start = 0;
    for index in 1..=line.len() {
        if index == line.len() || styles[index] != styles[run_start] {
            let style = styles[run_start].unwrap_or_default();
            spans.push(Span::styled(&line[run_start..index], style));
            run_start = index;
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::{Viewer, styled_line};
    use crate::highlight::HighlightSpan;
    use kataba_core::Rope;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::{Color, Style};

    #[test]
    fn styled_line_splits_runs() {
        let green = Style::default().fg(Color::Green);
        let spans = styled_line("abcdef", &[(1, 4, green)]);
        let content: Vec<_> = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(content, ["a", "bcd", "ef"]);
        assert_eq!(spans[1].style, green);
        assert_eq!(spans[0].style, Style::default());
    }

    #[test]
    fn styled_line_snaps_to_char_boundaries() {
        let red = Style::default().fg(Color::Red);
        // `é` spans bytes 1..3, so a range starting at byte 2 must snap back.
        let spans = styled_line("aébc", &[(2, 4, red)]);
        let content: Vec<_> = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(content, ["a", "éb", "c"]);
    }

    #[test]
    fn styled_line_later_ranges_win() {
        let red = Style::default().fg(Color::Red);
        let blue = Style::default().fg(Color::Blue);
        let spans = styled_line("abcdef", &[(1, 5, red), (2, 3, blue)]);
        let content: Vec<_> = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(content, ["a", "b", "c", "de", "f"]);
        assert_eq!(spans[2].style, blue);
    }

    #[test]
    fn renders_without_panicking() {
        let rope = Rope::from_str("{\n  \"a\": 1\n}\n");
        let spans = vec![HighlightSpan {
            start: 5,
            end: 6,
            style: Style::default().fg(Color::Green),
        }];
        let mut viewer = Viewer::new(
            "demo.json".into(),
            rope,
            spans,
            true,
            Some("json".into()),
            Some(14),
            String::new(),
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        terminal.draw(|frame| viewer.render(frame)).unwrap();
        viewer.highlight_enabled = false;
        terminal.draw(|frame| viewer.render(frame)).unwrap();
    }
}
