use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

/// Multi-line text input with a visible cursor, used for commit messages.
pub struct Input {
    pub lines: Vec<String>,
    cursor_line: usize,
    cursor_col: usize,
    scroll: usize,
}

impl Input {
    pub fn new() -> Self {
        Self { lines: vec![String::new()], cursor_line: 0, cursor_col: 0, scroll: 0 }
    }

    pub fn value(&self) -> String {
        self.lines.join("\n")
    }

    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.scroll = 0;
    }

    pub fn cursor_pos(&self) -> (usize, usize) {
        (self.cursor_line, self.cursor_col)
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn num_lines(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, idx: usize) -> &str {
        self.lines.get(idx).map(|s| s.as_str()).unwrap_or("")
    }

    fn clamp_cursor(&mut self) {
        if self.cursor_line >= self.lines.len() {
            self.cursor_line = self.lines.len().saturating_sub(1);
        }
        let max_col = self.lines.get(self.cursor_line).map(|l| l.len()).unwrap_or(0);
        if self.cursor_col > max_col {
            self.cursor_col = max_col;
        }
    }

    /// Handle a keypress. `area` is the input box inner size for scrolling.
    pub fn handle_key(&mut self, key: KeyEvent, area: Rect) {
        let visible = area.height.saturating_sub(2) as usize;
        let empty_visible = visible.max(1);

        match key.code {
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'a' => self.cursor_col = 0,
                        'e' => self.cursor_col = self.lines[self.cursor_line].len(),
                        'u' => {
                            self.lines[self.cursor_line].clear();
                            self.cursor_col = 0;
                        }
                        'k' => {
                            self.lines[self.cursor_line].truncate(self.cursor_col);
                        }
                        _ => {}
                    }
                } else {
                    let line = &mut self.lines[self.cursor_line];
                    line.insert(self.cursor_col, c);
                    self.cursor_col += 1;
                }
            }
            KeyCode::Enter => {
                let line = self.lines.remove(self.cursor_line);
                let (head, tail) = line.split_at(self.cursor_col);
                self.lines.insert(self.cursor_line, head.to_string());
                self.cursor_line += 1;
                self.lines.insert(self.cursor_line, tail.to_string());
                self.cursor_col = 0;
            }
            KeyCode::Backspace => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                    self.lines[self.cursor_line].remove(self.cursor_col);
                } else if self.cursor_line > 0 {
                    let col = self.lines[self.cursor_line - 1].len();
                    let line = self.lines.remove(self.cursor_line);
                    self.cursor_line -= 1;
                    self.lines[self.cursor_line].push_str(&line);
                    self.cursor_col = col;
                }
            }
            KeyCode::Delete => {
                let line = &self.lines[self.cursor_line];
                if self.cursor_col < line.len() {
                    self.lines[self.cursor_line].remove(self.cursor_col);
                } else if self.cursor_line + 1 < self.lines.len() {
                    let next = self.lines.remove(self.cursor_line + 1);
                    self.lines[self.cursor_line].push_str(&next);
                }
            }
            KeyCode::Left => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                } else if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = self.lines[self.cursor_line].len();
                }
            }
            KeyCode::Right => {
                let len = self.lines[self.cursor_line].len();
                if self.cursor_col < len {
                    self.cursor_col += 1;
                } else if self.cursor_line + 1 < self.lines.len() {
                    self.cursor_line += 1;
                    self.cursor_col = 0;
                }
            }
            KeyCode::Up => {
                if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
                }
            }
            KeyCode::Down => {
                if self.cursor_line + 1 < self.lines.len() {
                    self.cursor_line += 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
                }
            }
            KeyCode::Home => self.cursor_col = 0,
            KeyCode::End => self.cursor_col = self.lines[self.cursor_line].len(),
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(empty_visible);
                self.cursor_line = self.cursor_line.saturating_sub(empty_visible);
                self.cursor_line = self.cursor_line.max(self.scroll);
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(empty_visible);
            }
            _ => {}
        }
        self.clamp_cursor();

        // keep the cursor line scrolled into view
        if self.cursor_line < self.scroll {
            self.scroll = self.cursor_line;
        }
        if self.cursor_line >= self.scroll.saturating_add(empty_visible) {
            self.scroll = self.cursor_line.saturating_sub(empty_visible.saturating_sub(1));
        }
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}
