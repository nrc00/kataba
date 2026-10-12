//! Cursor motion functions for the editor core.
//!
//! The `move_*` kernels compute new character positions (pure, no side
//! effects). Horizontal steps are grapheme-aware. The `*_sel` layer is
//! selection-first: every motion maps over all ranges, vertical motions
//! preserve [`Selection::desired_col`], and horizontal motions reset it.

use crate::error::Error;
use crate::range::Range;
use crate::rope::Rope;
use crate::selection::Selection;
use unicode_segmentation::GraphemeCursor;
use unicode_width::UnicodeWidthChar;

// ---------- grapheme stepping ----------

fn char_to_byte(text: &str, char_pos: usize) -> usize {
    text.char_indices()
        .nth(char_pos)
        .map_or(text.len(), |(i, _)| i)
}

fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

/// Previous grapheme boundary before `pos` (char index).
#[must_use]
pub fn prev_grapheme_boundary(rope: &Rope, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let text = rope.to_string();
    let byte = char_to_byte(&text, pos.min(rope.len()));
    let mut cursor = GraphemeCursor::new(byte, text.len(), true);
    match cursor.prev_boundary(&text, 0) {
        Ok(Some(b)) => byte_to_char(&text, b),
        _ => pos.saturating_sub(1),
    }
}

/// Next grapheme boundary after `pos` (char index).
#[must_use]
pub fn next_grapheme_boundary(rope: &Rope, pos: usize) -> usize {
    let len = rope.len();
    if pos >= len {
        return len;
    }
    let text = rope.to_string();
    let byte = char_to_byte(&text, pos);
    let mut cursor = GraphemeCursor::new(byte, text.len(), true);
    match cursor.next_boundary(&text, 0) {
        Ok(Some(b)) => byte_to_char(&text, b),
        _ => (pos + 1).min(len),
    }
}

// ---------- visual columns ----------

/// Display width of a char in cells (tab counts as 1; view layer owns tab stops).
fn char_width(c: char) -> usize {
    if c == '\t' {
        1
    } else {
        c.width().unwrap_or(0)
    }
}

/// Display column of `pos` within its line.
#[must_use]
pub fn visual_col(rope: &Rope, pos: usize) -> u32 {
    let pos = pos.min(rope.len());
    let start = rope.line_to_char(rope.char_to_line(pos));
    let mut col = 0u32;
    for i in start..pos {
        if let Some(c) = rope.char(i) {
            col += char_width(c) as u32;
        }
    }
    col
}

/// End of `line` as a gap index, excluding the trailing newline.
fn line_end_char(rope: &Rope, line: usize) -> usize {
    if line + 1 < rope.len_lines() {
        rope.line_to_char(line + 1).saturating_sub(1)
    } else {
        rope.len()
    }
}

/// Gap index on `line` at display column `col` (clamped; snaps before wide chars).
#[must_use]
pub fn pos_at_visual_col(rope: &Rope, line: usize, col: u32) -> usize {
    if line >= rope.len_lines() {
        return rope.len();
    }
    let start = rope.line_to_char(line);
    let end = line_end_char(rope, line);
    let mut acc = 0u32;
    let mut pos = start;
    while pos < end {
        let c = match rope.char(pos) {
            Some(c) => c,
            None => break,
        };
        if acc + char_width(c) as u32 > col {
            break;
        }
        acc += char_width(c) as u32;
        pos += 1;
    }
    pos
}

// ---------- character-level motions ----------

/// Moves the cursor left by one grapheme cluster.
#[must_use]
pub fn move_left(rope: &Rope, pos: usize) -> usize {
    prev_grapheme_boundary(rope, pos)
}

/// Moves the cursor right by one grapheme cluster.
#[must_use]
pub fn move_right(rope: &Rope, pos: usize) -> usize {
    next_grapheme_boundary(rope, pos)
}

/// Moves the cursor to the start of the document.
#[must_use]
pub fn move_to_start(_rope: &Rope, _pos: usize) -> usize {
    0
}

/// Moves the cursor to the end of the document.
#[must_use]
pub fn move_to_end(rope: &Rope, _pos: usize) -> usize {
    rope.len()
}

/// Moves the cursor up by one line (character-column based).
#[must_use]
pub fn move_up(rope: &Rope, pos: usize) -> usize {
    let line = rope.char_to_line(pos);
    if line == 0 {
        return 0;
    }
    let col = pos - rope.line_to_char(line);
    let target_line = line - 1;
    let target_start = rope.line_to_char(target_line);
    let target_end = rope.line_to_char(target_line + 1).saturating_sub(1);
    let target_len = target_end.saturating_sub(target_start);
    target_start + col.min(target_len)
}

/// Moves the cursor down by one line (character-column based).
#[must_use]
pub fn move_down(rope: &Rope, pos: usize) -> usize {
    let line = rope.char_to_line(pos);
    let total_lines = rope.lines();
    if line + 1 >= total_lines {
        return rope.len();
    }
    let col = pos - rope.line_to_char(line);
    let target_line = line + 1;
    let target_start = rope.line_to_char(target_line);
    let target_end = rope.line_to_char(target_line + 1).saturating_sub(1);
    let target_len = target_end.saturating_sub(target_start);
    target_start + col.min(target_len)
}

/// Moves the cursor to the start of the current line.
#[must_use]
pub fn move_to_line_start(rope: &Rope, pos: usize) -> usize {
    let line = rope.char_to_line(pos);
    rope.line_to_char(line)
}

/// Moves the cursor to the end of the current line.
#[must_use]
pub fn move_to_line_end(rope: &Rope, pos: usize) -> usize {
    let line = rope.char_to_line(pos);
    rope.line_to_char(line + 1).saturating_sub(1)
}

/// Moves the cursor to the start of the previous word.
#[must_use]
pub fn move_to_prev_word(rope: &Rope, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut p = pos.saturating_sub(1);
    while p > 0 {
        let c = rope.char(p);
        if c.is_some_and(|ch| ch.is_whitespace()) {
            break;
        }
        p = p.saturating_sub(1);
    }
    p
}

/// Moves the cursor to the start of the next word.
#[must_use]
pub fn move_to_next_word(rope: &Rope, pos: usize) -> usize {
    let len = rope.len();
    if pos >= len {
        return len;
    }
    let mut p = pos;
    while p < len {
        let c = rope.char(p);
        if c.is_some_and(|ch| !ch.is_whitespace()) {
            break;
        }
        p += 1;
    }
    while p < len {
        let c = rope.char(p);
        if c.is_some_and(|ch| ch.is_whitespace()) {
            break;
        }
        p += 1;
    }
    p
}

/// Deletes the character at the given position.
///
/// Returns `Err(Error::OutOfBounds)` if the position is out of bounds.
pub fn delete_char(rope: &mut Rope, pos: usize) -> Result<(), Error> {
    if pos >= rope.len() {
        return Err(Error::out_of_bounds(pos, rope.len()));
    }
    rope.remove(pos, pos + 1);
    Ok(())
}

/// Deletes the character before the cursor (backspace).
pub fn backspace(rope: &mut Rope, pos: usize) -> Result<(), Error> {
    if pos == 0 {
        return Err(Error::out_of_bounds(0, rope.len()));
    }
    rope.remove(pos - 1, pos);
    Ok(())
}

// ---------- selection-first motions ----------

/// Shifts every range endpoint through `f` and resets `desired_col`.
fn shift_sel(rope: &Rope, sel: &Selection, f: impl Fn(&Rope, usize) -> usize) -> Selection {
    let ranges = sel
        .ranges
        .iter()
        .map(|r| Range::new(f(rope, r.anchor), f(rope, r.head)))
        .collect();
    let mut out = Selection::with_primary(ranges, sel.primary_index());
    out.reset_desired_col();
    out
}

/// Moves all cursors left by one grapheme (resets `desired_col`).
#[must_use]
pub fn move_left_sel(rope: &Rope, sel: &Selection) -> Selection {
    shift_sel(rope, sel, move_left)
}

/// Moves all cursors right by one grapheme (resets `desired_col`).
#[must_use]
pub fn move_right_sel(rope: &Rope, sel: &Selection) -> Selection {
    shift_sel(rope, sel, move_right)
}

/// Moves all cursors to the document start (resets `desired_col`).
#[must_use]
pub fn move_to_start_sel(rope: &Rope, sel: &Selection) -> Selection {
    shift_sel(rope, sel, move_to_start)
}

/// Moves all cursors to the document end (resets `desired_col`).
#[must_use]
pub fn move_to_end_sel(rope: &Rope, sel: &Selection) -> Selection {
    shift_sel(rope, sel, move_to_end)
}

/// Moves all cursors to their line starts (resets `desired_col`).
#[must_use]
pub fn move_to_line_start_sel(rope: &Rope, sel: &Selection) -> Selection {
    shift_sel(rope, sel, move_to_line_start)
}

/// Moves all cursors to their line ends (resets `desired_col`).
#[must_use]
pub fn move_to_line_end_sel(rope: &Rope, sel: &Selection) -> Selection {
    shift_sel(rope, sel, move_to_line_end)
}

/// Moves all cursors to the previous word start (resets `desired_col`).
#[must_use]
pub fn move_to_prev_word_sel(rope: &Rope, sel: &Selection) -> Selection {
    shift_sel(rope, sel, move_to_prev_word)
}

/// Moves all cursors to the next word start (resets `desired_col`).
#[must_use]
pub fn move_to_next_word_sel(rope: &Rope, sel: &Selection) -> Selection {
    shift_sel(rope, sel, move_to_next_word)
}

fn move_vert_sel(rope: &Rope, sel: &Selection, delta: isize) -> Selection {
    let primary_idx = sel.primary_index();
    let primary_head = sel.primary().head;
    let desired = sel
        .desired_col()
        .unwrap_or_else(|| visual_col(rope, primary_head));
    let move_end = |pos: usize, col: u32| {
        let clamped = pos.min(rope.len());
        let line = rope.char_to_line(clamped);
        let total = rope.len_lines().max(1) as isize;
        let target = (line as isize + delta).clamp(0, total - 1) as usize;
        if target == line {
            clamped
        } else {
            pos_at_visual_col(rope, target, col)
        }
    };
    let ranges = sel
        .ranges
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            let head_col = if idx == primary_idx {
                desired
            } else {
                visual_col(rope, r.head)
            };
            Range::new(
                move_end(r.anchor, visual_col(rope, r.anchor)),
                move_end(r.head, head_col),
            )
        })
        .collect();
    let mut out = Selection::with_primary(ranges, primary_idx);
    out.set_desired_col(desired);
    out
}

/// Moves all cursors up one line, preserving `desired_col`.
#[must_use]
pub fn move_up_sel(rope: &Rope, sel: &Selection) -> Selection {
    move_vert_sel(rope, sel, -1)
}

/// Moves all cursors down one line, preserving `desired_col`.
#[must_use]
pub fn move_down_sel(rope: &Rope, sel: &Selection) -> Selection {
    move_vert_sel(rope, sel, 1)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    #[test]
    fn test_motion_basic() {
        let mut rope = Rope::from_str("hello\nworld");
        assert_eq!(move_left(&rope, 3), 2);
        assert_eq!(move_left(&rope, 0), 0);
        assert_eq!(move_right(&rope, 3), 4);
        assert_eq!(move_to_start(&rope, 5), 0);
        assert_eq!(move_to_end(&rope, 0), 11);

        assert_eq!(move_to_line_start(&rope, 8), 6);
        assert_eq!(move_to_line_end(&rope, 2), 5);

        assert_eq!(move_down(&rope, 2), 8);
        assert_eq!(move_up(&rope, 8), 2);

        delete_char(&mut rope, 0).expect("delete_char should succeed");
        assert_eq!(rope.len(), 10);
        backspace(&mut rope, 1).expect("backspace should succeed");
        assert_eq!(rope.len(), 9);
    }

    #[test]
    fn test_grapheme_stepping() {
        // 'é' as e + combining acute: 2 chars, 1 grapheme.
        let rope = Rope::from_str("e\u{301}bc");
        assert_eq!(rope.len(), 4);
        assert_eq!(move_right(&rope, 0), 2);
        assert_eq!(move_right(&rope, 2), 3);
        assert_eq!(move_left(&rope, 2), 0);
        assert_eq!(move_left(&rope, 3), 2);

        // ZWJ family emoji: many chars, 1 grapheme.
        let rope = Rope::from_str("a👨‍👩‍👧b");
        assert_eq!(move_right(&rope, 0), 1);
        assert_eq!(move_right(&rope, 1), rope.len() - 1);
        assert_eq!(move_left(&rope, rope.len() - 1), 1);
    }

    #[test]
    fn test_visual_col_wide_chars() {
        // '中' is double-width.
        let rope = Rope::from_str("a中b");
        assert_eq!(visual_col(&rope, 0), 0);
        assert_eq!(visual_col(&rope, 1), 1);
        assert_eq!(visual_col(&rope, 2), 3);
        assert_eq!(visual_col(&rope, 3), 4);
        // Column 2 lands mid-wide-char: snaps back before it.
        assert_eq!(pos_at_visual_col(&rope, 0, 2), 1);
        assert_eq!(pos_at_visual_col(&rope, 0, 3), 2);
        assert_eq!(pos_at_visual_col(&rope, 0, 99), 3);
    }

    #[test]
    fn test_desired_col_memory() {
        // Long / short / long lines.
        let rope = Rope::from_str("abcdef\nx\nabcdef");
        // Line starts: 0, 7, 9. Cursor at line 0 col 5.
        let sel = Selection::point(5);
        assert_eq!(sel.desired_col(), None);

        // Down to the short line: clamps to its end, remembers col 5.
        let sel = move_down_sel(&rope, &sel);
        assert_eq!(sel.primary().head, 8);
        assert_eq!(sel.desired_col(), Some(5));

        // Down to the long line: restores col 5.
        let sel = move_down_sel(&rope, &sel);
        assert_eq!(sel.primary().head, 14);
        assert_eq!(sel.desired_col(), Some(5));

        // Up to the short line, then up to line 0: col 5 restored, not col 1.
        let sel = move_up_sel(&rope, &sel);
        assert_eq!(sel.primary().head, 8);
        let sel = move_up_sel(&rope, &sel);
        assert_eq!(sel.primary().head, 5);
        assert_eq!(sel.desired_col(), Some(5));

        // Horizontal motion resets the memory.
        let sel = move_left_sel(&rope, &sel);
        assert_eq!(sel.primary().head, 4);
        assert_eq!(sel.desired_col(), None);
    }

    #[test]
    fn test_multi_cursor_vertical() {
        let rope = Rope::from_str("ab\ncd\nef");
        let sel = Selection::with_primary(smallvec![Range::point(0), Range::point(4)], 1);
        let sel = move_down_sel(&rope, &sel);
        assert_eq!(sel.ranges[0], Range::point(3));
        assert_eq!(sel.ranges[1], Range::point(7));
        assert_eq!(sel.primary_index(), 1);
    }
}
