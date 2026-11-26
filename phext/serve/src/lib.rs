//! phext-serve — ravel's serving machinery, relocated from
//! phext-lattice in the 2026-11-26 de-slop (Will: "now that we have
//! the functionality, we need to remove the slop"). Provenance and
//! the full census: `../DESLOP.md`.
//!
//! Contents:
//! - `sparql` — the SPARQL subset evaluator (from phext-lattice
//!   PR #99; the subset is the construct set of every query the
//!   ravel stack issues, enumerated in `../SPARQL-SUBSET.md`),
//!   including the VecSource line-level ingestion and the #104
//!   fan-out deduplication (phext-lattice PR #106).
//! - bin `phext-serve` — one process hosting, per soul, a
//!   holo-phext store plus the evaluator (phext-lattice PR #105,
//!   Variant C), with checkpointing (PR #106).
//! - bin `phext-sparql` — the evaluator as a line-protocol process
//!   for non-Rust callers (PR #99).
//!
//! This crate deliberately does NOT depend on phext-lattice: the
//! lattice is storage/index/mapping/transaction; this is a
//! consumer's serving layer that happens to host the transaction
//! engine (holo-phext) directly.

pub mod sparql;
