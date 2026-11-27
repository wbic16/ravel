//! HoloPhext — the layer2 protocol over phext.
//!
//! Phext (the format) gives every scroll an address and a text body.
//! What it deliberately does not give — because a format cannot — is
//! *storage semantics*: atomic multi-scroll writes, crash recovery,
//! concurrent readers, change notification. That missing layer is
//! HoloPhext:
//!
//! * **the lattice** — the base document lives in a memory-mapped
//!   file behind a one-scan coordinate index (the phext-lattice
//!   design, kept here now): reads are O(1) span lookups.
//! * **async I/O** — commits flow through a single writer task;
//!   callers `await` a sequence number, readers never block on it.
//! * **update dispatch** — every committed transaction is broadcast
//!   to subscribers as an `Update` (sequence + changed coordinates),
//!   in commit order.
//! * **ACID** — Atomicity from WAL commit markers (libphext checked
//!   scrolls + commit scrolls, validated at recovery); Consistency
//!   from the phext invariants enforced at commit; Isolation from
//!   snapshot reads over swapped overlay generations; Durability
//!   from sync-before-acknowledge WAL writes and the checkpoint
//!   profile (temp, sync, rename, directory sync).
//!
//! Two consumers shaped this interface: HoloStore's phext-native
//! storage core (wbic16/holostore, PHEXT.md round 2), whose WAL /
//! snapshot / checkpoint design this generalizes, and phext-lattice,
//! whose `MappedLattice` API the `compat` module mirrors so the
//! editor stack can be re-implemented on this engine mechanically.

pub mod compat;
pub mod core;
pub mod index;

pub use core::{Core, Op, Snapshot, Txn};
pub use index::{Dimension, LatticeIndex, ScrollSpan};
use libphext::phext::Coordinate;
use std::io;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};

/// Errors from the async interface.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The writer task is gone (the store was dropped).
    Closed,
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {}", e),
            Error::Closed => write!(f, "holo-phext writer is closed"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// One committed transaction, dispatched to subscribers in order.
#[derive(Debug, Clone)]
pub struct Update {
    pub seq: u64,
    pub changes: Vec<(Coordinate, Op)>,
}

enum Job {
    Commit(Txn, oneshot::Sender<Result<u64>>),
    Checkpoint(oneshot::Sender<Result<()>>),
}

/// An open HoloPhext store: an async handle over one `Core`.
pub struct HoloPhext {
    jobs: mpsc::Sender<Job>,
    updates: broadcast::Sender<Update>,
    shared: Arc<core::Shared>,
}

impl HoloPhext {
    /// Opens (creating if needed) the store whose base document is
    /// at `base_path`; the WAL lives beside it. Recovery runs here:
    /// valid committed transactions replay, a torn tail is truncated.
    pub async fn open(base_path: &Path) -> Result<HoloPhext> {
        let path = base_path.to_path_buf();
        let mut core = tokio::task::spawn_blocking(move || Core::open(&path))
            .await
            .map_err(|e| Error::Io(io::Error::new(io::ErrorKind::Other, e)))??;
        let shared = core.shared.clone();
        let (jobs, mut rx) = mpsc::channel::<Job>(128);
        let (updates, _) = broadcast::channel::<Update>(256);
        let updates_writer = updates.clone();
        tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                // group commit: gather what is already queued so one
                // WAL write + one sync serves the whole batch
                let mut commits: Vec<(Txn, oneshot::Sender<Result<u64>>)> = Vec::new();
                let mut checkpoints: Vec<oneshot::Sender<Result<()>>> = Vec::new();
                match first {
                    Job::Commit(t, ack) => commits.push((t, ack)),
                    Job::Checkpoint(ack) => checkpoints.push(ack),
                }
                while let Ok(job) = rx.try_recv() {
                    match job {
                        Job::Commit(t, ack) => commits.push((t, ack)),
                        Job::Checkpoint(ack) => checkpoints.push(ack),
                    }
                    if commits.len() >= 64 {
                        break;
                    }
                }
                if !commits.is_empty() {
                    let txns: Vec<Txn> =
                        commits.iter().map(|(t, _)| t.clone()).collect();
                    match core.commit_batch(txns) {
                        Ok(seqs) => {
                            for ((txn, ack), seq) in commits.into_iter().zip(seqs) {
                                let _ = updates_writer.send(Update {
                                    seq,
                                    changes: txn.ops.clone(),
                                });
                                let _ = ack.send(Ok(seq));
                            }
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            for (_, ack) in commits {
                                let _ = ack.send(Err(Error::Io(io::Error::new(
                                    io::ErrorKind::Other,
                                    msg.clone(),
                                ))));
                            }
                        }
                    }
                }
                for ack in checkpoints {
                    let _ = ack.send(core.checkpoint().map_err(Error::from));
                }
            }
        });
        Ok(HoloPhext { jobs, updates, shared })
    }

    /// Commits one transaction atomically; resolves with its
    /// sequence number once the WAL write is synced and the
    /// transaction is visible to new snapshots.
    pub async fn commit(&self, txn: Txn) -> Result<u64> {
        let (ack, rx) = oneshot::channel();
        self.jobs.send(Job::Commit(txn, ack)).await.map_err(|_| Error::Closed)?;
        rx.await.map_err(|_| Error::Closed)?
    }

    /// Folds all committed state into the base document and
    /// restarts the WAL (see `Core::checkpoint`).
    pub async fn checkpoint(&self) -> Result<()> {
        let (ack, rx) = oneshot::channel();
        self.jobs.send(Job::Checkpoint(ack)).await.map_err(|_| Error::Closed)?;
        rx.await.map_err(|_| Error::Closed)?
    }

    /// A frozen read view at the current sequence. Cheap: two Arc
    /// clones. The snapshot never observes a partial transaction.
    pub fn snapshot(&self) -> Snapshot {
        self.shared.snapshot()
    }

    /// Reads one scroll from a fresh snapshot.
    pub fn read(&self, coord: &Coordinate) -> Option<String> {
        self.shared.snapshot().get(coord)
    }

    /// The latest committed sequence.
    pub fn seq(&self) -> u64 {
        self.shared.seq()
    }

    /// Subscribes to committed updates, in commit order. Lagging
    /// receivers skip (broadcast semantics) — a subscriber that must
    /// not miss changes should re-snapshot on `Lagged`.
    pub fn subscribe(&self) -> broadcast::Receiver<Update> {
        self.updates.subscribe()
    }
}
