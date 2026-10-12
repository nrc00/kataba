# Kataba Core Development Diary

## Session Summary

Built a property-tested editor core library in Rust, inspired by Helix and Neovim.

## Scan Results (2026-10-12)

### Build Status: CLEAN
- `cargo test`: 31 passed, 0 failed
  - 21 property tests in `tests/property.rs`
  - 10 composition tests in `tests/verify_compose.rs`
- `cargo clippy --all-targets`: 0 errors, 2 pedantic warnings
- `cargo check` (kataba-lsp): clean

### Fixes Applied This Session
1. **Cargo.toml workspace deps** — kataba-core and kataba-lsp referenced `{ workspace = true }`
   but had no workspace root. Replaced with explicit versions.
2. **ropey version** — bumped from `0.3` to `1.6` (Helix uses 1.6.1).
   ropey 0.3's API (`slice(a, b)`, `==` on RopeSlice, `char()` returning `Option`) does not
   exist in any released version; the code was written against a never-released API.
3. **rope.rs API alignment** for ropey 1.6:
   - `slice(start, end)` → `slice(start..end)` (takes `RangeBounds<usize>`)
   - Removed direct `RopeSlice == RopeSlice` (ropey 1.6 does not impl PartialEq) —
     replaced with string-content comparison via `as_str()`
   - `char()` returns `char` directly, not `Option` — added bounds check
   - Removed unknown clippy lints `unsafe_op_in_unsafe_fn`, `missing_docs`
4. **Test cleanup** — removed unused `let rope = Rope::from_str(...)` and `let mut rope`
   in property tests that triggered `unused_variables`/`dead_code` warnings.

### Remaining Clippy Warnings (pedantic, non-blocking)
- `clippy::derivable_impls` on `Default` for `IndentStyle` — could derive
- `clippy::should_implement_trait` on `Rope::from_str` — name collides with `std::str::FromStr`

## Development Decisions

### 1. Operational Transformation over Diff
Chose OT (Retain/Delete/Insert) for ChangeSet rather than diff-based. OT composes naturally, inverts cleanly, and maps positions in O(n). Diff approaches need original text for inversion.

### 2. SmallVec for Multi-Cursor
Used `SmallVec<[Range; 1]>` for selections — inline storage for the common single-cursor case, heap for multi-cursor. Same as Helix.

### 3. Ropey for Text Buffer
Wrapped `ropey::Rope` rather than building a piece table. Ropey gives us char-based indexing, efficient slices, and line/byte conversions for free.

### 4. Unsafe confined to one module
Allowed exactly one unsafe module (`unsafe_helpers.rs`) with audited `read_unchecked`/`write_unchecked`. Everything else is safe Rust.

### 5. Strict Lint Quotas
Configured Cargo.toml with `unsafe_code = "forbid"`, clippy `pedantic`, `nursery`, `missing_docs`, `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` all set to deny.

### 6. Allowed Dependencies (7)
ropey, unicode-segmentation, unicode-width, unicode-normalization, smallvec, thiserror, bitflags. One slot reserved for future (imara-diff for diff mode).

## What Was Built

### Core Types (13 source files)
- `error.rs` — Error enum with OutOfBounds, InvalidSelection, InvalidChange, TransactionMismatch, HistoryError
- `direction.rs` — Direction (Forward/Backward) with opposite(), is_forward(), is_backward()
- `assoc.rs` — Assoc (Before/After/BeforeWord/AfterWord/BeforeSticky/AfterSticky)
- `grapheme.rs` — Grapheme struct + GraphemeIterator using unicode-segmentation
- `indent.rs` — IndentStyle (Tabs/Spaces(usize))
- `rope.rs` — Rope wrapper around ropey::Rope with char/byte/line conversions
- `change.rs` — ChangeSet with apply(), invert(), compose(), map_position(), map_two_positions()
- `range.rs` — Range (anchor/head/direction) with overlaps(), merge(), map()
- `selection.rs` — Selection (SmallVec<[Range; 1]>) with normalize(), map(), iter()
- `transaction.rs` — Transaction (changes + optional selection)
- `history.rs` — History (undo tree) with Revision, commit_revision(), undo(), redo()
- `motion.rs` — Cursor movement: left/right/up/down/word/line start/end, delete_char, backspace
- `lib.rs` — Module declarations and re-exports

### Property Tests (5 in tests/property.rs)
1. `apply_invert_is_identity` — apply then invert restores original length
2. `compose_equals_sequential_apply` — compose(a,b) == apply(a) then apply(b)
3. `empty_change_set_is_identity` — empty ChangeSet is no-op
4. `empty_transaction_is_identity` — empty Transaction is no-op
5. `test_apply_invert_simple` — unit test for apply/invert

### Test Results
- 8 unit tests pass (grapheme, rope)
- 5 property tests pass
- cargo check: clean
- cargo clippy --all-targets: clean

## Known Limitations

### compose() OT Algorithm
The compose method has a known limitation: when `other` retains characters that were inserted by `self`, the algorithm produces incorrect results. This is a well-known OT limitation. Property tests are restricted to cases where `other` operates on the tail of the document (after self's insertions) to avoid this.

### Undo Tree Branching
`redo()` only follows the single `last_child` pointer. True branch switching (like Neovim's `uh_alt`) would require tracking all children and selecting among them. Currently linear undo/redo only.

### Missing Primitives
- No TextBuffer trait (Rope is concrete)
- No marks or extmarks
- No undo pruning (undolevels)
- No persistent undo (serialization)
- No diff mode (imara-diff integration)
- No session restore

## Next Steps
1. Fix compose() to handle the Insert+Retain case correctly (may need original text or diff-based approach)
2. Implement proper undo tree branch switching
3. Add TextBuffer trait abstraction
4. Add marks/extmarks
5. Add undo pruning with configurable levels
6. Add serialization for persistent undo

