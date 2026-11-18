use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::git::{ChangeKind, CommitInfo, FileItem, GitRepo, HeadInfo, Section};
use crate::input::Input;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Status,
    Diff,
    Commit,
    Log,
    Branch,
    Help,
}

/// A rendered line in the status body.
#[derive(Clone)]
pub enum StatusLine {
    SectionHeader { section: Section, title: String, count: usize },
    Blank,
    File { path: PathBuf, mode: ChangeKind, section: Section },
    Diff(String),
}

pub struct App {
    repo: GitRepo,
    view: View,
    head: HeadInfo,

    // status model
    status_files: Vec<FileItem>,
    open_files: HashSet<PathBuf>,            // files whose inline diff is shown
    open_file_diffs: HashMap<PathBuf, String>, // diff text per open file
    section_folded: [bool; 3],               // one per Section
    status_lines: Vec<StatusLine>,
    status_state: ListState,
    status_msg: Option<String>,

    // full-screen diff view
    diff_text: String,
    diff_title: String,
    diff_scroll: u16,

    // commit editor
    commit_input: Input,

    // log
    log_entries: Vec<CommitInfo>,
    log_state: ListState,

    // branches
    branches: Vec<String>,
    branch_state: ListState,

    terminal_size: ratatui::layout::Rect,

    should_quit: bool,
}

impl App {
    pub fn new(repo_path: &str) -> Result<Self> {
        let repo = GitRepo::open(repo_path)?;
        let head = repo.head_info()?;
        let status_files = repo.status()?;
        let log_entries = repo.log()?;
        let branches = repo.branches()?;

        let mut status_lines = Vec::new();
        build_status_lines(
            &status_files,
            &HashSet::new(),
            &HashMap::new(),
            &[false, false, false],
            &mut status_lines,
        );

        let mut status_state = ListState::default();
        if !status_lines.is_empty() {
            status_state.select(Some(0));
        }

        let mut log_state = ListState::default();
        if !log_entries.is_empty() {
            log_state.select(Some(0));
        }
        let mut branch_state = ListState::default();
        if !branches.is_empty() {
            branch_state.select(Some(0));
        }

        Ok(Self {
            repo,
            view: View::Status,
            head,
            status_files,
            open_files: HashSet::new(),
            open_file_diffs: HashMap::new(),
            section_folded: [false, false, false],
            status_lines,
            status_state,
            status_msg: None,
            diff_text: String::new(),
            diff_title: String::new(),
            diff_scroll: 0,
            commit_input: Input::new(),
            log_entries,
            log_state,
            branches,
            branch_state,
            terminal_size: ratatui::layout::Rect::default(),
            should_quit: false,
        })
    }

    pub fn run(
        mut self,
        terminal: &mut Terminal<impl ratatui::backend::Backend>,
    ) -> Result<()> {
        loop {
            terminal.draw(|f| {
                self.terminal_size = f.size();
                self.draw(f)
            })?;
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                self.handle_key(key.code, key.modifiers);
            }
            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn set_msg(&mut self, msg: impl Into<String>) {
        self.status_msg = Some(msg.into());
    }

    fn clear_msg(&mut self) {
        self.status_msg = None;
    }

    fn rebuild_status_lines(&mut self) {
        self.status_lines.clear();
        build_status_lines(
            &self.status_files,
            &self.open_files,
            &self.open_file_diffs,
            &self.section_folded,
            &mut self.status_lines,
        );
        if let Some(sel) = self.status_state.selected() {
            if sel >= self.status_lines.len() {
                self.status_state.select(Some(self.status_lines.len().saturating_sub(1)));
            }
        } else if !self.status_lines.is_empty() {
            self.status_state.select(Some(0));
        }
    }

    fn refresh_status(&mut self) {
        let status = self.repo.status();
        let head = self.repo.head_info();
        match (status, head) {
            (Ok(files), Ok(head)) => {
                self.head = head;
                self.status_files = files;
                self.open_files.clear();
                self.open_file_diffs.clear();
                self.section_folded = [false, false, false];
                self.rebuild_status_lines();
                if !self.status_lines.is_empty() {
                    self.status_state.select(Some(0));
                }
                self.clear_msg();
            }
            (Err(e), _) | (_, Err(e)) => self.set_msg(format!("refresh failed: {e}")),
        }
    }

    /// The file associated with the current cursor line (nearest File line at or above).
    fn current_file(&self) -> Option<&PathBuf> {
        let idx = self.status_state.selected()?;
        for i in (0..=idx).rev() {
            if let StatusLine::File { path, .. } = &self.status_lines[i] {
                return Some(path);
            }
        }
        None
    }

    /// The section header nearest at or above the cursor line.
    fn section_at_cursor(&self) -> Option<Section> {
        let idx = self.status_state.selected()?;
        for i in (0..=idx).rev() {
            if let StatusLine::SectionHeader { section, .. } = &self.status_lines[i] {
                return Some(*section);
            }
        }
        None
    }

    /// Whether the diff for a file should be staged (index vs HEAD) or worktree vs index.
    fn diff_is_staged_for(path: &PathBuf, files: &[FileItem]) -> bool {
        files.iter().find(|f| &f.path == path).map(|f| f.section == Section::Staged).unwrap_or(false)
    }

    // ---------- drawing ----------

    fn draw(&mut self, f: &mut Frame) {
        match self.view {
            View::Status => {
                ui::draw_status(
                    f,
                    f.size(),
                    &self.head,
                    &self.status_lines,
                    &mut self.status_state,
                    &self.open_files,
                    &self.section_folded,
                    self.status_msg.as_deref(),
                );
            }
            View::Diff => {
                ui::draw_diff(f, f.size(), &self.diff_title, &self.diff_text, self.diff_scroll);
            }
            View::Commit => {
                ui::draw_commit(f, f.size(), &self.commit_input, self.status_msg.as_deref());
            }
            View::Log => {
                let items: Vec<String> = self
                    .log_entries
                    .iter()
                    .map(|c| format!("{}  {}  {}", c.short_id, c.summary, c.author))
                    .collect();
                ui::draw_list_view(
                    f,
                    f.size(),
                    "commit log",
                    &items,
                    &mut self.log_state,
                    "j/k scroll · Enter/b back · q quit",
                );
            }
            View::Branch => {
                let mut items: Vec<String> = self
                    .branches
                    .iter()
                    .map(|b| {
                        if b.as_str() == self.head.branch && !self.head.detached {
                            format!("* {b}")
                        } else {
                            format!("  {b}")
                        }
                    })
                    .collect();
                if items.is_empty() {
                    items.push("(no branches)".to_string());
                }
                ui::draw_list_view(
                    f,
                    f.size(),
                    "branches",
                    &items,
                    &mut self.branch_state,
                    "j/k scroll · Enter checkout · q quit",
                );
            }
            View::Help => {
                ui::draw_help(f, f.size());
            }
        }
    }

    // ---------- input ----------

    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        match self.view {
            View::Status => self.handle_status_key(code, mods),
            View::Diff => self.handle_diff_key(code),
            View::Commit => self.handle_commit_key(code, mods),
            View::Log => self.handle_log_key(code),
            View::Branch => self.handle_branch_key(code),
            View::Help => self.handle_help_key(code),
        }
    }

    fn handle_status_key(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                let path = self.current_file().cloned();
                if let Some(path) = path {
                    if self.open_files.remove(&path) {
                        self.open_file_diffs.remove(&path);
                        self.rebuild_status_lines();
                        return;
                    }
                }
                self.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => self.next_line(),
            KeyCode::Char('k') | KeyCode::Up => self.prev_line(),
            KeyCode::Char('g') => self.status_state.select_first(),
            KeyCode::Char('G') => self.status_state.select_last(),
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_open_or_fold(),
            KeyCode::Char('s') => self.stage_selected(),
            KeyCode::Char('S') => self.stage_all_unstaged(),
            KeyCode::Char('u') => self.unstage_selected(),
            KeyCode::Char('U') => self.unstage_all_staged(),
            KeyCode::Char('a') => self.stage_all(),
            KeyCode::Char('x') => self.discard_selected(),
            KeyCode::Char('c') => self.open_commit(),
            KeyCode::Char('d') => self.open_diff(false),
            KeyCode::Char('D') => self.open_diff(true),
            KeyCode::Char('l') => self.open_log(),
            KeyCode::Char('b') => self.open_branch(),
            KeyCode::Char('h') | KeyCode::Char('?') => self.view = View::Help,
            KeyCode::Char('r') => self.refresh_status(),
            _ => {}
        }
    }

    fn handle_diff_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('b') | KeyCode::Enter | KeyCode::Left => {
                self.view = View::Status;
                self.diff_scroll = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.diff_scroll = self.diff_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.diff_scroll = self.diff_scroll.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn handle_commit_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.view = View::Status;
                self.clear_msg();
            }
            KeyCode::Char('c') => {
                let message = self.commit_input.value();
                let message = message.trim();
                if message.is_empty() {
                    self.set_msg("commit message is empty");
                    return;
                }
                match self.repo.commit(message) {
                    Ok(()) => {
                        self.commit_input.clear();
                        self.clear_msg();
                        self.view = View::Status;
                        self.refresh_status();
                        self.log_entries = self.repo.log().unwrap_or_default();
                        if !self.log_entries.is_empty() {
                            self.log_state.select(Some(0));
                        }
                    }
                    Err(e) => self.set_msg(format!("commit failed: {e}")),
                }
            }
            _ => self.commit_input.handle_key(KeyEvent::new(code, mods), self.terminal_size),
        }
    }

    fn handle_log_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('b') | KeyCode::Enter => {
                self.view = View::Status;
            }
            _ => {}
        }
    }

    fn handle_branch_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('b') | KeyCode::Enter => self.checkout_selected_branch(),
            KeyCode::Char('j') | KeyCode::Down => self.next_branch(),
            KeyCode::Char('k') | KeyCode::Up => self.prev_branch(),
            _ => {}
        }
    }

    fn handle_help_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('b') | KeyCode::Esc | KeyCode::Char('h') | KeyCode::Char('?') => {
                self.view = View::Status;
            }
            _ => {}
        }
    }

    // ---------- status navigation ----------

    fn next_line(&mut self) {
        let len = self.status_lines.len();
        if len == 0 {
            return;
        }
        let cur = self.status_state.selected().unwrap_or(0);
        let next = (cur + 1).min(len - 1);
        self.status_state.select(Some(next));
    }

    fn prev_line(&mut self) {
        let cur = self.status_state.selected().unwrap_or(0);
        if cur == 0 {
            return;
        }
        self.status_state.select(Some(cur - 1));
    }

    /// Toggle: section header -> fold/unfold section; file -> open/close inline diff.
    fn toggle_open_or_fold(&mut self) {
        let idx = match self.status_state.selected() {
            Some(i) => i,
            None => return,
        };
        // Snapshot what we need from the current line so we don't hold an
        // immutable borrow of self while mutating below.
        let section_idx = if let StatusLine::SectionHeader { section, .. } = &self.status_lines[idx] {
            Some(section.idx())
        } else {
            None
        };
        let file_path = if let StatusLine::File { path, .. } = &self.status_lines[idx] {
            Some(path.clone())
        } else {
            None
        };

        if let Some(s) = section_idx {
            self.section_folded[s] = !self.section_folded[s];
            self.rebuild_status_lines();
            self.status_state.select(Some(idx.min(self.status_lines.len() - 1)));
        } else if let Some(path) = file_path {
            if self.open_files.contains(&path) {
                self.open_files.remove(&path);
                self.open_file_diffs.remove(&path);
            } else {
                self.open_files.insert(path.clone());
                let staged = Self::diff_is_staged_for(&path, &self.status_files);
                match self.repo.diff(&path, staged) {
                    Ok(text) => {
                        self.open_file_diffs.insert(path, text);
                    }
                    Err(e) => {
                        self.set_msg(format!("diff failed: {e}"));
                    }
                }
            }
            self.rebuild_status_lines();
        }
    }

    // ---------- status actions ----------

    fn stage_selected(&mut self) {
        let path = match self.current_file().cloned() {
            Some(p) => p,
            None => {
                self.set_msg("no file selected");
                return;
            }
        };
        let section = self.section_at_cursor();
        match section {
            Some(Section::Unstaged) | Some(Section::Untracked) => {
                match self.repo.stage(&path) {
                    Ok(()) => {
                        self.clear_msg();
                        self.refresh_status();
                    }
                    Err(e) => self.set_msg(format!("stage failed: {e}")),
                }
            }
            _ => self.set_msg("file already staged or not stageable"),
        }
    }

    fn stage_all_unstaged(&mut self) {
        if let Err(e) = self.repo.stage_all() {
            self.set_msg(format!("stage all failed: {e}"));
            return;
        }
        self.clear_msg();
        self.refresh_status();
    }

    fn unstage_selected(&mut self) {
        let path = match self.current_file().cloned() {
            Some(p) => p,
            None => {
                self.set_msg("no file selected");
                return;
            }
        };
        if let Err(e) = self.repo.unstage(&path) {
            self.set_msg(format!("unstage failed: {e}"));
            return;
        }
        self.clear_msg();
        self.refresh_status();
    }

    fn unstage_all_staged(&mut self) {
        if let Err(e) = self.repo.unstage_all() {
            self.set_msg(format!("unstage all failed: {e}"));
            return;
        }
        self.clear_msg();
        self.refresh_status();
    }

    fn stage_all(&mut self) {
        if let Err(e) = self.repo.stage_all() {
            self.set_msg(format!("stage all failed: {e}"));
            return;
        }
        self.clear_msg();
        self.refresh_status();
    }

    fn discard_selected(&mut self) {
        let path = match self.current_file().cloned() {
            Some(p) => p,
            None => {
                self.set_msg("no file selected");
                return;
            }
        };
        let section = self.section_at_cursor();
        let result = match section {
            Some(Section::Untracked) => self.repo.remove_untracked(&path),
            _ => self.repo.discard(&path),
        };
        match result {
            Ok(()) => {
                self.clear_msg();
                self.refresh_status();
            }
            Err(e) => self.set_msg(format!("discard failed: {e}")),
        }
    }

    fn open_diff(&mut self, staged: bool) {
        let path = match self.current_file().cloned() {
            Some(p) => p,
            None => {
                self.set_msg("no file selected");
                return;
            }
        };
        match self.repo.diff(&path, staged) {
            Ok(text) => {
                self.diff_text = text;
                self.diff_title = if staged {
                    format!("staged diff · {}", path.display())
                } else {
                    format!("diff · {}", path.display())
                };
                self.diff_scroll = 0;
                self.view = View::Diff;
            }
            Err(e) => self.set_msg(format!("diff failed: {e}")),
        }
    }

    fn open_commit(&mut self) {
        self.view = View::Commit;
        self.clear_msg();
    }

    fn open_log(&mut self) {
        self.view = View::Log;
        if !self.log_entries.is_empty() {
            self.log_state.select(Some(0));
        }
    }

    fn open_branch(&mut self) {
        self.view = View::Branch;
        if !self.branches.is_empty() {
            self.branch_state.select(Some(0));
        }
    }

    // ---------- branches ----------

    fn next_branch(&mut self) {
        let len = self.branches.len();
        if len == 0 {
            return;
        }
        let cur = self.branch_state.selected().unwrap_or(0);
        let next = (cur + 1).min(len - 1);
        self.branch_state.select(Some(next));
    }

    fn prev_branch(&mut self) {
        let cur = self.branch_state.selected().unwrap_or(0);
        if cur == 0 {
            return;
        }
        self.branch_state.select(Some(cur - 1));
    }

    fn checkout_selected_branch(&mut self) {
        let idx = match self.branch_state.selected() {
            Some(i) => i,
            None => return,
        };
        let Some(line) = self.branches.get(idx) else { return };
        let branch = line.trim_start_matches("* ").trim();
        if branch == self.head.branch && !self.head.detached {
            self.set_msg(format!("already on branch {branch}"));
            self.view = View::Status;
            return;
        }
        match self.repo.checkout(branch) {
            Ok(()) => {
                self.set_msg(format!("switched to branch {branch}"));
                self.view = View::Status;
                self.refresh_status();
                self.branches = self.repo.branches().unwrap_or_default();
                if !self.branches.is_empty() {
                    self.branch_state.select(Some(0));
                }
            }
            Err(e) => self.set_msg(format!("checkout failed: {e}")),
        }
    }
}

/// Build the flat list of rendered status lines from the file model.
fn build_status_lines(
    files: &[FileItem],
    open_files: &HashSet<PathBuf>,
    open_file_diffs: &HashMap<PathBuf, String>,
    section_folded: &[bool; 3],
    out: &mut Vec<StatusLine>,
) {
    let mut by_section: Vec<Vec<&FileItem>> = vec![Vec::new(), Vec::new(), Vec::new()];
    for f in files {
        by_section[f.section.idx()].push(f);
    }

    for (section_idx, section_files) in by_section.iter().enumerate() {
        if section_files.is_empty() {
            continue;
        }
        let section = match section_idx {
            0 => Section::Staged,
            1 => Section::Unstaged,
            _ => Section::Untracked,
        };
        let folded = section_folded[section_idx];
        out.push(StatusLine::SectionHeader {
            section,
            title: section.title().to_string(),
            count: section_files.len(),
        });
        if folded {
            continue;
        }
        for f in section_files {
            out.push(StatusLine::File {
                path: f.path.clone(),
                mode: f.mode,
                section,
            });
            if open_files.contains(&f.path) {
                if let Some(diff) = open_file_diffs.get(&f.path) {
                    for line in diff.lines() {
                        out.push(StatusLine::Diff(line.to_string()));
                    }
                }
            }
        }
        out.push(StatusLine::Blank);
    }
}
