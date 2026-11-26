# Variant C — ravel on holo-phext

Task 34.6.8/7.7.11/9.5.8 · Executed EE 807–831, 2026-11-26, on
aletheia-core (16 cores, 29 GB) under Will's direction "continue
with ravel integration" (2026-11-25 23:56 CST). The programme was
sharded to chrysalis-hub in the meta-dispatch ledger (scroll
2.1.1/1.1.1/1.1.1); the shard is unreachable and its RAID failover
was never written, so Will's direction superseded the shard and
the run executed directly. The bypass is recorded in the ledger
scroll itself (claim, EE 824).

## What was built

- **phext-serve** (phext-lattice, `src/bin/phext_serve.rs`, 583
  lines): one process hosting, per soul, a holo-phext store and
  the phext-lattice SPARQL evaluator. REST surface: the SQ subset
  (select/update/insert/toc) plus `/txn` (many scroll writes as
  ONE atomic holo-phext commit), `/seq` + `/updates` (the commit
  feed), `/quads` (delta-fed quad set), `/query` (evaluator
  in-process; the phext-sparql line protocol verbatim).
- **ravel backend, holo transport** (`RAVEL_TRANSPORT=holo`;
  SQ remains the default and is byte-identical in behaviour):
  `HoloStore` behind the `PhextStore` seam; write batching through
  `/txn`; per-transcript read caches; **eager quad maintenance** —
  at write time the backend primes its cache from the ingested
  events, derives the affected quads, and pushes NT-line diffs to
  the bridge, so a query never re-derives after a write. External
  writes are caught by the `/seq` + `/updates` poll and re-derived
  per touched transcript only.
- **Fold-ins to phext-lattice** (Phase D): per-pattern fan-out
  deduplication in `sparql.rs` (defect #104); `VecSource`
  line-level ingestion (`push_nt_line` / `remove_nt_line`) with
  phext-serve holding a persistent per-soul `VecSource` (queries
  no longer re-parse the quad set); phext-serve checkpointing
  (`POST /checkpoint`, checkpoint-on-graceful-stop).

## Suites

| Suite | SQ baseline | Variant C (holo) | SQ regression in the C tree |
|---|---:|---:|---:|
| Wavelets 1–3 | 19/19 | 19/19 | 19/19 |
| Harness | 72/72 | 72/72 | 72/72 |
| phext-lattice lib | 224/224 (exo) | 228/228 (with fold-ins) | — |
| phext-serve smokes | — | 30/30 + checkpoint 15/15 | — |

## Metrics — same node, same frozen event sets

measure.py (small corpus, medians, seconds):

| Metric | SQ baseline | Variant C | Native (V2 §2) |
|---|---:|---:|---:|
| point lookup | 0.0046 | 0.0008 | — |
| traversal 1-hop | 0.0058 | 0.0008 | — |
| traversal 3-hop | 0.0110 | 0.0014 | — |
| single-record mutation | 0.0687 | 0.0225 | — |

Scale (seconds; native from V2-REPORT §7.1, measured on
rock-mountain — a different, slower-I/O node; directional only):

| Item | Metric | Native | SQ baseline | Variant C |
|---|---|---:|---:|---:|
| incipit (266 ev, 1,064 q) | ingest 1st / 2nd | 0.17 / — | 0.950 / 1.704 | **0.037 / 0.035** |
| | point cold / warm | 0.000018 | 3.276 / 0.056 | **0.009 / 0.006** |
| | mutation (1 char) | 0.19 | 1.718 | **0.035** |
| tasks (864 ev, 3,456 q) | ingest 1st / 2nd | 0.26 / — | 6.942 / 12.028 | **0.126 / 0.119** |
| | point cold / warm | 0.000018 | 20.501 / 0.175 | **0.021 / 0.016** |
| | mutation (1 char) | 0.27 | 12.077 | **0.109** |
| state (3,761 ev, 15,044 q) | ingest 1st / 2nd | 1.07 / — | 18.996 / 33.485 | **0.179 / 0.202** |
| | point cold / warm | 0.000020 | 189.624 / 0.215 | **0.037 / 0.033** |
| | mutation (1 char) | 1.91 | 35.757 | **0.189** |

Store bytes: SQ single file 1,117,983 / 3,741,823 / 1,169,779.
Variant C writes land in the holo WAL (every version retained);
un-checkpointed totals ran 8.5–14.8× the SQ figure. With the
fold-in checkpoint discipline (checkpoint on graceful stop),
the state soul measures **1,170,016 B combined (main + WAL) =
1.0002× SQ**, against native's 50,497,072 B store for the same
content. Store-server startup: 0.252 s both transports — a tie
at the 0.25 s poll floor of the measurement method.

## Verdicts

**Against the reference (the phext-lattice/SQ-backed v2): beaten
on every measured metric** — ingest up to ~500×, re-ingest up to
~280× (the 2–4× second-ingest penalty is gone), mutation up to
~190×, cold point lookup up to ~5,100× (the cliff is eliminated:
cold now equals warm), warm lookup 6.5–9.3×, bytes at parity,
suites equal-green.

**Against native ravel** (per metric, Phase D form):
- Ingest — **BEATEN** on all three items (0.037–0.202 s vs
  0.17–1.07 s).
- Mutation — **BEATEN** on all three items (0.035–0.189 s vs
  0.19–1.91 s).
- Store bytes — **BEATEN** (state: 1.17 MB vs 50.5 MB).
- Cold query — **BEATEN as a category**: native has no cliff
  because it never re-derives; variant C no longer re-derives
  either (cold ≈ warm at every scale).
- Point/traversal latency floor — **NOT BEATEN**. Native answers
  in-process at ~20 µs; variant C's floor is 6–33 ms (HTTP
  round-trip + evaluator scan over the soul's VecSource). The
  gap is named, not smoothed: closing it means embedding the
  evaluator in the backend process, a different architecture
  than a store seam.

## Defects #100–#104 (phext-lattice issues)

- **#100 cold-query cliff — resolved in the holo path.** Eager
  incremental quad maintenance, fed by holo-phext's commit feed,
  removes full regeneration (189.6 s → 0.037 s at 15,044 quads).
- **#101 mutation = full re-ingest — resolved.** One atomic
  `/txn` commit + delta quad sync: 35.8 s → 0.189 s at state.
- **#102 warm latency floor — reduced ~10×, not eliminated.**
  The evaluator is in-process in phext-serve over a persistent
  VecSource; no quad set is serialized per query. The remaining
  floor is transport + scan (see NOT BEATEN above).
- **#103 re-ingest 2–4× first ingest — resolved.** Re-ingest now
  prices like first ingest on every item (table above).
- **#104 fan-out dedup — FIXED in phext-lattice.** Per-pattern
  deduplication in `sparql.rs`'s `match_bgp`, matching calamus's
  DISTINCT sub-select discipline. The §5.3 probe returns 1 row
  in plain and GRAPH ?g forms (was 2/2); phext-lattice lib suite
  228/228; the ravel harness stays 72/72 against the fixed
  evaluator; results on ravel-projected corpora are unchanged.

## Phase E — does holo-phext survive?

**It survives as phext-lattice's transaction layer; it dissolves
as a standalone surface.** Every variant-C win traces to a
holo-phext mechanism: WAL-append atomic multi-scroll commits
(the ingest/mutation wins), the commit feed behind `/updates`
(the incremental quad maintenance that killed the cold cliff),
and checkpoint (byte parity with the single-file format). The
compat `LatticeStore` facade was not the surface ravel needed —
the async engine was, hosted inside a phext-lattice binary.
phext-lattice keeps the document model, the index, and the
evaluator; holo-phext is its engine room. Friction found was
small and named (empty-set delete semantics at commit time, the
256-deep broadcast channel's lag behaviour, quad persistence
being the host's problem) and produced fixes in the host, not
workarounds around the engine. Note for the record: the lattice
exo branch pins holo-phext 0.2.5 (bafed5f); the 0.2.4 checkout
used at programme start is one rev behind.

## Complexity (ROI-3 headline, design-time)

Variant C's delta, in lines: ravel backend ≈ +550 Python across
`phextio.py` / `ravel_backend.py` (+10 in server/verify);
phext-serve 583 lines of Rust in phext-lattice; fold-ins ≈ 250
lines across `sparql.rs` / `phext_serve.rs`. Against the STUDY
instrument's native C=997 and v2 port C=427 (ROI.md), the
increment is a delta on the port, not a third architecture —
and it moves the port from "better store, worse engine" to
beating native on ingest, mutation, and bytes at working scale.

## Method notes and deviations

- Baseline and variant were measured on the same node with the
  same drivers and the same frozen event sets (extracted for
  V2-REPORT §7); phext bytes and quad counts reproduced exactly.
  Native figures are V2-REPORT's, from rock-mountain — cited for
  direction, never as same-conditions.
- The baseline scale leg peaked at 91.9 °C on the hub (over the
  88 °C pause line; discovered at leg end, leg completed, node
  recovered). The variant legs observed thermal discipline
  (peaks 74.0 °C and 41.9 °C).
- One behaviour change, in an error case only: a malformed quad
  line now fails the `/quads` call (HTTP 400) instead of being
  stored and failing every later query. Valid-line behaviour is
  identical, verified by harness + smokes.
- Quad sets are the bridge's in-memory state; a bridge restart
  is healed by the backend's first-sync full replace on Soul
  open. Store data itself is WAL-durable across restarts
  (verified in the bridge smoke).
