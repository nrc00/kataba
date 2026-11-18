//! Command palette: searchable command execution.
//!
//! Type a command name to filter, then press Enter to execute.
//! Commands cover file operations, view switching, and editor actions.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::app::View;
use crate::app_message::AppMessage;
use crate::ui;

/// A registered command.
#[derive(Debug, Clone)]
pub struct Command {
    /// Display name shown in the palette.
    pub name: String,
    /// Description shown as a subtitle.
    pub description: String,
    /// The message produced when this command is executed.
    pub message: AppMessage,
}

/// Command palette state: commands, filter text, selected index.
pub struct CommandPalette {
    commands: Vec<Command>,
    filtered: Vec<usize>,
    filter: String,
    selected: usize,
    list_state: ListState,
}

impl CommandPalette {
    pub fn new() -> Self {
        let commands = Self::default_commands();
        let filtered = (0..commands.len()).collect();
        Self {
            commands,
            filtered,
            filter: String::new(),
            selected: 0,
            list_state: ListState::default(),
        }
    }

    /// Default command set.
    fn default_commands() -> Vec<Command> {
        vec![
            Command {
                name: "save".into(),
                description: "Save current buffer".into(),
                message: AppMessage::BufferSaveActive,
            },
            Command {
                name: "quit".into(),
                description: "Quit the editor".into(),
                message: AppMessage::Quit,
            },
            Command {
                name: "view editor".into(),
                description: "Switch to editor view".into(),
                message: AppMessage::SwitchView(View::Editor),
            },
            Command {
                name: "view vcs".into(),
                description: "Switch to VCS view".into(),
                message: AppMessage::SwitchView(View::Vcs),
            },
            Command {
                name: "view help".into(),
                description: "Switch to help view".into(),
                message: AppMessage::SwitchView(View::Help),
            },
            Command {
                name: "view file explorer".into(),
                description: "Switch to file explorer".into(),
                message: AppMessage::FileExplorerOpen,
            },
            Command {
                name: "view fuzzy finder".into(),
                description: "Switch to fuzzy finder".into(),
                message: AppMessage::FuzzyFinderOpen,
            },
            Command {
                name: "next buffer".into(),
                description: "Switch to next buffer".into(),
                message: AppMessage::BufferNext,
            },
            Command {
                name: "prev buffer".into(),
                description: "Switch to previous buffer".into(),
                message: AppMessage::BufferPrev,
            },
            Command {
                name: "close buffer".into(),
                description: "Close current buffer".into(),
                message: AppMessage::BufferCloseActive,
            },
            Command {
                name: "new scratch".into(),
                description: "Create a new scratch buffer".into(),
                message: AppMessage::BufferOpenScratch("scratch".into()),
            },
            Command {
                name: "insert mode".into(),
                description: "Enter insert mode".into(),
                message: AppMessage::EnterInsertMode,
            },
            Command {
                name: "normal mode".into(),
                description: "Return to normal mode".into(),
                message: AppMessage::Escape,
            },
        ]
    }

    /// Apply the current filter text to commands.
    pub fn apply_filter(&mut self) {
        self.filtered.clear();
        for (i, cmd) in self.commands.iter().enumerate() {
            if self.filter.is_empty()
                || cmd
                    .name
                    .to_lowercase()
                    .contains(&self.filter.to_lowercase())
                || cmd
                    .description
                    .to_lowercase()
                    .contains(&self.filter.to_lowercase())
            {
                self.filtered.push(i);
            }
        }
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

    /// Currently selected command, if any.
    pub fn selected_command(&self) -> Option<&Command> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.commands.get(i))
    }

    /// Current filter text.
    pub fn filter_text(&self) -> &str {
        &self.filter
    }

    /// Render the command palette into `frame` over `area`.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!("command palette ({})", self.filtered.len());
        let block = ui::block(&title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // Filter input line.
        let filter_line = Line::from(Span::styled(
            format!(" >{} ", self.filter),
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
        frame.render_widget(ratatui::widgets::Paragraph::new(filter_line), filter_area);

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
            .take(list_inner.height as usize)
            .map(|(list_i, &cmd_i)| {
                let selected = list_i == self.selected;
                let cmd = &self.commands[cmd_i];
                let style = if selected {
                    ui::menu_selected_style()
                } else {
                    Style::default().fg(ui::TEXT)
                };
                let desc_style = if selected {
                    Style::default().bg(ui::STATUSLINE_FG).fg(ui::POPUP_BG)
                } else {
                    Style::default().fg(ui::MUTED)
                };
                ListItem::new(vec![
                    Line::from(Span::styled(format!("  {}", cmd.name), style)),
                    Line::from(Span::styled(
                        format!("      {}", cmd.description),
                        desc_style,
                    )),
                ])
            })
            .collect();

        let list = List::new(items)
            .block(Block::default())
            .style(Style::default().fg(ui::TEXT))
            .highlight_style(ui::menu_selected_style());
        frame.render_stateful_widget(list, list_inner, &mut self.list_state);
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}
