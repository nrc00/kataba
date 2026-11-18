use kataba_core::change::ChangeSet;
use kataba_core::rope::Rope;

fn to_string(r: &Rope) -> String {
    let mut s = String::new();
    for i in 0..r.len() {
        if let Some(c) = r.char(i) {
            s.push(c);
        }
    }
    s
}

fn apply(mut r: Rope, c: &ChangeSet) -> Rope {
    assert!(c.apply(&mut r), "apply failed: len {} != doc len", c.len);
    r
}

fn check(original: &Rope, c1: ChangeSet, c2: ChangeSet) {
    assert_eq!(c1.len_after, c2.len, "test setup: len_after != next len");
    let sequential = apply(apply(original.clone(), &c1), &c2);
    let composed = c1.compose(c2);
    let via_compose = apply(original.clone(), &composed);
    assert_eq!(to_string(&sequential), to_string(&via_compose));
}

#[test]
fn insert_then_delete() {
    // "abc" -> insert "XY" at 1 -> "aXYbc" -> delete 1 char at 0 -> "XYbc"
    let original = Rope::from_str("abc");
    let c1 = ChangeSet::from_changes(&original, vec![(1, 1, Some("XY".into()))].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(0, 1, None)].into_iter());
    check(&original, c1, c2);
}

#[test]
fn delete_then_insert() {
    let original = Rope::from_str("abc");
    let c1 = ChangeSet::from_changes(&original, vec![(0, 3, None)].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(0, 0, Some("xyz".into()))].into_iter());
    check(&original, c1, c2);
}

#[test]
fn insert_insert_same_spot() {
    let original = Rope::from_str("XY");
    let c1 = ChangeSet::from_changes(&original, vec![(0, 0, Some("a".into()))].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(0, 0, Some("b".into()))].into_iter());
    check(&original, c1, c2);
}

#[test]
fn delete_delete_overlap() {
    let original = Rope::from_str("abcde");
    let c1 = ChangeSet::from_changes(&original, vec![(0, 3, None)].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(0, 2, None)].into_iter());
    check(&original, c1, c2);
}

#[test]
fn insert_delete_partial() {
    let original = Rope::from_str("");
    let c1 = ChangeSet::from_changes(&original, vec![(0, 0, Some("abc".into()))].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(0, 2, None)].into_iter());
    check(&original, c1, c2);
}

#[test]
fn insert_retain_drops_retain() {
    // "abcd" -> "Yabcd" -> delete 'a' at 1 -> "Ybcd"
    let original = Rope::from_str("abcd");
    let c1 = ChangeSet::from_changes(&original, vec![(0, 0, Some("Y".into()))].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(1, 2, None)].into_iter());
    check(&original, c1, c2);
}

#[test]
fn insert_delete_then_insert_order() {
    let original = Rope::from_str("");
    let c1 = ChangeSet::from_changes(&original, vec![(0, 0, Some("abc".into()))].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(0, 1, Some("Z".into()))].into_iter());
    check(&original, c1, c2);
}

#[test]
fn insert_retain_mid_insert() {
    let original = Rope::from_str("");
    let c1 = ChangeSet::from_changes(&original, vec![(0, 0, Some("abc".into()))].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(1, 1, Some("Z".into()))].into_iter());
    check(&original, c1, c2);
}

#[test]
fn insert_insert_two_from_other() {
    let original = Rope::from_str("");
    let c1 = ChangeSet::from_changes(&original, vec![(0, 0, Some("a".into()))].into_iter());
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(
        &mid,
        vec![
            (0, 0, Some("X".into())),
            (0, 0, Some("Y".into())),
        ]
        .into_iter(),
    );
    check(&original, c1, c2);
}

#[test]
fn consecutive_inserts_split_by_retain() {
    let original = Rope::from_str("hello");
    let c1 = ChangeSet::from_changes(
        &original,
        vec![(5, 5, Some("abZ".into()))].into_iter(),
    );
    let mut mid = original.clone();
    assert!(c1.apply(&mut mid));
    let c2 = ChangeSet::from_changes(&mid, vec![(5, 6, None)].into_iter());
    check(&original, c1, c2);
}
