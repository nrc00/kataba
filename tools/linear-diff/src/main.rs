//! LinearHistory vs helix History differential.
//! Suite A (linear programs): exact match on docs, cursors, availability.
//! Suite B (forking programs): docs must match; the ONLY allowed divergence
//! is redo availability after a fork (linear discards, helix orphans).

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
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

const ALPHA: &[char] = &['a', 'b', ' ', '_', '\n', 'x', '中', 'é', '😀', 'Z'];

fn gen_string(rng: &mut Rng, max_len: usize) -> String {
    (0..rng.below(max_len + 1)).map(|_| ALPHA[rng.below(ALPHA.len())]).collect()
}

fn main() {
    let mut rng = Rng(0x9e3779b9);
    let mut fails = 0u32;
    let mut checks = 0u64;
    let mut expected_divergences = 0u32;
    let mut fail = |ctx: String| {
        fails += 1;
        if fails <= 8 {
            println!("MISMATCH: {ctx}");
        }
    };

    for prog in 0..600 {
        let fork_allowed = prog % 2 == 1;
        let doc = gen_string(&mut rng, 40);
        let cur0 = rng.below(doc.chars().count() + 1);
        let mut kd = kataba_core::Rope::from_str(&doc);
        let mut hd = helix_core::Rope::from_str(&doc);
        let mut ks = kataba_core::Selection::point(cur0);
        let mut hs = helix_core::Selection::point(cur0);
        let mut kh = kataba_core::LinearHistory::new();
        let mut hh = helix_core::history::History::default();
        let hb = Instant::now();
        let mut clock = 0u64;
        // Tracks whether kataba is at its tip (== helix at tip for linear part).
        for _ in 0..rng.below(14) + 1 {
            let roll = rng.below(10);
            if roll <= 5 {
                // commit a random single edit
                let len = kd.len();
                let a = rng.below(len + 1);
                let b = rng.below(len + 1);
                let (f, t) = if a <= b { (a, b) } else { (b, a) };
                let rep = gen_string(&mut rng, 5);
                let rep = if rep.is_empty() { None } else { Some(rep) };
                let kt = kataba_core::Transaction::change(
                    &kd,
                    std::iter::once((f, t, rep.clone().map(|s| s.into()))),
                );
                let ht = helix_core::Transaction::change(
                    &hd,
                    std::iter::once((f, t, rep.map(|s| s.into()))),
                );
                let kstate = kataba_core::history::State { doc: kd.clone(), selection: ks.clone() };
                let hstate = helix_core::history::State { doc: hd.clone(), selection: hs.clone() };
                kh.commit(&kt, &kstate);
                clock += (10 + rng.below(50)) as u64;
                hh.commit_revision_at_timestamp(&ht, &hstate, hb + Duration::from_millis(clock));
                kt.apply_doc(&mut kd);
                ht.apply(&mut hd);
                ks = ks.map(kt.changes());
                hs = hs.map(ht.changes());
            } else if roll <= 7 {
                // undo
                checks += 1;
                let ku = kh.undo().cloned();
                let hu = hh.undo().cloned();
                match (ku, hu) {
                    (Some(k), Some(h)) => {
                        k.apply(&mut kd, &mut ks);
                        h.apply(&mut hd);
                        // helix doesn't move selection; mirror with stored inversion selection
                        hs = helix_core::Selection::new(
                            h.selection().expect("helix inversion has selection").ranges().to_vec().into(),
                            h.selection().expect("helix inversion has selection").primary_index(),
                        );
                        checks += 1;
                        if ks.primary().head != hs.primary().head {
                            fail(format!("prog {prog}: undo cursor k={} h={}", ks.primary().head, hs.primary().head));
                        }
                    }
                    (None, None) => {}
                    _ => fail(format!("prog {prog}: undo availability diverged")),
                }
            } else {
                // redo
                checks += 1;
                let kr = kh.redo().cloned();
                let hr = hh.redo().cloned();
                match (kr, hr) {
                    (Some(k), Some(h)) => {
                        k.apply_doc(&mut kd);
                        h.apply(&mut hd);
                        // both map live selection through the redone changes
                        ks = ks.map(k.changes());
                        hs = hs.map(h.changes());
                    }
                    (None, None) => {}
                    (None, Some(_)) if fork_allowed => {
                        // THE documented divergence: linear discarded the fork tail.
                        expected_divergences += 1;
                    }
                    _ => fail(format!("prog {prog}: redo availability diverged unexpectedly")),
                }
            }
            checks += 1;
            if kd.to_string() != hd.to_string() {
                fail(format!("prog {prog}: doc k={:?} h={:?}", kd.to_string(), hd.to_string()));
                break;
            }
            checks += 1;
            if kh.can_undo() != !hh.at_root() {
                // helix has no can_undo; at_root is the equivalent signal
                fail(format!("prog {prog}: undo availability k={} h=root={}", kh.can_undo(), hh.at_root()));
            }
        }
        let _ = fork_allowed;
    }

    println!("checks={checks} mismatches={fails} expected_fork_divergences={expected_divergences}");
    if fails > 0 {
        std::process::exit(1);
    }

    // ---- Scripted fork: both must redo the FORK (last child), orphan unreachable ----
    fn commit_both(
        kh: &mut kataba_core::LinearHistory,
        kd: &mut kataba_core::Rope,
        hh: &mut helix_core::history::History,
        hd: &mut helix_core::Rope,
        f: usize, t: usize, s: &str,
    ) {
        let kt = kataba_core::Transaction::change(kd, std::iter::once((f, t, Some(s.into()))));
        let ht = helix_core::Transaction::change(hd, std::iter::once((f, t, Some(s.into()))));
        let ks = kataba_core::history::State { doc: kd.clone(), selection: kataba_core::Selection::point(f) };
        let hs = helix_core::history::State { doc: hd.clone(), selection: helix_core::Selection::point(f) };
        kh.commit(&kt, &ks);
        hh.commit_revision(&ht, &hs);
        kt.apply_doc(kd);
        ht.apply(hd);
    }
    {
        let mut kd = kataba_core::Rope::from_str("ab");
        let mut hd = helix_core::Rope::from_str("ab");
        let mut kh = kataba_core::LinearHistory::new();
        let mut hh = helix_core::history::History::default();
        commit_both(&mut kh, &mut kd, &mut hh, &mut hd, 2, 2, "c"); // abc
        kh.undo().cloned().unwrap().apply_doc(&mut kd);
        hh.undo().cloned().unwrap().apply(&mut hd);
        commit_both(&mut kh, &mut kd, &mut hh, &mut hd, 2, 2, "d"); // fork -> abd on both
        assert_eq!(kd.to_string(), "abd");
        assert_eq!(hd.to_string(), "abd");
        kh.undo().cloned().unwrap().apply_doc(&mut kd);
        hh.undo().cloned().unwrap().apply(&mut hd);
        let kr = kh.redo().cloned().map(|t| { t.apply_doc(&mut kd); kd.to_string() });
        let hr = hh.redo().cloned().map(|t| { t.apply(&mut hd); hd.to_string() });
        // Both redo the fork ("abd"), NOT the orphan ("abc").
        assert_eq!(kr, Some("abd".to_string()), "kataba redo must follow fork");
        assert_eq!(hr, Some("abd".to_string()), "helix redo must follow fork");
        assert_eq!(kr, hr);
        println!("scripted fork redo: kataba={kr:?} helix={hr:?} (identical, orphan unreachable on both)");
    }
}
