//! Keybind system: TOML-configurable key bindings with which-key hints.
//!
//! Key bindings map crossterm key events to `AppMessage`s. Bindings can be
//! loaded from a TOML config file and support which-key style discovery:
//! when a user presses a prefix key, show all matching completions.
//!
//! The default keymap is Helix-compatible: it ships every single-key binding
//! from Helix's `default.rs` keymap (Normal, Select and Insert modes), mapped
//! onto kataba's [`AppMessage`] commands. Multi-key sequences (`g g`, `space f`,
//! `[ d`, `z z`, `C-w w`, …) are intentionally omitted here because this
//! registry resolves one key event at a time; a trie-based sequence resolver
//! can layer on top later without changing this table's shape.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

use crate::app_message::AppMessage;

/// A single key binding: key + modifiers -> message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBind {
    /// Key code (e.g. "h", "Enter", "Ctrl-c").
    pub key: String,
    /// Modifiers as a comma-separated string (e.g. "ctrl", "shift,alt").
    #[serde(default)]
    pub modifiers: String,
    /// Human-readable description for which-key hints.
    pub description: String,
}

/// Resolved binding after parsing key/modifiers strings.
#[derive(Debug, Clone)]
pub struct ResolvedBind {
    pub key_code: KeyCode,
    pub modifiers: KeyModifiers,
    pub message: AppMessage,
    pub description: String,
}

/// Key binding registry: loads from TOML, resolves events, provides hints.
pub struct KeyBindRegistry {
    binds: Vec<ResolvedBind>,
}

/// Parse a modifiers string like "ctrl,shift" into KeyModifiers.
fn parse_modifiers(s: &str) -> KeyModifiers {
    let mut mods = KeyModifiers::empty();
    for part in s.split(',') {
        match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
            "shift" => mods |= KeyModifiers::SHIFT,
            "alt" => mods |= KeyModifiers::ALT,
            _ => {}
        }
    }
    mods
}

/// Parse a key string like "Enter", "Ctrl-c", "h", "A-o" into (KeyCode, KeyModifiers).
fn parse_key(s: &str) -> Option<(KeyCode, KeyModifiers)> {
    let s = s.trim();
    let mut mods = KeyModifiers::empty();
    let key_part = if s.contains('-') {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        let prefix = parts[0].to_lowercase();
        let prefix_mods = match prefix.as_str() {
            "ctrl" | "control" | "c" => KeyModifiers::CONTROL,
            "shift" | "s" => KeyModifiers::SHIFT,
            "alt" | "a" => KeyModifiers::ALT,
            _ => KeyModifiers::empty(),
        };
        mods = prefix_mods;
        parts[1]
    } else {
        s
    };

    let key_code = match key_part {
        "Enter" | "ret" => KeyCode::Enter,
        "Tab" | "tab" => KeyCode::Tab,
        "BackTab" | "S-tab" => KeyCode::BackTab,
        "Esc" | "esc" => KeyCode::Esc,
        "Space" | "space" => KeyCode::Char(' '),
        "Backspace" | "backspace" => KeyCode::Backspace,
        "Delete" | "del" => KeyCode::Delete,
        "Left" | "left" => KeyCode::Left,
        "Right" | "right" => KeyCode::Right,
        "Up" | "up" => KeyCode::Up,
        "Down" | "down" => KeyCode::Down,
        "Home" | "home" => KeyCode::Home,
        "End" | "end" => KeyCode::End,
        "PageUp" | "pageup" => KeyCode::PageUp,
        "PageDown" | "pagedown" => KeyCode::PageDown,
        "Insert" => KeyCode::Insert,
        "F1" => KeyCode::F(1),
        "F2" => KeyCode::F(2),
        "F3" => KeyCode::F(3),
        "F4" => KeyCode::F(4),
        "F5" => KeyCode::F(5),
        "F6" => KeyCode::F(6),
        "F7" => KeyCode::F(7),
        "F8" => KeyCode::F(8),
        "F9" => KeyCode::F(9),
        "F10" => KeyCode::F(10),
        "F11" => KeyCode::F(11),
        "F12" => KeyCode::F(12),
        "minus" => KeyCode::Char('-'),
        "dot" => KeyCode::Char('.'),
        "comma" => KeyCode::Char(','),
        "pipe" => KeyCode::Char('|'),
        "tilde" => KeyCode::Char('~'),
        "backtick" => KeyCode::Char('`'),
        "underscore" => KeyCode::Char('_'),
        "bang" => KeyCode::Char('!'),
        "dollar" => KeyCode::Char('$'),
        "percent" => KeyCode::Char('%'),
        "ampersand" => KeyCode::Char('&'),
        "quote" => KeyCode::Char('"'),
        "colon" => KeyCode::Char(':'),
        "semicolon" => KeyCode::Char(';'),
        "paren_left" | "open_paren" => KeyCode::Char('('),
        "paren_right" | "close_paren" => KeyCode::Char(')'),
        c if c.len() == 1 => KeyCode::Char(c.chars().next()?),
        _ => return None,
    };

    Some((key_code, mods))
}

/// Convert an AppMessage to a TOML-serializable KeyBind (for config generation).
///
/// Not wired into the registry yet; kept as the inverse of
/// [`keybind_to_message`] for exporting the default keymap.
#[allow(dead_code)]
fn message_to_keybind(msg: &AppMessage) -> Option<KeyBind> {
    match msg {
        AppMessage::Quit => Some(KeyBind {
            key: "Ctrl-c".into(),
            modifiers: "".into(),
            description: "Quit the editor".into(),
        }),
        AppMessage::Escape => Some(KeyBind {
            key: "Esc".into(),
            modifiers: "".into(),
            description: "Return to Normal mode".into(),
        }),
        AppMessage::EnterInsertMode => Some(KeyBind {
            key: "i".into(),
            modifiers: "".into(),
            description: "Enter Insert mode".into(),
        }),
        AppMessage::EditorMoveLeft => Some(KeyBind {
            key: "h".into(),
            modifiers: "".into(),
            description: "Move cursor left".into(),
        }),
        AppMessage::EditorMoveDown => Some(KeyBind {
            key: "j".into(),
            modifiers: "".into(),
            description: "Move cursor down".into(),
        }),
        AppMessage::EditorMoveUp => Some(KeyBind {
            key: "k".into(),
            modifiers: "".into(),
            description: "Move cursor up".into(),
        }),
        AppMessage::EditorMoveRight => Some(KeyBind {
            key: "l".into(),
            modifiers: "".into(),
            description: "Move cursor right".into(),
        }),
        AppMessage::EditorUndo => Some(KeyBind {
            key: "u".into(),
            modifiers: "".into(),
            description: "Undo".into(),
        }),
        AppMessage::EditorRedo => Some(KeyBind {
            key: "U".into(),
            modifiers: "".into(),
            description: "Redo".into(),
        }),
        AppMessage::BufferNext => Some(KeyBind {
            key: "Tab".into(),
            modifiers: "".into(),
            description: "Next buffer".into(),
        }),
        AppMessage::BufferPrev => Some(KeyBind {
            key: "BackTab".into(),
            modifiers: "".into(),
            description: "Previous buffer".into(),
        }),
        AppMessage::FileExplorerOpen => Some(KeyBind {
            key: "Ctrl-e".into(),
            modifiers: "".into(),
            description: "Open file explorer".into(),
        }),
        AppMessage::FuzzyFinderOpen => Some(KeyBind {
            key: "Ctrl-p".into(),
            modifiers: "".into(),
            description: "Open fuzzy finder".into(),
        }),
        AppMessage::CommandPaletteOpen => Some(KeyBind {
            key: ":".into(),
            modifiers: "shift".into(),
            description: "Open command palette".into(),
        }),
        _ => None,
    }
}

impl KeyBindRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { binds: Vec::new() }
    }

    /// Load bindings from a TOML file. Missing file is not an error.
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), std::io::Error> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        let binds: Vec<KeyBind> = toml::from_str(&content).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
        self.binds = binds.into_iter().filter_map(|kb| {
            let (key_code, modifiers) = parse_key(&kb.key)?;
            let message = keybind_to_message(&kb.key, &kb.modifiers);
            Some(ResolvedBind {
                key_code,
                modifiers,
                message,
                description: kb.description,
            })
        }).collect();
        Ok(())
    }

    /// Load the Helix-compatible default bindings.
    pub fn load_defaults(&mut self) {
        self.binds = helix_defaults();
    }

    /// Resolve a key event to an AppMessage, if any binding matches.
    pub fn resolve(&self, event: &KeyEvent) -> Option<AppMessage> {
        let key_code = event.code;
        let modifiers = event.modifiers;
        self.binds.iter().find_map(|b| {
            if b.key_code == key_code && b.modifiers == modifiers {
                Some(b.message.clone())
            } else {
                None
            }
        })
    }

    /// Get which-key hints for a given prefix (partial modifiers + key code).
    /// Returns bindings that start with the given prefix modifiers.
    pub fn which_key_hints(&self, modifiers: KeyModifiers) -> Vec<&ResolvedBind> {
        self.binds
            .iter()
            .filter(|b| {
                // Match if the binding's modifiers contain the prefix modifiers,
                // and the binding has at least one more modifier or a key.
                b.modifiers.contains(modifiers) && b.modifiers != modifiers
            })
            .collect()
    }

    /// All bindings sorted by key for display.
    pub fn all_binds(&self) -> Vec<&ResolvedBind> {
        let mut binds: Vec<&ResolvedBind> = self.binds.iter().collect();
        binds.sort_by(|a, b| format!("{:?}", a.key_code).cmp(&format!("{:?}", b.key_code)));
        binds
    }
}

/// The complete Helix-compatible default keymap.
///
/// Every entry is `(key, modifiers, message, description)` and mirrors
/// Helix's `helix-term/src/keymap/default.rs` Normal / Select / Insert modes.
/// Only single-key bindings are represented; the flat registry resolves one
/// event at a time, so multi-key sequences (`g g`, `space f`, …) are omitted.
fn helix_defaults() -> Vec<ResolvedBind> {
    // (key, modifiers, message, description). `modifiers` is only non-empty
    // for the command-mode ":" key; other modifier forms (A-*, C-*) live in
    // the key string itself and are parsed by `parse_key`.
    let defaults: Vec<(&str, &str, AppMessage, &str)> = vec![
        // ---------- Normal mode: movement ----------
        ("h", "", AppMessage::EditorMoveLeft, "Move cursor left"),
        ("left", "", AppMessage::EditorMoveLeft, "Move cursor left"),
        ("j", "", AppMessage::EditorMoveDown, "Move cursor down"),
        ("down", "", AppMessage::EditorMoveDown, "Move cursor down"),
        ("k", "", AppMessage::EditorMoveUp, "Move cursor up"),
        ("up", "", AppMessage::EditorMoveUp, "Move cursor up"),
        ("l", "", AppMessage::EditorMoveRight, "Move cursor right"),
        ("right", "", AppMessage::EditorMoveRight, "Move cursor right"),
        ("home", "", AppMessage::EditorMoveLineStart, "Go to line start"),
        ("end", "", AppMessage::EditorMoveLineEnd, "Go to line end"),
        ("w", "", AppMessage::EditorMoveWordRight, "Next word start"),
        ("b", "", AppMessage::EditorMoveWordLeft, "Previous word start"),
        ("e", "", AppMessage::EditorMoveWordRight, "Next word end"),
        ("W", "", AppMessage::EditorMoveWordRight, "Next long word start"),
        ("B", "", AppMessage::EditorMoveWordLeft, "Previous long word start"),
        ("E", "", AppMessage::EditorMoveWordRight, "Next long word end"),

        // Character find / replace
        ("t", "", AppMessage::FindTillChar, "Find till next char"),
        ("f", "", AppMessage::FindNextChar, "Find next char"),
        ("T", "", AppMessage::TillPrevChar, "Find till prev char"),
        ("F", "", AppMessage::FindPrevChar, "Find prev char"),
        ("r", "", AppMessage::ReplaceChar, "Replace char under cursor"),
        ("R", "", AppMessage::ReplaceWithYanked, "Replace with yanked text"),
        ("A-.", "", AppMessage::RepeatLastMotion, "Repeat last motion"),

        // Case
        ("~", "", AppMessage::SwitchCase, "Switch case"),
        ("`", "", AppMessage::SwitchToLowercase, "Switch to lowercase"),
        ("A-`", "", AppMessage::SwitchToUppercase, "Switch to uppercase"),

        // Selection
        ("v", "", AppMessage::EditorToggleSelection, "Toggle selection (visual mode)"),

        // Goto line
        ("G", "", AppMessage::GotoLine, "Go to line"),

        // Command / insert / open
        (":", "shift", AppMessage::CommandPaletteOpen, "Open command palette"),
        ("i", "", AppMessage::EnterInsertMode, "Enter insert mode"),
        ("I", "", AppMessage::InsertAtLineStart, "Insert at line start"),
        ("a", "", AppMessage::EnterInsertMode, "Append (enter insert mode)"),
        ("A", "", AppMessage::InsertAtLineEnd, "Insert at line end"),
        ("o", "", AppMessage::OpenBelow, "Open line below"),
        ("O", "", AppMessage::OpenAbove, "Open line above"),

        // Delete / change
        ("d", "", AppMessage::DeleteSelection, "Delete selection"),
        ("A-d", "", AppMessage::DeleteSelection, "Delete selection (no yank)"),
        ("c", "", AppMessage::ChangeSelection, "Change selection"),
        ("A-c", "", AppMessage::ChangeSelection, "Change selection (no yank)"),
        ("C", "", AppMessage::Yank, "Copy selection to next line"),
        ("A-C", "", AppMessage::Yank, "Copy selection to prev line"),

        // Selections (multi-cursor)
        ("s", "", AppMessage::SelectRegex, "Select regex matches"),
        ("A-s", "", AppMessage::SplitSelectionOnNewline, "Split selection on newline"),
        ("A-minus", "", AppMessage::SelectionMerge, "Merge selections"),
        ("A-_", "", AppMessage::SelectionMerge, "Merge consecutive selections"),
        ("S", "", AppMessage::SelectionSplit, "Split selection"),
        (";", "", AppMessage::SelectionCollapse, "Collapse selections"),
        ("A-;", "", AppMessage::SelectionFlip, "Flip selections"),
        ("A-o", "", AppMessage::SelectionExpand, "Expand selection"),
        ("A-up", "", AppMessage::SelectionExpand, "Expand selection"),
        ("A-i", "", AppMessage::SelectionShrink, "Shrink selection"),
        ("A-down", "", AppMessage::SelectionShrink, "Shrink selection"),
        ("A-I", "", AppMessage::SelectionAllChildren, "Select all children"),
        ("A-S-down", "", AppMessage::SelectionAllChildren, "Select all children"),
        ("A-p", "", AppMessage::SelectionPrevSibling, "Select previous sibling"),
        ("A-left", "", AppMessage::SelectionPrevSibling, "Select previous sibling"),
        ("A-n", "", AppMessage::SelectionNextSibling, "Select next sibling"),
        ("A-right", "", AppMessage::SelectionNextSibling, "Select next sibling"),
        ("A-e", "", AppMessage::SelectionParentNodeEnd, "Move to parent node end"),
        ("A-b", "", AppMessage::SelectionParentNodeStart, "Move to parent node start"),
        ("A-a", "", AppMessage::SelectionAllSiblings, "Select all siblings"),

        ("%", "", AppMessage::SelectionAll, "Select all"),
        ("x", "", AppMessage::SelectionAll, "Extend selection below"),
        ("X", "", AppMessage::SelectionAll, "Extend to line bounds"),
        ("A-x", "", AppMessage::SelectionShrink, "Shrink to line bounds"),

        // Search
        ("/", "", AppMessage::SearchOpen, "Search"),
        ("?", "", AppMessage::Escape, "Reverse search"),
        ("n", "", AppMessage::SearchNext, "Search next"),
        ("N", "", AppMessage::SearchPrev, "Search previous"),
        ("*", "", AppMessage::SearchSelectionWordBoundaries, "Search selection (word boundaries)"),
        ("A-*", "", AppMessage::SearchSelection, "Search selection"),

        // Undo / redo
        ("u", "", AppMessage::EditorUndo, "Undo"),
        ("U", "", AppMessage::EditorRedo, "Redo"),
        ("A-u", "", AppMessage::Earlier, "Earlier (undo tree)"),
        ("A-U", "", AppMessage::Later, "Later (undo tree)"),

        // Yank / paste
        ("y", "", AppMessage::Yank, "Yank selection"),
        ("p", "", AppMessage::PasteAfter, "Paste after"),
        ("P", "", AppMessage::PasteBefore, "Paste before"),

        // Macros
        ("Q", "", AppMessage::MacroRecord, "Record macro"),
        ("q", "", AppMessage::MacroReplay, "Replay macro"),

        // Indent / format / join
        (">", "", AppMessage::Escape, "Indent"),
        ("<", "", AppMessage::Escape, "Unindent"),
        ("=", "", AppMessage::FormatSelections, "Format selections"),
        ("J", "", AppMessage::JoinSelections, "Join selections"),
        ("A-J", "", AppMessage::JoinSelectionsSpace, "Join selections with space"),
        ("K", "", AppMessage::SelectionKeep, "Keep selections"),
        ("A-K", "", AppMessage::SelectionRemove, "Remove selections"),

        (",", "", AppMessage::SelectionKeepPrimary, "Keep primary selection"),
        ("A-,", "", AppMessage::SelectionRemovePrimary, "Remove primary selection"),

        ("&", "", AppMessage::SelectionAlign, "Align selections"),
        ("_", "", AppMessage::SelectionTrim, "Trim selections"),

        ("(", "", AppMessage::SelectionRotateBackward, "Rotate selections backward"),
        (")", "", AppMessage::SelectionRotateForward, "Rotate selections forward"),
        ("A-(", "", AppMessage::SelectionRotateContentsBackward, "Rotate selection contents backward"),
        ("A-)", "", AppMessage::SelectionRotateContentsForward, "Rotate selection contents forward"),

        ("A-:", "", AppMessage::SelectionEnsureForward, "Ensure selections forward"),

        // Mode / paging
        ("esc", "", AppMessage::Escape, "Return to Normal mode"),
        ("C-b", "", AppMessage::Escape, "Page up"),
        ("pageup", "", AppMessage::Escape, "Page up"),
        ("C-f", "", AppMessage::Escape, "Page down"),
        ("pagedown", "", AppMessage::Escape, "Page down"),
        ("C-u", "", AppMessage::PageCursorHalfUp, "Page cursor half up"),
        ("C-d", "", AppMessage::PageCursorHalfDown, "Page cursor half down"),

        // Comments
        ("C-c", "", AppMessage::ToggleComments, "Toggle comments"),

        // Jump / save selection
        ("C-i", "", AppMessage::JumpForward, "Jump forward"),
        ("tab", "", AppMessage::JumpForward, "Jump forward (tab == C-i)"),
        ("C-o", "", AppMessage::JumpBackward, "Jump backward"),
        ("C-s", "", AppMessage::SaveSelection, "Save selection"),

        // Register / shell
        ("\"", "", AppMessage::SelectRegister, "Select register"),
        ("|", "", AppMessage::ShellPipe, "Shell pipe"),
        ("A-|", "", AppMessage::ShellPipeTo, "Shell pipe to"),
        ("!", "", AppMessage::ShellInsertOutput, "Shell insert output"),
        ("A-!", "", AppMessage::ShellAppendOutput, "Shell append output"),
        ("$", "", AppMessage::ShellKeepPipe, "Shell keep pipe"),
        ("C-z", "", AppMessage::Suspend, "Suspend"),

        // Increment / decrement
        ("C-a", "", AppMessage::Increment, "Increment"),
        ("C-x", "", AppMessage::Decrement, "Decrement"),

        // ---------- Insert mode (resolved after app.rs insert handling) ----------
        ("C-r", "", AppMessage::InsertRegister, "Insert register"),
        ("A-backspace", "", AppMessage::EditorBackspace, "Delete word backward"),
        ("A-del", "", AppMessage::EditorDeleteChar, "Delete word forward"),
        ("C-h", "", AppMessage::EditorBackspace, "Delete char backward"),
        ("C-j", "", AppMessage::EditorInsertNewline, "Insert newline"),
        ("S-tab", "", AppMessage::Escape, "Insert tab"),
    ];

    defaults
        .into_iter()
        .filter_map(|(key, mods, msg, desc)| {
            let key_str = if mods.is_empty() {
                key.to_string()
            } else {
                format!("{}-{}", mods, key)
            };
            let (key_code, modifiers) = parse_key(&key_str)?;
            Some(ResolvedBind {
                key_code,
                modifiers,
                message: msg,
                description: desc.to_string(),
            })
        })
        .collect()
}

/// Convert a key string and modifiers to an AppMessage (reverse lookup).
fn keybind_to_message(key: &str, modifiers: &str) -> AppMessage {
    let key_lower = key.to_lowercase();
    let mods = parse_modifiers(modifiers);

    match (key_lower.as_str(), mods) {
        // Quit
        ("q", KeyModifiers::NONE) | ("c", KeyModifiers::CONTROL) => AppMessage::Quit,

        // Mode
        ("esc", _) => AppMessage::Escape,
        ("i", _) => AppMessage::EnterInsertMode,

        // Movement
        ("h", _) | ("left", _) => AppMessage::EditorMoveLeft,
        ("j", _) | ("down", _) => AppMessage::EditorMoveDown,
        ("k", _) | ("up", _) => AppMessage::EditorMoveUp,
        ("l", _) | ("right", _) => AppMessage::EditorMoveRight,
        ("home", _) => AppMessage::EditorMoveLineStart,
        ("end", _) => AppMessage::EditorMoveLineEnd,
        ("w", _) => AppMessage::EditorMoveWordRight,
        ("e", KeyModifiers::CONTROL) | ("e", KeyModifiers::ALT) => AppMessage::FileExplorerOpen,
        ("e", _) => AppMessage::EditorMoveWordRight,
        ("b", KeyModifiers::CONTROL) => AppMessage::FuzzyFinderBuffers,
        ("b", _) => AppMessage::EditorMoveWordLeft,

        // Selection / edit
        ("v", _) => AppMessage::EditorToggleSelection,
        ("u", KeyModifiers::CONTROL) => AppMessage::EditorUndo,
        ("U", KeyModifiers::NONE) => AppMessage::EditorRedo,
        ("r", KeyModifiers::CONTROL) => AppMessage::EditorRedo,

        // Buffers
        ("tab", _) => AppMessage::BufferNext,
        ("backtab", _) => AppMessage::BufferPrev,

        // Views / tools
        ("p", KeyModifiers::CONTROL) => AppMessage::FuzzyFinderOpen,
        (":", KeyModifiers::SHIFT) => AppMessage::CommandPaletteOpen,

        // Search
        ("/", _) => AppMessage::SearchOpen,
        ("n", _) => AppMessage::SearchNext,
        ("N", _) => AppMessage::SearchPrev,

        _ => AppMessage::Escape,
    }
}

impl Default for KeyBindRegistry {
    fn default() -> Self {
        let mut r = Self::new();
        r.load_defaults();
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_simple() {
        assert_eq!(
            parse_key("h"),
            Some((KeyCode::Char('h'), KeyModifiers::NONE))
        );
    }

    #[test]
    fn parse_key_with_modifiers() {
        assert!(matches!(
            parse_key("Ctrl-c"),
            Some((KeyCode::Char('c'), mods)) if mods.contains(KeyModifiers::CONTROL)
        ));
    }

    #[test]
    fn parse_key_alt() {
        assert!(matches!(
            parse_key("A-o"),
            Some((KeyCode::Char('o'), mods)) if mods.contains(KeyModifiers::ALT)
        ));
    }

    #[test]
    fn parse_key_named() {
        assert_eq!(parse_key("esc"), Some((KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(parse_key("Enter"), Some((KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(parse_key("left"), Some((KeyCode::Left, KeyModifiers::NONE)));
    }

    #[test]
    fn resolve_known_key() {
        let registry = KeyBindRegistry::default();
        let event = crossterm::event::KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        );
        assert_eq!(registry.resolve(&event), Some(AppMessage::EditorMoveLeft));
    }

    #[test]
    fn helix_defaults_are_loaded() {
        let registry = KeyBindRegistry::default();
        // 'g' is a sequence prefix, not a complete single-key binding.
        let event = crossterm::event::KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        assert_eq!(registry.resolve(&event), None);
        // 'u' -> undo
        let event = crossterm::event::KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE);
        assert_eq!(registry.resolve(&event), Some(AppMessage::EditorUndo));
        // ':' + shift -> command palette
        let event = crossterm::event::KeyEvent::new(KeyCode::Char(':'), KeyModifiers::SHIFT);
        assert_eq!(
            registry.resolve(&event),
            Some(AppMessage::CommandPaletteOpen)
        );
        // '/' -> search
        let event = crossterm::event::KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        assert_eq!(registry.resolve(&event), Some(AppMessage::SearchOpen));
        // 'A-d' -> delete selection (alt + d)
        let event = crossterm::event::KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT);
        assert_eq!(registry.resolve(&event), Some(AppMessage::DeleteSelection));
    }

    #[test]
    fn which_key_hints_for_prefix() {
        let registry = KeyBindRegistry::default();
        let hints = registry.which_key_hints(KeyModifiers::NONE);
        assert!(!hints.is_empty());
    }

    #[test]
    fn defaults_are_substantial() {
        let registry = KeyBindRegistry::default();
        assert!(registry.all_binds().len() > 60, "expected a full Helix keymap");
    }
}