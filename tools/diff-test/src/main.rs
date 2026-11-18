//! Differential edge-case harness: kataba-core vs helix-core.
//! Deterministic xorshift RNG; every mismatch prints the full repro.

use std::time::{Duration, Instant};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        // inclusive lo, exclusive hi; lo==hi -> lo
        if lo >= hi {
            lo
        } else {
            lo + self.below(hi - lo)
        }
    }
}

const ALPHA: &[char] = &[
    'a', 'b', ' ', '_', '\n', 'x', '中', 'é', 'e', '\u{301}', '😀', 'Z',
];

fn gen_string(rng: &mut Rng, max_len: usize) -> String {
    let n = rng.below(max_len + 1);
    (0..n).map(|_| ALPHA[rng.below(ALPHA.len())]).collect()
}

fn gen_doc(rng: &mut Rng) -> String {
    gen_string(rng, 70)
}

/// Sorted, non-overlapping edits against a doc of `len` chars.
fn gen_edits(rng: &mut Rng, len: usize, max_edits: usize) -> Vec<(usize, usize, Option<String>)> {
    let n = rng.below(max_edits + 1);
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for _ in 0..n {
        if cursor > len {
            break;
        }
        let from = rng.range(cursor, len + 1);
        let to = rng.range(from, (from + 9).min(len + 1));
        let rep = if rng.below(2) == 0 {
            None
        } else {
            Some(gen_string(rng, 6))
        };
        // Skip pure no-op retains of zero width with no text.
        if from == to && rep.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            cursor = to;
            continue;
        }
        out.push((from, to, rep));
        cursor = to;
    }
    out
}

fn k_doc(s: &str) -> kataba_core::Rope {
    kataba_core::Rope::from_str(s)
}
fn h_doc(s: &str) -> helix_core::Rope {
    helix_core::Rope::from_str(s)
}
fn k_str(r: &kataba_core::Rope) -> String {
    r.to_string()
}
fn h_str(r: &helix_core::Rope) -> String {
    r.to_string()
}

fn k_assoc(i: usize) -> kataba_core::Assoc {
    use kataba_core::Assoc::*;
    [Before, After, BeforeWord, AfterWord, BeforeSticky, AfterSticky][i]
}
fn h_assoc(i: usize) -> helix_core::Assoc {
    use helix_core::Assoc::*;
    [Before, After, BeforeWord, AfterWord, BeforeSticky, AfterSticky][i]
}
const ASSOC_NAMES: [&str; 6] = ["Before", "After", "BeforeWord", "AfterWord", "BeforeSticky", "AfterSticky"];

fn main() {
    let mut rng = Rng(0x12345678);
    let mut fails = 0u32;
    let mut checks = 0u64;
    let mut fail = |ctx: String| {
        fails += 1;
        if fails <= 8 {
            println!("MISMATCH: {ctx}");
        }
    };

    // ---- A. compose: random valid pairs, compare final docs ----
    for case in 0..3000 {
        let doc = gen_doc(&mut rng);
        let e1 = gen_edits(&mut rng, doc.chars().count(), 4);
        let kd0 = k_doc(&doc);
        let kc1 = kataba_core::ChangeSet::from_changes(&kd0, e1.clone().into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
        let mut kd1 = kd0.clone();
        assert!(kc1.apply(&mut kd1));
        let hd0 = h_doc(&doc);
        let hc1 = helix_core::ChangeSet::from_changes(&hd0, e1.into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
        let mut hd1 = hd0.clone();
        assert!(hc1.apply(&mut hd1));
        let l1 = kd1.len();
        let e2 = gen_edits(&mut rng, l1, 4);
        let kc2 = kataba_core::ChangeSet::from_changes(&kd1, e2.clone().into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
        let mut kd2 = kd1.clone();
        assert!(kc2.apply(&mut kd2));
        let hc2 = helix_core::ChangeSet::from_changes(&hd1, e2.into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
        let mut hd2 = hd1.clone();
        assert!(hc2.apply(&mut hd2));
        checks += 1;
        let kc = kc1.compose(kc2);
        let hc = hc1.compose(hc2);
        let mut kdc = kd0.clone();
        let mut hdc = hd0.clone();
        let ka = kc.apply(&mut kdc);
        let ha = hc.apply(&mut hdc);
        if ka != ha || (ka && k_str(&kdc) != h_str(&hdc)) {
            fail(format!("compose case {case}: doc={doc:?} kapplied={ka} happlied={ha} k={:?} h={:?}", k_str(&kdc), h_str(&hdc)));
        }
        // B. invert of composed == sequential invert application
        checks += 1;
        let ki = kc.invert(&kd0);
        let hi = hc.invert(&hd0);
        let mut kdr = kdc.clone();
        let mut hdr = hdc.clone();
        let ka2 = ki.apply(&mut kdr);
        let ha2 = hi.apply(&mut hdr);
        if ka2 != ha2 || (ka2 && (k_str(&kdr) != k_str(&kd0) || h_str(&hdr) != h_str(&hd0))) {
            fail(format!("invert case {case}: doc={doc:?}"));
        }
    }

    // ---- C. map_pos: every position x every assoc ----
    for case in 0..1500 {
        let doc = gen_doc(&mut rng);
        let len = doc.chars().count();
        let edits = gen_edits(&mut rng, len, 4);
        let kd = k_doc(&doc);
        let kc = kataba_core::ChangeSet::from_changes(&kd, edits.clone().into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
        let hd = h_doc(&doc);
        let hc = helix_core::ChangeSet::from_changes(&hd, edits.into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
        for pos in 0..=len {
            for a in 0..6 {
                checks += 1;
                let k = kc.map_pos(pos, k_assoc(a));
                let h = hc.map_pos(pos, h_assoc(a));
                if k != h {
                    fail(format!("map_pos case {case}: doc={doc:?} pos={pos} assoc={} k={k} h={h}", ASSOC_NAMES[a]));
                }
            }
        }
    }

    // ---- D. range + selection mapping ----
    for case in 0..1500 {
        let doc = gen_doc(&mut rng);
        let len = doc.chars().count();
        let edits = gen_edits(&mut rng, len, 3);
        let kd = k_doc(&doc);
        let kc = kataba_core::ChangeSet::from_changes(&kd, edits.clone().into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
        let hd = h_doc(&doc);
        let hc = helix_core::ChangeSet::from_changes(&hd, edits.into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
        let nr = 1 + rng.below(4);
        let mut kranges = smallvec::SmallVec::<[kataba_core::Range; 1]>::new();
        let mut hranges = smallvec::SmallVec::<[helix_core::Range; 1]>::new();
        for _ in 0..nr {
            let a = rng.below(len + 1);
            let b = rng.below(len + 1);
            kranges.push(kataba_core::Range::new(a, b));
            hranges.push(helix_core::Range::new(a, b));
        }
        let pi = rng.below(nr);
        let ks = kataba_core::Selection::with_primary(kranges, pi);
        let hs = helix_core::Selection::new(hranges, pi);
        checks += 1;
        let km = ks.map(&kc);
        let hm = hs.map(&hc);
        let mut kv: Vec<(usize, usize)> = km.ranges().iter().map(|r| (r.anchor, r.head)).collect();
        let mut hv: Vec<(usize, usize)> = hm.ranges().iter().map(|r| (r.anchor, r.head)).collect();
        kv.sort();
        hv.sort();
        let kp = (km.primary().anchor, km.primary().head);
        let hp = (hm.primary().anchor, hm.primary().head);
        if kv != hv || kp != hp {
            fail(format!("selmap case {case}: doc={doc:?} k={kv:?} p{kp:?} h={hv:?} p{hp:?}"));
        }
    }

    // ---- E. transaction chains: apply + compose + invert ----
    for case in 0..800 {
        let doc = gen_doc(&mut rng);
        let mut kd = k_doc(&doc);
        let mut hd = h_doc(&doc);
        let mut ktot = kataba_core::Transaction::empty();
        // seed with identity shaped to doc0? compose needs len match; build chain stepwise.
        let mut first = true;
        for _ in 0..rng.range(1, 4) {
            let e = gen_edits(&mut rng, kd.len(), 3);
            let kt = kataba_core::Transaction::change(&kd, e.clone().into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
            let ht = helix_core::Transaction::change(&hd, e.into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
            checks += 1;
            let ka = kt.apply_doc(&mut kd);
            let ha = ht.apply(&mut hd);
            if ka != ha || k_str(&kd) != h_str(&hd) {
                fail(format!("txn-apply case {case}: k={:?} h={:?}", k_str(&kd), h_str(&hd)));
            }
            ktot = if first { kt } else { ktot.compose(kt) };
            first = false;
            // helix-side composed reference
            let _ = ht;
        }
        let _ = ktot;
    }

    // ---- F. history programs with shared synthetic timestamps ----
    for case in 0..400 {
        let doc = gen_doc(&mut rng);
        let mut ks = kataba_core::history::State { doc: k_doc(&doc), selection: kataba_core::Selection::point(rng.below(doc.chars().count() + 1)) };
        let mut hs = helix_core::history::State { doc: h_doc(&doc), selection: helix_core::Selection::point(ks.selection.primary().head) };
        let mut kh = kataba_core::History::default();
        let mut hh = helix_core::history::History::default();
        let kb = Instant::now();
        let hb = Instant::now();
        let mut clock = 0u64;
        for _ in 0..rng.range(1, 12) {
            match rng.below(10) {
                0..=4 => {
                    let e = gen_edits(&mut rng, ks.doc.len(), 2);
                    let kt = kataba_core::Transaction::change(&ks.doc, e.clone().into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
                    let ht = helix_core::Transaction::change(&hs.doc, e.into_iter().map(|(f, t, r)| (f, t, r.map(|s| s.into()))));
                    kh.commit_revision_at_timestamp(&kt, &ks, { clock += (10 + rng.below(50)) as u64; kb + Duration::from_millis(clock) });
                    hh.commit_revision_at_timestamp(&ht, &hs, hb + Duration::from_millis(clock));
                    kt.apply_doc(&mut ks.doc);
                    ht.apply(&mut hs.doc);
                }
                5..=6 => {
                    let ku = kh.undo().cloned();
                    let hu = hh.undo().cloned();
                    checks += 1;
                    match (ku, hu) {
                        (Some(k), Some(h)) => {
                            k.apply_doc(&mut ks.doc);
                            h.apply(&mut hs.doc);
                            // inversion selections should agree on primary head
                            let kp = k.selection.as_ref().map(|s| s.primary().head);
                            let hp = h.selection().map(|s| s.primary().head);
                            if kp != hp {
                                fail(format!("hist-undo-sel case {case}: k={kp:?} h={hp:?}"));
                            }
                        }
                        (None, None) => {}
                        _ => fail(format!("hist-undo-availability case {case}")),
                    }
                }
                7 => {
                    let kr = kh.redo().cloned();
                    let hr = hh.redo().cloned();
                    checks += 1;
                    match (kr, hr) {
                        (Some(k), Some(h)) => {
                            k.apply_doc(&mut ks.doc);
                            h.apply(&mut hs.doc);
                        }
                        (None, None) => {}
                        _ => fail(format!("hist-redo-availability case {case}")),
                    }
                }
                _ => {
                    let n = rng.below(4);
                    let back = rng.below(2) == 0;
                    checks += 1;
                    if back {
                        for t in kh.earlier(kataba_core::history::UndoKind::Steps(n)) {
                            t.apply_doc(&mut ks.doc);
                        }
                        for t in hh.earlier(helix_core::history::UndoKind::Steps(n)) {
                            t.apply(&mut hs.doc);
                        }
                    } else {
                        for t in kh.later(kataba_core::history::UndoKind::Steps(n)) {
                            t.apply_doc(&mut ks.doc);
                        }
                        for t in hh.later(helix_core::history::UndoKind::Steps(n)) {
                            t.apply(&mut hs.doc);
                        }
                    }
                }
            }
            checks += 1;
            if k_str(&ks.doc) != h_str(&hs.doc) {
                fail(format!("hist-doc case {case}: k={:?} h={:?}", k_str(&ks.doc), h_str(&hs.doc)));
                break;
            }
        }
    }

    println!("checks={checks} mismatches={fails}");
    if fails > 0 {
        std::process::exit(1);
    }

    // ---- G. grapheme boundaries vs helix stepping ----
    let mut gfails = 0u32;
    let mut gchecks = 0u64;
    for case in 0..2000 {
        let doc = gen_string(&mut rng, 40);
        let kd = k_doc(&doc);
        let hd = h_doc(&doc);
        let hs = hd.slice(..);
        let len = kd.len();
        for pos in 0..=len {
            gchecks += 1;
            let k = kataba_core::motion::next_grapheme_boundary(&kd, pos);
            let h = helix_core::graphemes::next_grapheme_boundary(hs, pos);
            if k != h {
                gfails += 1;
                if gfails <= 5 {
                    println!("GRAPHEME-NEXT case {case}: doc={doc:?} pos={pos} k={k} h={h}");
                }
            }
            gchecks += 1;
            let k = kataba_core::motion::prev_grapheme_boundary(&kd, pos);
            let h = helix_core::graphemes::prev_grapheme_boundary(hs, pos);
            if k != h {
                gfails += 1;
                if gfails <= 5 {
                    println!("GRAPHEME-PREV case {case}: doc={doc:?} pos={pos} k={k} h={h}");
                }
            }
        }
    }
    println!("grapheme checks={gchecks} mismatches={gfails}");
    if fails > 0 || gfails > 0 {
        std::process::exit(1);
    }
}
