//! Translation of kataba-core change sets into tree-sitter input edits.
//!
//! [`kataba_core::ChangeSet`] speaks in **character** coordinates over the old
//! document; tree-sitter speaks in **bytes** plus `(row, column-in-bytes)`
//! points. This module bridges the two, using tree-sitter's own newline
//! convention: rows advance on `\n` bytes only (no Unicode line breaks), and
//! columns count bytes since the last `\n`.
//!
//! Getting this exactly right is what makes
//! [`SyntaxParser::parse_incremental`](crate::SyntaxParser::parse_incremental)
//! produce trees identical to a full reparse; see the crate-level invariant.

use kataba_core::{ChangeSet, Operation, Rope};
use tree_sitter::{InputEdit, Point};

/// Converts an ordered [`ChangeSet`] over `old` into [`InputEdit`]s.
///
/// The edits are returned in document order, with **all coordinates expressed
/// in the old document**. Before reparsing they must be applied via
/// [`tree_sitter::Tree::edit`] in **reverse** order (last edit first): a
/// forward pass would let an early edit shift the coordinate space that a
/// later edit describes. [`SyntaxParser::parse_incremental`] does this
/// internally.
///
/// A replacement — `Insert` immediately followed by `Delete`, as produced by
/// [`ChangeSet::from_change`] — is emitted as a single edit.
///
/// Points follow tree-sitter semantics: `row` counts `\n` bytes before the
/// position, `column` counts bytes since the last `\n`. Pass the **old**
/// document; the new end positions are derived from the inserted text itself.
///
/// [`SyntaxParser::parse_incremental`]: crate::SyntaxParser::parse_incremental
pub fn generate_edits(old: &Rope, changeset: &ChangeSet) -> Vec<InputEdit> {
    let mut edits = Vec::new();
    if changeset.is_empty() {
        return edits;
    }

    let mut old_char = 0usize;
    let mut scanner = PointScanner::new(old);
    let mut ops = changeset.changes().iter().peekable();

    while let Some(operation) = ops.next() {
        match operation {
            Operation::Retain(count) => {
                old_char += count;
            }
            Operation::Delete(count) => {
                edits.push(scanner.build_edit(old, old_char, *count, ""));
                old_char += count;
            }
            Operation::Insert(text) => {
                // Replacements arrive as `Insert` followed by `Delete`; fold
                // them into one edit so tree-sitter never sees a zero-width
                // intermediate state.
                let deleted = match ops.peek() {
                    Some(Operation::Delete(count)) => *count,
                    _ => 0,
                };
                if deleted > 0 {
                    ops.next();
                }
                edits.push(scanner.build_edit(old, old_char, deleted, text));
                old_char += deleted;
            }
        }
    }

    edits
}

/// Walks a rope forward while tracking tree-sitter-style points.
///
/// The walk is monotonic: one scanner instance is shared by all edits of a
/// change set, so positions cost one pass over the document bytes up to the
/// last edit instead of one pass per edit.
struct PointScanner<'r> {
    rope: &'r Rope,
    byte: usize,
    row: usize,
    column: usize,
}

impl<'r> PointScanner<'r> {
    fn new(rope: &'r Rope) -> Self {
        Self {
            rope,
            byte: 0,
            row: 0,
            column: 0,
        }
    }

    fn point(&self) -> Point {
        Point {
            row: self.row,
            column: self.column,
        }
    }

    /// Advances the scanner to `target` (a byte offset into the old document).
    fn advance_to(&mut self, target: usize) {
        debug_assert!(target >= self.byte, "edits must be generated in order");
        debug_assert!(target <= self.rope.byte_len());
        while self.byte < target {
            let (chunk, chunk_start, _char_start, _line_start) = self.rope.chunk_at_byte(self.byte);
            let chunk_end = chunk_start + chunk.len();
            let end = target.min(chunk_end);
            let span = &chunk.as_bytes()[self.byte - chunk_start..end - chunk_start];
            match span.iter().rposition(|&byte| byte == b'\n') {
                Some(last_newline) => {
                    self.row += span.iter().filter(|&&byte| byte == b'\n').count();
                    self.column = span.len() - last_newline - 1;
                }
                None => self.column += span.len(),
            }
            self.byte = end;
        }
    }

    /// Builds one edit replacing `deleted` chars at `old_char` with `text`.
    fn build_edit(&mut self, old: &Rope, old_char: usize, deleted: usize, text: &str) -> InputEdit {
        let start_byte = old.char_to_byte(old_char);
        self.advance_to(start_byte);
        let start_position = self.point();

        let old_end_byte = old.char_to_byte(old_char + deleted);
        let old_end_position = if deleted == 0 {
            start_position
        } else {
            self.advance_to(old_end_byte);
            self.point()
        };

        InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte: start_byte + text.len(),
            start_position,
            old_end_position,
            new_end_position: advance_point(start_position, text),
        }
    }
}

/// Advances a point over inserted text using tree-sitter's newline rules.
fn advance_point(start: Point, text: &str) -> Point {
    match text.rfind('\n') {
        Some(last_newline) => Point {
            row: start.row + text.bytes().filter(|&byte| byte == b'\n').count(),
            column: text.len() - last_newline - 1,
        },
        None => Point {
            row: start.row,
            column: start.column + text.len(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::generate_edits;
    use kataba_core::{Change, ChangeSet, Rope};
    use tree_sitter::{InputEdit, Point};

    fn point(row: usize, column: usize) -> Point {
        Point { row, column }
    }

    fn edit(
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
        start_position: Point,
        old_end_position: Point,
        new_end_position: Point,
    ) -> InputEdit {
        InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        }
    }

    #[test]
    fn unchanged_document_yields_no_edits() {
        let doc = Rope::from_str("hello\nworld");
        assert!(generate_edits(&doc, &ChangeSet::from_doc(&doc)).is_empty());
        assert!(generate_edits(&doc, &ChangeSet::empty()).is_empty());
    }

    #[test]
    fn delete_mid_line() {
        let doc = Rope::from_str("hello world");
        let changeset = ChangeSet::from_change(&doc, (5, 6, None));
        assert_eq!(
            generate_edits(&doc, &changeset),
            vec![edit(5, 6, 5, point(0, 5), point(0, 6), point(0, 5))]
        );
    }

    #[test]
    fn insert_with_newlines_tracks_rows() {
        let doc = Rope::from_str("ab\ncd");
        let changeset = ChangeSet::from_change(&doc, (5, 5, Some("X\nYZ\n".into())));
        assert_eq!(
            generate_edits(&doc, &changeset),
            vec![edit(5, 5, 10, point(1, 2), point(1, 2), point(3, 0))]
        );
    }

    #[test]
    fn replacement_is_a_single_edit() {
        let doc = Rope::from_str("abcdef");
        let changeset = ChangeSet::from_change(&doc, (1, 3, Some("X".into())));
        assert_eq!(
            generate_edits(&doc, &changeset),
            vec![edit(1, 3, 2, point(0, 1), point(0, 3), point(0, 2))]
        );
    }

    #[test]
    fn multibyte_columns_count_bytes() {
        let doc = Rope::from_str("héllo");
        // Delete the `é` (chars 1..2; bytes 1..3).
        let changeset = ChangeSet::from_change(&doc, (1, 2, None));
        assert_eq!(
            generate_edits(&doc, &changeset),
            vec![edit(1, 3, 1, point(0, 1), point(0, 3), point(0, 1))]
        );
    }

    #[test]
    fn lone_carriage_return_does_not_advance_row() {
        let doc = Rope::from_str("a\r\nb\rcd");
        // Delete `d`: the `\r` on row 1 is a column byte, not a line break.
        let changeset = ChangeSet::from_change(&doc, (6, 7, None));
        assert_eq!(
            generate_edits(&doc, &changeset),
            vec![edit(6, 7, 6, point(1, 3), point(1, 4), point(1, 3))]
        );
    }

    #[test]
    fn multiple_edits_accumulate_forward() {
        let doc = Rope::from_str("one\ntwo\nthree");
        let replacement: Vec<Change> = vec![
            (4, 7, Some("2".into())),    // "two" -> "2"
            (8, 13, Some("III".into())), // "three" -> "III"
        ];
        let changeset = ChangeSet::from_changes(&doc, replacement.into_iter());
        assert_eq!(
            generate_edits(&doc, &changeset),
            vec![
                edit(4, 7, 5, point(1, 0), point(1, 3), point(1, 1)),
                edit(8, 13, 11, point(2, 0), point(2, 5), point(2, 3)),
            ]
        );
    }

    #[test]
    fn delete_across_lines() {
        let doc = Rope::from_str("a\nbc\nd");
        // Delete "\nbc" (chars 1..4).
        let changeset = ChangeSet::from_change(&doc, (1, 4, None));
        assert_eq!(
            generate_edits(&doc, &changeset),
            vec![edit(1, 4, 1, point(0, 1), point(1, 2), point(0, 1))]
        );
    }
}
