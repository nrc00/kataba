//! Tree-sitter syntax highlighting for editor buffers.
//!
//! Detects the language from a file extension, loads the corresponding
//! tree-sitter grammar via `kataba-treesitter`, and incrementally reparses
//! the buffer as it changes. Highlights come from Helix's `highlights.scm`
//! queries: capture names are mapped to Helix default-theme colors once per
//! parse, then sliced per rendered line. If no grammar or query is available,
//! the buffer renders as plain text.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kataba_core::change::ChangeSet;
use kataba_core::rope::Rope;
use kataba_treesitter::{load_language, SyntaxParser, TreeError};
use ratatui::style::{Color, Modifier, Style};
use tree_sitter::{Language, Query, QueryCursor, StreamingIterator, Tree};

use crate::ui;

/// Cache of loaded tree-sitter languages.
#[derive(Default, Clone)]
pub struct SyntaxCache {
    inner: Arc<std::sync::RwLock<HashMap<String, Language>>>,
}

impl SyntaxCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a cached language for `language_id`.
    pub fn get(&self, language_id: &str) -> Option<Language> {
        // A poisoned lock only means another thread panicked while holding it;
        // the map itself is still consistent enough to read.
        let inner = match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        inner.get(language_id).cloned()
    }

    /// Insert a language for `language_id`.
    pub fn insert(&self, language_id: String, language: Language) {
        let mut inner = match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        inner.insert(language_id, language);
    }
}

/// Detect language id from a file path extension.
pub fn detect_language(path: &Path) -> Option<&'static str> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| match ext {
            "rs" => Some("rust"),
            "py" => Some("python"),
            "js" => Some("javascript"),
            "ts" => Some("typescript"),
            "go" => Some("go"),
            "c" => Some("c"),
            "cpp" | "cc" | "cxx" => Some("cpp"),
            "h" | "hpp" => Some("c"),
            "java" => Some("java"),
            "rb" => Some("ruby"),
            "lua" => Some("lua"),
            "json" => Some("json"),
            "toml" => Some("toml"),
            "yaml" | "yml" => Some("yaml"),
            "md" => Some("markdown"),
            "html" => Some("html"),
            "css" => Some("css"),
            "sh" => Some("bash"),
            "zig" => Some("zig"),
            "swift" => Some("swift"),
            "kt" | "kts" => Some("kotlin"),
            "scala" => Some("scala"),
            _ => None,
        })
}

/// Directories searched for compiled tree-sitter grammars.
///
/// Order matters: an explicit `KATABA_TREESITTER_GRAMMAR_DIR` wins, then the
/// user's Kataba/Helix runtimes, then a `helix/runtime/grammars` checkout next
/// to (or above) the current working directory — handy when hacking on both
/// projects side by side.
fn grammar_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("KATABA_TREESITTER_GRAMMAR_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".config/kataba/grammars"));
        dirs.push(home.join(".config/helix/runtime/grammars"));
        dirs.push(home.join(".local/share/helix/runtime/grammars"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("helix/runtime/grammars"));
        if let Some(parent) = cwd.parent() {
            dirs.push(parent.join("helix/runtime/grammars"));
        }
    }
    dirs
}

/// Resolve the shared-library path for `language_id`, if one exists.
fn grammar_path(language_id: &str) -> Option<PathBuf> {
    let file = format!("{language_id}{}", std::env::consts::DLL_SUFFIX);
    grammar_dirs()
        .into_iter()
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
}

/// Directories searched for Helix-compatible `queries/<lang>/*.scm` files.
fn query_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("KATABA_TREESITTER_QUERY_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".config/kataba/queries"));
        dirs.push(home.join(".config/helix/runtime/queries"));
        dirs.push(home.join(".local/share/helix/runtime/queries"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("helix/runtime/queries"));
        if let Some(parent) = cwd.parent() {
            dirs.push(parent.join("helix/runtime/queries"));
        }
    }
    // Queries bundled with this crate, as a last-resort fallback.
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/queries"));
    dirs
}

/// Resolve the highlight query for `language_id`, if one exists.
///
/// Accepts both Helix's layout (`<lang>/highlights.scm`) and the flat bundled
/// layout (`<lang>.scm`).
fn query_path(language_id: &str) -> Option<PathBuf> {
    let nested = Path::new(language_id).join("highlights.scm");
    let flat = format!("{language_id}.scm");
    query_dirs().into_iter().find_map(|dir| {
        let candidate = dir.join(&nested);
        if candidate.is_file() {
            return Some(candidate);
        }
        let candidate = dir.join(&flat);
        candidate.is_file().then_some(candidate)
    })
}

/// Round `byte` down to the nearest UTF-8 char boundary.
fn floor_boundary(text: &str, byte: usize) -> usize {
    let mut i = byte.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// A styled character range in the document.
#[derive(Debug, Clone, Copy)]
pub struct HighlightSpan {
    /// Inclusive start, as a document char offset.
    pub start: usize,
    /// Exclusive end, as a document char offset.
    pub end: usize,
    /// Style resolved from the capture name.
    pub style: Style,
}

/// A syntax-highlighted buffer state: parser, tree, query, and cached spans.
pub struct SyntaxState {
    parser: Option<SyntaxParser>,
    tree: Option<Tree>,
    language_id: Option<String>,
    old_text: Rope,
    query: Option<Query>,
    /// Style per query capture index (`None` for non-visual captures).
    capture_styles: Vec<Option<Style>>,
    /// Styled char ranges for the whole document, in paint order.
    spans: Vec<HighlightSpan>,
}

impl Default for SyntaxState {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxState {
    pub fn new() -> Self {
        Self {
            parser: None,
            tree: None,
            language_id: None,
            old_text: Rope::new(),
            query: None,
            capture_styles: Vec::new(),
            spans: Vec::new(),
        }
    }

    /// Set the language for this buffer, loading the grammar if available, and
    /// parse `text` so highlighting is active immediately.
    pub fn set_language(
        &mut self,
        language_id: &str,
        cache: &SyntaxCache,
        text: &Rope,
    ) -> Result<(), TreeError> {
        // Try cache first.
        if let Some(language) = cache.get(language_id) {
            self.configure(language_id, &language)?;
            return self.reparse_full(text);
        }

        // Try to load grammar from disk.
        let Some(path) = grammar_path(language_id) else {
            self.language_id = Some(language_id.to_string());
            return Ok(());
        };
        let grammar = match load_language(language_id, &path) {
            Ok(g) => g,
            Err(_) => {
                self.language_id = Some(language_id.to_string());
                return Ok(());
            }
        };

        let language = grammar.language().clone();
        cache.insert(language_id.to_string(), language.clone());
        self.configure(language_id, &language)?;
        self.reparse_full(text)
    }

    /// Build the parser and compile the highlight query for `language`.
    fn configure(&mut self, language_id: &str, language: &Language) -> Result<(), TreeError> {
        self.language_id = Some(language_id.to_string());
        self.tree = None;
        self.old_text = Rope::new();
        self.spans.clear();
        self.parser = Some(SyntaxParser::new(language)?);

        self.query = None;
        self.capture_styles.clear();
        if let Some(path) = query_path(language_id) {
            if let Ok(source) = std::fs::read_to_string(&path) {
                if let Ok(query) = Query::new(language, &source) {
                    self.capture_styles = query
                        .capture_names()
                        .iter()
                        .map(|name| capture_style(name))
                        .collect();
                    self.query = Some(query);
                }
            }
        }
        Ok(())
    }

    /// Reparse the buffer incrementally after a change.
    pub fn reparse(&mut self, new_text: &Rope, changeset: &ChangeSet) -> Result<(), TreeError> {
        let Some(parser) = self.parser.as_mut() else {
            return Ok(());
        };
        let Some(tree) = self.tree.take() else {
            let tree = parser.parse(new_text)?;
            self.tree = Some(tree);
            self.old_text = new_text.clone();
            self.recompute_spans(new_text);
            return Ok(());
        };

        let outcome = parser.parse_incremental(
            &self.old_text,
            &tree,
            new_text,
            changeset,
            None,
        )?;

        match outcome {
            kataba_treesitter::ParseOutcome::Parsed(tree) => {
                self.tree = Some(tree);
                self.old_text = new_text.clone();
            }
            kataba_treesitter::ParseOutcome::BudgetExceeded => {
                self.tree = Some(tree);
            }
        }

        self.recompute_spans(new_text);
        Ok(())
    }

    /// Reparse the whole buffer. Used after undo/redo, where the inverse
    /// transaction is applied directly rather than through [`Self::reparse`].
    pub fn reparse_full(&mut self, text: &Rope) -> Result<(), TreeError> {
        let Some(parser) = self.parser.as_mut() else {
            return Ok(());
        };
        let tree = parser.parse(text)?;
        self.tree = Some(tree);
        self.old_text = text.clone();
        self.recompute_spans(text);
        Ok(())
    }

    /// Run the highlight query over the whole document and cache the styled
    /// char ranges. Called after every parse; a query-less state caches none.
    fn recompute_spans(&mut self, text: &Rope) {
        self.spans.clear();
        let (Some(tree), Some(query)) = (self.tree.as_ref(), self.query.as_ref()) else {
            return;
        };

        let source = text.slice_text(0, text.len());
        let mut pending: Vec<(usize, usize, usize, Style)> = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
        while let Some(query_match) = matches.next() {
            let pattern = query_match.pattern_index;
            for capture in query_match.captures {
                let Some(style) = self
                    .capture_styles
                    .get(capture.index as usize)
                    .copied()
                    .flatten()
                else {
                    continue;
                };
                let node = capture.node;
                if node.start_byte() < node.end_byte() {
                    pending.push((node.start_byte(), node.end_byte(), pattern, style));
                }
            }
        }

        // Paint outer spans first; later patterns win for exact ties.
        pending.sort_by_key(|&(start, end, pattern, _)| {
            (start, std::cmp::Reverse(end), pattern)
        });

        let mut byte_cursor = 0;
        let mut char_cursor = 0;
        self.spans = pending
            .into_iter()
            .map(|(start_byte, end_byte, _, style)| {
                let start = floor_boundary(&source, start_byte);
                let end = floor_boundary(&source, end_byte).max(start);
                if start > byte_cursor {
                    char_cursor += source[byte_cursor..start].chars().count();
                    byte_cursor = start;
                }
                let start_char = char_cursor;
                let end_char = start_char + source[start..end].chars().count();
                HighlightSpan {
                    start: start_char,
                    end: end_char,
                    style,
                }
            })
            .collect();
    }

    /// The language this buffer was configured for, if any.
    pub fn language_id(&self) -> Option<&str> {
        self.language_id.as_deref()
    }

    /// Whether a parsed tree is available for highlighting.
    pub fn is_active(&self) -> bool {
        self.tree.is_some()
    }

    /// Highlight spans for a line, as **line-relative char offsets**.
    ///
    /// `char_start` is the document char offset of the line start; `text` is
    /// the line content. Spans are returned in paint order, so a later span
    /// covering the same character is the more specific one.
    pub fn highlight_line(&self, char_start: usize, text: &str) -> Vec<(usize, usize, Style)> {
        let char_end = char_start.saturating_add(text.chars().count());
        let mut spans = Vec::new();
        for span in &self.spans {
            if span.end <= char_start {
                continue;
            }
            if span.start >= char_end {
                break;
            }
            let start = span.start.max(char_start) - char_start;
            let end = span.end.min(char_end) - char_start;
            if start < end {
                spans.push((start, end, span.style));
            }
        }
        spans
    }
}

/// Map a tree-sitter capture name to a Helix default-theme style.
///
/// Mirrors the scopes in Helix's `theme.toml`: comments are sirocco, strings
/// silver, numbers chamois, keywords almond, functions/types white, variables
/// and punctuation lavender, escapes honey, builtins mint. Non-visual captures
/// (`@spell`, injections, string content) get `None`.
pub fn capture_style(name: &str) -> Option<Style> {
    if name.starts_with("spell") || name.contains("injection") || name.contains("content") {
        return None;
    }
    let plain = |color: Color| Some(Style::default().fg(color));

    if name.contains("comment") {
        Some(Style::default().fg(SIROCCO).add_modifier(Modifier::ITALIC))
    } else if name.contains("escape") {
        plain(HONEY)
    } else if name.contains("string") {
        plain(SILVER)
    } else if name.contains("number") || name.contains("float") {
        plain(CHAMOIS)
    } else if name.contains("boolean")
        || name.contains("constant")
        || name.contains("function")
        || name.contains("method")
        || name.contains("constructor")
        || name.contains("type")
        || name.contains("class")
    {
        plain(WHITE)
    } else if name.contains("keyword")
        || name.contains("conditional")
        || name.contains("repeat")
        || name.contains("storage")
    {
        plain(ALMOND)
    } else if name.contains("operator") {
        plain(ui::STATUSLINE_FG)
    } else if name.contains("punctuation") || name.contains("delimiter") || name.contains("bracket")
    {
        plain(ui::TEXT)
    } else if name.contains("label") || name.contains("attribute") || name.contains("decorator") {
        plain(HONEY)
    } else if name.contains("tag") {
        plain(ALMOND)
    } else if name.contains("property") || name.contains("member") || name.contains("field") {
        plain(WHITE)
    } else if name.contains("parameter") {
        plain(ui::TEXT)
    } else if name.contains("builtin") {
        plain(MINT)
    } else if name.contains("namespace") || name.contains("module") {
        plain(ui::STATUSLINE_FG)
    } else if name.contains("variable") {
        plain(ui::TEXT)
    } else {
        None
    }
}

// Helix default-theme palette entries used by [`capture_style`].
const WHITE: Color = Color::Rgb(0xff, 0xff, 0xff);
const SILVER: Color = Color::Rgb(0xcc, 0xcc, 0xcc);
const SIROCCO: Color = Color::Rgb(0x69, 0x7c, 0x81);
const MINT: Color = Color::Rgb(0x9f, 0xf2, 0x8f);
const ALMOND: Color = Color::Rgb(0xec, 0xcd, 0xba);
const CHAMOIS: Color = Color::Rgb(0xe8, 0xdc, 0xa0);
const HONEY: Color = Color::Rgb(0xef, 0xba, 0x5d);

