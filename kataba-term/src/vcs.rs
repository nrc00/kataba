//! VCS view: git status listing backed by `kataba-vcs`.
//!
//! Lists changed files from `git status --porcelain=v2 -z` and lets the user
//! navigate and open them in the editor. Stage/unstage/delete are delegated
//! to `kataba_vcs::git::write`.

use std::path::{Path, PathBuf};

use kataba_vcs::git::write::{stage_file, unstage_file};
use kataba_vcs::git::{GitRepo, Trust, repo_status};
use kataba_vcs::git::status::head_name;
use kataba_vcs::model::ChangeKind;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::ui;

/// A single changed-file entry.
#[derive(Debug, Clone)]
struct VcsEntry {
    path: PathBuf,
    kind: ChangeKind,
}

impl VcsEntry {
    fn icon(&self) -> &'static str {
        match self.kind {
            ChangeKind::Untracked => "?",
            ChangeKind::Modified => "M",
            ChangeKind::Deleted => "D",
            ChangeKind::Renamed { .. } => "R",
            ChangeKind::Conflict => "!",
        }
    }

    fn icon_style(&self) -> Style {
        match self.kind {
            ChangeKind::Modified => Style::default().fg(ui::WARNING),
            ChangeKind::Deleted => Style::default().fg(ui::DIFF_MINUS),
            ChangeKind::Renamed { .. } => Style::default().fg(ui::DIFF_PLUS),
            ChangeKind::Conflict => Style::default().fg(ui::ERROR),
            ChangeKind::Untracked => Style::default().fg(ui::MUTED),
        }
    }
}

/// Git status panel: lists changed files with navigation.
pub struct VcsView {
    repo: Option<GitRepo>,
    entries: Vec<VcsEntry>,
    selected: usize,
    list_state: ListState,
}

impl VcsView {
    /// Open the repo at `cwd` and load its status.
    pub fn new(cwd: &str) -> Result<Self, kataba_vcs::Error> {
        let repo = GitRepo::open(Path::new(cwd), Trust::Reduced).ok();
        let entries = Self::load_entries(repo.as_ref());
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        Ok(Self {
            repo,
            entries,
            selected: 0,
            list_state,
        })
    }

    /// Re-read git status from disk.
    pub fn refresh(&mut self) {
        self.entries = Self::load_entries(self.repo.as_ref());
        if self.entries.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
        } else if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
            self.list_state.select(Some(self.selected));
        }
    }

    /// Path of the currently selected entry, if any.
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.entries.get(self.selected).map(|e| e.path.clone())
    }

    /// Move selection to the previous entry.
    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.list_state.select(Some(self.selected));
    }

    /// Move selection to the next entry.
    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.entries.len() - 1);
        self.list_state.select(Some(self.selected));
    }

    /// Stage the selected file.
    pub fn stage_selected(&mut self) {
        if let Some(path) = self.selected_path() {
            if let Some(repo) = &self.repo {
                let cwd = path.parent().unwrap_or(repo.worktree());
                let _ = stage_file(cwd, &[path.as_path()]);
                self.refresh();
            }
        }
    }

    /// Unstage the selected file.
    pub fn unstage_selected(&mut self) {
        if let Some(path) = self.selected_path() {
            if let Some(repo) = &self.repo {
                let cwd = path.parent().unwrap_or(repo.worktree());
                let _ = unstage_file(cwd, &[path.as_path()]);
                self.refresh();
            }
        }
    }

    /// Render the VCS panel into `frame` over `area`.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!("vcs ({})", self.entries.len());
        let block = ui::block(&title);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let selected = i == self.selected;
                let icon = entry.icon();
                let path = entry.path.to_string_lossy().to_string();
                let style = if selected {
                    ui::menu_selected_style()
                } else {
                    Style::default().fg(ui::TEXT)
                };
                let icon_style = if selected {
                    entry.icon_style().add_modifier(Modifier::BOLD)
                } else {
                    entry.icon_style()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", icon), icon_style),
                    Span::styled(path, style),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default())
            .style(Style::default().fg(ui::TEXT))
            .highlight_style(ui::menu_selected_style());
        frame.render_stateful_widget(list, inner, &mut self.list_state);
    }

    fn load_entries(repo: Option<&GitRepo>) -> Vec<VcsEntry> {
        let Some(repo) = repo else {
            return Vec::new();
        };
        let head = head_name(&repo.repo).unwrap_or(None);
        let snap = match repo_status(&repo.repo, repo.worktree(), head, 0) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        snap.changes
            .into_vec()
            .into_iter()
            .map(|fc| VcsEntry {
                path: fc.path,
                kind: fc.kind,
            })
            .collect()
    }
}
