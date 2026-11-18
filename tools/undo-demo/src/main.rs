use kataba_core::history::{History, State};
use kataba_core::rope::Rope;
use kataba_core::selection::Selection;
use kataba_core::transaction::Transaction;

fn state(doc: &str, cursor: usize) -> State {
    State { doc: Rope::from_str(doc), selection: Selection::point(cursor) }
}
fn commit(h: &mut History, s: &mut State, from: usize, to: usize, text: &str) {
    let rep = if text.is_empty() { None } else { Some(text.into()) };
    let txn = Transaction::change(&s.doc, std::iter::once((from, to, rep)));
    h.commit_revision_state(&txn, s);
    txn.apply_doc(&mut s.doc);
}
fn undo(h: &mut History, s: &mut State) {
    h.undo().expect("undo").clone().apply_doc(&mut s.doc);
}

fn main() {
    // Branch A: root -> r1 -> r2 (2 deep). Current: root -> r3.
    let mut h = History::default();
    let mut s = state("a", 1);
    commit(&mut h, &mut s, 1, 1, "b"); // r1: "ab"
    let r1 = h.current_revision();
    commit(&mut h, &mut s, 2, 2, "c"); // r2: "abc"
    let r2 = h.current_revision();
    undo(&mut h, &mut s);
    undo(&mut h, &mut s);
    commit(&mut h, &mut s, 1, 1, "z"); // r3: "az" (current)
    // Budget: keep everything except orphan leaf r2.
    let r2bytes = h.get(r2).expect("r2").approx_bytes;
    h.set_undo_limit_bytes(h.allocated_bytes() - r2bytes + 64);
    println!("r1_alive={} r2_alive={}", h.is_alive(r1), h.is_alive(r2));
    println!("nearest_live_ancestor(r2) = {} (CORRECT answer: {r1}, since r1 is live)", h.nearest_live_ancestor(r2));
}
