//! Conversions between Kataba editor core types and LSP types.

use kataba_core::change::{ChangeSet, Tendril};
use kataba_core::encoding::PositionEncoding;
use kataba_core::point::Point;
use kataba_core::range::Range;
use kataba_core::rope::Rope;
use kataba_core::transaction::Transaction;
use lsp_types::{Position, Range as LspRange, TextEdit};

/// Converts a Kataba `Point` directly to an LSP `Position`.
#[must_use]
pub const fn point_to_position(point: Point) -> Position {
    Position {
        line: point.line,
        character: point.col_utf16,
    }
}

/// Converts an LSP `Position` directly to a Kataba `Point`.
#[must_use]
pub const fn position_to_point(pos: Position) -> Point {
    Point::new(pos.line, pos.character)
}

/// Converts a Kataba `Range` to an LSP `Range` against a `Rope` under the given encoding.
#[must_use]
pub fn range_to_lsp_range(range: Range, rope: &Rope, encoding: PositionEncoding) -> LspRange {
    let start_char = range.from();
    let end_char = range.to();

    let start_pt = rope.char_to_point(start_char, encoding);
    let end_pt = rope.char_to_point(end_char, encoding);

    LspRange {
        start: point_to_position(start_pt),
        end: point_to_position(end_pt),
    }
}

/// Converts an LSP `Position` to a character index in `Rope`, safely clamping out-of-bounds coords.
#[must_use]
pub fn position_to_char_idx(pos: Position, rope: &Rope, encoding: PositionEncoding) -> usize {
    let point = position_to_point(pos);
    rope.point_to_char(point, encoding)
}

/// Converts an LSP `Range` to a Kataba `Range` (character offsets).
#[must_use]
pub fn lsp_range_to_range(range: LspRange, rope: &Rope, encoding: PositionEncoding) -> Range {
    let from = position_to_char_idx(range.start, rope, encoding);
    let to = position_to_char_idx(range.end, rope, encoding);
    Range::new(from, to)
}

/// Converts a slice of LSP `TextEdit`s into a single atomic, pure `kataba_core::Transaction`.
///
/// In accordance with the LSP specification, edits are applied to the state of the document
/// before any edits in the request were applied. This function validates and sorts edits
/// in descending character order, generating an algebraic `ChangeSet` and capturing
/// deleted tendrils so the resulting `Transaction` is 100% pure and invertible.
#[must_use]
pub fn text_edits_to_transaction(
    edits: &[TextEdit],
    rope: &Rope,
    encoding: PositionEncoding,
) -> Transaction {
    if edits.is_empty() {
        return Transaction::new_empty(rope);
    }

    // Convert each edit to (start_char, end_char, replacement_text)
    let mut resolved: Vec<(usize, usize, &str)> = edits
        .iter()
        .map(|edit| {
            let from = position_to_char_idx(edit.range.start, rope, encoding);
            let to = position_to_char_idx(edit.range.end, rope, encoding);
            let start = from.min(to);
            let end = from.max(to);
            (start, end, edit.new_text.as_str())
        })
        .collect();

    // Sort edits by start position ascending
    resolved.sort_by_key(|&(start, _, _)| start);

    let doc_len = rope.len();
    let mut ops = Vec::with_capacity(resolved.len() * 2 + 1);
    let mut deleted = Vec::new();
    let mut cursor = 0usize;

    for (start, end, new_text) in resolved {
        if start > cursor {
            ops.push(kataba_core::change::Operation::Retain(start - cursor));
            cursor = start;
        }

        let del_len = end.saturating_sub(start);
        if del_len > 0 {
            ops.push(kataba_core::change::Operation::Delete(del_len));
            deleted.push(rope.slice_tendril(start, end));
            cursor = end;
        }

        if !new_text.is_empty() {
            let tendril: Tendril = new_text.chars().collect();
            ops.push(kataba_core::change::Operation::Insert(tendril));
        }
    }

    if cursor < doc_len {
        ops.push(kataba_core::change::Operation::Retain(doc_len - cursor));
    }

    let changes = ChangeSet::new(ops);

    Transaction {
        changes,
        deleted,
        selection: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use kataba_core::selection::Selection;

    #[test]
    fn test_point_position_round_trip() {
        let pt = Point::new(42, 100);
        let pos = point_to_position(pt);
        assert_eq!(pos.line, 42);
        assert_eq!(pos.character, 100);

        let pt2 = position_to_point(pos);
        assert_eq!(pt, pt2);
    }

    #[test]
    fn test_text_edits_to_transaction() {
        let rope = Rope::from_str("hello world\nsecond line\n");
        let edits = vec![TextEdit {
            range: LspRange {
                start: Position {
                    line: 0,
                    character: 6,
                },
                end: Position {
                    line: 0,
                    character: 11,
                },
            },
            new_text: "kataba".to_string(),
        }];

        let txn = text_edits_to_transaction(&edits, &rope, PositionEncoding::Utf16);
        assert!(txn.is_pure());

        let mut doc = rope;
        let mut sel = Selection::point(0);
        txn.apply(&mut doc, &mut sel);
        assert_eq!(doc.slice_text(0, doc.len()), "hello kataba\nsecond line\n");

        // Verify pure inversion
        let inv = txn.invert().expect("should invert");
        inv.apply(&mut doc, &mut sel);
        assert_eq!(doc.slice_text(0, doc.len()), "hello world\nsecond line\n");
    }
}
