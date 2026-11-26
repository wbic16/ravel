# Optimization pass — the point-query floor

Executed 2026-11-26 on aletheia-core (16 cores, 29 GB) under
Will's direction "pull reword in, then optimize further"
(2026-11-26 02:09 CST). Two steps: the upstream pull, then an
attack on the one floor `phext/VARIANT-C.md` named NOT BEATEN —
point-query latency (Variant C: 6–33 ms, "HTTP round-trip +
evaluator scan over the soul's VecSource", vs native's ~20 µs
in-process).

## Step 1 — upstream pull

Merged repolex-ai/ravel main @ `e59f811` ("raveld reword-api",
+264 lines across `src/bin/raveld.rs`, `src/memory_relive.rs`,
`src/writer.rs`) into our main: merge commit `8471091`,
conflict-free by construction (our divergence is purely additive
under `phext/` + `PATCHES.md` and touches zero of the reword
commit's files). Recorded in `PATCHES.md`. The reword code cannot
be compile-verified in this tree — ravel's native Rust build
remains blocked on repolex-ai git access for the private
calamus/tessera dependencies (standing fact since v1, not a
regression). Post-merge harness SQ regression: 72/72.

## Step 2 — where the 23 ms actually was

Attribution first, same frozen event sets, same drivers as
Variant C (`vc-scale_driver.py`'s definitions, re-run against
this tree; probe instruments hub-side at `~/probe.py` and
`~/ravel-opt-bench/`, state soul, 15,046 quads, medians of 30):

| Component | Before | After |
|---|---:|---:|
| evaluator `evaluate()`, in-process (Rust bench) | 21.53 ms | **0.0017 ms** |
| raw `POST /query` over HTTP | 22.9 ms | 0.185 ms |
| `GET /seq` poll | 0.14 ms | 0.15 ms |
| full `Soul.run_query` (point) | 23.4 ms | **0.349 ms** |
| two-pattern join (point-shaped) | 18.8 ms | 0.372 ms |

The named "transport + scan" split was lopsided: **the scan was
~21.5 ms of the ~23 ms**; HTTP transport was ~0.5–1 ms of it. Per
query, `evaluate()` materialized the whole dataset
(`DatasetView::resolve` cloned and re-hashed every quad),
`triples_in` cloned it again, and `match_bgp` then scanned all
15k triples per pattern. The fix is structural, in
`phext/serve/src/sparql.rs`:

- **`VecSource` position indexes.** The `Vec<Quad>` stays the
  store of record; three indexes (subject / predicate / object →
  ascending quad positions) and a per-graph count are maintained
  on every mutation — O(1) append on push, rebuild after a
  removal (a removal is already a linear retain). Two new unit
  tests pin the contract: `quads_matching` agrees with the
  full-listing filter for every position/graph/term-set, and the
  indexes stay exact across removals (28/28 crate tests).
- **`QuadSource::quads_matching`** (new trait seam, default =
  filter the full listing): quads in one graph whose term at a
  position equals one of a term set, in listing order.
- **Per-pattern candidates.** `DatasetView` now resolves to graph
  names + a dedup flag, not triples. `match_bgp` fetches
  candidates per pattern under the *current* solution, so a
  variable bound by one pattern becomes the next pattern's index
  key. The index position is chosen subject → predicate →
  object, with the prep expansions folded into the term set
  (predicate equivalence for a constant predicate; the subclass
  closure for a constant `rdf:type` object where the predicate
  can be `rdf:type`). The fetched set is always a superset of the
  pattern's matches and `match_pattern` still verifies every
  candidate, so results are unchanged — the harness's
  quad-for-quad discipline is the proof, not the argument.
  DESCRIBE's subject closure goes through the subject index too.
- No result caching anywhere: the indexes are write-maintained
  structure, so **cold still equals warm** — there is no cliff
  to reintroduce.

Transport, decided by the numbers: after the scan fix the whole
remaining path is ~0.35 ms, of which the evaluator is ~2 µs and
the rest is the `/seq` poll (~0.15 ms), the loopback HTTP
`/query` round trip, and Python glue. Embedding the evaluator in
the backend process (the only way to remove that remainder) is
the different architecture VARIANT-C named; at a third of a
millisecond of remainder it is **not justified** — recorded as a
decision, not an oversight.

## Results — scale driver, same definitions as Variant C

Point lookup, seconds (cold = first query after a write; warm =
median of 3):

| Item | VARIANT-C record | This pass, before | This pass, after |
|---|---:|---:|---:|
| incipit (1,064 q) cold / warm | 0.009 / 0.006 | 0.006 / 0.002 | **0.003 / 0.001** |
| tasks (3,456 q) cold / warm | 0.021 / 0.016 | 0.008 / 0.006 | **0.003 / 0.001** |
| state (15,044 q) cold / warm | 0.037 / 0.033 | 0.027 / 0.025 | **0.003 / 0.001** |

(The driver reports at ms resolution; the probe's finer medians
for the state soul are in the table above: 23.4 ms → 0.349 ms
full-path.) Ingest and mutation are unchanged within noise
(state: ingest 0.188/0.190 s, mutation 0.184 s, before and
after); store bytes identical. Against native's ~20 µs
(different node and method in V2-REPORT — directional only):
the evaluator itself now answers in ~2 µs in-process; the
end-to-end floor is the transport remainder below.

## Suites (after)

- phext-serve crate: `cargo test` 28/28 (26 + 2 new index tests)
- phext-serve smokes: 30/30 + checkpoint 15/15
- Harness: 72/72 holo, 72/72 SQ regression
- Wavelets 1–3: 19/19 in both transports

## The remaining floor, named

- **Point query, end to end: ~0.35 ms**, made of the `/seq`
  external-change poll, one loopback HTTP round trip, and Python
  glue — the store-seam transport, not evaluation (~2 µs).
  Closing it means embedding the evaluator in the backend
  process; not justified at this size, per above.
- **Full-scan queries did not get the same win.** The
  `GRAPH ?g` COUNT over the state soul still takes ~4–5 s
  (4,977 ms before → 4,166 ms after, from the resolve-clone
  removal alone). Its dominant cost is identified, not fixed:
  `match_bgp`'s fan-out dedup keeps extensions in a `Vec` and
  membership-tests each new binding against all of them
  (`first.contains`), which is quadratic on all-variable
  patterns. That dedup is defect #104's discipline; making it
  hash-priced needs a canonical hashable binding key and its
  own pass. A scan is a scan — but it needn't be a quadratic
  one, and that is the next frontier if one is wanted.

Thermal discipline held throughout (build peak 69.2 °C, scale
peaks 62.6 °C before / 59.2 °C after; the 88 °C pause line was
not approached).

## The dedup pass — the full-scan floor (defect #104, hash-priced)

Executed 2026-11-26 on aletheia-core under Will's direction
"do it now" (2026-11-26 07:29 CST). The remaining-floor section
above named the target: `match_bgp`'s fan-out dedup
membership-tested each new extension against every kept extension
(`Vec::contains` over `Vec<Binding>`) — quadratic on all-variable
patterns.

### Attribution, three ways (all before any fix)

- **Scaling.** In-process `evaluate()` of the `GRAPH ?g` COUNT
  over prefix subsets of the state soul's quad set (3,762 / 7,523
  / 15,046 quads): minima 188 / 773 / 3,190 ms — ×4.1 per
  doubling, the quadratic signature.
- **Profile.** `perf` on the count: 61.7% of samples under
  `BuildHasher::hash_one` with `HashMap PartialEq::eq` stacks —
  the membership scan comparing binding maps.
- **Ablation.** A throwaway build with only the membership test
  removed (semantics off, never landed): count 5,486 → 15.2 ms,
  raw scan 4,603 → 27.8 ms. The membership test was ~99.7% of
  the full-scan cost.

### The fix

- **`BindingKey`** (`phext/serve/src/sparql.rs`): the canonical
  hashable key for a `Binding` — its `(variable, term)` entry
  pairs sorted ascending by variable name. Key equality is
  exactly `Binding` equality (the sorted pair list is a canonical
  form of the map's contents, compared element-wise — not a
  digest), and the definition is total over partial bindings: an
  unbound variable is simply absent, as in map equality.
- `match_bgp` keeps the extensions in first-occurrence order in
  the `Vec` (the recursion consumes them in that order — order
  preserved exactly) and answers membership from a
  `HashSet<BindingKey>`: the dedup is now linear in the number of
  extensions. Two new unit tests pin the key's equality contract
  and the dedup's exactness at 400-subject fan-out scale (crate
  30/30).
- **Sibling audit** (same evaluation path): projection DISTINCT
  already hashes (sorted-items string keys in a `HashSet`) —
  left; the `SELECT *` variable census linear-scans a
  variable-name list bounded by the query, not the data — left;
  CONSTRUCT/DESCRIBE output dedup and the dataset view's
  cross-graph dedup already use `HashSet<Triple>` — left;
  `eval_group`'s GRAPH-variable and OPTIONAL fan-outs append
  without membership tests (their cost is the result size
  itself) — left. The `Vec::contains` in `match_bgp` was the
  only data-scaled quadratic membership in the evaluator.

### Results — in-process `evaluate()`, state soul (15,046 quads), medians of 11

| Shape | Before (ms) | After (ms) |
|---|---:|---:|
| `GRAPH ?g` COUNT(*) | 5,486 | **46.5** |
| COUNT(DISTINCT ?s) | 3,993 | **54.6** |
| raw scan `?s ?p ?o` | 4,603 | **71.9** |
| GROUP BY ?g count | 3,705 | **54.8** |
| SELECT DISTINCT ?p | 4,687 | **71.6** |
| join-scan (all-variable first pattern) | 4,792 | **102.2** |
| subject join (two patterns) | 172.6 | **19.0** |
| OPTIONAL over 3,761 solutions | 177.1 | **23.6** |
| fan-out (asked class, prep expansions) | 244.8 | **4.7** |
| point lookup | 0.0018 | 0.0019 |

Scaling after: count at 3,762 / 7,523 / 15,046 quads = 6.4 /
20.1 / 46.5 ms — the quadratic term is gone; what remains is
linear passes (candidate materialization, per-candidate binding
clones) plus ~2 µs per extension of key canonicalization, the
honest price of the dedup itself (the ablated no-dedup count was
15.2 ms).

Full path (`probe.py`, state soul): `count_full_scan_median_ms`
3,966 → **54.5 ms**; point `full_run_query` 0.355 → 0.360 ms —
unchanged. Scale driver (`opt-scale_driver.py`, all three
souls): point cold/warm unchanged (state 0.003 / 0.000–0.001 s);
ingest and mutation unchanged within the day's shared-node noise
(state ingest 0.210/0.211 s vs the 0.188/0.190 s recorded above,
mutation 0.198 vs 0.184 — the node carried another tenant's
pinned cores throughout this pass); store bytes identical
(state 17,338,049 combined).

### Equivalence

Differential digests of evaluation output (canonical sorted
serialization) for a 12-query battery over the state soul's quad
set, before vs after builds: **identical on every query**,
including the 15,046-row raw scan, the all-variable join-scan,
and the fan-out shape whose duplicate extensions the dedup
exists to collapse. Suites: crate 30/30; serve smokes 30/30 +
checkpoint 15/15; harness 72/72 holo and 72/72 SQ; Wavelets
19/19 in both transports.

### Thermal note

This pass shared the node with another tenant's pinned-core
batch queue from 07:48 on (the package held at 81–90 °C under
that load). The after-timings leg peaked at 88.6 °C, crossing
the 88 °C pause line at its end (found at leg end; the node was
heat-soaked from the preceding legs). The suite legs then ran
under the vc-runner discipline (start below the line, SIGSTOP
on three sustained >88 °C samples, resume <82 °C) and completed
with no pause triggered.
