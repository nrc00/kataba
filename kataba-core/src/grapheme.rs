//! Grapheme cluster support for Unicode-aware cursor movement.

use unicode_segmentation::GraphemeCursor;

/// A Unicode grapheme cluster.
///
/// A grapheme cluster is the smallest unit of text that users perceive as a
/// single character — for example, `é` (e + combining acute) or `👨‍👩‍👧` (family
/// emoji with ZWJ sequences).
#[derive(Debug, Clone)]
pub struct Grapheme {
    /// The grapheme cluster as a string slice.
    slice: Box<str>,
}

impl Grapheme {
    /// Creates a new grapheme from a string slice.
    ///
    /// The caller must ensure that `slice` is a valid grapheme cluster.
    #[must_use]
    pub fn new(slice: &str) -> Self {
        Self {
            slice: slice.into(),
        }
    }

    /// Returns the grapheme as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.slice
    }

    /// Returns the number of Unicode scalar values in this grapheme.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slice.chars().count()
    }

    /// Returns the byte length of this grapheme.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.slice.len()
    }

    /// Returns `true` if this grapheme is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slice.is_empty()
    }
}

impl PartialEq for Grapheme {
    fn eq(&self, other: &Self) -> bool {
        self.slice == other.slice
    }
}

impl Eq for Grapheme {}

impl PartialOrd for Grapheme {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Grapheme {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.slice.cmp(&other.slice)
    }
}

impl std::fmt::Display for Grapheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.slice)
    }
}

/// Iterator over grapheme clusters in a string.
pub struct GraphemeIterator<'a> {
    cursor: GraphemeCursor,
    cursor_back: GraphemeCursor,
    string: &'a str,
}

impl<'a> GraphemeIterator<'a> {
    /// Creates a new grapheme iterator over the given string.
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        let len = text.len();
        Self {
            cursor: GraphemeCursor::new(0, len, true),
            cursor_back: GraphemeCursor::new(len, len, true),
            string: text,
        }
    }
}

impl<'a> Iterator for GraphemeIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.cursor.cur_cursor();
        if start == self.cursor_back.cur_cursor() {
            return None;
        }
        let next = self.cursor.next_boundary(self.string, 0).ok()??;
        Some(&self.string[start..next])
    }
}

/// Splits a string into grapheme clusters.
///
/// Returns a vector of grapheme cluster string slices.
#[must_use]
pub fn graphemes(text: &str) -> Vec<&str> {
    GraphemeIterator::new(text).collect()
}

/// Returns the number of grapheme clusters in a string.
#[must_use]
pub fn grapheme_count(text: &str) -> usize {
    GraphemeIterator::new(text).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grapheme_count_ascii() {
        assert_eq!(grapheme_count("hello"), 5);
    }

    #[test]
    fn test_grapheme_count_emoji() {
        assert_eq!(grapheme_count("👨‍👩‍👧"), 1);
    }

    #[test]
    fn test_grapheme_count_combining() {
        assert_eq!(grapheme_count("e\u{0301}"), 1);
    }

    #[test]
    fn test_graphemes_iteration() {
        let result: Vec<&str> = GraphemeIterator::new("abc").collect();
        assert_eq!(result, vec!["a", "b", "c"]);
    }
}