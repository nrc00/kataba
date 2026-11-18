//! File explorer view: oil-style filesystem navigation.
//!
//! The file explorer presents the filesystem as an editable buffer-like
//! interface. You can navigate directories, open files, create/delete/rename
//! entries, and all changes are applied to the real filesystem.

use std::path::{Path, PathBuf};

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::ui;

/// A single filesystem entry.
#[derive(Debug, Clone)]
struct FileEntry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

/// Oil-style file explorer: navigate and edit the filesystem.
pub struct FileExplorer {
    cwd: PathBuf,
    entries: Vec<FileEntry>,
    selected: usize,
    list_state: ListState,
    show_hidden: bool,
}

impl FileExplorer {
    /// Create a new file explorer for the given directory.
    pub fn new(cwd: &Path) -> Self {
        let mut explorer = Self {
            cwd: cwd.to_path_buf(),
            entries: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            show_hidden: false,
        };
        explorer.refresh();
        explorer
    }

    /// Re-read the directory from disk.
    pub fn refresh(&mut self) {
        self.entries = Self::read_dir(&self.cwd, self.show_hidden);
        if self.entries.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
        } else if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
            self.list_state.select(Some(self.selected));
        } else {
            self.list_state.select(Some(self.selected));
        }
    }

    /// Read directory entries (sorted: dirs first, then files, alphabetically).
    fn read_dir(dir: &Path, show_hidden: bool) -> Vec<FileEntry> {
        let mut entries: Vec<FileEntry> = match std::fs::read_dir(dir) {
            Ok(read_dir) => read_dir
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let path = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    if !show_hidden && name.starts_with('.') {
                        return None;
                    }
                    let is_dir = path.is_dir();
                    Some(FileEntry { path, name, is_dir })
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        entries.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        entries
    }

    /// Toggle whether dotfiles are listed, then re-read the directory.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.refresh();
    }

    /// Currently selected entry path.
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.entries.get(self.selected).map(|e| e.path.clone())
    }

    /// Currently selected entry name.
    pub fn selected_name(&self) -> Option<String> {
        self.entries.get(self.selected).map(|e| e.name.clone())
    }

    /// Move selection up.
    pub fn select_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.list_state.select(Some(self.selected));
    }

    /// Move selection down.
    pub fn select_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.entries.len() - 1);
        self.list_state.select(Some(self.selected));
    }

    /// Enter the selected directory, or return the selected file path.
    pub fn enter_selected(&mut self) -> Option<PathBuf> {
        let entry = self.entries.get(self.selected)?.clone();
        if entry.is_dir {
            self.cwd = entry.path.clone();
            self.refresh();
            None
        } else {
            Some(entry.path)
        }
    }

    /// Go up to the parent directory.
    pub fn go_up(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            self.cwd = parent.to_path_buf();
            self.refresh();
        }
    }

    /// Create a new file in the current directory.
    pub fn create_file(&mut self, name: &str) -> Result<(), std::io::Error> {
        let path = self.cwd.join(name);
        std::fs::File::create(&path)?;
        self.refresh();
        Ok(())
    }

    /// Create a new directory in the current directory.
    pub fn create_dir(&mut self, name: &str) -> Result<(), std::io::Error> {
        let path = self.cwd.join(name);
        std::fs::create_dir(&path)?;
        self.refresh();
        Ok(())
    }

    /// Delete the selected entry (file or empty directory).
    pub fn delete_selected(&mut self) -> Result<(), std::io::Error> {
        let path = self.selected_path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no entry selected")
        })?;
        if self.entries[self.selected].is_dir {
            std::fs::remove_dir(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
        self.refresh();
        Ok(())
    }

    /// Render the file explorer into `frame` over `area`.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!("file explorer ({})", self.cwd.display());
        let block = ui::block(&title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // Header: current directory.
        let header_area = Rect {
            height: 1,
            ..inner
        };
        let header = Line::from(Span::styled(
            format!(" {} ", self.cwd.display()),
            Style::default()
                .fg(ui::STATUSLINE_FG)
                .add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(ratatui::widgets::Paragraph::new(header), header_area);

        let list_area = Rect {
            y: inner.y + 1,
            height: inner.height.saturating_sub(1),
            ..inner
        };

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let selected = i == self.selected;
                let icon = if entry.is_dir { "📁 " } else { "📄 " };
                let style = if selected {
                    ui::menu_selected_style()
                } else if entry.is_dir {
                    Style::default().fg(ui::STATUSLINE_FG)
                } else {
                    Style::default().fg(ui::TEXT)
                };
                ListItem::new(Line::from(Span::styled(
                    format!("{}{}", icon, entry.name),
                    style,
                )))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default())
            .style(Style::default().fg(ui::TEXT))
            .highlight_style(ui::menu_selected_style());
        frame.render_stateful_widget(list, list_area, &mut self.list_state);
    }

    /// Current working directory path.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }
}
