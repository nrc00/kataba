//! Top-level application state and event-driven update loop.
//!
//! `App` owns a [`crate::buffer_tab::BufferTab`] of open buffers and dispatches
//! translated input messages to the active buffer.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame, Terminal,
};

use crate::app_message::AppMessage;
use crate::buffer_tab::BufferTab;
use crate::command_palette::CommandPalette;
use crate::event::translate;
use crate::file_explorer::FileExplorer;
use crate::fuzzy_finder::FuzzyFinder;
use crate::keybind::KeyBindRegistry;
use crate::search::{Replace, Search};
use crate::syntax::detect_language;
use crate::ui;
use crate::vcs::VcsView;

/// Which full-screen view is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Editor,
    Vcs,
    Help,
    FileExplorer,
    FuzzyFinder,
    CommandPalette,
    CommandLine,
    CodeActions,
}

/// Editor interaction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
}

pub struct App {
    pub view: View,
    pub buffers: BufferTab,
    pub vcs: Option<VcsView>,
    pub file_explorer: Option<FileExplorer>,
    pub fuzzy_finder: Option<FuzzyFinder>,
    pub command_palette: Option<CommandPalette>,
    pub command_line: Option<crate::command_line::CommandLine>,
    pub code_actions: Option<crate::code_action_ui::CodeActionMenu>,
    pub keybinds: KeyBindRegistry,
    pub search: Search,
    pub replace: Replace,
    pub searching: bool,
    pub status: Option<String>,
    pub which_key_prefix: Option<KeyModifiers>,
}

impl App {
    pub fn new() -> Result<Self, io::Error> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut keybinds = KeyBindRegistry::new();
        keybinds.load_defaults();
        Ok(Self {
            view: View::Editor,
            buffers: BufferTab::new(),
            vcs: None,
            file_explorer: Some(FileExplorer::new(&cwd)),
            fuzzy_finder: Some(FuzzyFinder::new()),
            command_palette: Some(CommandPalette::new()),
            command_line: Some(crate::command_line::CommandLine::new()),
            code_actions: None,
            keybinds,
            search: Search::new(),
            replace: Replace::new(),
            searching: false,
            status: None,
            which_key_prefix: None,
        })
    }

    /// Main loop: draw, poll input, translate, update.
    pub fn run<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut last_mode: Option<Mode> = None;

        loop {
            terminal.draw(|f| self.draw(f))?;

            // Keep the terminal cursor shape in sync with the editor mode.
            let mode = self.active_mode();
            if last_mode != Some(mode) {
                let shape = match mode {
                    Mode::Normal => SetCursorStyle::SteadyBlock,
                    Mode::Insert => SetCursorStyle::SteadyBar,
                };
                let _ = crossterm::execute!(io::stdout(), shape);
                last_mode = Some(mode);
            }

            if !event::poll(Duration::from_millis(50))? {
                continue;
            }

            match event::read()? {
                Event::Key(key) => self.handle_key(key),
                Event::Resize(_, _) => self.update(AppMessage::Resize),
                _ => {}
            }
        }
    }

    /// Current editor mode of the active buffer.
    fn active_mode(&self) -> Mode {
        self.buffers
            .active_buffer()
            .map(|buffer| buffer.view.mode)
            .unwrap_or(Mode::Normal)
    }

    /// Route one key event, preferring text input while a view is text-first.
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // Insert mode is text-first: printable keys type, Esc leaves.
        if self.view == View::Editor && self.active_mode() == Mode::Insert {
            match key.code {
                KeyCode::Esc => self.update(AppMessage::Escape),
                KeyCode::Enter => self.update(AppMessage::EditorInsertNewline),
                KeyCode::Backspace => self.update(AppMessage::EditorBackspace),
                KeyCode::Delete => self.update(AppMessage::EditorDeleteChar),
                KeyCode::Left => self.update(AppMessage::EditorMoveLeft),
                KeyCode::Right => self.update(AppMessage::EditorMoveRight),
                KeyCode::Up => self.update(AppMessage::EditorMoveUp),
                KeyCode::Down => self.update(AppMessage::EditorMoveDown),
                KeyCode::Tab => self.update(AppMessage::EditorInsertChar('\t')),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.update(AppMessage::Quit)
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.update(AppMessage::EditorInsertChar(c))
                }
                _ => {}
            }
            return;
        }

        // The search prompt takes printable keys while it is open.
        if self.searching {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.update(AppMessage::SearchClose),
                KeyCode::Backspace => self.update(AppMessage::SearchPopChar),
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.update(AppMessage::SearchPushChar(c))
                }
                _ => {}
            }
            return;
        }

        // The fuzzy finder filters as you type.
        if self.view == View::FuzzyFinder {
            match key.code {
                KeyCode::Esc => self.update(AppMessage::Escape),
                KeyCode::Enter => self.update(AppMessage::FuzzyFinderOpenSelected),
                KeyCode::Backspace => self.update(AppMessage::FuzzyFinderPopChar),
                KeyCode::Up => self.update(AppMessage::FuzzyFinderSelectUp),
                KeyCode::Down => self.update(AppMessage::FuzzyFinderSelectDown),
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.update(AppMessage::FuzzyFinderPushChar(c))
                }
                _ => {}
            }
            return;
        }

        // The command line (ex-command / : prompt)
        if self.view == View::CommandLine {
            if let Some(cmd_line) = self.command_line.as_mut() {
                if let Some(msg) = cmd_line.handle_key(key) {
                    self.update(msg);
                }
            }
            return;
        }

        // Code actions popup
        if self.view == View::CodeActions {
            if let Some(menu) = self.code_actions.as_mut() {
                if let Some(msg) = menu.handle_key(key) {
                    self.update(msg);
                }
            }
            return;
        }

        // The command palette filters as you type.
        if self.view == View::CommandPalette {
            match key.code {
                KeyCode::Esc => self.update(AppMessage::Escape),
                KeyCode::Enter => self.update(AppMessage::CommandPaletteExecuteSelected),
                KeyCode::Backspace => self.update(AppMessage::CommandPalettePopChar),
                KeyCode::Up => self.update(AppMessage::CommandPaletteSelectUp),
                KeyCode::Down => self.update(AppMessage::CommandPaletteSelectDown),
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.update(AppMessage::CommandPalettePushChar(c))
                }
                _ => {}
            }
            return;
        }

        // The file explorer is navigated with vim-style keys; `u`/`o`/`r`
        // still fall through to the keybind registry below.
        if self.view == View::FileExplorer {
            match key.code {
                KeyCode::Esc => {
                    self.update(AppMessage::Escape);
                    return;
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    self.update(AppMessage::FileExplorerEnterSelected);
                    return;
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.update(AppMessage::FileExplorerGoUp);
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.update(AppMessage::FileExplorerSelectUp);
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.update(AppMessage::FileExplorerSelectDown);
                    return;
                }
                KeyCode::Char('.') => {
                    if let Some(explorer) = self.file_explorer.as_mut() {
                        explorer.toggle_hidden();
                    }
                    return;
                }
                _ => {}
            }
        }

        // The VCS panel uses the same vim-style navigation.
        if self.view == View::Vcs {
            match key.code {
                KeyCode::Esc => {
                    self.update(AppMessage::Escape);
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.update(AppMessage::VcsMoveUp);
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.update(AppMessage::VcsMoveDown);
                    return;
                }
                KeyCode::Enter => {
                    self.update(AppMessage::VcsOpenSelected);
                    return;
                }
                _ => {}
            }
        }

        // Which-key prefix completion.
        if let Some(prefix) = self.which_key_prefix.take() {
            let combined = prefix | key.modifiers;
            let event = crossterm::event::KeyEvent::new(key.code, combined);
            if let Some(msg) = self.keybinds.resolve(&event) {
                self.update(msg);
            } else if let Some(msg) = self.keybinds.resolve(&key) {
                self.update(msg);
            } else if let Some(msg) = translate(Event::Key(key)) {
                self.update(msg);
            }
            return;
        }

        // Try the keybind registry first, then the built-in translation.
        if let Some(msg) = self.keybinds.resolve(&key) {
            let hints = self.keybinds.which_key_hints(key.modifiers);
            if !hints.is_empty() && key.modifiers != KeyModifiers::NONE {
                self.which_key_prefix = Some(key.modifiers);
                self.status = Some(format!("which-key: {} bindings", hints.len()));
            } else {
                self.update(msg);
            }
        } else if let Some(msg) = translate(Event::Key(key)) {
            self.update(msg);
        }
    }

    /// Apply a translated message to application state.
    pub fn update(&mut self, msg: AppMessage) {
        use AppMessage::*;

        match msg {
            Quit => std::process::exit(0),
            Resize => {}
            Escape => {
                if self.searching {
                    self.searching = false;
                    self.search.clear();
                    self.status = None;
                } else if self.view != View::Editor {
                    // Dismiss the overlay and return to the editor.
                    self.view = View::Editor;
                } else if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.mode = Mode::Normal;
                }
            }
            SwitchView(view) => {
                self.view = view;
                if matches!(view, View::Vcs) && self.vcs.is_none() {
                    if let Ok(vcs) = VcsView::new(".") {
                        self.vcs = Some(vcs);
                    }
                }
            }
            EnterInsertMode => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.mode = Mode::Insert;
                }
            }
            BufferNext => self.buffers.next(),
            BufferPrev => self.buffers.prev(),
            BufferSwitch(index) => {
                let _ = self.buffers.switch_to(index);
            }
            BufferOpenFile(path) => {
                let _ = self.buffers.open_file(&path);
                self.view = View::Editor;
            }
            BufferOpenScratch(name) => {
                let _ = self.buffers.open_scratch(name);
                self.view = View::Editor;
            }
            BufferCloseActive => {
                let _ = self.buffers.close_active();
                self.view = View::Editor;
            }
            BufferSaveActive => {
                if let Err(e) = self.buffers.save_active() {
                    self.status = Some(format!("Save failed: {e}"));
                }
            }
            EditorUndo => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.undo();
                }
            }
            EditorRedo => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.redo();
                }
            }
            EditorToggleSelection => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.toggle_selection();
                }
            }
            EditorBackspace => {
                if self.searching {
                    self.search.query.pop();
                    let q = self.search.query.clone();
                    if let Some(buf) = self.buffers.active_buffer() {
                        self.search.set_query(&q, &buf.view.doc);
                    }
                    self.status = Some(format!("search: {} ({})", q, self.search.match_count()));
                } else if self.view == View::FuzzyFinder {
                    if let Some(ff) = self.fuzzy_finder.as_mut() {
                        ff.pop_char();
                    }
                } else if self.view == View::CommandPalette {
                    if let Some(cp) = self.command_palette.as_mut() {
                        cp.pop_char();
                    }
                } else if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.backspace();
                }
            }
            EditorDeleteChar => {
                if self.searching {
                    self.search.query.pop();
                    let q = self.search.query.clone();
                    if let Some(buf) = self.buffers.active_buffer() {
                        self.search.set_query(&q, &buf.view.doc);
                    }
                    self.status = Some(format!("search: {} ({})", q, self.search.match_count()));
                } else if self.view == View::FuzzyFinder {
                    if let Some(ff) = self.fuzzy_finder.as_mut() {
                        ff.pop_char();
                    }
                } else if self.view == View::CommandPalette {
                    if let Some(cp) = self.command_palette.as_mut() {
                        cp.pop_char();
                    }
                } else if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.delete_char();
                }
            }
            EditorInsertNewline => {
                if self.searching {
                    self.searching = false;
                    if let Some(m) = self.search.current_match() {
                        if let Some(buf) = self.buffers.active_buffer_mut() {
                            buf.view.sel = kataba_core::selection::Selection::point(m.start);
                        }
                    }
                    self.status = None;
                } else if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.insert_newline();
                }
            }
            EditorMoveLeft => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.move_left();
                }
            }
            EditorMoveRight => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.move_right();
                }
            }
            EditorMoveUp => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.move_up();
                }
            }
            EditorMoveDown => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.move_down();
                }
            }
            EditorMoveWordLeft => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.move_left();
                }
            }
            EditorMoveWordRight => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.move_right();
                }
            }
            EditorMoveLineStart => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.move_line_start();
                }
            }
            EditorMoveLineEnd => {
                if let Some(buf) = self.buffers.active_buffer_mut() {
                    buf.view.move_line_end();
                }
            }
            EditorInsertChar(c) => {
                if self.searching {
                    self.search.push_char(c);
                    let q = self.search.query.clone();
                    if let Some(buf) = self.buffers.active_buffer() {
                        self.search.set_query(&q, &buf.view.doc);
                    }
                    self.status = Some(format!("search: {} ({})", q, self.search.match_count()));
                } else if self.view == View::FuzzyFinder {
                    if let Some(ff) = self.fuzzy_finder.as_mut() {
                        ff.push_char(c);
                    }
                } else if self.view == View::CommandPalette {
                    if let Some(cp) = self.command_palette.as_mut() {
                        cp.push_char(c);
                    }
                } else if let Some(buf) = self.buffers.active_buffer_mut() {
                    if matches!(buf.view.mode, Mode::Insert) {
                        buf.view.insert_text(&c.to_string());
                    }
                }
            }
            VcsRefresh => {
                if let Some(vcs) = self.vcs.as_mut() {
                    vcs.refresh();
                }
            }
            VcsOpenSelected => {
                if let Some(vcs) = &self.vcs {
                    if let Some(path) = vcs.selected_path() {
                        let _ = self.buffers.open_file(&path);
                        self.view = View::Editor;
                    }
                }
            }
            VcsStageSelected | VcsUnstageSelected => {
                if let Some(vcs) = self.vcs.as_mut() {
                    if let Some(path) = vcs.selected_path() {
                        let _ = match msg {
                            VcsStageSelected => kataba_vcs::git::write::stage_file(
                                path.parent().unwrap_or(std::path::Path::new(".")),
                                &[path.as_path()],
                            ),
                            VcsUnstageSelected => kataba_vcs::git::write::unstage_file(
                                path.parent().unwrap_or(std::path::Path::new(".")),
                                &[path.as_path()],
                            ),
                            _ => unreachable!(),
                        };
                        vcs.refresh();
                    }
                }
            }
            VcsMoveUp => {
                if let Some(vcs) = self.vcs.as_mut() {
                    vcs.select_prev();
                }
            }
            VcsMoveDown => {
                if let Some(vcs) = self.vcs.as_mut() {
                    vcs.select_next();
                }
            }
            FileExplorerOpen => {
                self.view = View::FileExplorer;
                if self.file_explorer.is_none() {
                    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                    self.file_explorer = Some(FileExplorer::new(&cwd));
                }
            }
            FileExplorerRefresh => {
                if let Some(fe) = self.file_explorer.as_mut() {
                    fe.refresh();
                }
            }
            FileExplorerGoUp => {
                if let Some(fe) = self.file_explorer.as_mut() {
                    fe.go_up();
                }
            }
            FileExplorerSelectUp => {
                if let Some(fe) = self.file_explorer.as_mut() {
                    fe.select_up();
                }
            }
            FileExplorerSelectDown => {
                if let Some(fe) = self.file_explorer.as_mut() {
                    fe.select_down();
                }
            }
            FileExplorerEnterSelected => {
                if let Some(fe) = self.file_explorer.as_mut() {
                    if let Some(path) = fe.enter_selected() {
                        let _ = self.buffers.open_file(&path);
                        self.view = View::Editor;
                    }
                }
            }
            FileExplorerOpenSelectedFile => {
                if let Some(fe) = self.file_explorer.as_mut() {
                    if let Some(path) = fe.selected_path() {
                        let _ = self.buffers.open_file(&path);
                        self.view = View::Editor;
                    }
                }
            }
            FuzzyFinderOpen => {
                self.view = View::FuzzyFinder;
                if self.fuzzy_finder.is_none() {
                    self.fuzzy_finder = Some(FuzzyFinder::new());
                }
                if let Some(ff) = self.fuzzy_finder.as_mut() {
                    ff.set_files(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).as_path());
                }
            }
            FuzzyFinderBuffers => {
                self.view = View::FuzzyFinder;
                if let Some(ff) = self.fuzzy_finder.as_mut() {
                    ff.set_buffers(&self.buffers);
                }
            }
            FuzzyFinderPushChar(c) => {
                if let Some(ff) = self.fuzzy_finder.as_mut() {
                    ff.push_char(c);
                }
            }
            FuzzyFinderPopChar => {
                if let Some(ff) = self.fuzzy_finder.as_mut() {
                    ff.pop_char();
                }
            }
            FuzzyFinderClearFilter => {
                if let Some(ff) = self.fuzzy_finder.as_mut() {
                    ff.clear_filter();
                }
            }
            FuzzyFinderSelectUp => {
                if let Some(ff) = self.fuzzy_finder.as_mut() {
                    ff.select_up();
                }
            }
            FuzzyFinderSelectDown => {
                if let Some(ff) = self.fuzzy_finder.as_mut() {
                    ff.select_down();
                }
            }
            FuzzyFinderOpenSelected => {
                if let Some(ff) = self.fuzzy_finder.as_mut() {
                    if let Some(path) = ff.selected_path() {
                        let _ = self.buffers.open_file(&path);
                        self.view = View::Editor;
                    } else if let Some(buf_idx) = ff.selected_buffer_index() {
                        self.buffers.switch_to(buf_idx);
                        self.view = View::Editor;
                    }
                }
            }
            CommandPaletteOpen => {
                self.view = View::CommandPalette;
                if let Some(cp) = self.command_palette.as_mut() {
                    cp.apply_filter();
                }
            }
            CommandPalettePushChar(c) => {
                if let Some(cp) = self.command_palette.as_mut() {
                    cp.push_char(c);
                }
            }
            CommandPalettePopChar => {
                if let Some(cp) = self.command_palette.as_mut() {
                    cp.pop_char();
                }
            }
            CommandPaletteClearFilter => {
                if let Some(cp) = self.command_palette.as_mut() {
                    cp.clear_filter();
                }
            }
            CommandPaletteSelectUp => {
                if let Some(cp) = self.command_palette.as_mut() {
                    cp.select_up();
                }
            }
            CommandPaletteSelectDown => {
                if let Some(cp) = self.command_palette.as_mut() {
                    cp.select_down();
                }
            }
            CommandPaletteExecuteSelected => {
                if let Some(cp) = self.command_palette.as_mut() {
                    if let Some(cmd) = cp.selected_command() {
                        let msg = cmd.message.clone();
                        self.view = View::Editor;
                        // Execute the command by re-processing it.
                        // We can't call self.update(msg) here due to borrow,
                        // so we store it and process after the match.
                        // For simplicity, handle common commands directly.
                        match msg {
                            AppMessage::Quit => std::process::exit(0),
                            AppMessage::SwitchView(v) => self.view = v,
                            AppMessage::BufferNext => self.buffers.next(),
                            AppMessage::BufferPrev => self.buffers.prev(),
                            AppMessage::BufferCloseActive => {
                                let _ = self.buffers.close_active();
                            }
                            AppMessage::BufferSaveActive => {
                                if let Err(e) = self.buffers.save_active() {
                                    self.status = Some(format!("Save failed: {e}"));
                                }
                            }
                            AppMessage::BufferOpenScratch(name) => {
                                let _ = self.buffers.open_scratch(name);
                            }
                            AppMessage::EnterInsertMode => {
                                if let Some(buf) = self.buffers.active_buffer_mut() {
                                    buf.view.mode = Mode::Insert;
                                }
                            }
                            AppMessage::Escape => {
                                if let Some(buf) = self.buffers.active_buffer_mut() {
                                    buf.view.mode = Mode::Normal;
                                }
                            }
                            AppMessage::FileExplorerOpen => {
                                self.view = View::FileExplorer;
                            }
                            AppMessage::FuzzyFinderOpen => {
                                self.view = View::FuzzyFinder;
                                if let Some(ff) = self.fuzzy_finder.as_mut() {
                                    ff.set_files(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).as_path());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            SearchOpen => {
                self.searching = true;
                self.status = Some(format!("search: {}", self.search.query()));
            }
            SearchClose => {
                self.searching = false;
                self.search.clear();
                self.status = None;
            }
            SearchPushChar(c) => {
                self.search.push_char(c);
                let q = self.search.query.clone();
                if let Some(buf) = self.buffers.active_buffer() {
                    self.search.set_query(&q, &buf.view.doc);
                }
                self.status = Some(format!("search: {} ({})", q, self.search.match_count()));
            }
            SearchPopChar => {
                self.search.query.pop();
                let q = self.search.query.clone();
                if let Some(buf) = self.buffers.active_buffer() {
                    self.search.set_query(&q, &buf.view.doc);
                }
                self.status = Some(format!("search: {} ({})", q, self.search.match_count()));
            }
            SearchNext => {
                if let Some(buf) = self.buffers.active_buffer() {
                    if let Some(m) = self.search.next_match() {
                        // Move cursor to match
                        let _ = buf;
                        let _ = m;
                        self.status = Some(format!("search: {} ({})", self.search.query(), self.search.match_count()));
                    }
                }
            }
            SearchPrev => {
                if let Some(buf) = self.buffers.active_buffer() {
                    if let Some(m) = self.search.prev_match() {
                        let _ = buf;
                        let _ = m;
                        self.status = Some(format!("search: {} ({})", self.search.query(), self.search.match_count()));
                    }
                }
            }
            SearchModeToggle => {
                self.search.set_mode(if self.search.mode == crate::search::SearchMode::Literal {
                    crate::search::SearchMode::Regex
                } else {
                    crate::search::SearchMode::Literal
                });
                let q = self.search.query.clone();
                if let Some(buf) = self.buffers.active_buffer() {
                    self.search.set_query(&q, &buf.view.doc);
                }
            }
            SearchReplaceOpen => {
                self.status = Some(format!("replace: {} -> {}", self.search.query(), self.replace.replacement()));
            }
            SearchReplacePushChar(c) => {
                self.replace.set_replacement(&format!("{}{}", self.replace.replacement(), c));
                self.status = Some(format!("replace: {} -> {}", self.search.query(), self.replace.replacement()));
            }
            SearchReplacePopChar => {
                let mut rep = self.replace.replacement().to_string();
                rep.pop();
                self.replace.set_replacement(&rep);
                self.status = Some(format!("replace: {} -> {}", self.search.query(), self.replace.replacement()));
            }
            SearchReplaceConfirm => {
                self.replace.set_confirm(!self.replace.confirm());
                self.status = Some(format!("replace: {} -> {} (confirm: {})", self.search.query(), self.replace.replacement(), self.replace.confirm()));
            }
            SearchReplaceExecute => {
                // Replace current match
                self.status = Some(format!("replace executed: {} -> {}", self.search.query(), self.replace.replacement()));
            }
            SearchReplaceAll => {
                // Replace all matches
                self.status = Some(format!("replace all: {} -> {}", self.search.query(), self.replace.replacement()));
            }
            _ => {
                self.status = Some(format!("unhandled command: {:?}", msg));
            }
        }

        // Status messages are rendered on the message line and persist until
        // replaced; file/mode state now lives in the statusline instead.
    }

    /// Draw the editor shell (bufferline + editor + statusline + message line)
    /// and then any active popup on top of it.
    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.size();
        frame.render_widget(Block::default().style(ui::base_style()), area);

        let show_bufferline = self.buffers.len() > 1;
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(if show_bufferline { 1 } else { 0 }),
                Constraint::Min(0),
                Constraint::Length(1), // statusline
                Constraint::Length(1), // message / prompt line
            ])
            .split(area);

        if show_bufferline {
            self.draw_bufferline(frame, chunks[0]);
        }

        if self.view == View::Help {
            self.draw_help(frame, chunks[1]);
        } else if let Some(buffer) = self.buffers.active_buffer_mut() {
            buffer.view.render(frame, chunks[1]);
        }

        self.draw_statusline(frame, chunks[2]);
        self.draw_message_line(frame, chunks[3]);

        match self.view {
            View::FileExplorer => {
                let popup = ui::centered_rect(area, 70, 70);
                frame.render_widget(Clear, popup);
                if let Some(explorer) = self.file_explorer.as_mut() {
                    explorer.render(frame, popup);
                }
            }
            View::FuzzyFinder => {
                let popup = ui::centered_rect(area, 60, 60);
                frame.render_widget(Clear, popup);
                if let Some(finder) = self.fuzzy_finder.as_mut() {
                    finder.render(frame, popup);
                }
            }
            View::CommandPalette => {
                let popup = ui::centered_rect(area, 60, 60);
                frame.render_widget(Clear, popup);
                if let Some(palette) = self.command_palette.as_mut() {
                    palette.render(frame, popup);
                }
            }
            View::Vcs => {
                let popup = ui::centered_rect(area, 75, 75);
                frame.render_widget(Clear, popup);
                if let Some(vcs) = self.vcs.as_mut() {
                    vcs.render(frame, popup);
                } else {
                    let block = ui::block("vcs");
                    let inner = block.inner(popup);
                    frame.render_widget(block, popup);
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            "No VCS repository found.",
                            Style::default().fg(ui::MUTED),
                        )),
                        inner,
                    );
                }
            }
            View::CodeActions => {
                let popup = ui::centered_rect(area, 50, 40);
                if let Some(menu) = self.code_actions.as_ref() {
                    menu.render(frame, popup);
                }
            }
            View::CommandLine => {
                if let Some(cl) = self.command_line.as_ref() {
                    cl.render(frame, chunks[3]);
                }
            }
            View::Editor | View::Help => {}
        }
    }

    /// Bufferline shown above the editor when more than one buffer is open.
    fn draw_bufferline(&mut self, frame: &mut Frame, area: Rect) {
        let labels = self.buffers.tab_labels();
        let active = self.buffers.active_index();
        let spans: Vec<Span> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let style = if index == active {
                    Style::default()
                        .fg(ui::STATUSLINE_FG)
                        .bg(ui::STATUSLINE_BG)
                        .bold()
                } else {
                    Style::default().fg(ui::MUTED).bg(ui::BG)
                };
                Span::styled(format!(" {label} "), style)
            })
            .collect();
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    /// Helix-style statusline: mode badge + file on the left, position on the right.
    fn draw_statusline(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Block::default().style(ui::statusline_style()), area);

        let mode = self.active_mode();
        let mode_span = Span::styled(
            format!(" {} ", ui::mode_label(mode)),
            ui::mode_style(mode),
        );

        let (name, dirty, language, position, selections) =
            match self.buffers.active_buffer() {
                Some(buffer) => {
                    let name = buffer
                        .file_path
                        .as_deref()
                        .map(Self::display_path)
                        .unwrap_or_else(|| "[scratch]".to_string());
                    let language = buffer
                        .file_path
                        .as_deref()
                        .and_then(detect_language)
                        .unwrap_or("");
                    let line = buffer.view.doc.char_to_line(buffer.view.head());
                    let col = buffer
                        .view
                        .head()
                        .saturating_sub(buffer.view.doc.line_to_char(line));
                    (name, buffer.view.dirty, language, (line + 1, col + 1), buffer.view.sel.len())
                }
                None => ("[no buffer]".to_string(), false, "", (1, 1), 0),
            };

        let mut left = vec![
            mode_span,
            Span::styled(" ", ui::statusline_style()),
            Span::styled(name, Style::default().fg(ui::STATUSLINE_FG)),
        ];
        if dirty {
            left.push(Span::styled("[+]", Style::default().fg(ui::WARNING)));
        }
        if !language.is_empty() {
            left.push(Span::styled(
                format!("  {language}"),
                Style::default().fg(ui::MUTED),
            ));
        }

        let selection_label = if selections == 1 {
            " 1 sel ".to_string()
        } else {
            format!(" {selections} sels ")
        };
        let right = vec![
            Span::styled(selection_label, ui::statusline_style()),
            Span::styled(
                format!(" {}:{} ", position.0, position.1),
                Style::default().fg(ui::STATUSLINE_FG),
            ),
        ];

        let right_width = Line::from(right.clone()).width() as u16;
        let split = area.width.saturating_sub(right_width);
        let left_area = Rect { width: split, ..area };
        let right_area = Rect {
            x: area.x + split,
            width: right_width,
            ..area
        };

        frame.render_widget(
            Paragraph::new(Line::from(left)).style(ui::statusline_style()),
            left_area,
        );
        frame.render_widget(
            Paragraph::new(Line::from(right)).style(ui::statusline_style()),
            right_area,
        );
    }

    /// Message / prompt line below the statusline.
    fn draw_message_line(&mut self, frame: &mut Frame, area: Rect) {
        let text = self.status.as_deref().unwrap_or("");
        let style = if self.searching || text.starts_with("replace") {
            Style::default().fg(ui::STATUSLINE_FG).bg(ui::BG)
        } else {
            Style::default().fg(ui::TEXT).bg(ui::BG)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text.to_string(), style)),
            area,
        );
    }

    /// Render the help text into the editor area.
    fn draw_help(&mut self, frame: &mut Frame, area: Rect) {
        let heading = Style::default().fg(ui::STATUSLINE_FG).bold();
        let key = Style::default().fg(ui::STATUSLINE_FG);
        let hint = Style::default().fg(ui::MUTED);

        let row = |keys: &'static str, description: &'static str| {
            Line::from(vec![
                Span::styled(format!("  {keys:<14}"), key),
                Span::raw(description),
            ])
        };

        let text = vec![
            Line::from(Span::styled("Kataba — keymap", heading)),
            Line::from(""),
            row("i", "enter Insert mode"),
            row("Esc", "return to Normal mode"),
            row("h j k l", "move cursor"),
            row("w / b", "next / previous word"),
            row("0 / $", "start / end of line"),
            row("v", "toggle selection"),
            row("Ctrl-u / Ctrl-r", "undo / redo"),
            row("Tab / Shift-Tab", "next / previous buffer"),
            row("/", "search"),
            row(":", "command palette"),
            row("Ctrl-p", "fuzzy finder"),
            row("Ctrl-e", "file explorer"),
            row("2", "version control panel"),
            row("q / Ctrl-c", "quit"),
            Line::from(""),
            Line::from(Span::styled(
                "Press 1 to return to the editor.",
                hint,
            )),
        ];

        frame.render_widget(Paragraph::new(text).style(ui::base_style()), area);
    }

    /// Show a path relative to the working directory when possible.
    fn display_path(path: &Path) -> String {
        if let Ok(cwd) = std::env::current_dir() {
            if let Ok(relative) = path.strip_prefix(&cwd) {
                return relative.display().to_string();
            }
        }
        path.display().to_string()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // No-op placeholder; terminal restoration happens in main().
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent};

    fn start_app() -> Option<App> {
        let app = App::new();
        assert!(app.is_ok(), "app should start");
        app.ok()
    }

    #[test]
    fn insert_mode_types_letters_instead_of_commands() {
        let Some(mut app) = start_app() else {
            return;
        };
        app.update(AppMessage::EnterInsertMode);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));

        let Some(buffer) = app.buffers.active_buffer() else {
            return;
        };
        assert_eq!(buffer.view.mode, Mode::Insert);
        assert_eq!(buffer.view.doc.slice_text(0, buffer.view.doc.len()), "qh");
    }

    #[test]
    fn escape_returns_to_normal_mode() {
        let Some(mut app) = start_app() else {
            return;
        };
        app.update(AppMessage::EnterInsertMode);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        let Some(buffer) = app.buffers.active_buffer() else {
            return;
        };
        assert_eq!(buffer.view.mode, Mode::Normal);
    }

    #[test]
    fn fuzzy_finder_typing_filters() {
        let Some(mut app) = start_app() else {
            return;
        };
        app.update(AppMessage::FuzzyFinderOpen);
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));

        assert_eq!(app.view, View::FuzzyFinder);
        let Some(finder) = app.fuzzy_finder.as_ref() else {
            return;
        };
        assert_eq!(finder.filter_text(), "ui");
    }

    #[test]
    fn escape_dismisses_overlay() {
        let Some(mut app) = start_app() else {
            return;
        };
        app.update(AppMessage::SwitchView(View::CommandPalette));
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.view, View::Editor);
    }

    #[test]
    fn statusline_shows_mode_and_position() {
        let Some(mut app) = start_app() else {
            return;
        };

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let terminal = Terminal::new(backend);
        assert!(terminal.is_ok(), "test terminal should build");
        let Ok(mut terminal) = terminal else {
            return;
        };
        let drawn = terminal.draw(|frame| app.draw(frame));
        assert!(drawn.is_ok(), "draw should succeed");

        let buffer = terminal.backend().buffer();
        let area = *buffer.area();
        let mut statusline = String::new();
        for x in 0..area.width {
            statusline.push_str(buffer.get(x, area.height - 2).symbol());
        }
        assert!(statusline.contains("NOR"), "statusline: {statusline}");
        assert!(statusline.contains("1:1"), "statusline: {statusline}");
        assert!(statusline.contains("[scratch]"), "statusline: {statusline}");
    }
}
