//! Property: incremental parsing equals a full reparse, node for node.
//!
//! For random documents and random disjoint edit scripts, the tree obtained
//! by editing the old tree and reparsing must be *identical* to the tree a
//! fresh parse produces: same kinds, byte ranges, positions, child counts,
//! and error/missing flags for every node. This is the crate's central
//! correctness invariant (T1), and the main reason `generate_edits` computes
//! full points instead of relying on byte offsets alone.

use kataba_core::{Change, ChangeSet, Rope, Tendril};
use kataba_treesitter::{Grammar, ParseOutcome, SyntaxParser};
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, TestCaseError};
use tree_sitter::{Node, Tree};

mod common;

use common::{fail, fixture_grammar};

/// Small alphabet that stresses newlines, carriage returns, multibyte chars,
/// and JSON punctuation (grammar error recovery included on purpose).
const ALPHABET: [char; 15] = [
    'a', 'b', 'c', ' ', 'é', 'ß', '\n', '\r', '{', '}', '[', ']', ':', ',', '"',
];

fn doc_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(ALPHABET.to_vec()), 0..40)
        .prop_map(|chars| chars.into_iter().collect())
}

/// A raw `(from, to, replacement)` triple in character coordinates; indices
/// are normalized into a sorted, disjoint script during the test.
fn change_spec() -> impl Strategy<Value = (usize, usize, Option<String>)> {
    (
        prop::option::of(prop::collection::vec(
            prop::sample::select(ALPHABET.to_vec()),
            0..8,
        )),
        0usize..40,
        0usize..40,
    )
        .prop_map(|(text, from, to)| {
            let text = text.map(|chars| chars.into_iter().collect::<String>());
            let (from, to) = if from <= to { (from, to) } else { (to, from) };
            (from, to, text)
        })
}

/// Clamps raw specs to the document and sorts them into a disjoint script.
fn normalize_changes(doc: &Rope, specs: Vec<(usize, usize, Option<String>)>) -> Vec<Change> {
    let len = doc.len();
    let mut specs = specs;
    specs.sort_by_key(|(from, _, _)| *from);

    let mut changes: Vec<Change> = Vec::new();
    let mut cursor = 0usize;
    for (from, to, text) in specs {
        let from = from.min(len).max(cursor);
        let to = to.clamp(from, len);
        if from == to && text.as_deref().is_none_or(str::is_empty) {
            continue;
        }
        changes.push((from, to, text.map(Tendril::from)));
        cursor = to;
    }
    changes
}

/// Structural equality of two trees, including positions and error flags.
fn trees_equal(actual: &Tree, expected: &Tree) -> bool {
    nodes_equal(&actual.root_node(), &expected.root_node(), 0)
}

fn nodes_equal(actual: &Node<'_>, expected: &Node<'_>, depth: usize) -> bool {
    if depth > 64
        || actual.kind_id() != expected.kind_id()
        || actual.start_byte() != expected.start_byte()
        || actual.end_byte() != expected.end_byte()
        || actual.start_position() != expected.start_position()
        || actual.end_position() != expected.end_position()
        || actual.is_missing() != expected.is_missing()
        || actual.is_error() != expected.is_error()
        || actual.child_count() != expected.child_count()
    {
        return false;
    }
    (0..actual.child_count()).all(|index| match (actual.child(index), expected.child(index)) {
        (Some(actual_child), Some(expected_child)) => {
            nodes_equal(&actual_child, &expected_child, depth + 1)
        }
        _ => false,
    })
}

fn assert_matches_full(
    grammar: &Grammar,
    text: &Rope,
    incremental: &Tree,
) -> Result<(), TestCaseError> {
    let mut fresh = SyntaxParser::new(grammar.language()).map_err(fail)?;
    let full = fresh.parse(text).map_err(fail)?;
    if trees_equal(incremental, &full) {
        Ok(())
    } else {
        Err(TestCaseError::fail(
            "incremental tree diverges from a full reparse",
        ))
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource(
            "proptest-regressions",
        ))),
        ..ProptestConfig::default()
    })]

    /// Incremental reparse ≡ full reparse, node-for-node.
    #[test]
    fn incremental_matches_full_reparse(
        doc in doc_strategy(),
        specs in prop::collection::vec(change_spec(), 0..5),
    ) {
        let Some(grammar) = fixture_grammar("json") else {
            return Ok(());
        };

        let old_text = Rope::from_str(&doc);
        let changeset =
            ChangeSet::from_changes(&old_text, normalize_changes(&old_text, specs).into_iter());

        let mut new_text = old_text.clone();
        prop_assert!(
            changeset.apply(&mut new_text),
            "change set must apply to its own document"
        );

        let mut parser = SyntaxParser::new(grammar.language()).map_err(fail)?;
        let old_tree = parser.parse(&old_text).map_err(fail)?;

        let outcome = parser
            .parse_incremental(&old_text, &old_tree, &new_text, &changeset, None)
            .map_err(fail)?;
        let ParseOutcome::Parsed(incremental_tree) = outcome else {
            return Err(TestCaseError::fail("budget-aborted parse with no budget set"));
        };

        // A second parser instance keeps any internal state out of the
        // comparison.
        let mut fresh = SyntaxParser::new(grammar.language()).map_err(fail)?;
        let full_tree = fresh.parse(&new_text).map_err(fail)?;

        prop_assert!(
            trees_equal(&incremental_tree, &full_tree),
            "incremental tree diverges from a full reparse"
        );
    }
}

/// A hand-written two-step typing session, so a regression has a readable
/// reproduction independent of proptest's shrinking.
#[test]
fn multi_step_typing_matches_full_reparse() -> Result<(), TestCaseError> {
    let Some(grammar) = fixture_grammar("json") else {
        return Ok(());
    };

    let mut parser = SyntaxParser::new(grammar.language()).map_err(fail)?;
    let mut text = Rope::from_str("{\"a\": 1, \"b\": 2}\n");
    let mut tree = parser.parse(&text).map_err(fail)?;

    // Step 1: `1` -> `10`.
    let changeset = ChangeSet::from_change(&text, (6, 7, Some("10".into())));
    let mut next_text = text.clone();
    if !changeset.apply(&mut next_text) {
        return Err(TestCaseError::fail("replacement did not apply"));
    }
    let outcome = parser
        .parse_incremental(&text, &tree, &next_text, &changeset, None)
        .map_err(fail)?;
    let ParseOutcome::Parsed(next_tree) = outcome else {
        return Err(TestCaseError::fail(
            "budget-aborted parse with no budget set",
        ));
    };
    assert_matches_full(&grammar, &next_text, &next_tree)?;
    text = next_text;
    tree = next_tree;

    // Step 2: insert a new line before the final brace.
    let at = text.len() - 2;
    let changeset = ChangeSet::from_change(&text, (at, at, Some(",\n  \"c\": 3".into())));
    let mut next_text = text.clone();
    if !changeset.apply(&mut next_text) {
        return Err(TestCaseError::fail("insertion did not apply"));
    }
    let outcome = parser
        .parse_incremental(&text, &tree, &next_text, &changeset, None)
        .map_err(fail)?;
    let ParseOutcome::Parsed(next_tree) = outcome else {
        return Err(TestCaseError::fail(
            "budget-aborted parse with no budget set",
        ));
    };
    assert_matches_full(&grammar, &next_text, &next_tree)
}
