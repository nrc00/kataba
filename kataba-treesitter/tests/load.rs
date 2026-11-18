//! Loading and ABI-gating tests, using a real compiled grammar when one is
//! available on the machine (see `common`).

use proptest::test_runner::TestCaseError;

use kataba_treesitter::{
    is_abi_supported, load_language, probe_abi, supported_abi_range, TreeError,
};

mod common;

use common::{fail, fixture_grammar, fixture_library_path};

#[test]
fn supported_abi_window_rejects_out_of_range() {
    let window = supported_abi_range();
    let min = *window.start();
    let max = *window.end();

    assert!(min >= 1);
    assert!(max >= min);
    assert!(is_abi_supported(min));
    assert!(is_abi_supported(max));
    assert!(!is_abi_supported(min - 1));
    assert!(!is_abi_supported(max + 1));
}

#[test]
fn missing_library_reports_dl_open() {
    let result = load_language("json", std::path::Path::new("/nonexistent/kataba/json.so"));
    assert!(matches!(result, Err(TreeError::DlOpen { .. })));
}

#[test]
fn wrong_symbol_reports_dl_sym() {
    let Some(path) = fixture_library_path("json") else {
        return;
    };
    let result = load_language("definitely-not-a-grammar", &path);
    assert!(matches!(result, Err(TreeError::DlSym { .. })));
}

#[test]
fn loads_fixture_grammar_with_supported_abi() -> Result<(), TestCaseError> {
    let Some(grammar) = fixture_grammar("json") else {
        return Ok(());
    };
    assert_eq!(grammar.name(), "json");
    assert!(is_abi_supported(grammar.abi()));

    let Some(path) = fixture_library_path("json") else {
        return Ok(());
    };
    let probe = probe_abi("json", &path).map_err(fail)?;
    assert_eq!(probe.name, "json");
    assert_eq!(probe.abi, grammar.abi());
    assert!(probe.is_compatible());
    assert_eq!(probe.supported, supported_abi_range());
    Ok(())
}
