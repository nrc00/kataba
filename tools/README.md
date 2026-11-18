# tools

Development and comparison harnesses that used to live in `~/Projects/tester`.
They are a separate Cargo workspace so the main kataba workspace stays small.

Run them from this directory:

```sh
cargo test -p kataba-tests
cargo test -p kataba-lsp-integration
cargo run -p treesitter-viz -- <file>
cargo run -p undo-demo
```

## Members

- `kataba-tests` — property, compose, and neovim-parity suites for `kataba-core`.
- `kataba-lsp-integration` — end-to-end tests against `kataba-lsp`.
- `treesitter-viz` — small ratatui viewer for tree-sitter output.
- `undo-demo` — undo/redo demo binary.

## Archived comparisons

`cmp-bench`, `diff-test`, and `linear-diff` benchmark kataba-core against
`helix-core`. They are excluded from this workspace because they need a Helix
checkout at `../../helix` (next to `kataba/`):

```sh
git clone https://github.com/helix-editor/helix ../helix
cargo run --manifest-path cmp-bench/Cargo.toml
```
