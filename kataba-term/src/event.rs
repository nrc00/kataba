//! Input translation: crossterm events -> typed `AppMessage`s.
//!
//! Keeps the render/update loop decoupled from terminal event specifics.
//! Each message is a single, describable command the `App` can act on.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};

use crate::app_message::AppMessage;

/// Translate a crossterm event into an `AppMessage`, or `None` if the event
/// is not handled by the current view.
pub fn translate(event: Event) -> Option<AppMessage> {
    match event {
        Event::Key(key) => translate_key(key),
        Event::Mouse(_mouse) => None,
        Event::Resize(_, _) => Some(AppMessage::Resize),
        Event::Paste(_) => None,
        Event::FocusGained => None,
        Event::FocusLost => None,
    }
}

fn translate_key(key: KeyEvent) -> Option<AppMessage> {
    // Only act on press, not release, so repeat handling is natural.
    if key.kind != KeyEventKind::Press {
        return None;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match key.code {
        KeyCode::Char('q') => Some(AppMessage::Quit),
        KeyCode::Char('c') if ctrl => Some(AppMessage::Quit),
        KeyCode::Char('1') => Some(AppMessage::SwitchView(crate::app::View::Editor)),
        KeyCode::Char('2') => Some(AppMessage::SwitchView(crate::app::View::Vcs)),
        KeyCode::Char('?') => Some(AppMessage::SwitchView(crate::app::View::Help)),
        KeyCode::Esc => Some(AppMessage::Escape),

        // Buffer switching
        KeyCode::Tab if !shift => Some(AppMessage::BufferNext),
        KeyCode::BackTab => Some(AppMessage::BufferPrev),

        // File explorer
        KeyCode::Char('e') if ctrl => Some(AppMessage::FileExplorerOpen),
        KeyCode::Char('e') if alt => Some(AppMessage::FileExplorerOpen),
        KeyCode::Char('u') if !ctrl => Some(AppMessage::FileExplorerGoUp),
        KeyCode::Char('o') => Some(AppMessage::FileExplorerOpenSelectedFile),
        KeyCode::Char('r') if !ctrl => Some(AppMessage::FileExplorerRefresh),

        // Fuzzy finder
        KeyCode::Char('p') if ctrl => Some(AppMessage::FuzzyFinderOpen),
        KeyCode::Char('b') if ctrl => Some(AppMessage::FuzzyFinderBuffers),

        // Command palette
        KeyCode::Char(':') if shift => Some(AppMessage::CommandPaletteOpen),

        // Search & replace
        KeyCode::Char('/') => Some(AppMessage::SearchOpen),
        KeyCode::Char('n') => Some(AppMessage::SearchNext),
        KeyCode::Char('N') if shift => Some(AppMessage::SearchPrev),

        // Editor navigation
        KeyCode::Char('h') | KeyCode::Left => Some(AppMessage::EditorMoveLeft),
        KeyCode::Char('j') | KeyCode::Down => Some(AppMessage::EditorMoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(AppMessage::EditorMoveUp),
        KeyCode::Char('l') | KeyCode::Right => Some(AppMessage::EditorMoveRight),
        KeyCode::Char('w') => Some(AppMessage::EditorMoveWordRight),
        KeyCode::Char('b') => Some(AppMessage::EditorMoveWordLeft),
        KeyCode::Char('0') => Some(AppMessage::EditorMoveLineStart),
        KeyCode::Char('$') => Some(AppMessage::EditorMoveLineEnd),
        KeyCode::Enter => Some(AppMessage::EditorInsertNewline),
        KeyCode::Backspace => Some(AppMessage::EditorBackspace),
        KeyCode::Delete => Some(AppMessage::EditorDeleteChar),
        KeyCode::Char('u') if ctrl => Some(AppMessage::EditorUndo),
        KeyCode::Char('r') if ctrl => Some(AppMessage::EditorRedo),
        KeyCode::Char('i') => Some(AppMessage::EnterInsertMode),
        KeyCode::Char('v') => Some(AppMessage::EditorToggleSelection),

        // Command palette
        KeyCode::Char(':') if shift => Some(crate::app_message::AppMessage::BufferOpenScratch(
            "command".into(),
        )),

        // Undo/redo with shift variants (optional future)
        KeyCode::Char('U') if shift => Some(AppMessage::EditorUndo),
        KeyCode::Char('R') if ctrl => Some(AppMessage::EditorRedo),

        _ if alt => None,
        KeyCode::Char(c) => Some(AppMessage::EditorInsertChar(c)),
        _ => None,
    }
}

/// Translate a mouse event into an `AppMessage`. Currently unused; reserved
/// for future click-to-position and scroll-wheel support.
pub fn translate_mouse(_mouse: MouseEvent) -> Option<AppMessage> {
    None
}
