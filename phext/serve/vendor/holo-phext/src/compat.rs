//! The phext-lattice compatibility facade: the `MappedLattice`
//! surface, implemented on the HoloPhext core.
//!
//! phext-lattice's editor stack talks to `MappedLattice`
//! (src/mmap.rs there): reads hit a memory-mapped file plus a
//! copy-on-write overlay; `save()` merges the overlay into the file;
//! `revert()` drops it. That is exactly a staged transaction plus a
//! commit-and-checkpoint, so the mapping is mechanical:
//!
//! | MappedLattice            | LatticeStore (here)              |
//! |--------------------------|----------------------------------|
//! | open(path)               | open(path)                       |
//! | read_scroll              | read_scroll (staged-aware)       |
//! | write_scroll / _raw      | write_scroll (stages a set)      |
//! | delete_scroll            | delete_scroll (stages a delete)  |
//! | has_scroll               | has_scroll (staged-aware)        |
//! | populated_coordinates    | populated_coordinates            |
//! | scroll_count             | scroll_count                     |
//! | is_dirty / pending_*     | is_dirty / pending_changes       |
//! | revert                   | revert (drops the staged txn)    |
//! | save                     | save = commit staged + checkpoint|
//! | to_phext_bytes           | to_phext_bytes (merged document) |
//!
//! The one semantic upgrade: between `save()` calls, every staged
//! batch that a lattice-based server chooses to commit through the
//! async interface instead is crash-safe and broadcast — the
//! overlay stops being a single-process convenience.

use crate::core::{Core, Op};
use libphext::phext::{self, Coordinate};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

pub struct LatticeStore {
    core: Core,
    staged: BTreeMap<Coordinate, Option<String>>,
    path: PathBuf,
}

impl LatticeStore {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<LatticeStore> {
        let core = Core::open(path.as_ref())?;
        Ok(LatticeStore { core, staged: BTreeMap::new(), path: path.as_ref().to_path_buf() })
    }

    /// Opens with a prebuilt index for the base document, skipping
    /// the scan (see `Core::open_with_index`).
    pub fn open_with_index<P: AsRef<Path>>(path: P, index: crate::index::LatticeIndex) -> io::Result<LatticeStore> {
        let core = Core::open_with_index(path.as_ref(), index)?;
        Ok(LatticeStore { core, staged: BTreeMap::new(), path: path.as_ref().to_path_buf() })
    }

    pub fn read_scroll(&self, coord: &Coordinate) -> Option<String> {
        if let Some(entry) = self.staged.get(coord) {
            return entry.clone();
        }
        self.core.shared.snapshot().get(coord)
    }

    pub fn write_scroll(&mut self, coord: Coordinate, content: String) -> bool {
        // Staged verbatim — including an empty string, which reads
        // back as Some("") while staged (MappedLattice parity). At
        // commit time an empty value cannot exist as a scroll, so
        // `save` translates it to a delete — the same end state the
        // original serializer produced by dropping it from the file.
        self.staged.insert(coord, Some(content));
        true
    }

    pub fn delete_scroll(&mut self, coord: Coordinate) {
        let in_base = self.core.shared.snapshot().get(&coord).is_some();
        match self.staged.get(&coord) {
            Some(Some(_)) if !in_base => {
                // staged insert of a scroll that exists nowhere else:
                // deleting it removes the entry, as the original
                // overlay did — no pending change remains
                self.staged.remove(&coord);
            }
            _ if in_base || self.staged.contains_key(&coord) => {
                self.staged.insert(coord, None);
            }
            _ => {}
        }
    }

    pub fn has_scroll(&self, coord: &Coordinate) -> bool {
        self.read_scroll(coord).is_some()
    }

    pub fn populated_coordinates(&self) -> Vec<Coordinate> {
        let mut out: Vec<Coordinate> = self
            .core
            .shared
            .snapshot()
            .coordinates()
            .into_iter()
            .filter(|c| !matches!(self.staged.get(c), Some(None)))
            .collect();
        for (coord, value) in &self.staged {
            if value.is_some() && !out.contains(coord) {
                out.push(*coord);
            }
        }
        out.sort();
        out
    }

    pub fn scroll_count(&self) -> usize {
        self.populated_coordinates().len()
    }

    pub fn is_dirty(&self) -> bool {
        !self.staged.is_empty()
    }

    pub fn pending_changes(&self) -> usize {
        self.staged.len()
    }

    /// Coordinates with staged edits, sorted.
    pub fn pending_coordinates(&self) -> Vec<Coordinate> {
        self.staged.keys().copied().collect()
    }

    /// True when nothing is staged and every committed transaction
    /// has been folded into the base file — the state in which the
    /// base file's bytes are the whole document.
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty() && self.core.seq() == self.core.checkpoint_seq()
    }

    /// The committed document (base + committed overlay, staged
    /// edits excluded), serialized.
    pub fn committed_bytes(&self) -> Vec<u8> {
        self.merged_bytes(false)
    }

    pub fn revert(&mut self) {
        self.staged.clear();
    }

    /// Commits the staged changes as one transaction, without
    /// checkpointing. On failure the staged edits are restored —
    /// a failed commit consumes nothing.
    pub fn commit_staged(&mut self) -> io::Result<()> {
        if !self.staged.is_empty() {
            let backup = self.staged.clone();
            let ops: Vec<(Coordinate, Op)> = std::mem::take(&mut self.staged)
                .into_iter()
                .map(|(c, v)| {
                    (c, match v {
                        // an empty scroll cannot be committed — it
                        // does not exist — so an empty staged value
                        // commits as a delete
                        Some(s) if !s.is_empty() => Op::Set(s),
                        _ => Op::Delete,
                    })
                })
                .collect();
            if let Err(e) = self.core.commit_batch(vec![crate::core::Txn { ops }]) {
                self.staged = backup;
                return Err(e);
            }
        }
        Ok(())
    }

    /// Folds all committed state into the base file (see
    /// `Core::checkpoint`).
    pub fn checkpoint(&mut self) -> io::Result<()> {
        self.core.checkpoint()
    }

    /// Commits the staged changes as one transaction, then
    /// checkpoints — after `save`, the base file at `path()` is the
    /// complete merged document, as with `MappedLattice::save`.
    pub fn save(&mut self) -> io::Result<()> {
        self.commit_staged()?;
        self.checkpoint()
    }

    /// The merged document (base + committed overlay + staged),
    /// serialized — `MappedLattice::to_phext_bytes`.
    pub fn to_phext_bytes(&self) -> Vec<u8> {
        self.merged_bytes(true)
    }

    fn merged_bytes(&self, include_staged: bool) -> Vec<u8> {
        let snapshot = self.core.shared.snapshot();
        let base_text = std::str::from_utf8(snapshot.base_bytes()).unwrap_or("");
        let mut map = phext::explode(base_text);
        for coord in snapshot.coordinates() {
            if let Some(v) = snapshot.get(&coord) {
                map.insert(coord, v);
            }
        }
        if include_staged {
            for (coord, value) in &self.staged {
                match value {
                    Some(v) => {
                        map.insert(*coord, v.clone());
                    }
                    None => {
                        map.remove(coord);
                    }
                }
            }
        }
        phext::implode(map).into_bytes()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Access to the underlying core, for servers that want the
    /// transactional interface alongside the lattice surface.
    pub fn core(&mut self) -> &mut Core {
        &mut self.core
    }
}
