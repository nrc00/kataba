# Kataba Term — Ratatui TUI Plan

## Goals
- Terminal frontend for the Kataba editor stack.
- Render a `kataba-core` buffer (rope + multi-cursor selection + transaction history) as an editable text view.
- Render `kataba-vcs` git status/diff in a side panel (changed-files picker + hunk gutter).
- Strict safety: `#![forbid(unsafe_code)]`, `deny(unwrap_used/expect_used/panic)`, zero panics in library code.
- Async-free where possible: synchronous editor core drives the UI; VCS runs via bounded process calls off the hot path (mirroring `kataba-vcs`'s `exec.rs`).

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│ App                                                     │
│   ├── active_view: Editor | VcsStatus | Help             │
│   ├── editor: EditorView (kataba-core buffer state)     │
│   ├── vcs: Option<VcsView> (kataba-vcs status/diff)     │
│   ├── terminal: ratatui Terminal<CrosstermBackend>      │
│   └── run loop: poll input → update → render            │
└─────────────────────────────────────────────────────────┘
```

- **`app.rs`**: `App` owns the terminal, the active view, and the top-level event dispatch. Quit on `q`/`Ctrl-c`; view switching via keys.
- **`editor.rs`**: `EditorView` wraps `kataba_core::Rope`, `kataba_core::Selection`, and `kataba_core::Transaction` history (linear undo/redo). Renders visible lines, cursor(s), selection highlights, and a VCS gutter column (+/-/±) driven by `kataba-vcs` diff hunks.
- **`vcs.rs`**: `VcsView` lists changed files from `kataba-vcs::git::repo_status` and shows a selected file's diff hunks. Keymaps: `j/k` navigate, `Enter` opens file in editor, `s`/`u` stage/unstage, `r` refresh.
- **`ui.rs`**: shared styles, theme constants, block helpers.
- **`event.rs`**: crossterm event translation into `AppMessage` (typed command), keeping input decoupled from rendering.
- **`main.rs`**: crossterm raw mode + alternate screen, panic hook restore, `App::run()`.

## Data flow
1. `App::run()` loops: read crossterm event → `AppMessage` → `app.update(msg)` → `app.render(frame)`.
2. Buffer edits (typing, motions) mutate `kataba_core` state inside `EditorView`; each edit pushes a `Transaction` onto `LinearHistory` for undo/redo.
3. VCS status is fetched lazily when the VCS view opens or on `r` refresh; results stored in `VcsView`, never blocking the render loop (offloaded via `kataba-vcs::exec::run_in`).

## Milestones
- **M0**: blank editor view with cursor movement, insert mode, basic undo/redo, quit.
- **M1**: multi-line rendering, line numbers, selection highlighting, clipboard-ish cut/paste placeholder.
- **M2**: VCS status panel + gutter signs, open file from picker.
- **M3**: hunk stage/unstage from gutter, syntax highlighting placeholder (treesitter later).

## Files
- `kataba-term/Cargo.toml` (done)
- `kataba-term/src/main.rs`
- `kataba-term/src/app.rs`
- `kataba-term/src/editor.rs`
- `kataba-term/src/vcs.rs`
- `kataba-term/src/ui.rs`
- `kataba-term/src/event.rs`

## Dependencies
- `ratatui = "0.27"`, `crossterm = "0.28"` (standard TUI stack)
- `kataba-core`, `kataba-vcs` (workspace path deps)
