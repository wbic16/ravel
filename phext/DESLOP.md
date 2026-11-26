# phext-lattice slop census — 2026-11-26

Will's direction (2026-11-26 01:26 CST): "phext-lattice is getting
slop-ified" / "now that we have the functionality, we need to remove
the slop" / "never allow scope creep to ruin a good design."

**The design line restored:** phext-lattice is the lattice — storage,
index, mapping, and (per the Variant C verdict) the holo-phext
transaction layer. Everything that is a *consumer's* machinery —
serving HTTP, SPARQL evaluation as a hosted service, per-soul quad
hosting for ravel — is scope creep in this repo, however well it
works. Functionality is proven; now it goes home.

Measured on exo @ 1d49be1 (which already contains PRs #105/#106 —
see premise note at the bottom). src/ = 36,449 lines; tests/ = 1,926.

## Census

| Piece | Lines | Arrived via | Class | Destination |
|---|---:|---|---|---|
| `src/index.rs` (shim over holo-phext index) | 14 | PR #70 (holo re-impl) | TRANSACTION LAYER | stays |
| `src/mmap.rs` (MappedLattice; plain mode on holo-phext, chains) | 1,213 | core; chains PR #64; re-impl PR #70 | LATTICE / TRANSACTION | stays |
| `src/coordinate_ext.rs` | 333 | core | LATTICE | stays |
| `src/lib.rs` | 69 | core | LATTICE | stays (loses one line: `pub mod sparql`) |
| Library surface: cursor 345, navigator 288, editor 306, modes 605, undo 586, search 415, term_index 355, names 219, sidecar 1,652, stats 306, media 159, sentron 423, tts 320 + `phext_tts` bin 268, uml6d 313 + uml6d_diagram 814 | 7,374 | v1.x era, pre-creep | LATTICE (library) — see judgment call 3 | stays |
| Editor app: `phext_edit` bin 15,584 (121 tests), `uml6d_edit` bin 1,594, `phext_nav` bin 1,022, web-gated modules (oauth 3,452, claims 252, joins 140, sponsor 261, submissions 302, traversals 273, wavefront 196), `web/` assets 3.5 MB / 26 files, teixml-to-mutable dep (editor TEI load, PR #64) | 23,076 + assets | in-repo since the 2026-03 flatten; TEI/chains PR #64 | CREEP by the design line — **judgment call 1: NOT extracted in this pass** | stays (Will's call) |
| `src/sparql.rs` — SPARQL subset evaluator incl. VecSource + `push_nt_line`/`remove_nt_line` + #104 fan-out dedup | 3,609 (26 tests) | PR #99 (2026-11-23); fold-ins PR #106 | CREEP — judgment call 2 | **→ ravel `phext/serve/`** |
| `src/bin/phext_sparql.rs` — evaluator as a line-protocol process | 104 | PR #99 | CREEP | **→ ravel `phext/serve/`** |
| `src/bin/phext_serve.rs` — HTTP server: holo store hosting, /txn, /seq, /updates, /quads, /query, checkpointing | 657 | PR #105 (Variant C); fold-ins PR #106 | CREEP | **→ ravel `phext/serve/`** |
| `scripts/phext_serve_smoke.sh`, `scripts/phext_serve_checkpoint_smoke.sh` | 2 scripts | PRs #105/#106 | CREEP (the server's own tests) | **→ ravel `phext/serve/scripts/`** |
| Cargo.toml: direct `tokio` dep (sole consumer: phext_serve.rs), `[[bin]]` stanzas for phext-serve + phext-sparql | — | PRs #99/#105 | CREEP | removed |

Extracted total: **4,370 lines + 2 scripts + manifest wiring.**
phext-serve's entire lattice dependency was one import line
(`phext_lattice::sparql::{…}`); sparql.rs references nothing else in
the crate (zero `crate::` uses, std only). The extraction severs the
edge completely: the receiving crate depends on holo-phext +
libphext + tokio, **not** on phext-lattice.

## Why ravel is the home

phext-serve exists for exactly one consumer: ravel's holo transport
(`RAVEL_TRANSPORT=holo` in `phext/backend/phextio.py`). The
evaluator's subset is defined by the queries the ravel stack issues —
its own header says so, and the defining document (SPARQL-SUBSET.md)
already lives in ravel's tree. Ravel's V1-REPORT recorded the
evaluator graduating *out* of the lattice when a second non-Rust
consumer appears; none has. Per the brief's option, it lands as a
small dedicated crate inside the ravel repo (`phext/serve/`,
package `phext-serve`), not a new repo.

## Judgment calls (flagged, per the brief)

1. **The editor stratum is not extracted in this pass.** By the
   design line, phext_edit + the web feature + web assets are an
   application, not the lattice — the single largest non-lattice
   mass in the repo (~23k lines). It is also the shipping
   edit.phext.io product, in-repo since March, with deploy tooling
   (`deploy/`, Dockerfile, compose) bound to this tree. Extracting
   it is a product decision for Will, not a cleanup step. Recorded
   here so the decision is explicit, not silent.
2. **sparql.rs is a library, not a server — extracted anyway.**
   Its only in-repo consumers were the two extracted bins; nothing
   in storage/index/mapping/transaction uses it. If a second
   consumer ever appears, it can graduate back out of ravel as a
   shared crate, per ravel's own V1-REPORT trigger.
3. **The library surface stays, flagged.** Under the narrowest
   reading of the design line, navigation/editing modules
   (cursor, undo, sidecar, sentron, …) are a second stratum above
   storage. They are the crate's documented mission ("foundation
   layer for phext-fluent editing", lib.rs), they predate the
   creep, they serve the editor product, and they are not another
   project's machinery. Classified LATTICE (library); untouched.
4. **Defect #104's fix changes home.** The fan-out dedup was
   landed as a phext-lattice fix in PR #106. It lives inside
   sparql.rs, so it moves with the evaluator into ravel's crate.
   The defect stays fixed; provenance is recorded here and in
   ravel's PATCHES.md.
5. **No version bump.** Removing `pub mod sparql` and two bins is
   breaking for external consumers of `phext_lattice::sparql`;
   the only known consumers are the extracted bins themselves.
   The bump (1.3.1 → next) is Will's merge call.

## Premise note

The brief was written when PRs #105/#106 were open. By execution
time they had been **merged** to exo (c052318, 1d49be1), so the
"revise/close the PRs" path no longer exists: the lattice side is
a fresh removal PR against current exo, referencing #105/#106 as
the placement it supersedes. Nothing is deleted from history —
extraction is relocation with provenance: the code's git history
in phext-lattice remains, and the receiving crate records where
each file came from.

## Test accounting

Baseline (exo @ 1d49be1, aletheia-core, `cargo test`):
lib **228/228** (incl. sparql's 26), phext_edit bin **121/121**,
image_support 4/4, real_phext 3/3, doc 0. e2e_local 1/2 —
`local_editor_regression` fails at baseline on a 50 ms page-latency
limit (61.30 ms best-of-3): the known pre-existing timing failure,
present on the unmodified tree.

After extraction (expected, verified in the PRs):
- phext-lattice lib **202/202** (228 − 26 sparql tests, which move
  with the file and run in the ravel crate: 26/26 there).
- phext_edit 121/121, image_support 4/4, real_phext 3/3 unchanged;
  e2e_local identical to baseline.
- ravel: phext-serve smokes 30/30 + checkpoint 15/15 from their
  new home; harness 72/72 + Wavelets 19/19 with
  `RAVEL_TRANSPORT=holo` against the relocated server.
