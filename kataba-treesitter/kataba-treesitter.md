# Kataba Treesitter: Unified Vision, Architecture, Feature Reference & Development Diary

> Module: `kataba-treesitter` · Version: 0.1.0 (M0 complete) · Depends on: upstream `tree-sitter` 0.25 (ABI window 13..=15), `libloading` 0.8, `kataba-core`
>
> This document is the living blueprint for Kataba's syntax engine layer. It is deliberately
> framed like `KATABA_CORE.md` and `KATABA_LSP.md`: axioms, module audit, roadmap, and a diary
> that records what is actually built versus what is planned.

## Table of Contents

1. [Section 1: Executive Vision & Architectural Foundations](#section-1-executive-vision--architectural-foundations)
   - 1.1 The Four-Way Synthesis
   - 1.2 The Kataba Strategic Wedge
   - 1.3 Explicit Scope: In-Scope vs. Out-of-Scope
   - 1.4 Core Axioms (Set T)
   - 1.5 Coordinate Spaces & The Edit Translation Invariant
   - 1.6 The Grammar Pipeline Boundary (`kataba-store`)
   - 1.7 Competitive Comparison
   - 1.8 Phased Engineering Roadmap (M0 to M6)
2. [Section 2: Detailed Architectural Layers & Module Audit](#section-2-detailed-architectural-layers--module-audit)
   - 2.1 Architectural Hierarchy Diagram
   - 2.2 Layer 0: Edit Translation
   - 2.3 Layer 1: Grammar Loading (the unsafe island)
   - 2.4 Layer 2: Parsing
   - 2.5 Error Surface
   - 2.6 Code Quality & Safety Scorecard
3. [Section 3: Development Diary & Milestones](#section-3-development-diary--milestones)

---

# Section 1: Executive Vision & Architectural Foundations

## 1.1 The Four-Way Synthesis

Kataba's syntax layer synthesizes four systems, taking the best of each:

| Source | What we take | What we reject |
|---|---|---|
| **Helix** | The grammar model: pinned revisions, local compile, load-only at runtime; the `generate_edits` shape; trust gates for project-local configuration | tree-house as the binding (fork lag, Helix-controlled ABI window, zeroed input-edit points); CLI-only grammar install |
| **Neovim** | Query-driven everything (highlights, locals, injections as data, not code); per-language configuration richness | The Lua/plugin-host requirement; user-supplied grammar build scripts as the primary path |
| **Emacs** | `treesit`'s clean separation of engine vs. language definitions in user config | Buffer-locality-first design that makes multi-language files awkward |
| **mason.nvim / lazy.nvim** | The install UX contract: an in-editor menu, per-language install, status, update | The menu itself — that is UI, and lives above this module |

The synthesis is a Rust-only engine that upstream tree-sitter tracks directly, verified by
property tests, with a clean boundary to a future package-manager module (`kataba-store`).

## 1.2 The Kataba Strategic Wedge

What makes `kataba-treesitter` better than the incumbents **for Kataba's goals**:

1. **Verified incremental equivalence (Axiom T1).** The incumbents assume that "edit the tree,
   reparse" matches a full reparse; none of them test it. Kataba makes it a machine-checked
   invariant: a proptest suite compares incremental and full parses **node-for-node — kinds,
   byte ranges, and points** — over random documents and random multi-edit scripts. This
   harness already caught a real ordering bug during M0 (see §1.5 and §3.2).
2. **Fully computed input-edit points.** Helix/tree-house ship `Point::ZERO` in every
   `InputEdit` (bytes only). tree-sitter's `ts_tree_edit` does real point arithmetic
   (`point_add`/`point_sub`) with those fields, so zeroed points corrupt node positions in
   edited regions until a full reparse. Kataba computes `(row, column-in-bytes)` exactly,
   using tree-sitter's own newline semantics.
3. **ABI gating from the runtime, never hardcoded.** The accepted window is read from
   `tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION ..= tree_sitter::LANGUAGE_VERSION` at compile
   time from the linked crate. `probe_abi` lets the store layer record and preflight grammar
   compatibility *before* a user ever opens a file (`doctor`-style diagnostics).
4. **Cooperative parse budgets.** Long parses cancel through tree-sitter's own progress
   callback (`parse_with_options`) and surface as an explicit `BudgetExceeded` outcome — the
   caller keeps the previous tree rather than blocking a thread.
5. **Upstream crate, zero fork surface.** We bind the official `tree-sitter` crate and write
   the (small, audited) dynamic loading ourselves, instead of adopting a downstream fork whose
   ABI window and release cadence are governed by another editor.
6. **A clean store boundary.** Fetching, building, installing, updating, and trusting grammars
   is `kataba-store`'s job; this crate only *loads a path* and validates it. The two compose
   without either owning the other's concerns.

## 1.3 Explicit Scope: In-Scope vs. Out-of-Scope

### In-Scope (What `kataba-treesitter` Owns)

- Translating `kataba-core` `ChangeSet`s into tree-sitter `InputEdit`s (Axiom T1).
- Dynamic grammar loading with symbol, null, and ABI validation (Axioms T2, T4).
- Parsing: full and incremental, rope-backed, zero-copy chunk reads, budgeted cancellation
  (Axiom T3).
- (M1+) Query compilation, hot reload, and query cursor execution.
- (M2+) Highlight capture extraction with byte-accurate, UTF-16-convertible ranges.
- (M3+) Layer tree: injections, layer-local invalidation, per-layer parse reuse.
- (M4+) Things registry (captures-as-facts) and the locals service.
- (M5+) Folds, indent computation, rainbow delimiters as derived facts.
- (M6+) Corpus tests, fuzz targets, hardening.

### Out-of-Scope (Strict Non-Goals)

- **Grammar acquisition of any kind** — fetching, building, installing, updating, removing,
  trusting. That is `kataba-store` (see §1.6). No `build.rs` auto-fetch, ever.
- The install menu UI (lazy.nvim/mason-style flows) — the *backend contract* is store-side;
  the menu is a UI consumer.
- Syntax-config parsing (`languages.toml`-alikes) — belongs to `kataba-config` + store glue.
- LSP semantic-token merging — the *merge* happens above, in `kataba-syntax`.
- Incremental *re-parse scheduling* policy (debouncing, background threads) — the workspace
  layer decides; this crate provides deterministic, budgeted entry points.
- FFI/Lua/WASM hosts of any kind (deferred to ≥ 0.2 by the Rust-only mandate).

## 1.4 Core Axioms (Set T)

**Axiom T1 (Incremental Equivalence).** For every document `O`, change set `C`, and
`N = C(O)`: `parse_incremental(O, tree(O), N, C)` returns a tree structurally identical to
`parse(N)` — same node kinds, byte ranges, points, child counts, and missing/error flags.
Verified by the proptest harness in `tests/equivalence.rs`.

**Axiom T2 (ABI Gating).** A `Language` is only ever handed out for ABI versions inside
`supported_abi_range()`, which is derived from the linked tree-sitter crate's constants.
`probe_abi` reports out-of-window grammars without rejecting them, so tools can explain *why*
a grammar will not load.

**Axiom T3 (Bounded Work).** Every parse entry point that can take unbounded time accepts an
optional wall-clock budget and cancels cooperatively; cancellation is an outcome
(`BudgetExceeded`), never a silent partial tree and never an error to panic on.

**Axiom T4 (Library Lifetime Safety).** Grammar libraries are never unloaded. A `Language` is
a raw pointer into the library's static data; any unload could dangle it while trees, queries,
or cached metadata still reference it. We `mem::forget` the `Library` by design (the same
policy as Helix/tree-house), and the "leak" is one mapping per grammar actually used.

**Axiom T5 (Zero API-Level `unsafe`).** All `unsafe` in this crate is confined to `load.rs`
— currently four documented blocks, all around `dlopen`, symbol lookup, entry-point call, and
`LanguageFn` construction. Both `#![deny(unsafe_code)]` (crate) and a module-level
`#![allow(unsafe_code)]` (only in `load.rs`) make violations compile errors everywhere else.

**Axiom T6 (No Ambient I/O).** Loading reads exactly the path it is given; parsing reads
exactly the rope it is given. No environment sniffing, no auto-fetch, no hidden `HomeDir`
lookups.

## 1.5 Coordinate Spaces & The Edit Translation Invariant

Three coordinate systems meet in this module, and confusing them is where incremental
parsing bugs live:

1. **kataba-core**: character indices over `Rope` (ropey), LSP-facing positions already exist
   separately.
2. **tree-sitter**: byte offsets for ranges, plus `Point { row, column }` where **rows advance
   on `\n` bytes only** and **columns count bytes since the last `\n`** (not UTF-16, not
   chars, not Unicode line breaks).
3. **LSP**: UTF-16 code units (converted elsewhere; this module stays in engine space).

The translation rule (Axiom T1) is:

- `generate_edits(old, changeset)` walks the change set once and emits `InputEdit`s **in
  document order, with all coordinates in the old document**. Points are computed by a
  monotonic scanner over the rope's chunks (one pass up to the last edit), with newline
  handling that deliberately ignores ropey's configured Unicode line breaks — only `\n`
  counts, matching tree-sitter exactly. Old-end points come from the old text; new-end points
  are advanced over the inserted text itself (so an inserted `"a\nbb"` moves rows correctly).
- **Edits must then be applied last-first.** tree-house states the rule in a comment:
  *"Apply the edits in reverse. If we applied them in order then edit 1 would disrupt the
  positioning of edit 2."* Our M0 proptest rediscovered exactly this failure mode when the
  first implementation applied them forward; `parse_incremental` now applies them in reverse.
- Replacements (`Insert` followed by `Delete`, as `ChangeSet::from_change` produces) are folded
  into **one** `InputEdit`, so tree-sitter never observes a zero-width intermediate state.

Known divergence to track: `kataba-core`'s `ropey` dependency uses default features, which
include `unicode_lines` (lone `\r`, VT, FF, NEL, U+2028/2029 count as line breaks there).
Helix pins `ropey = { default-features = false, features = ["simd"] }`, making its line
semantics `\n`/`\r\n` only. `kataba-treesitter` sidesteps the question entirely by scanning
`\n` itself; a future `kataba-core` task may align core's ropey features with Helix so all
consumers share one line model.

## 1.6 The Grammar Pipeline Boundary (`kataba-store`)

Grammar *lifecycle* is a separate module. The contract between the two:

| Concern | Owned by | Interface |
|---|---|---|
| Which grammars, from where, at which revision | `kataba-store` | manifest + trust gate; shells out to `git`; never `build.rs` auto-fetch |
| Compiling `.so`/`.dylib`/`.dll` (single language or `--all`, in parallel) | `kataba-store` | `cc` locates a compiler; `threadpool` fans builds out |
| Per-grammar metadata (name, source rev, ABI, build time, hash) | `kataba-store` | sidecar meta manifest; ABI read via `kataba_treesitter::probe_abi` |
| Install/remove/update/status CLI + in-editor menu backend | `kataba-store` | emits `StoreEvent::{Installed, Removed}` |
| Runtime ABI preflight and diagnostics | both | `probe_abi` reports; `load_language` enforces |
| Loading a resolved artifact | `kataba-treesitter` | `load_language(name, path) -> Grammar` |

`kataba-treesitter` never knows how a `.so` appeared on disk; `kataba-store` never parses.

## 1.7 Competitive Comparison

Legend: ✅ done · ◐ partial · ○ absent · 🔜 planned

| Capability | Helix | Neovim | Emacs (`treesit`) | Kataba |
|---|---|---|---|---|
| Grammar install | CLI only (`fetch`/`build`) | Lua plugin (nvim-treesitter) | manual / distro | 🔜 store: per-language + `--all`, menu backend |
| Loading | tree-house (fork) | own dlopen | shared lib | upstream crate + `libloading`, audited island |
| ABI handling | window fixed by tree-house fork | per-parser checks | compile-time | ✅ window read from linked runtime + `probe_abi` |
| Input-edit points | zeroed (`Point::ZERO`) | computed (C internals) | C internals | ✅ fully computed, `\n`-exact |
| Edit application order | reverse (documented in tree-house) | per-change | n/a | ✅ reverse, tested |
| Incremental ≡ full reparse | untested | untested | untested | ✅ property-tested (node-for-node) |
| Cooperative parse budget | timeout inside tree-house | timeout option | ○ | ✅ progress-callback budget → `BudgetExceeded` |
| Hot-reloadable queries | ○ (restart) | ◐ (plugin reload) | ○ | 🔜 M1 (ArcSwap) |
| Injection layers w/ local invalidation | ✅ tree-house layers | ✅ (Lua-injected) | ◐ basic ranges | 🔜 M3 (layer-local invalidation) |
| Locals as a service | locals.scm per language | locals queries | basic | 🔜 M4 (binding service) |
| Folds / indent / rainbow as facts | computed ad hoc | plugin | basic | 🔜 M5 (universal query contract) |

## 1.8 Phased Engineering Roadmap (M0 to M6)

- **M0 — Foundation (this crate's first landing): ✅ DONE.**
  `edits.rs` (ChangeSet → InputEdit), `load.rs` (dlopen + symbol + ABI window), `parser.rs`
  (rope chunk reader, full + incremental parses, budgeted cancellation), `error.rs`, plus the
  equivalence proptest and loading tests.
- **M1 — Queries.** Query compilation against a `Language`, error surfaces with row/column
  diagnostics, hot reload (ArcSwap), cursor execution helpers. Gate: a query test corpus per
  language key; reload-under-load test.
- **M2 — Highlights.** Capture extraction to byte-ranged `HighlightSpan`s with injectable
  priority, combined ranges for the UI, LSP semantic-token merge point defined (merge lives
  above). Gate: golden highlight tests against fixture grammars.
- **M3 — Layers.** Injection parsing via `set_included_ranges`, per-layer trees, layer-local
  invalidation (a change only dirties layers whose ranges overlap it). Gate: proptest
  extension — layered trees equal full layered reparses.
- **M4 — Things & Locals.** Capture→fact registry ("Things"), locals as a first-class binding
  service with scoped resolution. Gate: scoping property tests (definition ≺ use ordering).
- **M5 — Folds, Indent, Rainbow.** Derived facts with sticky folds across reparse; indent
  from node queries; rainbow depth. Gate: fold-stability tests across incremental updates.
- **M6 — Hardening.** Corpus/fuzz tests, grammar matrix CI, doctor integration with
  `kataba-store`, performance budgets (parse latency percentiles per file size class).

### Concrete Verification Checkpoints

- **M0 gate (met):** `cargo test --workspace` = 73/73 green (49 core, 10 lsp, 14
  treesitter); 2000-case release-mode proptest run green; clippy clean for the package;
  `cargo fmt` clean for the package.
- **M1 gate:** query corpus green across ≥ 3 grammars; reload race test green.
- **M3 gate:** layered-equivalence proptest green; invalidation test shows no
  cross-layer reparses on disjoint edits.
- **Per-milestone:** every new unsafe-free guarantee graphed against Axioms T1–T6.

---

# Section 2: Detailed Architectural Layers & Module Audit

## 2.1 Architectural Hierarchy Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│ kataba-workspace / kataba-syntax   (future: scheduling, merge,   │
│                                     highlights consumer, UI feeds)│
├──────────────────────────────────────────────────────────────────┤
│ kataba-treesitter (THIS CRATE)                                    │
│                                                                   │
│   parser.rs ── SyntaxParser (full / incremental / budgeted)       │
│      │  rope chunk reader (zero-copy, via Rope::chunk_at_byte)    │
│      │                                                            │
│   edits.rs ── generate_edits(old: &Rope, cs: &ChangeSet)          │
│      │        PointScanner (monotonic, '\n'-exact)                │
│      │                                                            │
│   load.rs ── load_language / probe_abi / abi window (UNSAFE ISLE) │
│      │        libloading + LanguageFn + mem::forget policy        │
│      │                                                            │
│   error.rs ── TreeError (DlOpen, DlSym, NullLanguage,             │
│               IncompatibleAbi, SetLanguage, ParseFailed)          │
├──────────────────────────────────────────────────────────────────┤
│ kataba-core ── Rope (+chunk_at_byte), ChangeSet, Tendril, Point   │
└──────────────────────────────────────────────────────────────────┘
        ▲                                    ▲
        │ Grammar { language, abi }          │ .so artifacts + meta
        │                                    │
   (future kataba-syntax)              kataba-store (planned module)
```

## 2.2 Layer 0: Edit Translation (`src/edits.rs`)

- **Contract:** Axiom T1. Pure function from `(old rope, change set)` to `Vec<InputEdit>`.
- **Design:** single forward walk of `Operation`s; `Retain` advances, `Delete` emits a
  deletion edit, `Insert` peeks for a paired `Delete` and folds a replacement into one edit.
  All byte/point math is old-document based; new-end points advance over inserted text.
- **Why a `PointScanner`:** ropey's line APIs are configured with `unicode_lines` in core
  (see §1.5), so tree-sitter-exact points are computed by scanning `\n` bytes over ropey
  chunks — one monotonic pass per change set, O(prefix up to the last edit). A future
  optimization can replace the scan with a line index if core aligns its ropey features.
- **Tests (8 unit):** unchanged docs; mid-line delete; newline-bearing inserts; replacement
  folding; multibyte byte-columns; lone-`\r` row semantics; multi-edit accumulation; deletes
  spanning lines.

## 2.3 Layer 1: Grammar Loading (`src/load.rs`) — the Unsafe Island

- **Contract:** Axioms T2, T4, T5. Safe public API; every `unsafe` block has a SAFETY
  comment; the crate denies `unsafe_code` everywhere else, module-level `allow` only here.
- **Pipeline:** `Library::new(path)` → `library.get::<extern "C" fn() -> *const ()>(
  "tree_sitter_<name>")` (dashes normalized to underscores) → null check → `LanguageFn`
  → `Language::new` → ABI read → window check.
- **`probe_abi`:** reports `{ name, abi, supported }` without rejecting, for store-side
  metadata and doctor diagnostics.
- **Lifetime policy:** `mem::forget(library)` — documented in the module header; the leak is
  bounded by distinct grammars in use (Axiom T4).
- **Tests (4 integration):** ABI window boundary behavior; missing library → `DlOpen`;
  wrong symbol → `DlSym`; fixture grammar loads with reported ABI inside the window and
  `probe_abi` agrees with the loaded `Grammar`.

## 2.4 Layer 2: Parsing (`src/parser.rs`)

- **Contract:** Axioms T1, T3, T6.
- **Reader:** a closure over `Rope::chunk_at_byte` feeds tree-sitter chunked input without
  copying the document; end-of-text returns `""` per contract; a defensive boundary walk
  guards non-boundary offsets.
- **Entry points:**
  - `parse(&Rope) -> Tree` — full parse, no cancellation path (returns the tree directly).
  - `parse_incremental(old_text, old_tree, new_text, changeset, budget) -> ParseOutcome` —
    clones the old tree, applies generated edits **in reverse**, reparses new text with the
    edited tree, optionally under a wall-clock budget via `ParseOptions::progress_callback`.
- **Outcome model:** `ParseOutcome::{Parsed(Tree), BudgetExceeded}`; the only `TreeError`
  reachable from a completed call is an internal-failure `ParseFailed` (tree-sitter returning
  no tree without cancellation).

## 2.5 Error Surface (`src/error.rs`)

`TreeError` is a closed `thiserror` enum: `DlOpen`, `DlSym`, `NullLanguage`,
`IncompatibleAbi { found, supported }`, `SetLanguage`, `ParseFailed`. Invariant: every
failure mode that a user can *fix* (path, symbol, ABI) has a distinct, actionable variant;
bugs surface as `ParseFailed`.

## 2.6 Code Quality & Safety Scorecard

| Concern | Status |
|---|---|
| `unsafe` | 4 documented blocks, all in `load.rs`; denied crate-wide elsewhere |
| Unwraps/panics in library code | 0 (`unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` denied) |
| API-level error handling | `thiserror` throughout; no `anyhow` in library code |
| Logging | none yet in M0 (pure functions + loads); `tracing` arrives with M1 query errors |
| Formats | `cargo fmt` clean for this package (core's pre-existing drift untouched) |
| Clippy | clean for this package across `--all-targets` |
| Dependencies | `tree-sitter`, `tree-sitter-language`, `libloading`, `thiserror`, `kataba-core` (dev: `proptest`) |

---

# Section 3: Development Diary & Milestones

## 3.1 Session Log — M0 Landing (2026-10-16)

Built from zero to a compiling, fully tested M0 in one session:

1. `Cargo.toml` (workspace member added), `src/lib.rs`, `src/error.rs`, `src/edits.rs`,
   `src/load.rs`, `src/parser.rs`.
2. `kataba-core`: added `Rope::chunk_at_byte` passthrough (ropey chunks, O(log N), needed for
   zero-copy parse input and exact point scanning).
3. Tests: 8 unit tests (edit translation), equivalence proptest + two-step typing regression
   test, 4 loading tests with fixture skip logic.
4. Resolved dependency versions at landing: `tree-sitter 0.25.10` (vendored ABI window
   13..=15), `tree-sitter-language 0.1.9`, `libloading 0.8.9`.

## 3.2 Findings Recorded During M0

- **Edit ordering is load-bearing.** The first `parse_incremental` applied generated edits in
  forward order; the proptest failed immediately (minimal case: delete at offset 0 + insert at
  EOF). tree-house's comment confirms the rule; edits must be applied last-first. The docs of
  `generate_edits` and `parse_incremental` now state this explicitly.
- **Helix ships zeroed points.** `helix-core::generate_edits` fills `Point::ZERO` for all six
  point fields; `ts_tree_edit` uses them in real arithmetic. Kataba computes them exactly, and
  the equivalence test compares `start_position`/`end_position` per node.
- **`ropey` feature drift.** Core enables ropey defaults (`unicode_lines`, `simd`); Helix runs
  `default-features = false, features = ["simd"]`. This crate is immune (scans `\n` itself),
  but core's line model is noted as a future alignment task.
- **Fixture grammar**: the sibling Helix checkout's prebuilt `json.so` (ABI 14) loads cleanly
  through `load_language`, exercising a mid-window ABI value.

## 3.3 Current Test Verification Status (2026-10-16)

- `cargo test --workspace` (from `kataba/`): **73 passed, 0 failed** —
  49 `kataba-core`, 10 `kataba-lsp`, 8 treesitter unit, 2 treesitter equivalence
  (256 proptest cases + scripted typing test), 4 treesitter loading.
- Release-mode stress: `PROPTEST_CASES=2000` equivalence run green.
- `cargo clippy -p kataba-treesitter --all-targets`: clean.
- `cargo fmt -p kataba-treesitter -- --check`: clean.

## 3.4 Known Limitations & Next Steps

- **Point scanning cost:** O(bytes up to the last edit) per change set; fine for M0–M2,
  revisit with a line index in M6 hardening (or when core aligns ropey features).
- **Budget test:** no deterministic unit test for `BudgetExceeded` yet (tiny documents finish
  before the first progress check); M1 adds a large-synthetic-document test.
- **Fixture availability:** equivalence/load tests skip (with a note) on machines without a
  compiled grammar; CI for M1+ should vendor a small grammar or build `json` in a job.
- **Next milestone:** M1 — query compilation with diagnostics, hot reload (ArcSwap), cursor
  helpers. Then `kataba-store` for the grammar lifecycle and in-editor install backend.
