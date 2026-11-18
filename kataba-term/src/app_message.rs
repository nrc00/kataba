//! Typed application messages: crossterm events are translated into these
//! commands by [`crate::event::translate`], and [`crate::app::App`] acts on them.

use crate::app::View;

/// A single, describable command the `App` can act on.
#[derive(Debug, Clone, PartialEq)]
pub enum AppMessage {
    /// Quit the application.
    Quit,
    /// Terminal was resized.
    Resize,
    /// Return to Normal mode / cancel current operation.
    Escape,
    /// Switch to a different full-screen view.
    SwitchView(View),
    /// Enter Insert mode.
    EnterInsertMode,

    // Buffer management
    BufferNext,
    BufferPrev,
    BufferSwitch(usize),
    BufferOpenFile(std::path::PathBuf),
    BufferOpenScratch(String),
    BufferCloseActive,
    BufferSaveActive,

    // Editor mutations
    EditorMoveLeft,
    EditorMoveRight,
    EditorMoveUp,
    EditorMoveDown,
    EditorMoveWordLeft,
    EditorMoveWordRight,
    EditorMoveLineStart,
    EditorMoveLineEnd,
    EditorInsertNewline,
    EditorBackspace,
    EditorDeleteChar,
    EditorToggleSelection,
    EditorUndo,
    EditorRedo,
    EditorInsertChar(char),

    // VCS
    VcsRefresh,
    VcsOpenSelected,
    VcsStageSelected,
    VcsUnstageSelected,
    VcsMoveUp,
    VcsMoveDown,

    // File explorer
    FileExplorerOpen,
    FileExplorerRefresh,
    FileExplorerGoUp,
    FileExplorerSelectUp,
    FileExplorerSelectDown,
    FileExplorerEnterSelected,
    FileExplorerOpenSelectedFile,

    // Fuzzy finder
    FuzzyFinderOpen,
    FuzzyFinderBuffers,
    FuzzyFinderPushChar(char),
    FuzzyFinderPopChar,
    FuzzyFinderClearFilter,
    FuzzyFinderSelectUp,
    FuzzyFinderSelectDown,
    FuzzyFinderOpenSelected,

    // Command palette
    CommandPaletteOpen,
    CommandPalettePushChar(char),
    CommandPalettePopChar,
    CommandPaletteClearFilter,
    CommandPaletteSelectUp,
    CommandPaletteSelectDown,
    CommandPaletteExecuteSelected,

    // Search & replace
    SearchOpen,
    SearchClose,
    SearchPushChar(char),
    SearchPopChar,
    SearchNext,
    SearchPrev,
    SearchModeToggle,
    SearchReplaceOpen,
    SearchReplacePushChar(char),
    SearchReplacePopChar,
    SearchReplaceConfirm,
    SearchReplaceExecute,
    SearchReplaceAll,

    // LSP / language features
    LspGotoDefinition,
    LspGotoDeclaration,
    LspGotoTypeDefinition,
    LspGotoReference,
    LspGotoImplementation,
    LspCodeAction,
    LspRenameSymbol,
    LspHover,
    LspSelectReferences,
    LspDiagnosticsPicker,
    LspWorkspaceDiagnosticsPicker,
    LspSymbolPicker,
    LspWorkspaceSymbolPicker,

    // Window management
    WindowRotate,
    WindowHsplit,
    WindowVsplit,
    WindowTranspose,
    WindowGotoFileHsplit,
    WindowGotoFileVsplit,
    WindowClose,
    WindowOnly,
    // Toggle the embedded terminal pane (show / hide).
    ToggleTerminal,
    // Open the ':' command line prompt.
    OpenCommandLine,
    // Submit a completed command line.
    SubmitCommand(String),
    WindowJumpLeft,
    WindowJumpDown,
    WindowJumpUp,
    WindowJumpRight,
    WindowSwapLeft,
    WindowSwapDown,
    WindowSwapUp,
    WindowSwapRight,
    WindowHsplitNew,
    WindowVsplitNew,

    // Selection
    SelectionAll,
    SelectionExpand,
    SelectionShrink,
    SelectionAllChildren,
    SelectionPrevSibling,
    SelectionNextSibling,
    SelectionParentNodeEnd,
    SelectionParentNodeStart,
    SelectionAllSiblings,
    SelectionSplit,
    SelectionCollapse,
    SelectionFlip,
    SelectionMerge,
    SelectionKeep,
    SelectionRemove,
    SelectionKeepPrimary,
    SelectionRemovePrimary,
    SelectionAlign,
    SelectionTrim,
    SelectionRotateBackward,
    SelectionRotateForward,
    SelectionRotateContentsBackward,
    SelectionRotateContentsForward,
    SelectionEnsureForward,

    // Macro / register / clipboard
    MacroRecord,
    MacroReplay,
    Yank,
    PasteAfter,
    PasteBefore,
    YankToClipboard,
    YankMainSelectionToClipboard,
    PasteClipboardAfter,
    PasteClipboardBefore,
    ReplaceSelectionsWithClipboard,
    SelectRegister,
    InsertRegister,

    // Search
    SearchSelection,
    SearchSelectionWordBoundaries,
    GlobalSearch,

    // Motion / goto
    GotoLine,
    GotoColumn,
    GotoLastLine,
    GotoFile,
    GotoFirstNonWhitespace,
    GotoWindowTop,
    GotoWindowCenter,
    GotoWindowBottom,
    GotoLastAccessedFile,
    GotoLastModifiedFile,
    GotoWord,
    MoveLineUp,
    MoveLineDown,
    GotoLastModification,
    FindTillChar,
    FindNextChar,
    TillPrevChar,
    FindPrevChar,

    // Edit
    ReplaceChar,
    ReplaceWithYanked,
    SwitchCase,
    SwitchToLowercase,
    SwitchToUppercase,
    RepeatLastMotion,
    FormatSelections,
    JoinSelections,
    JoinSelectionsSpace,
    ToggleComments,
    ToggleBlockComments,
    ToggleLineComments,
    SaveSelection,
    JumpForward,
    JumpBackward,
    Earlier,
    Later,
    Increment,
    Decrement,

    // View / scrolling
    AlignViewCenter,
    AlignViewTop,
    AlignViewBottom,
    AlignViewMiddle,
    ScrollUp,
    ScrollDown,
    PageCursorHalfUp,
    PageCursorHalfDown,

    // Misc
    Suspend,
    ShellPipe,
    ShellPipeTo,
    ShellInsertOutput,
    ShellAppendOutput,
    ShellKeepPipe,

    // Insert mode helpers
    InsertAtLineStart,
    InsertAtLineEnd,
    OpenBelow,
    OpenAbove,
    DeleteSelection,
    ChangeSelection,
    SelectRegex,
    SplitSelectionOnNewline,
}
