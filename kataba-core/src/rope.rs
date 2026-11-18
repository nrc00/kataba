//! Rope text buffer wrapper around the `ropey` crate.

use crate::encoding::PositionEncoding;
use crate::point::Point;
use ropey::Rope as RopeyRope;

/// A text buffer backed by a rope data structure.
///
/// This is a thin wrapper around `ropey::Rope` that provides a clean API
/// for the editor core.
#[derive(Debug, Clone)]
pub struct Rope {
    inner: RopeyRope,
}

impl Rope {
    /// Creates an empty rope.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RopeyRope::new(),
        }
    }

    /// Creates a rope from a string.
    #[must_use]
#[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        Self {
            inner: RopeyRope::from_str(text),
        }
    }

    /// Returns the length of the rope in characters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len_chars()
    }

    /// Returns `true` if the rope is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.len_chars() == 0
    }

    /// Returns the number of lines in the rope.
    #[must_use]
    pub fn lines(&self) -> usize {
        self.inner.lines().count()
    }

    /// Returns a slice of the rope.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> RopeSlice<'_> {
        RopeSlice {
            inner: self.inner.slice(start..end),
        }
    }

    /// Copies a character span into an owned string via chunk iteration.
    #[must_use]
    pub fn slice_text(&self, start: usize, end: usize) -> String {
        self.inner.slice(start..end).chunks().collect()
    }

    /// Copies a character span directly into a tendril (no intermediate `String`).
    #[must_use]
    pub fn slice_tendril(&self, start: usize, end: usize) -> crate::change::Tendril {
        self.inner
            .slice(start..end)
            .chunks()
            .flat_map(|chunk| chunk.chars())
            .collect()
    }

    /// Returns the full rope as a slice.
    #[must_use]
    pub fn as_slice(&self) -> RopeSlice<'_> {
        RopeSlice {
            inner: self.inner.slice(0..self.len()),
        }
    }
    /// Inserts text at the given character position.
    /// Inserts text at the given character position.
    pub fn insert(&mut self, pos: usize, text: &str) {
        self.inner.insert(pos, text);
    }

    /// Removes characters in the given range.
    pub fn remove(&mut self, start: usize, end: usize) {
        self.inner.remove(start..end);
    }

    /// Returns the character at the given position, or `None` if out of bounds.
    #[must_use]
    pub fn char(&self, pos: usize) -> Option<char> {
        if pos < self.inner.len_chars() {
            Some(self.inner.char(pos))
        } else {
            None
        }
    }

    /// Returns the line number for the given character position.
    #[must_use]
    pub fn char_to_line(&self, pos: usize) -> usize {
        self.inner.char_to_line(pos)
    }

    /// Returns the character position for the given line number.
    #[must_use]
    pub fn line_to_char(&self, line: usize) -> usize {
        self.inner.line_to_char(line)
    }

    /// Returns the byte length of the rope.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.inner.len_bytes()
    }

    /// Returns the byte offset for the given character position.
    #[must_use]
    pub fn char_to_byte(&self, pos: usize) -> usize {
        self.inner.char_to_byte(pos)
    }

    /// Returns the character position for the given byte offset.
    #[must_use]
    pub fn byte_to_char(&self, byte: usize) -> usize {
        self.inner.byte_to_char(byte)
    }

    /// Returns the chunk containing the given byte offset, along with the byte
    /// index, char index, and line index of the chunk's start.
    ///
    /// Chunks are borrowed slices of the rope's backing storage, which makes
    /// this the access path for zero-copy consumers such as tree-sitter's
    /// chunked input callback. Runs in O(log N) time. Panics if
    /// `byte_idx > byte_len()`.
    #[must_use]
    pub fn chunk_at_byte(&self, byte_idx: usize) -> (&str, usize, usize, usize) {
        self.inner.chunk_at_byte(byte_idx)
    }

    /// Returns the number of lines in the rope.
    #[must_use]
    pub fn len_lines(&self) -> usize {
        self.inner.len_lines()
    }

    /// Returns the line number for the given character position as u32.
    #[must_use]
    pub fn line_of_char(&self, pos: usize) -> u32 {
        self.inner.char_to_line(pos) as u32
    }

    /// Converts a character index to a Point (line, column) in the specified encoding.
    #[must_use]
    pub fn char_to_point(&self, char_idx: usize, encoding: PositionEncoding) -> Point {
        let line = self.line_of_char(char_idx);
        let line_start = self.inner.line_to_char(line as usize);
        let char_offset = char_idx - line_start;
        let col = self.column_in_line(line_start, char_offset, encoding);
        Point::new(line, col)
    }

    /// Converts a Point (line, column) in the specified encoding to a character index.
    #[must_use]
    pub fn point_to_char(&self, point: Point, encoding: PositionEncoding) -> usize {
        let line = point.line as usize;
        if line >= self.len_lines() {
            return self.len();
        }
        let line_start = self.inner.line_to_char(line);
        let line_end = if line + 1 < self.len_lines() {
            self.inner.line_to_char(line + 1)
        } else {
            self.len()
        };
        let line_len = line_end - line_start;
        let col = point.col_utf16 as usize;
        let char_offset = self.offset_in_line_by_encoding(line_start, line_len, col, encoding);
        line_start + char_offset.min(line_len)
    }

    /// Converts a Point to a byte offset.
    #[must_use]
    pub fn point_to_byte(&self, point: Point, encoding: PositionEncoding) -> usize {
        let char_idx = self.point_to_char(point, encoding);
        self.char_to_byte(char_idx)
    }

    /// Converts a byte offset to a Point in the specified encoding.
    #[must_use]
    pub fn byte_to_point(&self, byte_idx: usize, encoding: PositionEncoding) -> Point {
        let char_idx = self.byte_to_char(byte_idx);
        self.char_to_point(char_idx, encoding)
    }

    /// Computes the column offset within a line for a given character offset and encoding.
    fn column_in_line(&self, line_start: usize, char_offset: usize, encoding: PositionEncoding) -> u32 {
        match encoding {
            PositionEncoding::Utf8 => self.char_offset_to_utf8_col(line_start, char_offset),
            PositionEncoding::Utf16 => self.char_offset_to_utf16_col(line_start, char_offset),
            PositionEncoding::Utf32 => char_offset as u32,
        }
    }

    /// Computes the character offset within a line for a given column and encoding.
    fn offset_in_line_by_encoding(&self, line_start: usize, line_len: usize, col: usize, encoding: PositionEncoding) -> usize {
        match encoding {
            PositionEncoding::Utf8 => self.utf8_offset_to_char_offset(line_start, line_len, col),
            PositionEncoding::Utf16 => self.utf16_offset_to_char_offset(line_start, line_len, col),
            PositionEncoding::Utf32 => col.min(line_len),
        }
    }

    /// Converts a UTF-8 byte column offset to a character offset within a line.
    fn utf8_offset_to_char_offset(&self, line_start: usize, line_len: usize, utf8_col: usize) -> usize {
        let mut char_offset = 0;
        let mut current_utf8 = 0;
        while char_offset < line_len && current_utf8 < utf8_col {
            let ch = self.char(line_start + char_offset).unwrap_or('\0');
            let ch_len = ch.len_utf8();
            if current_utf8 + ch_len > utf8_col {
                break;
            }
            current_utf8 += ch_len;
            char_offset += 1;
        }
        char_offset
    }

    /// Computes the UTF-8 byte column offset for a character offset within a line.
    fn char_offset_to_utf8_col(&self, line_start: usize, char_offset: usize) -> u32 {
        let mut utf8_col = 0u32;
        for i in 0..char_offset {
            let ch = self.char(line_start + i).unwrap_or('\0');
            utf8_col += ch.len_utf8() as u32;
        }
        utf8_col
    }

    /// Converts a UTF-16 column offset to a character offset within a line.
    fn utf16_offset_to_char_offset(&self, line_start: usize, line_len: usize, utf16_col: usize) -> usize {
        let mut char_offset = 0;
        let mut current_utf16 = 0;
        
        while char_offset < line_len && current_utf16 < utf16_col {
            let ch = self.char(line_start + char_offset).unwrap_or('\0');
            let ch_utf16_len = ch.len_utf16();
            if current_utf16 + ch_utf16_len > utf16_col {
                break;
            }
            current_utf16 += ch_utf16_len;
            char_offset += 1;
        }
        char_offset
    }

    /// Computes the UTF-16 column offset for a character offset within a line.
    fn char_offset_to_utf16_col(&self, line_start: usize, char_offset: usize) -> u32 {
        let mut utf16_col = 0u32;
        for i in 0..char_offset {
            let ch = self.char(line_start + i).unwrap_or('\0');
            utf16_col += ch.len_utf16() as u32;
        }
        utf16_col
    }
}

impl Default for Rope {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Rope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for i in 0..self.len() {
            if let Some(c) = self.char(i) {
                write!(f, "{c}")?;
            }
        }
        Ok(())
    }
}

impl PartialEq for Rope {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for Rope {}

/// A slice into a rope.
#[derive(Debug)]
pub struct RopeSlice<'a> {
    inner: ropey::RopeSlice<'a>,
}

impl<'a> RopeSlice<'a> {
    /// Returns the length of the slice in characters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len_chars()
    }

    /// Returns `true` if the slice is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.len_chars() == 0
    }

    /// Returns the character at the given position, or `None` if out of bounds.
    #[must_use]
    pub fn char(&self, pos: usize) -> Option<char> {
        if pos < self.inner.len_chars() {
            Some(self.inner.char(pos))
        } else {
            None
        }
    }

    /// Returns the byte length of the slice.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.inner.len_bytes()
    }

    /// Returns a sub-slice.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> RopeSlice<'_> {
        RopeSlice {
            inner: self.inner.slice(start..end),
        }
    }

    /// Returns the slice as a string slice, or `None` if it contains non-UTF8 data.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        self.inner.as_str()
    }

    /// Returns the number of lines in the slice.
    #[must_use]
    pub fn lines(&self) -> usize {
        self.inner.lines().count()
    }

    /// Returns the character at the given position as a byte.
    #[must_use]
    pub fn byte(&self, pos: usize) -> u8 {
        self.inner.byte(pos)
    }
}

impl PartialEq for RopeSlice<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.chunks().eq(other.inner.chunks())
    }
}

impl Eq for RopeSlice<'_> {}

impl Clone for RopeSlice<'_> {
    fn clone(&self) -> Self {
        let len = self.inner.len_chars();
        Self {
            inner: self.inner.slice(0..len),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rope_new() {
        let rope = Rope::new();
        assert!(rope.is_empty());
        assert_eq!(rope.len(), 0);
    }

    #[test]
    fn test_rope_from_str() {
        let rope = Rope::from_str("hello");
        assert!(!rope.is_empty());
        assert_eq!(rope.len(), 5);
        assert_eq!(rope.char(0), Some('h'));
    }

    #[test]
    fn test_rope_insert() {
        let mut rope = Rope::from_str("hello");
        rope.insert(5, " world");
        assert_eq!(rope.len(), 11);
    }

    #[test]
    fn test_rope_remove() {
        let mut rope = Rope::from_str("hello world");
        rope.remove(5, 11);
        assert_eq!(rope.len(), 5);
    }

    #[test]
    fn test_rope_multi_chunk_equality() {
        let text_a = "a".repeat(5000);
        let text_b = "a".repeat(5000);
        let text_c = "b".repeat(5000);

        let r_a = Rope::from_str(&text_a);
        let r_b = Rope::from_str(&text_b);
        let r_c = Rope::from_str(&text_c);

        assert_eq!(r_a, r_b);
        assert_ne!(r_a, r_c);
    }

    #[test]
    fn test_rope_utf8_encoding_multibyte() {
        // "你好\n" : '你' is 3 bytes, '好' is 3 bytes
        let rope = Rope::from_str("你好\nworld");
        let p0 = rope.char_to_point(0, PositionEncoding::Utf8);
        assert_eq!(p0.line, 0);
        assert_eq!(p0.col_utf16, 0);

        let p1 = rope.char_to_point(1, PositionEncoding::Utf8);
        assert_eq!(p1.line, 0);
        assert_eq!(p1.col_utf16, 3); // 3 bytes in UTF-8

        let p2 = rope.char_to_point(2, PositionEncoding::Utf8);
        assert_eq!(p2.line, 0);
        assert_eq!(p2.col_utf16, 6); // 6 bytes in UTF-8

        // Round trip
        assert_eq!(rope.point_to_char(p0, PositionEncoding::Utf8), 0);
        assert_eq!(rope.point_to_char(p1, PositionEncoding::Utf8), 1);
        assert_eq!(rope.point_to_char(p2, PositionEncoding::Utf8), 2);
    }
}