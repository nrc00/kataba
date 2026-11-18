//! Autocompletion backend: frecency ranking, autocorrect, Blink.cmp-style scoring.
//!
//! Provides a modular completion system with multiple sources (buffer words,
//! LSP, file paths), frecency-based ranking, and fuzzy/autocorrect matching.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// A single completion item.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// Text to display in the menu.
    pub label: String,
    /// Text to insert when selected.
    pub insert_text: String,
    /// Optional detail shown after the label.
    pub detail: Option<String>,
    /// Kind of completion (keyword, function, variable, etc.).
    pub kind: CompletionKind,
    /// Source that provided this completion.
    pub source: CompletionSource,
    /// Sort score (higher = better).
    pub score: f64,
}

/// Completion item kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

/// Source of a completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionSource {
    Lsp,
    Buffer,
    Path,
    File,
    Snippet,
}

/// Completion result: items and whether to auto-insert.
#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub items: Vec<CompletionItem>,
    pub is_incomplete: bool,
}

/// Completion backend: aggregates sources, ranks by frecency, handles autocorrect.
pub struct CompletionBackend {
    sources: Vec<Box<dyn CompletionSourceProvider>>,
    frecency: FrecencyTable,
    buffer_words: BufferWordIndex,
}

/// A completion source provider.
pub trait CompletionSourceProvider {
    /// Name of the source.
    fn name(&self) -> &'static str;
    /// Trigger characters that start completion from this source.
    fn trigger_chars(&self) -> &'static [char];
    /// Compute completions for the given context.
    fn compute(
        &self,
        context: &CompletionContext,
    ) -> Result<Vec<CompletionItem>, Box<dyn std::error::Error>>;
    /// Priority of this source (higher = preferred).
    fn priority(&self) -> i32;
}

/// Context for completion computation.
#[derive(Debug, Clone)]
pub struct CompletionContext {
    /// Document text before cursor.
    pub text_before_cursor: String,
    /// Document text after cursor.
    pub text_after_cursor: String,
    /// Byte offset of cursor.
    pub cursor_byte: usize,
    /// Line number (0-indexed).
    pub line: usize,
    /// Column on line.
    pub col: usize,
}

/// Frecency table: tracks frequency + recency of completion selections.
pub struct FrecencyTable {
    scores: HashMap<String, f64>,
    last_access: HashMap<String, u64>,
}

impl Default for FrecencyTable {
    fn default() -> Self {
        Self::new()
    }
}

impl FrecencyTable {
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
            last_access: HashMap::new(),
        }
    }

    /// Record that `item` was selected, boosting its score.
    pub fn record_selection(&mut self, item: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let score = self.scores.entry(item.to_string()).or_insert(0.0);
        *score += 1.0;
        // Decay: older selections matter less.
        *score = (*score * 0.9) + 1.0;

        self.last_access.insert(item.to_string(), now);
    }

    /// Get the frecency boost for `item`.
    pub fn boost(&self, item: &str) -> f64 {
        let base = self.scores.get(item).copied().unwrap_or(0.0);
        let recency = self.last_access.get(item).copied().unwrap_or(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let age = now.saturating_sub(recency);
        // Recency decay: items accessed recently get higher boost.
        let recency_factor = 1.0 / (1.0 + age as f64 / 3600.0); // decay over hours
        base * recency_factor
    }
}

/// Buffer word index: extracts words from the current buffer for completion.
pub struct BufferWordIndex {
    words: HashMap<String, u32>,
}

impl Default for BufferWordIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferWordIndex {
    pub fn new() -> Self {
        Self {
            words: HashMap::new(),
        }
    }

    /// Index all words in `text`.
    pub fn index(&mut self, text: &str) {
        for word in text.split_whitespace() {
            let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if word.len() >= 2 {
                *self.words.entry(word.to_string()).or_insert(0) += 1;
            }
        }
    }

    /// Get candidate words matching `prefix`.
    pub fn candidates(&self, prefix: &str) -> Vec<&str> {
        self.words
            .keys()
            .filter(|w| w.starts_with(prefix))
            .map(|s| s.as_str())
            .collect()
    }
}

impl CompletionBackend {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            frecency: FrecencyTable::new(),
            buffer_words: BufferWordIndex::new(),
        }
    }

    /// Register a completion source.
    pub fn register_source(&mut self, source: Box<dyn CompletionSourceProvider>) {
        self.sources.push(source);
    }

    /// Index buffer text for buffer-word completions.
    pub fn index_buffer(&mut self, text: &str) {
        self.buffer_words.index(text);
    }

    /// Compute completions for the given context.
    pub fn compute(
        &mut self,
        context: &CompletionContext,
    ) -> Result<CompletionResult, Box<dyn std::error::Error>> {
        let prefix = self.extract_prefix(context);
        let mut all_items: Vec<CompletionItem> = Vec::new();

        // Collect from all sources.
        for source in &self.sources {
            if source.trigger_chars().iter().any(|c| prefix.contains(*c)) {
                continue; // skip if trigger char not in prefix
            }
            let items = source.compute(context)?;
            for mut item in items {
                // Apply frecency boost.
                item.score += self.frecency.boost(&item.insert_text);
                all_items.push(item);
            }
        }

        // Add buffer word completions.
        for word in self.buffer_words.candidates(&prefix) {
            all_items.push(CompletionItem {
                label: word.to_string(),
                insert_text: word.to_string(),
                detail: None,
                kind: CompletionKind::Text,
                source: CompletionSource::Buffer,
                score: self.frecency.boost(word),
            });
        }

        // Sort by score (descending).
        all_items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Limit results.
        all_items.truncate(50);

        Ok(CompletionResult {
            items: all_items,
            is_incomplete: false,
        })
    }

    /// Extract the word prefix before cursor.
    fn extract_prefix(&self, context: &CompletionContext) -> String {
        let text = &context.text_before_cursor;
        let mut start = text.len();
        for (index, c) in text.char_indices().rev() {
            if c.is_alphanumeric() || c == '_' {
                start = index;
            } else {
                break;
            }
        }
        text[start..].to_string()
    }

    /// Record that an item was selected (for frecency).
    pub fn record_selection(&mut self, item: &str) {
        self.frecency.record_selection(item);
    }
}

impl Default for CompletionBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Autocorrect: suggest corrections for misspelled words using edit distance.
pub struct Autocorrect {
    dictionary: Vec<String>,
}

impl Default for Autocorrect {
    fn default() -> Self {
        Self::new()
    }
}

impl Autocorrect {
    pub fn new() -> Self {
        Self {
            dictionary: Vec::new(),
        }
    }

    /// Add words to the dictionary.
    pub fn add_words(&mut self, words: &[String]) {
        self.dictionary.extend(words.iter().cloned());
        self.dictionary.sort();
        self.dictionary.dedup();
    }

    /// Suggest corrections for `word`, sorted by edit distance.
    pub fn suggest(&self, word: &str, max_suggestions: usize) -> Vec<String> {
        if self.dictionary.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(String, usize)> = self
            .dictionary
            .iter()
            .filter(|w| w.as_str() != word)
            .map(|w| (w.clone(), edit_distance(word, w)))
            .filter(|(_, d)| *d <= 2) // max edit distance of 2
            .collect();

        scored.sort_by_key(|(_, d)| *d);
        scored
            .into_iter()
            .take(max_suggestions)
            .map(|(w, _)| w)
            .collect()
    }
}

/// Levenshtein edit distance.
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            dp[i][j] = std::cmp::min(
                std::cmp::min(dp[i - 1][j] + 1, dp[i][j - 1] + 1),
                dp[i - 1][j - 1] + cost,
            );
        }
    }

    dp[m][n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frecency_boost() {
        let mut table = FrecencyTable::new();
        assert_eq!(table.boost("foo"), 0.0);
        table.record_selection("foo");
        assert!(table.boost("foo") > 0.0);
    }

    #[test]
    fn edit_distance_test() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", "abc"), 0);
    }

    #[test]
    fn autocorrect_suggest() {
        let mut ac = Autocorrect::new();
        ac.add_words(&["hello".into(), "world".into(), "rust".into()]);
        let suggestions = ac.suggest("helo", 5);
        assert!(suggestions.contains(&"hello".to_string()));
    }

    #[test]
    fn buffer_word_index() {
        let mut index = BufferWordIndex::new();
        index.index("hello world hello rust");
        let candidates = index.candidates("he");
        assert!(candidates.contains(&"hello"));
    }
}
