//! Interactive Code Actions UI popup menu for Kataba.
//!
//! Provides a popup menu for selecting and triggering Language Server Protocol
//! code actions (quick-fixes, refactorings, imports) without leaving the editor.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app_message::AppMessage;
use crate::ui;

/// Represents an available code action item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeActionItem {
    pub title: String,
    pub kind: Option<String>,
    pub is_preferred: bool,
}

/// Interactive state for the code actions picker popup.
#[derive(Debug, Clone)]
pub struct CodeActionMenu {
    pub actions: Vec<CodeActionItem>,
    pub selected: usize,
}

impl CodeActionMenu {
    /// Construct a menu from items.
    #[must_use]
    pub fn new(actions: Vec<CodeActionItem>) -> Self {
        Self {
            actions,
            selected: 0,
        }
    }

    /// True if there are any actions to display.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Move selection down.
    pub fn select_down(&mut self) {
        if !self.actions.is_empty() {
            self.selected = (self.selected + 1) % self.actions.len();
        }
    }

    /// Move selection up.
    pub fn select_up(&mut self) {
        if !self.actions.is_empty() {
            if self.selected == 0 {
                self.selected = self.actions.len() - 1;
            } else {
                self.selected -= 1;
            }
        }
    }

    /// Handle key navigation.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<AppMessage> {
        match key.code {
            KeyCode::Esc => Some(AppMessage::Escape),
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_down();
                None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_up();
                None
            }
            KeyCode::Enter => {
                if let Some(action) = self.actions.get(self.selected) {
                    Some(AppMessage::SubmitCommand(format!("code-action:{}", action.title)))
                } else {
                    Some(AppMessage::Escape)
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(AppMessage::Escape)
            }
            _ => None,
        }
    }

    /// Render popup.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = ui::block("Code Actions");
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        if self.actions.is_empty() {
            let p = Paragraph::new(Span::styled(
                "No code actions available",
                Style::default().fg(ui::MUTED),
            ));
            frame.render_widget(p, inner);
            return;
        }

        let items: Vec<ListItem> = self
            .actions
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let is_selected = idx == self.selected;
                let mut spans = Vec::new();

                let bullet = if is_selected { "> " } else { "  " };
                spans.push(Span::styled(
                    bullet,
                    if is_selected {
                        Style::default().fg(Color::Yellow).bold()
                    } else {
                        Style::default().fg(ui::MUTED)
                    },
                ));

                let style = if is_selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                spans.push(Span::styled(&item.title, style));

                if item.is_preferred {
                    spans.push(Span::styled(
                        " [preferred]",
                        Style::default().fg(Color::Green),
                    ));
                }

                if let Some(kind) = &item.kind {
                    spans.push(Span::styled(
                        format!(" ({kind})"),
                        Style::default().fg(ui::MUTED),
                    ));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_wraps_correctly() {
        let actions = vec![
            CodeActionItem {
                title: "Import std::io".into(),
                kind: Some("quickfix".into()),
                is_preferred: true,
            },
            CodeActionItem {
                title: "Fill match arms".into(),
                kind: Some("refactor".into()),
                is_preferred: false,
            },
        ];
        let mut menu = CodeActionMenu::new(actions);
        assert_eq!(menu.selected, 0);
        menu.select_down();
        assert_eq!(menu.selected, 1);
        menu.select_down();
        assert_eq!(menu.selected, 0);
        menu.select_up();
        assert_eq!(menu.selected, 1);
    }
}
