//! Shared helpers for kataba-treesitter integration tests.
//!
//! Fixture grammars are picked up from the sibling Helix checkout's prebuilt
//! runtime (`<workspace>/helix/runtime/grammars`), or from the directory named
//! by `KATABA_TREESITTER_GRAMMAR_DIR`. Compiled grammars are per-machine build
//! artifacts, so when no fixture is available the dependent tests skip with a
//! note instead of failing.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use kataba_treesitter::{load_language, Grammar};
use proptest::test_runner::TestCaseError;

/// Skip notes are printed once per test binary, not once per proptest case.
static WARNED: AtomicBool = AtomicBool::new(false);

fn warn_once(message: &str) {
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!("{message}");
    }
}

/// Directory that holds compiled fixture grammars, if any.
pub fn fixture_grammar_dir() -> PathBuf {
    std::env::var_os("KATABA_TREESITTER_GRAMMAR_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../helix/runtime/grammars"),
        PathBuf::from,
    )
}

/// Path of the compiled fixture grammar `name`, or `None` if it is absent.
pub fn fixture_library_path(name: &str) -> Option<PathBuf> {
    // `DLL_SUFFIX` already includes the dot (`"so"` / `".so"` on Linux).
    let path = fixture_grammar_dir().join(format!("{name}{}", std::env::consts::DLL_SUFFIX));
    if path.exists() {
        Some(path)
    } else {
        warn_once(&format!(
            "kataba-treesitter: skipping fixture tests — no grammar at {}",
            path.display()
        ));
        None
    }
}

/// Loads fixture grammar `name` through the real loading path, or `None` when
/// no usable fixture is available.
pub fn fixture_grammar(name: &str) -> Option<Grammar> {
    let path = fixture_library_path(name)?;
    match load_language(name, &path) {
        Ok(grammar) => Some(grammar),
        Err(error) => {
            warn_once(&format!(
                "kataba-treesitter: skipping fixture tests — cannot load {}: {error}",
                path.display()
            ));
            None
        }
    }
}

/// Turns any error into a proptest failure reason.
pub fn fail(error: impl std::fmt::Display) -> TestCaseError {
    TestCaseError::fail(error.to_string())
}
