//! The HoloPhext core: a memory-mapped base lattice, an append-only
//! phext WAL, and snapshot overlays — the ACID machinery the format
//! layer (libphext) deliberately does not own.
//!
//! Files: `<name>.phext` is the base document (pure user content,
//! memory-mapped read-only, replaced wholesale at checkpoint) and
//! `<name>.wal.phext` is the transaction log since the last checkpoint.
//!
//! WAL layout (all addressing inside the WAL phext):
//! * meta scroll at 1.1.1/1.1.1/1.1.1 — `format=` + `checkpoint_seq=`
//! * transaction `seq`: entries at volume = seq, chapter = 2,
//!   scroll = 1..=n; the commit marker at scroll n+1
//! * each entry is a libphext *checked scroll* whose body is
//!   `coord=<address>` / `op=set|del` / blank / raw value
//! * the marker is a libphext *commit scroll* over the entries
//!
//! Atomicity: a transaction applies iff every entry verifies and the
//! commit marker verifies — a torn tail is truncated at recovery.
//! Isolation: readers hold a `Snapshot` (an Arc of the base lattice
//! plus an Arc of the overlay as of one sequence); a commit swaps the
//! overlay Arc once per transaction, so a reader sees all of a
//! transaction or none of it. Durability: the WAL suffix is written
//! and `sync_data`'d before any op is applied or acknowledged;
//! checkpoints follow the libphext durability profile (temp file,
//! sync, rename, directory sync).

use crate::index::LatticeIndex;
use libphext::phext::{self, Coordinate};
use memmap2::Mmap;
use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// One write inside a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Set(String),
    Delete,
}

/// A multi-scroll transaction: a set of ops applied atomically or
/// not at all.
#[derive(Debug, Clone, Default)]
pub struct Txn {
    pub ops: Vec<(Coordinate, Op)>,
}

impl Txn {
    pub fn new() -> Txn {
        Txn { ops: Vec::new() }
    }
    pub fn set(mut self, coord: Coordinate, content: &str) -> Txn {
        self.ops.push((coord, Op::Set(content.to_string())));
        self
    }
    pub fn delete(mut self, coord: Coordinate) -> Txn {
        self.ops.push((coord, Op::Delete));
        self
    }
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// The read-only memory-mapped base lattice.
pub struct BaseLattice {
    map: Option<Mmap>,
    index: LatticeIndex,
}

impl BaseLattice {
    pub fn open(path: &Path) -> io::Result<BaseLattice> {
        if !path.exists() {
            File::create(path)?;
        }
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            return Ok(BaseLattice { map: None, index: LatticeIndex::default() });
        }
        let map = unsafe { Mmap::map(&file)? };
        let index = LatticeIndex::build(&map);
        Ok(BaseLattice { map: Some(map), index })
    }

    /// Opens with an index built elsewhere (e.g. a persisted span
    /// table), skipping the scan. The index must describe exactly
    /// this file's bytes.
    pub fn open_with_index(path: &Path, index: LatticeIndex) -> io::Result<BaseLattice> {
        if !path.exists() {
            File::create(path)?;
        }
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if len as usize != index.buffer_len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("index covers {} bytes but the base file has {}", index.buffer_len(), len),
            ));
        }
        if len == 0 {
            return Ok(BaseLattice { map: None, index });
        }
        let map = unsafe { Mmap::map(&file)? };
        Ok(BaseLattice { map: Some(map), index })
    }

    pub fn read_scroll(&self, coord: &Coordinate) -> Option<String> {
        let span = self.index.get(coord)?;
        let map = self.map.as_ref()?;
        std::str::from_utf8(&map[span.start..span.end]).ok().map(|s| s.to_string())
    }

    pub fn bytes(&self) -> &[u8] {
        self.map.as_deref().unwrap_or(&[])
    }

    pub fn coordinates(&self) -> &[Coordinate] {
        self.index.coordinates()
    }
}

/// Shared, snapshot-able read state. Writers swap Arcs; readers clone
/// them into a `Snapshot` and are thereafter immune to new commits.
/// Accumulated uncompacted changes that trigger a fold into the
/// compacted map. Folding costs one clone+merge of the map; at
/// this threshold the amortized cost per change is negligible and
/// the delta scan on reads stays short.
const COMPACT_THRESHOLD: usize = 8192;

pub struct Shared {
    base: RwLock<Arc<BaseLattice>>,
    /// Folded overlay state: every committed change up to the last
    /// compaction.
    compacted: RwLock<Arc<BTreeMap<Coordinate, Option<String>>>>,
    /// Batches since compaction, oldest first: (the batch's final
    /// seq, its changes in order). Applying a batch appends here —
    /// O(batch), never O(map).
    deltas: RwLock<Arc<Vec<(u64, Vec<(Coordinate, Op)>)>>>,
    seq: RwLock<u64>,
}

impl Shared {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            base: self.base.read().unwrap().clone(),
            compacted: self.compacted.read().unwrap().clone(),
            deltas: self.deltas.read().unwrap().clone(),
            seq: *self.seq.read().unwrap(),
        }
    }

    pub fn seq(&self) -> u64 {
        *self.seq.read().unwrap()
    }
}

/// A frozen read view at one sequence number.
pub struct Snapshot {
    base: Arc<BaseLattice>,
    compacted: Arc<BTreeMap<Coordinate, Option<String>>>,
    deltas: Arc<Vec<(u64, Vec<(Coordinate, Op)>)>>,
    seq: u64,
}

impl Snapshot {
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// The raw bytes of the base document under this snapshot.
    pub fn base_bytes(&self) -> &[u8] {
        self.base.bytes()
    }

    pub fn get(&self, coord: &Coordinate) -> Option<String> {
        // Newest first: later batches win, and within a batch the
        // later op wins. Ops are absolute, so the first hit decides.
        for (bseq, changes) in self.deltas.iter().rev() {
            if *bseq > self.seq {
                continue;
            }
            for (c, op) in changes.iter().rev() {
                if c == coord {
                    return match op {
                        Op::Set(v) => Some(v.clone()),
                        Op::Delete => None,
                    };
                }
            }
        }
        if let Some(entry) = self.compacted.get(coord) {
            return entry.clone();
        }
        self.base.read_scroll(coord)
    }

    /// The fully merged overlay at this snapshot (checkpoint's
    /// view; O(map), called once per checkpoint, never per read).
    pub fn overlay_map(&self) -> BTreeMap<Coordinate, Option<String>> {
        let mut map = (*self.compacted).clone();
        for (bseq, changes) in self.deltas.iter() {
            if *bseq > self.seq {
                continue;
            }
            for (c, op) in changes {
                map.insert(
                    *c,
                    match op {
                        Op::Set(v) => Some(v.clone()),
                        Op::Delete => None,
                    },
                );
            }
        }
        map
    }

    pub fn contains(&self, coord: &Coordinate) -> bool {
        self.get(coord).is_some()
    }

    /// Every populated coordinate in this snapshot, ascending.
    pub fn coordinates(&self) -> Vec<Coordinate> {
        let overlay = self.overlay_map();
        let mut out: Vec<Coordinate> = self
            .base
            .coordinates()
            .iter()
            .copied()
            .filter(|c| !matches!(overlay.get(c), Some(None)))
            .collect();
        for (coord, value) in overlay.iter() {
            if value.is_some() && !self.base.coordinates().contains(coord) {
                out.push(*coord);
            }
        }
        out.sort();
        out
    }
}

fn meta_coord() -> Coordinate {
    phext::to_coordinate("1.1.1/1.1.1/1.1.1")
}

/// Delimiter bytes, highest dimension first (library .. scroll).
const BREAKS: [u8; 9] = [0x01, 0x1F, 0x1E, 0x1D, 0x1C, 0x1A, 0x19, 0x18, 0x17];

fn comps(c: &Coordinate) -> [usize; 9] {
    [
        c.z.library, c.z.shelf, c.z.series, c.y.collection, c.y.volume, c.y.book, c.x.chapter,
        c.x.section, c.x.scroll,
    ]
}

/// Emit delimiter bytes that advance `cur` to `tgt` (tgt must not
/// precede cur). A break increments its dimension and resets all
/// lower dimensions to 1 — the phext addressing rule, applied
/// forwards. This is the engine's append cursor at work: extending
/// the WAL is O(batch), with no re-parse of the existing log.
fn emit_until(out: &mut Vec<u8>, cur: &mut [usize; 9], tgt: [usize; 9]) {
    while *cur != tgt {
        let i = (0..9)
            .find(|&i| cur[i] < tgt[i])
            .expect("append target precedes current coordinate");
        out.push(BREAKS[i]);
        cur[i] += 1;
        for j in i + 1..9 {
            cur[j] = 1;
        }
    }
}

fn entry_coord(seq: u64, index: usize) -> Coordinate {
    phext::to_coordinate(&format!("1.1.1/1.{}.1/2.1.{}", seq, index))
}

fn meta_content(checkpoint_seq: u64) -> String {
    format!("format=holo-phext-wal/1\ncheckpoint_seq={}\n", checkpoint_seq)
}

fn wal_path_for(base: &Path) -> PathBuf {
    let name = base.file_name().unwrap_or_default().to_string_lossy().to_string();
    let stem = name.strip_suffix(".phext").unwrap_or(&name);
    base.with_file_name(format!("{}.wal.phext", stem))
}

fn is_dimension_break(b: u8) -> bool {
    matches!(b, 0x01 | 0x17 | 0x18 | 0x19 | 0x1A | 0x1C | 0x1D | 0x1E | 0x1F)
}

/// Byte offset where the delimiter run introducing `coord` begins.
fn scroll_start(text: &str, coord: Coordinate) -> usize {
    let mut at = phext::offset(text, coord);
    let bytes = text.as_bytes();
    while at > 0 && is_dimension_break(bytes[at - 1]) {
        at -= 1;
    }
    at
}

fn sync_dir(path: &Path) -> io::Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    File::open(dir)?.sync_all()
}

/// The synchronous engine. The async interface drives one `Core`
/// from a single writer task; the compat facade drives it directly.
pub struct Core {
    base_path: PathBuf,
    wal_path: PathBuf,
    /// Opened lazily on the first write: opening a store read-only
    /// must not create the WAL file beside the document.
    wal_file: Option<File>,
    wal_text: String,
    /// Components of the WAL's last non-empty scroll — the append
    /// cursor. Maintained by open/commit/checkpoint so extending
    /// the log never re-parses it.
    wal_last: [usize; 9],
    next_seq: u64,
    checkpoint_seq: u64,
    pub shared: Arc<Shared>,
}

impl Core {
    pub fn open(base_path: &Path) -> io::Result<Core> {
        Self::open_inner(base_path, BaseLattice::open(base_path)?)
    }

    /// Opens with a prebuilt index for the base document — the
    /// scan happens exactly once, in the caller (phext-lattice's
    /// open path builds it for its own navigation anyway).
    pub fn open_with_index(base_path: &Path, index: LatticeIndex) -> io::Result<Core> {
        Self::open_inner(base_path, BaseLattice::open_with_index(base_path, index)?)
    }

    fn open_inner(base_path: &Path, base: BaseLattice) -> io::Result<Core> {
        let wal_path = wal_path_for(base_path);

        // ---- load or create the WAL ----
        let wal_existed = wal_path.exists();
        let mut wal_text = if wal_existed {
            let bytes = std::fs::read(&wal_path)?;
            // A torn final write can split a UTF-8 sequence; the
            // valid prefix is the log. Truncate the file to it
            // before replay — the partial tail is not a transaction
            // and must not sit where appends will land.
            let valid = match std::str::from_utf8(&bytes) {
                Ok(_) => bytes.len(),
                Err(e) => e.valid_up_to(),
            };
            if valid < bytes.len() {
                let f = OpenOptions::new().write(true).open(&wal_path)?;
                f.set_len(valid as u64)?;
                f.sync_all()?;
            }
            String::from_utf8(bytes[..valid].to_vec()).unwrap_or_default()
        } else {
            let mut m = HashMap::new();
            m.insert(meta_coord(), meta_content(0));
            phext::implode(m)
        };
        let checkpoint_seq = phext::fetch(&wal_text, meta_coord())
            .lines()
            .find_map(|l| l.strip_prefix("checkpoint_seq="))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        // ---- replay: validate transaction by transaction ----
        let exploded = phext::explode(&wal_text);
        let mut by_seq: BTreeMap<u64, Vec<Coordinate>> = BTreeMap::new();
        for coord in exploded.keys() {
            if coord.y.collection == 1 && coord.x.chapter == 2 {
                by_seq.entry(coord.y.volume as u64).or_default().push(*coord);
            }
        }
        let mut overlay: BTreeMap<Coordinate, Option<String>> = BTreeMap::new();
        let mut applied_seq = checkpoint_seq;
        let mut truncate_at: Option<usize> = None;
        for (seq, mut coords) in by_seq {
            coords.sort();
            if seq != applied_seq + 1 {
                truncate_at = Some(scroll_start(&wal_text, coords[0]));
                break;
            }
            let (commit_at, entry_coords) = coords.split_last().unwrap();
            let mut ops: Vec<(Coordinate, Op)> = Vec::new();
            // Verify from the exploded map — never by fetching from
            // the raw text. libphext's fetch parses the whole
            // document per call, so per-transaction verification over
            // the text is quadratic in the WAL size (found by the
            // HoloStore engine integration: recovery of a
            // 10,050-transaction WAL spun for tens of minutes). From
            // the map, recovery is linear in the file.
            let pairs: Vec<(Coordinate, String)> = entry_coords
                .iter()
                .map(|c| (*c, exploded.get(c).cloned().unwrap_or_default()))
                .collect();
            // The marker is stored as the bare commit_scroll body
            // (its integrity is the embedded checksum over the
            // entries); the entries themselves are checked scrolls.
            let commit_body = exploded.get(commit_at);
            let mut valid = match &commit_body {
                Some(body) => {
                    let mut check_input = String::new();
                    for (c, checked) in &pairs {
                        check_input += &c.to_string();
                        check_input.push('\n');
                        check_input += checked;
                        check_input.push('\n');
                    }
                    **body
                        == format!(
                            "phext-commit:1\ncount={}\ncheck={}",
                            pairs.len(),
                            phext::checksum(&check_input)
                        )
                }
                None => false,
            };
            if valid {
                for (_c, checked) in &pairs {
                    match phext::uncheck_scroll(checked)
                        .and_then(|body| parse_entry(&body))
                    {
                        Some(op) => ops.push(op),
                        None => {
                            valid = false;
                            break;
                        }
                    }
                }
            }
            if !valid {
                truncate_at = Some(scroll_start(&wal_text, coords[0]));
                break;
            }
            for (coord, op) in ops {
                overlay.insert(coord, match op { Op::Set(v) => Some(v), Op::Delete => None });
            }
            applied_seq = seq;
        }
        if let Some(at) = truncate_at {
            wal_text.truncate(at);
            let f = OpenOptions::new().write(true).open(&wal_path)?;
            f.set_len(at as u64)?;
            f.sync_all()?;
        }

        let _ = wal_existed;

        let wal_last = phext::explode(&wal_text)
            .keys()
            .max()
            .map(|c| comps(c))
            .unwrap_or_else(|| comps(&meta_coord()));
        let shared = Arc::new(Shared {
            base: RwLock::new(Arc::new(base)),
            compacted: RwLock::new(Arc::new(overlay)),
            deltas: RwLock::new(Arc::new(Vec::new())),
            seq: RwLock::new(applied_seq),
        });
        Ok(Core {
            base_path: base_path.to_path_buf(),
            wal_path,
            wal_file: None,
            wal_text,
            wal_last,
            next_seq: applied_seq + 1,
            checkpoint_seq,
            shared,
        })
    }
}

fn parse_entry(body: &str) -> Option<(Coordinate, Op)> {
    let (head, value) = match body.split_once("\n\n") {
        Some((h, v)) => (h, Some(v)),
        None => (body, None),
    };
    let mut coord = None;
    let mut op = None;
    for line in head.lines() {
        if let Some(v) = line.strip_prefix("coord=") {
            coord = Some(phext::to_coordinate(v));
        }
        if let Some(v) = line.strip_prefix("op=") {
            op = Some(v.to_string());
        }
    }
    match (coord, op.as_deref()) {
        (Some(c), Some("set")) => value.map(|v| (c, Op::Set(v.to_string()))),
        (Some(c), Some("del")) => Some((c, Op::Delete)),
        _ => None,
    }
}

fn entry_body(coord: &Coordinate, op: &Op) -> String {
    match op {
        Op::Set(value) => format!("coord={}\nop=set\n\n{}", coord, value),
        Op::Delete => format!("coord={}\nop=del", coord),
    }
}

impl Core {
    fn wal_file(&mut self) -> io::Result<&mut File> {
        if self.wal_file.is_none() {
            self.wal_file = Some(OpenOptions::new().append(true).create(true).open(&self.wal_path)?);
        }
        Ok(self.wal_file.as_mut().unwrap())
    }

    pub fn seq(&self) -> u64 {
        self.shared.seq()
    }

    pub fn checkpoint_seq(&self) -> u64 {
        self.checkpoint_seq
    }

    pub fn wal_path(&self) -> &Path {
        &self.wal_path
    }

    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    /// Commits a batch of transactions: all of them are serialized
    /// into the WAL, written as one suffix, and synced ONCE; only
    /// then is each transaction applied, in order, one overlay swap
    /// per transaction. Returns the sequence of each transaction.
    /// This is the group-commit point the async writer drives.
    pub fn commit_batch(&mut self, txns: Vec<Txn>) -> io::Result<Vec<u64>> {
        let mut seqs = Vec::with_capacity(txns.len());
        let start_seq = self.next_seq;
        let old_len = self.wal_text.len();
        // Validate and serialize every transaction first; nothing
        // is mutated until the whole batch is known-good.
        let mut all_entries: Vec<Vec<(Coordinate, String)>> = Vec::with_capacity(txns.len());
        for txn in &txns {
            if txn.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty transaction"));
            }
            let seq = start_seq + all_entries.len() as u64;
            let mut entries: Vec<(Coordinate, String)> = Vec::with_capacity(txn.ops.len() + 1);
            for (i, (coord, op)) in txn.ops.iter().enumerate() {
                if let Op::Set(v) = op {
                    if v.is_empty() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "a set with an empty value would delete the scroll; use Op::Delete",
                        ));
                    }
                }
                entries.push((entry_coord(seq, i + 1), phext::checked_scroll(&entry_body(coord, op))));
            }
            let pairs: Vec<(Coordinate, &str)> =
                entries.iter().map(|(c, s)| (*c, s.as_str())).collect();
            let marker = phext::commit_scroll(&pairs);
            entries.push((entry_coord(seq, txn.ops.len() + 1), marker));
            all_entries.push(entries);
            seqs.push(seq);
        }
        // Extend the log from the append cursor: the suffix is
        // O(batch). libphext::append_batch re-parses the document
        // on every call — right for a stateless library that cannot
        // know the end position, quadratic when chained per
        // transaction (the second defect this integration found).
        // The engine knows exactly where its WAL ends.
        let mut text = std::mem::take(&mut self.wal_text);
        let mut cursor = self.wal_last;
        let mut bytes: Vec<u8> = Vec::new();
        for entries in &all_entries {
            for (coord, content) in entries {
                emit_until(&mut bytes, &mut cursor, comps(coord));
                bytes.extend_from_slice(content.as_bytes());
            }
        }
        let suffix = String::from_utf8(bytes).expect("WAL suffix is valid UTF-8");
        text.push_str(&suffix);
        self.next_seq = start_seq + seqs.len() as u64;
        // one write, one sync — then, and only then, apply
        let wal = self.wal_file()?;
        wal.write_all(&text.as_bytes()[old_len..])?;
        wal.sync_data()?;
        self.wal_text = text;
        self.wal_last = cursor;
        // Apply the batch as ONE delta entry. Visibility is per
        // batch — acks are too (the batch synced as one) — and the
        // batch's seqs are only revealed to callers now, so no
        // observer can hold a mid-batch sequence. The code this
        // replaces cloned the entire overlay map per transaction:
        // O(batch × map), which is the write-path collapse the
        // HoloStore benchmarks measured (1,490 ops/s against a
        // 156k ops/s hand-rolled baseline at 4,096 live scrolls).
        let changes: Vec<(Coordinate, Op)> =
            txns.iter().flat_map(|t| t.ops.iter().cloned()).collect();
        let final_seq = seqs.last().copied().unwrap_or(0);
        {
            let mut next = (**self.shared.deltas.read().unwrap()).clone();
            next.push((final_seq, changes));
            *self.shared.deltas.write().unwrap() = Arc::new(next);
            *self.shared.seq.write().unwrap() = final_seq;
        }
        self.maybe_compact();
        Ok(seqs)
    }

    /// Fold the accumulated deltas into the compacted map once
    /// they pass the threshold. Safe against concurrent snapshots:
    /// ops are absolute, so a reader who sees the new compacted
    /// map together with the not-yet-cleared deltas gets the same
    /// answers (delta entries win, and they agree).
    fn maybe_compact(&mut self) {
        let total: usize = self
            .shared
            .deltas
            .read()
            .unwrap()
            .iter()
            .map(|(_, ch)| ch.len())
            .sum();
        if total <= COMPACT_THRESHOLD {
            return;
        }
        let merged = self.shared.snapshot().overlay_map();
        *self.shared.compacted.write().unwrap() = Arc::new(merged);
        *self.shared.deltas.write().unwrap() = Arc::new(Vec::new());
    }

    /// Folds the overlay into the base file (temp + sync + rename +
    /// directory sync), remaps the base, and restarts the WAL with
    /// only its meta scroll. Crash between the rename and the WAL
    /// rewrite is safe: replaying already-folded transactions onto
    /// the new base is idempotent, because ops are absolute.
    pub fn checkpoint(&mut self) -> io::Result<()> {
        let snapshot = self.shared.snapshot();
        let base_text = std::str::from_utf8(snapshot.base.bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut map: HashMap<Coordinate, String> = phext::explode(base_text);
        for (coord, value) in snapshot.overlay_map().iter() {
            match value {
                Some(v) => {
                    map.insert(*coord, v.clone());
                }
                None => {
                    map.remove(coord);
                }
            }
        }
        let text = phext::implode(map);
        let tmp = self.base_path.with_extension("tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.base_path)?;
        sync_dir(&self.base_path)?;

        self.checkpoint_seq = snapshot.seq;
        let mut m = HashMap::new();
        m.insert(meta_coord(), meta_content(self.checkpoint_seq));
        self.wal_text = phext::implode(m);
        self.wal_last = comps(&meta_coord());
        // clone before taking the file handle: wal_file() borrows
        // all of self mutably, and the meta text is a few dozen bytes
        let meta = self.wal_text.clone();
        let wal = self.wal_file()?;
        wal.set_len(0)?;
        wal.seek(SeekFrom::Start(0))?;
        wal.write_all(meta.as_bytes())?;
        wal.sync_all()?;

        *self.shared.base.write().unwrap() = Arc::new(BaseLattice::open(&self.base_path)?);
        *self.shared.compacted.write().unwrap() = Arc::new(BTreeMap::new());
        *self.shared.deltas.write().unwrap() = Arc::new(Vec::new());
        Ok(())
    }
}
