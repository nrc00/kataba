//! `kataba-term`: a ratatui terminal frontend for the Kataba editor stack.
//!
//! Entry point, shared UI state, and the editor/VCS/help views.
//!
//! Zero `unsafe`; every failure path is handled rather than panicked.

pub mod app;
pub mod app_message;
pub mod buffer;
pub mod buffer_tab;
pub mod code_action_ui;
pub mod command_line;
pub mod command_palette;
pub mod completion;
pub mod diagnostics_gutter;
pub mod editor;
pub mod event;
pub mod file_explorer;
pub mod fuzzy_finder;
pub mod keybind;
pub mod language_keymaps;
pub mod search;
pub mod session;
pub mod syntax;
pub mod ui;
pub mod vcs;

pub use app::{App, Mode, View};
pub use app_message::AppMessage;
pub use code_action_ui::{CodeActionItem, CodeActionMenu};
pub use command_line::CommandLine;
pub use command_palette::CommandPalette;
pub use completion::{CompletionBackend, CompletionItem, CompletionKind, CompletionSource};
pub use diagnostics_gutter::{DiagnosticSeverity, DiagnosticsGutter, LineDiagnostic};
pub use file_explorer::FileExplorer;
pub use fuzzy_finder::FuzzyFinder;
pub use keybind::KeyBindRegistry;
pub use language_keymaps::LanguageKeymapRegistry;
pub use search::{Replace, Search};
pub use session::{BufferSessionRecord, Session};
pub use syntax::{capture_style, HighlightSpan, SyntaxCache, SyntaxState};
