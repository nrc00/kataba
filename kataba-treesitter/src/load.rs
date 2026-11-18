//! Dynamic loading of compiled tree-sitter grammars.
//!
//! This is the crate's single, documented `unsafe` island. Everything unsafe
//! is confined to calling the `tree_sitter_<name>` entry point of a shared
//! library and wrapping the returned pointer. All public entry points are
//! safe: they check the symbol, the null case, and the ABI window before a
//! [`Language`] is ever handed out.
//!
//! Loaded libraries are intentionally never unloaded (`mem::forget`): a
//! [`Language`] is a bare pointer into the library's data segment, and trees
//! and queries built from it can outlive any handle we could keep alive, so
//! unloading would be unsound. This mirrors the Helix/tree-house policy; the
//! "leak" is bounded by the number of distinct grammars actually used.
#![allow(unsafe_code)]

use std::ops::RangeInclusive;
use std::path::Path;

use libloading::{Library, Symbol};
use tree_sitter::{Language, LANGUAGE_VERSION, MIN_COMPATIBLE_LANGUAGE_VERSION};
use tree_sitter_language::LanguageFn;

use crate::error::TreeError;

/// The ABI window supported by the linked tree-sitter runtime.
///
/// Read from the runtime constants — never hardcoded — so upgrading the
/// `tree-sitter` dependency automatically moves the window with it.
#[must_use]
pub fn supported_abi_range() -> RangeInclusive<usize> {
    MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION
}

/// Returns `true` if `abi` falls inside [`supported_abi_range`].
#[must_use]
pub fn is_abi_supported(abi: usize) -> bool {
    supported_abi_range().contains(&abi)
}

/// ABI facts about a compiled grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiInfo {
    /// Grammar name the library was probed under.
    pub name: String,
    /// ABI version reported by the grammar itself.
    pub abi: usize,
    /// Window supported by this build of Kataba.
    pub supported: RangeInclusive<usize>,
}

impl AbiInfo {
    /// Returns `true` when the grammar can be handed to a parser.
    #[must_use]
    pub fn is_compatible(&self) -> bool {
        self.supported.contains(&self.abi)
    }
}

/// A compiled grammar that has passed symbol, null, and ABI checks.
#[derive(Debug)]
pub struct Grammar {
    name: String,
    language: Language,
    abi: usize,
}

impl Grammar {
    /// The grammar name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The ABI version reported by the grammar.
    #[must_use]
    pub fn abi(&self) -> usize {
        self.abi
    }

    /// The loaded language, ready for [`tree_sitter::Parser::set_language`].
    #[must_use]
    pub fn language(&self) -> &Language {
        &self.language
    }

    /// Consumes the handle, returning the owned [`Language`].
    #[must_use]
    pub fn into_language(self) -> Language {
        self.language
    }
}

/// Probes `path` for a grammar called `name` and reports its ABI version.
///
/// Unlike [`load_language`], an out-of-window ABI is **reported, not
/// rejected**: the store layer uses this at build time to record ABI metadata
/// and to warn about grammars that would be refused at load time.
pub fn probe_abi(name: &str, path: &Path) -> Result<AbiInfo, TreeError> {
    let language = load_raw(name, path)?;
    Ok(AbiInfo {
        name: name.to_string(),
        abi: language.abi_version(),
        supported: supported_abi_range(),
    })
}

/// Loads grammar `name` from `path`, enforcing the runtime ABI window.
///
/// # Errors
///
/// [`TreeError::DlOpen`] if the library cannot be opened,
/// [`TreeError::DlSym`] if it does not export `tree_sitter_<name>`,
/// [`TreeError::NullLanguage`] if the entry point returns null, and
/// [`TreeError::IncompatibleAbi`] if the grammar is outside
/// [`supported_abi_range`].
pub fn load_language(name: &str, path: &Path) -> Result<Grammar, TreeError> {
    let language = load_raw(name, path)?;
    let abi = language.abi_version();
    if !is_abi_supported(abi) {
        return Err(TreeError::IncompatibleAbi {
            name: name.to_string(),
            found: abi,
            supported: supported_abi_range(),
        });
    }
    Ok(Grammar {
        name: name.to_string(),
        language,
        abi,
    })
}

/// Opens `path`, calls its `tree_sitter_<name>` entry point, and wraps the
/// result. The library is intentionally leaked; see the module docs.
fn load_raw(name: &str, path: &Path) -> Result<Language, TreeError> {
    let symbol_name = format!("tree_sitter_{}", name.replace('-', "_"));

    // SAFETY: `dlopen` only maps the library into the process; no code from it
    // runs until the entry point is called below, and a failing open is a
    // normal, surfaced error.
    let library = unsafe { Library::new(path) }.map_err(|source| TreeError::DlOpen {
        path: path.to_path_buf(),
        source,
    })?;

    let language_fn = {
        // SAFETY: the symbol is looked up but not called yet; its type follows
        // the tree-sitter grammar convention `fn() -> *const TSLanguage`.
        let symbol: Symbol<'_, unsafe extern "C" fn() -> *const ()> =
            unsafe { library.get(symbol_name.as_bytes()) }.map_err(|source| TreeError::DlSym {
                path: path.to_path_buf(),
                symbol: symbol_name.clone(),
                source,
            })?;

        // SAFETY: calling a tree-sitter grammar entry point yields a pointer
        // to a process-lifetime static table; well-formed grammars never
        // return null, and null is rejected before the pointer is used.
        let raw = unsafe { symbol() };
        if raw.is_null() {
            return Err(TreeError::NullLanguage {
                name: name.to_string(),
            });
        }

        // SAFETY: `LanguageFn` requires the wrapped function to be a
        // tree-sitter grammar entry point; that contract is this module's
        // documented precondition (`path` names a compiled grammar).
        unsafe { LanguageFn::from_raw(*symbol) }
    };

    // Keep the library mapped for the process lifetime: the `Language` points
    // into its static data, and trees/queries may outlive any handle we could
    // return. See the module docs.
    std::mem::forget(library);

    Ok(Language::new(language_fn))
}
