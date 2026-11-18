//! Command-line (ex-command / `:` prompt) for Kataba.
//!
//! Provides a persistent or modal `:` ex-command line prompt that parses
//! commands (:w, :q, :wq, :q!, :e, :b, :term, :help, :vcs, etc.) and dispatches
//! corresponding [`AppMessage`] variants.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app_message::AppMessage;
use crate::ui;

/// Interactive state of the `:` command prompt.
#[derive(Debug, Clone)]
pub struct CommandLine {
    /// Command line text excluding the leading `:`.
    pub input: String,
    /// Byte cursor offset within `input`.
    pub cursor: usize,
    /// History of executed commands.
    pub history: Vec<String>,
    /// Current history index if browsing up/down.
    pub history_idx: Option<usize>,
}

impl Default for CommandLine {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandLine {
    /// Construct a clean command line instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_idx: None,
        }
    }

    /// Reset input state when re-opening the prompt.
    pub fn open(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.history_idx = None;
    }

    /// Push a character at the cursor position.
    pub fn push_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete character before the cursor (backspace).
    pub fn pop_char(&mut self) {
        if self.cursor > 0 {
            // Find preceding char boundary
            let prev = self.input[..self.cursor]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            self.input.remove(prev);
            self.cursor = prev;
        }
    }

    /// Move cursor left.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.input[..self.cursor]
                .char_indices()
                .last()
                .map(|(idx, _)| idx)
                .unwrap_or(0);
        }
    }

    /// Move cursor right.
    pub fn move_right(&mut self) {
        if let Some((idx, _)) = self.input[self.cursor..].char_indices().nth(1) {
            self.cursor += idx;
        } else {
            self.cursor = self.input.len();
        }
    }

    /// Navigate previous command history.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next_idx = match self.history_idx {
            None => self.history.len().saturating_sub(1),
            Some(i) => i.saturating_sub(1),
        };
        self.history_idx = Some(next_idx);
        if let Some(cmd) = self.history.get(next_idx) {
            self.input = cmd.clone();
            self.cursor = self.input.len();
        }
    }

    /// Navigate next command history.
    pub fn history_next(&mut self) {
        if let Some(i) = self.history_idx {
            if i + 1 < self.history.len() {
                let next_idx = i + 1;
                self.history_idx = Some(next_idx);
                if let Some(cmd) = self.history.get(next_idx) {
                    self.input = cmd.clone();
                    self.cursor = self.input.len();
                }
            } else {
                self.history_idx = None;
                self.input.clear();
                self.cursor = 0;
            }
        }
    }

    /// Submit current input, store to history, and parse command into an [`AppMessage`].
    pub fn submit(&mut self) -> Option<AppMessage> {
        let raw = self.input.trim().to_string();
        if raw.is_empty() {
            return Some(AppMessage::Escape);
        }
        if self.history.last().map(|s| s.as_str()) != Some(&raw) {
            self.history.push(raw.clone());
        }
        self.input.clear();
        self.cursor = 0;
        self.history_idx = None;
        Some(parse_command(&raw))
    }

    /// Handle key input for the command line.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<AppMessage> {
        match key.code {
            KeyCode::Esc => Some(AppMessage::Escape),
            KeyCode::Enter => self.submit(),
            KeyCode::Backspace => {
                self.pop_char();
                None
            }
            KeyCode::Left => {
                self.move_left();
                None
            }
            KeyCode::Right => {
                self.move_right();
                None
            }
            KeyCode::Up => {
                self.history_prev();
                None
            }
            KeyCode::Down => {
                self.history_next();
                None
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(AppMessage::Escape)
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.push_char(c);
                None
            }
            _ => None,
        }
    }

    /// Render the `:` prompt at the bottom line.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let prompt_style = Style::default().fg(Color::Cyan).bold();
        let text_style = Style::default().fg(Color::White);
        let cursor_style = Style::default().bg(Color::White).fg(Color::Black);

        let before_cursor = &self.input[..self.cursor];
        let at_cursor = self.input[self.cursor..].chars().next().unwrap_or(' ');
        let after_cursor = if self.cursor < self.input.len() {
            let offset = self.input[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.input.len());
            &self.input[offset..]
        } else {
            ""
        };

        let line = Line::from(vec![
            Span::styled(":", prompt_style),
            Span::styled(before_cursor, text_style),
            Span::styled(at_cursor.to_string(), cursor_style),
            Span::styled(after_cursor, text_style),
        ]);

        let p = Paragraph::new(line).block(Block::default().style(ui::base_style()));
        frame.render_widget(p, area);
    }
}

/// Parse a raw command string (without leading `:`) into an [`AppMessage`].
#[must_use]
pub fn parse_command(cmd: &str) -> AppMessage {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return AppMessage::Escape;
    }

    match parts[0] {
        "w" | "write" => AppMessage::BufferSaveActive,
        "q" | "quit" => AppMessage::Quit,
        "wq" | "x" => {
            // In Kataba, we trigger save then quit can follow or be composed
            AppMessage::BufferSaveActive
        }
        "q!" => AppMessage::Quit,
        "bn" | "bnext" => AppMessage::BufferNext,
        "bp" | "bprev" => AppMessage::BufferPrev,
        "bd" | "bclose" => AppMessage::BufferCloseActive,
        "e" | "open" => {
            if parts.len() > 1 {
                AppMessage::BufferOpenFile(std::path::PathBuf::from(parts[1]))
            } else {
                AppMessage::FileExplorerOpen
            }
        }
        "vcs" => AppMessage::SwitchView(crate::app::View::Vcs),
        "help" | "h" => AppMessage::SwitchView(crate::app::View::Help),
        "find" | "f" => AppMessage::FuzzyFinderOpen,
        "explorer" | "exp" => AppMessage::FileExplorerOpen,
        _ => AppMessage::SubmitCommand(cmd.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_commands() {
        assert_eq!(parse_command("w"), AppMessage::BufferSaveActive);
        assert_eq!(parse_command("write"), AppMessage::BufferSaveActive);
        assert_eq!(parse_command("q"), AppMessage::Quit);
        assert_eq!(parse_command("quit"), AppMessage::Quit);
        assert_eq!(parse_command("bn"), AppMessage::BufferNext);
        assert_eq!(parse_command("bp"), AppMessage::BufferPrev);
        assert_eq!(parse_command("bd"), AppMessage::BufferCloseActive);
        assert_eq!(
            parse_command("e src/main.rs"),
            AppMessage::BufferOpenFile(std::path::PathBuf::from("src/main.rs"))
        );
        assert_eq!(
            parse_command("custom arg"),
            AppMessage::SubmitCommand("custom arg".to_string())
        );
    }

    #[test]
    fn command_line_editing_and_history() {
        let mut cl = CommandLine::new();
        cl.push_char('w');
        cl.push_char('q');
        assert_eq!(cl.input, "wq");
        cl.pop_char();
        assert_eq!(cl.input, "w");

        let msg = cl.submit();
        assert_eq!(msg, Some(AppMessage::BufferSaveActive));
        assert_eq!(cl.history, vec!["w".to_string()]);

        cl.history_prev();
        assert_eq!(cl.input, "w");
    }
}
