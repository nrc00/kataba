use std::time::Instant;

fn bench(name: &str, rounds: u32, mut f: impl FnMut()) {
    f(); // warmup (also builds caches)
    let mut best = u128::MAX;
    for _ in 0..rounds {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed().as_micros());
    }
    println!("{name:38} best-of-{rounds}: {best:>10} us");
}

fn main() {
    let rounds = 5;
    // 100KB ascii doc, multi-line.
    let line = "the quick brown fox jumps over the lazy dog 0123456789\n";
    let big: String = line.repeat(2000);
    println!("doc bytes: {}", big.len());

    // 1. Sequential typing: 500 single-char inserts w/ apply.
    bench("kataba typing x500", rounds, || {
        let mut doc = kataba_core::Rope::from_str(&big);
        for i in 0..500 {
            let pos = (i * 7919) % (doc.len() + 1);
            let txn = kataba_core::Transaction::change(&doc, std::iter::once((pos, pos, Some("x".into()))));
            txn.apply_doc(&mut doc);
        }
        std::hint::black_box(doc.len());
    });
    bench("helix typing x500", rounds, || {
        let mut doc = helix_core::Rope::from_str(&big);
        for i in 0..500 {
            let pos = (i * 7919) % (doc.len_chars() + 1);
            let txn = helix_core::Transaction::change(&doc, std::iter::once((pos, pos, Some("x".into()))));
            txn.apply(&mut doc);
        }
        std::hint::black_box(doc.len_chars());
    });

    // 2. Compose two large disjoint changesets.
    bench("kataba compose 2x2k-ops", rounds, || {
        let doc = kataba_core::Rope::from_str(&big);
        let mut v1 = Vec::new();
        let mut v2 = Vec::new();
        for k in 0..2000 {
            v1.push((k * 50, k * 50 + 10, Some("replacement".into())));
            v2.push((k * 50, k * 50 + 5, None));
        }
        let c1 = kataba_core::ChangeSet::from_changes(&doc, v1.into_iter());
        let mut mid = doc.clone();
        assert!(c1.apply(&mut mid));
        let c2 = kataba_core::ChangeSet::from_changes(&mid, v2.into_iter());
        let c = c1.compose(c2);
        std::hint::black_box(c.len_after);
    });
    bench("helix compose 2x2k-ops", rounds, || {
        let doc = helix_core::Rope::from_str(&big);
        let mut v1 = Vec::new();
        let mut v2 = Vec::new();
        for k in 0..2000 {
            v1.push((k * 50, k * 50 + 10, Some("replacement".into())));
            v2.push((k * 50, k * 50 + 5, None));
        }
        let c1 = helix_core::ChangeSet::from_changes(&doc, v1.into_iter());
        let mut mid = doc.clone();
        assert!(c1.apply(&mut mid));
        let c2 = helix_core::ChangeSet::from_changes(&mid, v2.into_iter());
        let c = c1.compose(c2);
        std::hint::black_box(c.changes().len());
    });

    // 3. Invert a large changeset.
    bench("kataba invert 2k-ops", rounds, || {
        let doc = kataba_core::Rope::from_str(&big);
        let v: Vec<_> = (0..2000).map(|k| (k * 50, k * 50 + 10, Some("replacement".into()))).collect();
        let txn = kataba_core::Transaction::change(&doc, v.into_iter());
        let inv = txn.invert().unwrap();
        std::hint::black_box(inv.deleted.len());
    });
    bench("helix invert 2k-ops", rounds, || {
        let doc = helix_core::Rope::from_str(&big);
        let v: Vec<_> = (0..2000).map(|k| (k * 50, k * 50 + 10, Some("replacement".into()))).collect();
        let txn = helix_core::Transaction::change(&doc, v.into_iter());
        let inv = txn.invert(&doc);
        std::hint::black_box(inv.changes().changes().len());
    });

    // 4. Selection map over large changeset (multi-cursor).
    bench("kataba sel-map 1k cursors", rounds, || {
        let doc = kataba_core::Rope::from_str(&big);
        let ranges: smallvec::SmallVec<[kataba_core::Range; 1]> =
            (0..1000).map(|k| kataba_core::Range::point(k * 100)).collect();
        let sel = kataba_core::Selection::with_primary(ranges, 0);
        let v: Vec<_> = (0..2000).map(|k| (k * 50, k * 50 + 10, Some("replacement".into()))).collect();
        let cs = kataba_core::ChangeSet::from_changes(&doc, v.into_iter());
        let out = sel.map(&cs);
        std::hint::black_box(out.len());
    });
    bench("helix sel-map 1k cursors", rounds, || {
        let doc = helix_core::Rope::from_str(&big);
        let ranges: smallvec::SmallVec<[helix_core::Range; 1]> =
            (0..1000).map(|k| helix_core::Range::point(k * 100)).collect();
        let sel = helix_core::Selection::new(ranges, 0);
        let v: Vec<_> = (0..2000).map(|k| (k * 50, k * 50 + 10, Some("replacement".into()))).collect();
        let cs = helix_core::ChangeSet::from_changes(&doc, v.into_iter());
        let out = sel.map(&cs);
        std::hint::black_box(out.len());
    });
}
