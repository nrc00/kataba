# Kataba 0.1 Release Module Analysis

## Executive Summary

Based on analyzing Helix's LSP client (1816 lines in `client.rs` alone) and the current `kataba-lsp` implementation, here's what's needed for a production-ready 0.1 release.

---

## Current kataba-lsp Module Structure

```
kataba-lsp/
├── src/
│   ├── lib.rs                    ✅ Public exports
│   ├── client.rs                 ✅ 25+ LSP feature methods (core)
│   ├── conversion.rs             ✅ Point/Position translation
│   ├── error.rs                  ✅ Error types
│   ├── event_loop.rs             ✅ Actor loop (tokio::select!)
│   ├── framing.rs                ✅ Content-Length codec
│   ├── manager.rs                ✅ Multi-server orchestration
│   ├── rpc.rs                    ✅ JSON-RPC request/response
│   ├── state.rs                  ✅ FSM + CapabilityStore + VersionTracker
│   ├── sync.rs                   ✅ didOpen/didChange/didSave/didClose
│   ├── transport.rs              ✅ ProcessTransport + ServerConfig
│   ├── watcher.rs                ✅ File watcher (notify crate)
│   └── handlers/
│       ├── mod.rs                ✅ Re-exports
│       ├── registry.rs           ✅ LspHandlers trait (12 callbacks)
│       ├── diagnostics.rs        ✅ DiagnosticStore
│       ├── progress.rs           ✅ ProgressStore
│       └── workspace_edit.rs     ✅ WorkspaceEdit → Transaction
```

---

## Gap Analysis: Helix LSP Client vs kataba-lsp

### Features Helix Has That kataba-lsp Is Missing

| Feature | Helix Method | kataba-lsp Status | Priority |
|---------|--------------|-------------------|----------|
| **Pull Diagnostics** | `text_document_diagnostic()` | ❌ Missing | P0 |
| **Document Colors** | `text_document_document_color()` | ❌ Missing | P1 |
| **Color Presentation** | (resolves colors) | ❌ Missing | P1 |
| **Inlay Hints** | `text_document_range_inlay_hints()` | ❌ Missing | P1 |
| **Document Links** | `text_document_document_link()` + resolve | ❌ Missing | P1 |
| **Call Hierarchy** | prepare/incoming/outgoing | ❌ Missing | P1 |
| **Signature Help** | `text_document_signature_help()` | ❌ Missing | P1 |
| **Workspace File Operations** | will_create/did_create/will_rename/did_rename/will_delete/did_delete | ❌ Missing | P1 |
| **Prepare Rename** | `prepare_rename()` | ❌ Missing | P1 |
| **Rename Symbol** | `rename_symbol()` | ❌ Missing | P1 |
| **Execute Command** | `command()` | ⚠️ Partial (no args) | P0 |
| **Completion Resolve** | `resolve_completion_item()` | ⚠️ Partial (client only) | P0 |
| **Code Action Resolve** | `resolve_code_action()` | ⚠️ Partial (client only) | P0 |
| **Document Link Resolve** | `resolve_document_link()` | ❌ Missing | P1 |
| **Semantic Tokens** | (full/range) | ⚠️ API only, no impl | P2 |
| **Selection Range** | | ❌ Missing | P2 |
| **Folding Range** | | ⚠️ API only, no impl | P2 |
| **Linked Editing Range** | | ❌ Missing | P2 |
| **Moniker** | | ❌ Missing | P2 |
| **Inline Values** | | ❌ Missing | P2 |

---

## Missing Modules for 0.1 Release

### P0 - Critical (Block Helix Integration)

1. **`handlers/completion_resolve.rs`** - CompletionItem resolve support
   - Needed for `completionItem/resolve` request handling

2. **`handlers/code_action_resolve.rs`** - CodeAction resolve support  
   - Needed for `codeAction/resolve` request handling

3. **`handlers/execute_command.rs`** - Full `workspace/executeCommand` with arguments
   - Current implementation only returns error

4. **`handlers/workspace_folders.rs`** - Workspace folder management
   - `workspace/workspaceFolders` request
   - `workspace/didChangeWorkspaceFolders` notification

5. **`handlers/configuration.rs`** - Workspace configuration
   - `workspace/configuration` request (partially done in registry)

6. **`handlers/watched_files.rs`** - File watching registration
   - `workspace/didChangeWatchedFiles` dynamic registration

### P1 - High Priority (Core LSP 3.17 Features)

7. **`features/pull_diagnostics.rs`** - Pull diagnostics support
   - `textDocument/diagnostic` request
   - Diagnostic refresh (`workspace/diagnostic/refresh`)

8. **`features/document_color.rs`** - Document color + presentation
   - `textDocument/documentColor`
   - `textDocument/colorPresentation`

9. **`features/inlay_hints.rs`** - Inlay hints
   - `textDocument/inlayHint`
   - `inlayHint/resolve`

10. **`features/document_links.rs`** - Document links
    - `textDocument/documentLink`
    - `documentLink/resolve`

11. **`features/call_hierarchy.rs`** - Call hierarchy
    - `textDocument/prepareCallHierarchy`
    - `callHierarchy/incomingCalls`
    - `callHierarchy/outgoingCalls`

12. **`features/signature_help.rs`** - Signature help
    - `textDocument/signatureHelp`

13. **`features/prepare_rename.rs`** - Prepare rename
    - `textDocument/prepareRename`

14. **`features/rename.rs`** - Rename symbol
    - `textDocument/rename`

15. **`features/file_operations.rs`** - Workspace file operations
    - `workspace/willCreateFiles`
    - `workspace/didCreateFiles`
    - `workspace/willRenameFiles`
    - `workspace/didRenameFiles`
    - `workspace/willDeleteFiles`
    - `workspace/didDeleteFiles`

16. **`features/semantic_tokens.rs`** - Semantic tokens
    - `textDocument/semanticTokens/full`
    - `textDocument/semanticTokens/range`
    - `textDocument/semanticTokens/full/delta`

### P2 - Medium Priority (Advanced Features)

17. **`features/selection_range.rs`** - Selection ranges
18. **`features/folding_range.rs`** - Folding ranges
19. **`features/linked_editing.rs`** - Linked editing ranges
20. **`features/moniker.rs`** - Monikers
21. **`features/inline_values.rs`** - Inline values (debug)

---

## Neovim Integration Requirements

Since Neovim is C/Lua-based, kataba needs:

| Module | Purpose | Approach |
|--------|---------|----------|
| `kataba-ffi` | C FFI bindings | `cbindgen` + `cdylib` |
| `kataba-nvim` | Neovim plugin (Lua) | Lua wrapper calling FFI |
| `kataba-lsp` | Core LSP library | Already exists |

### FFI API Surface Needed

```c
// kataba-ffi.h
typedef struct KatabaLspClient KatabaLspClient;
typedef struct KatabaLspConfig KatabaLspConfig;

KatabaLspClient* kataba_lsp_start(const char* server_cmd, const char* root_uri);
void kataba_lsp_stop(KatabaLspClient* client);
int kataba_lsp_initialize(KatabaLspClient* client, const char* config_json);
int kataba_lsp_did_open(KatabaLspClient* client, const char* uri, const char* text);
int kataba_lsp_did_change(KatabaLspClient* client, const char* uri, const char* changes_json);
int kataba_lsp_completion(KatabaLspClient* client, const char* uri, int line, int col, char** result_json);
int kataba_lsp_hover(KatabaLspClient* client, const char* uri, int line, int col, char** result_json);
int kataba_lsp_goto_definition(KatabaLspClient* client, const char* uri, int line, int col, char** result_json);
int kataba_lsp_code_action(KatabaLspClient* client, const char* uri, int start_line, int start_col, int end_line, int end_col, char** result_json);
int kataba_lsp_formatting(KatabaLspClient* client, const char* uri, const char* options_json, char** result_json);
int kataba_lsp_shutdown(KatabaLspClient* client);
void kataba_lsp_free_string(char* str);
```

---

## Architecture Alignment Required

### Current kataba-lsp vs Helix Expectations

| Aspect | Helix Expects | kataba-lsp Current | Action |
|--------|---------------|-------------------|--------|
| **Client Type** | `Client` with `Arc<Client>` | `LspClient` (Arc internally) | Compatible |
| **Capabilities** | `OnceCell<ServerCapabilities>` | `Arc<RwLock<CapabilityStore>>` | Add `OnceCell` wrapper |
| **Request Timeout** | Per-request + global | Per-request + global | ✅ Compatible |
| **Position Encoding** | `OffsetEncoding` enum | Uses `PositionEncoding` from kataba-core | Add conversion |
| **Workspace Folders** | Built-in management | Via `did_change_workspace_folders` | Add `workspace/workspaceFolders` |
| **Diagnostics** | `LanguageServerId` per server | `DiagnosticStore` by URI | Add `LanguageServerId` tracking |
| **Progress** | `LspProgressMap` | `ProgressStore` | ✅ Compatible |
| **File Operations** | `FileOperationsInterest` | ❌ Missing | Implement P1 feature |

---

## Recommended Implementation Order

### Phase 1: Core Protocol Completeness (Week 1-2)
```
1. handlers/completion_resolve.rs
2. handlers/code_action_resolve.rs
3. handlers/execute_command.rs (full impl)
4. handlers/workspace_folders.rs
5. client.rs: Add missing feature methods (see P0 table)
```

### Phase 2: Core LSP 3.17 Features (Week 2-3)
```
6. features/pull_diagnostics.rs
7. features/document_color.rs
8. features/inlay_hints.rs
9. features/document_links.rs
9. features/call_hierarchy.rs
10. features/signature_help.rs
11. features/prepare_rename.rs
12. features/rename.rs
```

### Phase 3: Workspace Operations (Week 3)
```
13. features/file_operations.rs
14. handlers/watched_files.rs (dynamic registration)
```

### Phase 4: Neovim FFI (Week 4)
```
15. kataba-ffi crate (cdylib)
16. kataba-nvim Lua plugin
17. Integration tests with actual Neovim
```

### Phase 5: Polish & Release (Week 4-5)
```
18. Documentation (README, API docs)
19. Benchmarks vs helix-lsp
20. Version 0.1.0 tag
```

---

## Code Generation Opportunities

Since Helix uses `lsp-types` directly and kataba-lsp also uses `lsp-types`, many feature methods can be generated from the LSP spec. Consider:

- `xtask/codegen` - Generate feature methods from LSP spec JSON
- Use `lsp-types` derive macros where possible
- Share `conversion.rs` logic with Helix's `util.rs`

---

## Testing Requirements

| Test Type | Current | Needed for 0.1 |
|-----------|---------|----------------|
| Unit tests | 59 passing | Maintain >90% coverage |
| Integration (rust-analyzer) | 4 passing | Add all P0/P1 features |
| Helix integration | ❌ | `helix-lsp` compatibility test |
| Neovim integration | ❌ | FFI + Lua plugin test |
| Mock server | ❌ | In-memory LSP server for CI |

---

## Dependencies to Add

```toml
# kataba-lsp/Cargo.toml additions
[dependencies]
# For semantic tokens
lsp-types = { version = "0.95", features = ["semantic-tokens"] }

# For FFI
ctypes = "0.1"  # or libc

# For codegen (dev)
serde_json = "1.0"
```

---

## Summary

**Modules to Implement: ~21 new modules**
- 6 P0 (critical for Helix)
- 10 P1 (core LSP 3.17)
- 5 P2 (advanced)

**New Crates: 2**
- `kataba-ffi` (C FFI)
- `kataba-nvim` (Neovim Lua plugin)

**Estimated Effort: 4-5 weeks** for full 0.1 with both Helix and Neovim integration.