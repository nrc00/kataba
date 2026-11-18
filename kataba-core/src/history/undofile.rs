//! Undofile persistence: `encode`/`decode` with SHA-256 integrity header.
//! Pure bytes; file I/O stays outside core.

use crate::error::{Error, Result};
use crate::rope::Rope;
use sha2::{Digest, Sha256};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::dag::{History, Revision, RevisionId};

fn doc_hash(doc: &Rope) -> [u8; 32] {
    let mut hasher = Sha256::new();
    // Feed in char order without materializing giant strings twice.
    let mut buf = String::with_capacity(doc.len().min(8192));
    for i in 0..doc.len() {
        if let Some(c) = doc.char(i) {
            buf.push(c);
            if buf.len() >= 8192 {
                hasher.update(buf.as_bytes());
                buf.clear();
            }
        }
    }
    if !buf.is_empty() {
        hasher.update(buf.as_bytes());
    }
    hasher.finalize().into()
}

fn put_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    put_u64(buf, s.len() as u64);
    buf.extend_from_slice(s.as_bytes());
}

fn put_transaction(buf: &mut Vec<u8>, txn: &crate::transaction::Transaction) {
    use crate::change::Operation;

    put_u64(buf, txn.changes.changes().len() as u64);
    for op in txn.changes.changes() {
        match op {
            Operation::Retain(n) => {
                buf.push(0);
                put_u64(buf, *n as u64);
            }
            Operation::Delete(n) => {
                buf.push(1);
                put_u64(buf, *n as u64);
            }
            Operation::Insert(s) => {
                buf.push(2);
                put_str(buf, s);
            }
        }
    }
    put_u64(buf, txn.changes.len as u64);
    put_u64(buf, txn.changes.len_after as u64);
    put_u64(buf, txn.deleted.len() as u64);
    for d in &txn.deleted {
        put_str(buf, d);
    }
    match &txn.selection {
        None => buf.push(0),
        Some(sel) => {
            buf.push(1);
            put_u64(buf, sel.ranges.len() as u64);
            put_u64(buf, sel.primary_index as u64);
            for r in sel.ranges.iter() {
                put_u64(buf, r.anchor as u64);
                put_u64(buf, r.head as u64);
                buf.push(if r.is_forward() { 0 } else { 1 });
            }
        }
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.saturating_add(n);
        if end > self.bytes.len() {
            return Err(Error::corrupt("truncated undofile".to_string()));
        }
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        let mut arr = [0u8; 8];
        arr.copy_from_slice(b);
        Ok(u64::from_le_bytes(arr))
    }

    fn string(&mut self) -> Result<String> {
        let len = self.u64()? as usize;
        if len > 32 * 1024 * 1024 {
            return Err(Error::corrupt("implausible string length".to_string()));
        }
        let b = self.take(len)?;
        std::str::from_utf8(b)
            .map(|s| s.to_string())
            .map_err(|_| Error::corrupt("invalid utf-8 in undofile".to_string()))
    }

    fn rest(&self) -> &'a [u8] {
        &self.bytes[self.pos..]
    }
}

fn get_transaction(cur: &mut Cursor<'_>) -> Result<crate::transaction::Transaction> {
    use crate::change::{ChangeSet, Operation};
    use crate::selection::Selection;

    let nops = cur.u64()? as usize;
    if nops > 1_000_000 {
        return Err(Error::corrupt("implausible op count".to_string()));
    }
    let mut ops = Vec::with_capacity(nops.min(1024));
    for _ in 0..nops {
        let tag = cur.take(1)?[0];
        match tag {
            0 => ops.push(Operation::Retain(cur.u64()? as usize)),
            1 => ops.push(Operation::Delete(cur.u64()? as usize)),
            2 => ops.push(Operation::Insert(cur.string()?.into())),
            _ => return Err(Error::corrupt("unknown op tag".to_string())),
        }
    }
    let len = cur.u64()? as usize;
    let len_after = cur.u64()? as usize;
    let check = ChangeSet::new(ops);
    if check.len != len || check.len_after != len_after {
        return Err(Error::corrupt("changeset length mismatch".to_string()));
    }
    let ndel = cur.u64()? as usize;
    if ndel > 1_000_000 {
        return Err(Error::corrupt("implausible deleted count".to_string()));
    }
    let mut deleted = Vec::with_capacity(ndel.min(1024));
    for _ in 0..ndel {
        deleted.push(cur.string()?.into());
    }
    let has_sel = cur.take(1)?[0];
    let selection = match has_sel {
        0 => None,
        1 => {
            let nranges = cur.u64()? as usize;
            if nranges == 0 || nranges > 100_000 {
                return Err(Error::corrupt("implausible range count".to_string()));
            }
            let primary = cur.u64()? as usize;
            let mut ranges: SmallVec<[crate::range::Range; 1]> = SmallVec::new();
            for _ in 0..nranges {
                let anchor = cur.u64()? as usize;
                let head = cur.u64()? as usize;
                match cur.take(1)?[0] {
                    // Direction is derived from anchor/head ordering; the tag
                    // only selects which endpoint is stored first.
                    0 => ranges.push(crate::range::Range::new(anchor, head)),
                    1 => ranges.push(crate::range::Range::new(head, anchor)),
                    _ => return Err(Error::corrupt("unknown direction".to_string())),
                }
            }
            if primary >= ranges.len() {
                return Err(Error::corrupt("primary index out of range".to_string()));
            }
            Some(Selection::with_primary(ranges, primary))
        }
        _ => return Err(Error::corrupt("unknown selection tag".to_string())),
    };
    let mut txn = crate::transaction::Transaction::new(check);
    txn.deleted = deleted;
    txn.selection = selection;
    Ok(txn)
}

impl History {
    /// Serializes this history with an integrity header bound to `doc`.
    ///
    /// [`History::decode`] quarantines (rejects) data whose magic, version,
    /// document length/hash, or trailing checksum mismatch.
    #[must_use]
    pub fn encode(&self, doc: &Rope) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"KTBUNDO1");
        put_u16(&mut buf, 1);
        put_u64(&mut buf, doc.len() as u64);
        buf.extend_from_slice(&doc_hash(doc));
        put_u64(&mut buf, self.undo_limit_bytes as u64);
        put_u64(&mut buf, self.next_id);
        put_u64(&mut buf, self.current.0);
        put_u64(&mut buf, self.lru_generation);
        put_u64(&mut buf, self.revisions.len() as u64);
        for rev in &self.revisions {
            put_u64(&mut buf, rev.id.0);
            put_u64(&mut buf, rev.parent.map(|p| p.0).unwrap_or(u64::MAX));
            put_u64(&mut buf, rev.children.len() as u64);
            for c in rev.children.iter() {
                put_u64(&mut buf, c.0);
            }
            put_u64(
                &mut buf,
                rev.last_child.map(|c| c.0).unwrap_or(u64::MAX),
            );
            put_u64(&mut buf, rev.stamp_ms);
            put_u64(&mut buf, rev.lru_generation);
            put_transaction(&mut buf, &rev.transaction);
            put_transaction(&mut buf, &rev.inversion);
        }
        let sum = Sha256::digest(&buf);
        buf.extend_from_slice(&sum);
        buf
    }

    /// Restores a history previously produced by [`History::encode`].
    ///
    /// Returns [`Error::Corrupt`] when the magic, version, document binding,
    /// structure, or trailing checksum is invalid. Never mutates `doc`.
    pub fn decode(bytes: &[u8], doc: &Rope) -> Result<Self> {
        let mut cur = Cursor::new(bytes);
        let magic = cur.take(8)?;
        if magic != b"KTBUNDO1" {
            return Err(Error::corrupt("bad undofile magic".to_string()));
        }
        if cur.u16()? != 1 {
            return Err(Error::corrupt("unsupported undofile version".to_string()));
        }
        let doc_len = cur.u64()? as usize;
        if doc_len != doc.len() {
            return Err(Error::corrupt(
                "document length changed; cannot use undo info".to_string(),
            ));
        }
        let want_hash = cur.take(32)?;
        if want_hash != doc_hash(doc).as_slice() {
            return Err(Error::corrupt(
                "document contents changed; cannot use undo info".to_string(),
            ));
        }
        let undo_limit_bytes = cur.u64()? as usize;
        let next_id = cur.u64()?;
        let current = RevisionId(cur.u64()?);
        let lru_generation = cur.u64()?;
        let count = cur.u64()? as usize;
        if count == 0 || count > 1_000_000 {
            return Err(Error::corrupt("implausible revision count".to_string()));
        }
        let mut revisions = Vec::with_capacity(count);
        for _ in 0..count {
            let id = RevisionId(cur.u64()?);
            let parent_raw = cur.u64()?;
            let parent = if parent_raw == u64::MAX {
                None
            } else {
                Some(RevisionId(parent_raw))
            };
            let nchildren = cur.u64()? as usize;
            if nchildren > 1024 {
                return Err(Error::corrupt("implausible child count".to_string()));
            }
            let mut children: SmallVec<[RevisionId; 2]> = SmallVec::new();
            for _ in 0..nchildren {
                children.push(RevisionId(cur.u64()?));
            }
            let last_raw = cur.u64()?;
            let last_child = if last_raw == u64::MAX {
                None
            } else {
                Some(RevisionId(last_raw))
            };
            let stamp_ms = cur.u64()?;
            let rev_lru = cur.u64()?;
            let transaction = get_transaction(&mut cur)?;
            let inversion = get_transaction(&mut cur)?;
            let mut rev = Revision::new(id, parent, transaction, inversion, stamp_ms, rev_lru);
            rev.children = children;
            rev.last_child = last_child;
            revisions.push(rev);
        }
        let rest = cur.rest();
        if rest.len() != 32 {
            return Err(Error::corrupt("truncated undofile".to_string()));
        }
        let body_len = bytes.len() - 32;
        let sum = Sha256::digest(&bytes[..body_len]);
        if rest != sum.as_slice() {
            return Err(Error::corrupt("undofile checksum mismatch".to_string()));
        }
        // Structural validation: root first, ids unique + ordered, links resolve.
        let mut index = HashMap::with_capacity(revisions.len());
        for (i, rev) in revisions.iter().enumerate() {
            if index.insert(rev.id, i).is_some() {
                return Err(Error::corrupt("duplicate revision id".to_string()));
            }
            if i > 0 && rev.id.0 <= revisions[i - 1].id.0 {
                return Err(Error::corrupt("revision ids out of order".to_string()));
            }
        }
        if revisions.first().map(|r| r.id) != Some(RevisionId::ROOT) {
            return Err(Error::corrupt("missing root revision".to_string()));
        }
        for rev in &revisions {
            if let Some(p) = rev.parent {
                if !index.contains_key(&p) {
                    return Err(Error::corrupt("dangling parent link".to_string()));
                }
            }
            for c in rev.children.iter() {
                if !index.contains_key(c) {
                    return Err(Error::corrupt("dangling child link".to_string()));
                }
            }
            if let Some(lc) = rev.last_child {
                if !index.contains_key(&lc) {
                    return Err(Error::corrupt("dangling last-child link".to_string()));
                }
            }
        }
        if !index.contains_key(&current) {
            return Err(Error::corrupt("current revision missing".to_string()));
        }
        if next_id <= revisions.last().map(|r| r.id.0).unwrap_or(0) {
            return Err(Error::corrupt("next id not monotonic".to_string()));
        }
        let allocated_bytes = revisions.iter().map(|r| r.approx_bytes).sum();
        let birth = Instant::now();
        let base = revisions.first().map(|r| r.stamp_ms).unwrap_or(0);
        for rev in &mut revisions {
            rev.timestamp = birth + Duration::from_millis(rev.stamp_ms.saturating_sub(base));
        }
        Ok(Self {
            revisions,
            index,
            current,
            next_id,
            lru_generation,
            allocated_bytes,
            undo_limit_bytes,
            birth,
            ghosts: Vec::new(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::change::Tendril;
    use crate::range::Range as DocRange;
    use crate::selection::Selection;
    use crate::transaction::Transaction;

    fn commit_str(history: &mut History, state: &mut crate::history::dag::State, from: usize, to: usize, text: &str) {
        let rep = if text.is_empty() {
            None
        } else {
            Some(text.into())
        };
        let txn = Transaction::change(&state.doc, std::iter::once((from, to, rep)));
        history.commit_revision_state(&txn, state);
        txn.apply_doc(&mut state.doc);
    }

    #[test]
    fn test_undofile_round_trip() {
        let mut history = History::default();
        let mut state = crate::history::dag::State {
            doc: Rope::from_str("hello"),
            selection: Selection::point(5),
        };
        commit_str(&mut history, &mut state, 5, 5, " world");
        commit_str(&mut history, &mut state, 0, 5, "goodbye");
        let bytes = history.encode(&state.doc);
        let mut loaded = History::decode(&bytes, &state.doc).expect("decode");
        assert_eq!(loaded.len(), history.len());
        assert_eq!(loaded.current_revision(), history.current_revision());
        // Navigation survives the round trip.
        let undo = loaded.undo().expect("undo").clone();
        assert!(undo.apply_doc(&mut state.doc));
        let redo = loaded.redo().expect("redo").clone();
        assert!(redo.apply_doc(&mut state.doc));
    }

    #[test]
    fn test_undofile_quarantines_tampering_and_doc_change() {
        let mut history = History::default();
        let mut state = crate::history::dag::State {
            doc: Rope::from_str("hello"),
            selection: Selection::point(5),
        };
        commit_str(&mut history, &mut state, 5, 5, " world");
        let mut bytes = history.encode(&state.doc);
        // Tamper with a payload byte (not the trailing checksum).
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        assert!(History::decode(&bytes, &state.doc).is_err());
        // Same bytes against a changed document are rejected.
        let bytes = history.encode(&state.doc);
        let other = Rope::from_str("hello!");
        assert!(History::decode(&bytes, &other).is_err());
    }
}
