# Kataba Editor — Release 0.0.1 Goals

**Vision:** A modern TUI editor in Rust that surpasses Helix and Neovim by avoiding their architectural mistakes. Selection-first editing, zero-plugin philosophy, minimal startup/memory, and a best-in-class VCS experience out of the box.

## 0.0.1 Module Map (Fine-Grained)

### Core
| Module | Purpose |
|--------|---------|
| `kataba-core` | Buffer (Rope), selection, change sets, transactions, linear history, motion, encoding |
| `kataba-term` | Ratatui TUI shell — event loop, terminal setup, main entry |
| `kataba-config` | TOML config loading, validation, keybind definitions |
| `kataba-ui` | Shared UI primitives — blocks, styles, colors, widgets, layout helpers |

### Editing
| Module | Purpose |
|--------|---------|
| `editor/buffer` | Rope-backed text buffer with line/char mapping |
| `editor/selection` | Multi-cursor selection state, primary/secondary cursors |
| `editor/motion` | Cursor movement (char, word, line, paragraph, tree-sitter node) |
| `editor/text_objects` | Text object definitions (word, sentence, paragraph, brackets, quotes, tags) |
| `editor/undo` | Linear undo/redo history |
| `editor/changeset` | Insert/delete/apply change sets against buffer |

### LSP (kataba-lsp)
| Module | Purpose |
|--------|---------|
| `lsp/client` | LspClient handle, request/response channel, send_request/send_notification |
| `lsp/event_loop` | Background actor loop (tokio::select!) |
| `lsp/transport` | ProcessTransport, stdio server spawning |
| `lsp/framing` | Content-Length JSON-RPC framing codec |
| `lsp/rpc` | JSON-RPC request/response/notification types |
| `lsp/state` | CapabilityStore, DocumentVersionTracker, LanguageServerFeature enum |
| `lsp/handlers` | DiagnosticStore, ProgressStore, LspHandlers trait |
| `lsp/sync` | didOpen/didChange/didSave/didClose notification builders |
| `lsp/watcher` | File watcher (notify crate) |

### LSP Features (each is its own module)
| Module | Purpose |
|--------|---------|
| `lsp/features/completion` | textDocument/completion + completionItem/resolve |
| `lsp/features/hover` | textDocument/hover |
| `lsp/features/definition` | textDocument/definition/typeDefinition/implementation/declaration |
| `lsp/features/references` | textDocument/references |
| `lsp/features/signature` | textDocument/signatureHelp |
| `lsp/features/code_action` | textDocument/codeAction + codeAction/resolve |
| `lsp/features/code_lens` | textDocument/codeLens + codeLens/resolve |
| `lsp/features/formatting` | textDocument/formatting/rangeFormatting/onTypeFormatting |
| `lsp/features/rename` | textDocument/prepareRename + textDocument/rename |
| `lsp/features/document_symbol` | textDocument/documentSymbol |
| `lsp/features/workspace_symbol` | workspace/symbol |
| `lsp/features/document_highlight` | textDocument/documentHighlight |
| `lsp/features/document_links` | textDocument/documentLink + documentLink/resolve |
| `lsp/features/document_color` | textDocument/documentColor + colorPresentation |
| `lsp/features/folding` | textDocument/foldingRange |
| `lsp/features/selection_range` | textDocument/selectionRange |
| `lsp/features/semantic_tokens` | semanticTokens/full/range/delta |
| `lsp/features/inlay_hints` | textDocument/inlayHint + inlayHint/resolve |
| `lsp/features/inline_values` | textDocument/inlineValue |
| `lsp/features/moniker` | textDocument/moniker |
| `lsp/features/linked_editing` | textDocument/linkedEditingRange |
| `lsp/features/call_hierarchy` | prepareCallHierarchy/incomingCalls/outgoingCalls |
| `lsp/features/execute_command` | workspace/executeCommand |
| `lsp/features/pull_diagnostics` | textDocument/diagnostic + workspace/diagnostic/refresh |
| `lsp/features/file_operations` | willCreate/didCreate/willRename/didRename/willDelete/didDeleteFiles |
| `lsp/features/workspace_folders` | workspace/workspaceFolders + didChangeWorkspaceFolders |
| `lsp/features/configuration` | workspace/configuration |
| `lsp/features/watched_files` | workspace/didChangeWatchedFiles dynamic registration |

### VCS (kataba-vcs)
| Module | Purpose |
|--------|---------|
| `vcs/git_repo` | GitRepo handle, open/discover, trust level |
| `vcs/status` | repo_status via `git status --porcelain=v2 -z` |
| `vcs/write` | stage_file, unstage_file, discard, commit, amend |
| `vcs/branch` | branch_create, branch_switch, branch_delete |
| `vcs/stash` | stash_push, stash_pop, stash_list, stash_drop |
| `vcs/diff` | diff computation, hunk parsing |
| `vcs/blame` | git blame per line |
| `vcs/log` | commit log navigation |
| `vcs/model` | ChangeKind, FileChange, StatusSnapshot, DiffLine |

### TUI Views (kataba-term)
| Module | Purpose |
|--------|---------|
| `term/app` | App state, active view, mode, event dispatch |
| `term/editor_view` | Editor buffer rendering, cursor, line numbers, gutter |
| `term/vcs_view` | VCS status panel, stage/unstage, blame |
| `term/file_explorer` | Oil-style file explorer (navigate + edit filesystem) |
| `term/fuzzy_finder` | Fuzzy file/symbol/buffer finder |
| `term/help_view` | Help/keybinding discovery display |
| `term/command_palette` | Command input and execution |
| `term/status_line` | Status bar (mode, file, position, diagnostics) |
| `term/theme` | Color theme definitions, syntax highlight styles |

### Input
| Module | Purpose |
|--------|---------|
| `input/translate` | KeyEvent → AppMessage translation |
| `input/keybinds` | Keybind registry, which-key-style hints |
| `input/macros` | Macro record/repeat |

### Search
| Module | Purpose |
|--------|---------|
| `search/search` | Buffer search (forward/backward, regex, literal) |
| `search/replace` | Search and replace (single, all, confirm) |

### Completion
| Module | Purpose |
|--------|---------|
| `completion/backend` | Frecency ranking, autocorrect, source prioritization |
| `completion/sources` | LSP, buffer, path, file sources |
| `completion/ui` | Completion menu rendering, filtering |

### Formatting
| Module | Purpose |
|--------|---------|
| `format/lsp` | LSP-based formatting delegation |
| `format/dedicated` | Dedicated formatter execution (shfmt, stylua, etc.) |

## 0.0.1 Feature Scope

| Feature | Requirement | Modules |
|---------|-------------|---------|
| Editing model | Helix-style selection-first (select then act), no mode confusion | editor/selection, editor/motion, editor/text_objects |
| Keybindings | Helix-compatible keybinds + which-key-style discovery hints | input/keybinds, term/help_view |
| LSP support | Full language server protocol integration, built-in client | lsp/client, lsp/event_loop, lsp/transport, lsp/features/* |
| Tree-sitter | Native syntax highlighting, indentation, selection (Tree-sitter only) | kataba-core, term/editor_view |
| Plugin system | None — planned for 0.0.2 | Excluded |
| Configuration | TOML, declarative, simple | kataba-config |
| Multiple cursors | Native multicursor | editor/selection |
| Text objects | Surround/around/inner | editor/text_objects |
| Undo/Redo | Linear (undo tree in 0.0.2) | editor/undo |
| VCS | Stage/unstage/commit/blame/diff/log — better than Helix+Neovim+Neogit+Gitsigns | vcs/*, term/vcs_view |
| File explorer | Oil-style | term/file_explorer |
| Fuzzy finder | Built-in, Helix-comparable or better | term/fuzzy_finder |
| Terminal emulator | None (0.0.2) | Excluded |
| Macro repeat | Helix-like, improved in 0.0.2 | input/macros |
| Search & replace | Helix-style, improved in 0.0.2 | search/*, term/command_palette |
| Autocompletion | Fast, frecency, autocorrect, Blink.cmp-style ranking | completion/* |
| UI | Helix-resemble, clean minimal | term/*, kataba-ui |
| Startup time | Minimal | All |
| Memory usage | Minimal | All |
| Remote editing | None (0.0.2) | Excluded |
| Session management | None (0.0.2) | Excluded |
| Code actions | VSCode-quality | lsp/features/code_action |
| Formatter | Built-in, LSP or dedicated | format/* |
| Semantic tokens | LSP + Tree-sitter; highlighting Tree-sitter only | lsp/features/semantic_tokens, kataba-core |
| Folding | Built-in (Helix lacks this) | lsp/features/folding, term/editor_view |
| File operations | Create/rename/delete from editor | lsp/features/file_operations |
| Snippet support | None (0.0.2) | Excluded |
| Mouse support | Helix-like (advanced in 0.0.2) | input/translate |

## Non-Goals for 0.0.1

- No plugin system
- No terminal emulator
- No undo tree
- No snippets
- No remote editing
- No session management
- No Neovim FFI / Lua plugin (kataba is standalone)

## Design Principles (Avoiding Helix & Neovim Mistakes)

1. **Zero-plugin default:** Everything ships in the binary.
2. **Selection-first:** Like Helix/Kakoune — select then act.
3. **Tree-sitter native:** No regex highlighting fallback.
4. **TOML config:** Declarative, hard to break.
5. **Minimal overhead:** Single Rust binary, no Lua VM.
6. **VCS-first:** Version control is a first-class editor concern.
7. **Small modules:** Each module does one thing, max ~300 lines.

## Success Criteria for 0.0.1

- Cold start < 50ms with Tree-sitter highlighting
- LSP features work without plugin setup
- Multicursor and text objects work out of the box
- VCS panel: stage, unstage, commit, diff, blame without leaving editor
- File explorer and fuzzy finder usable on first launch
- Config is one TOML file with keybind discovery
- Memory under 30MB idle, startup under 100ms
- Every module under 300 lines of code
