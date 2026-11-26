# PATCHES.md

What changed since the fork, and why. Divergence from upstream is recorded here, entry by entry.

## 2026-11-23 — ravel-phext v1: phext backend added under `phext/`

- Added `phext/backend/` — a Python backend for ravel on phext (via SQ + phext-lattice), implementing the same daemon API and store seam as the Oxigraph-backed store, with Oxigraph off the critical path. One phext per soul; transcript bytes stay canonical, the graph remains a rebuildable projection.
- Added `phext/verify/` — the verification suite: lossless roundtrip in both directions, mutation control, quad-for-quad equality against an independently transcribed reference projector (65/65 checks).
- Added `phext/STUDY.md` (the study: API contract, census, complexity 967 / cache-ability 93 instrument), `phext/SPARQL-SUBSET.md` (the scoped SPARQL subset determination), `phext/V1-REPORT.md` (v1 comparison: complexity 967 → 397, locked packages 331 → 0, budget verdicts, abstraction plan).
- The SPARQL subset evaluator does NOT live in this repo: it was abstracted into phext-lattice (`src/sparql.rs`, PR https://github.com/wbic16/phext-lattice/pull/99, merged 2026-11-23).
- No upstream ravel files modified. Native head-to-head timings remain unmeasured: ravel's Rust build is blocked on repolex-ai git access (calamus/tessera) — see `phext/V1-REPORT.md`.
- 2026-11-23: phext/verify/ gains wavelets_test.py + README.md — Wavelets 1–3 documented as unit tests in ravel's terms (19/19 passing against the real backend + SQ).
- 2026-11-24: phext/backend ontology_prep() rewritten (v2 erratum C-1): the v1 line regex matched zero edges in the shipped Turtle, so abstract-name rewriting never fired; now a faithful port of calamus Index (real Turtle parse, all four edge kinds), diffed identical against the real crate.
- 2026-11-24: phext/backend recent_turns() corrected (v2 erratum P-1): timestamp-less turns are never listed in the recent graph, matching native keep_recent; harness want_recent corrected to the same rule and an outside phase added (72/72; the fixed harness fails the v1 backend on exactly these two errata).
- 2026-11-24: phext/ gains V2-REPORT.md + SCALE.md and phext/verify/v2/ (the v2 instruments); V1-REPORT.md carries dated erratum notes at the C-1 and P-1 claim sites.
- 2026-11-24: phext/ROI.md added — complexity-ROI of the two variants on the STUDY §6.1 instrument (native C=997 vs phext port C=427), per-axis returns from the v2 measurements. Delivered locally first; pushed on Will's scope correction — the local-only scope was Emise's error, never Will's instruction. A design-time three-variant ROI follows separately.

## 2026-11-26 — ravel-phext Variant C: holo-phext backend (phext-serve)

- Added a second transport for `phext/backend/` behind the existing store seam: `HoloStore` in `phextio.py` talks to **phext-serve**, a phext-lattice binary hosting a holo-phext store + the lattice SPARQL evaluator in one process. Selected by `RAVEL_TRANSPORT=holo`; SQ remains the default and is behaviour-identical (harness 72/72 and Wavelets 19/19 in both transports, SQ regression included).
- Write path: multi-scroll writes batch into ONE atomic holo-phext commit (`/txn`); quad maintenance is eager and incremental — the backend primes its per-transcript caches at ingest and pushes NT-line diffs to the bridge's persistent quad set, fed by holo-phext's commit feed (`/seq` + `/updates`). The v2 cold-query cliff (189.6 s at 15,044 quads) is eliminated: cold now equals warm (0.037 s). At working scale Variant C beats the SQ backend on every measured metric and beats native ravel on ingest, mutation, and store bytes; the point-query latency floor (ms vs native's in-process µs) is named NOT BEATEN in `phext/VARIANT-C.md`.
- Added `phext/VARIANT-C.md` — the full Phase C/D/E record: metric tables (baseline and variant measured same-node, same frozen event sets), per-metric verdicts, defect #100–#104 dispositions, and the holo-phext survival verdict (survives as phext-lattice's transaction layer; dissolves as a standalone surface).
- Companion changes in phext-lattice (PRs, Will merges): phext-serve itself; `sparql.rs` fan-out deduplication fixing defect #104 (probe 2 rows → 1, calamus-equal); `VecSource` line-level ingestion; phext-serve checkpointing (WAL compaction; state soul 1,170,016 B combined vs SQ's 1,169,779 B).
- Executed on aletheia-core under Will's "continue with ravel integration" (2026-11-25), bypassing the unreachable chrysalis-hub shard in the meta-dispatch ledger; the bypass and attestation are recorded in the ledger scroll 2.1.1/1.1.1/1.1.1.
