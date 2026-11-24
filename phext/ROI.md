# Ravel — ROI in terms of complexity, for the two variants

Task 34.6.8/7.7.11/9.5.8 · 2026-11-24 · Will's directive (2026-11-24
00:49 CST, on the V2 report): "look at ROI in terms of complexity for
the two variants".

**Variant A — native ravel:** the Rust daemon + Oxigraph/RocksDB +
calamus + tessera, as ravel ships it.
**Variant B — the phext port:** the Python backend + the phext-lattice
SPARQL subset evaluator + SQ + libphext, implementing the same store
seam and route surface.

No new benchmarks were run for this analysis. Every performance number
below is V2-REPORT.md's, measured on rock-mountain; every complexity
input is traced to STUDY.md, V1-REPORT.md (with its v2 errata notes),
SCALE.md, DEP-AUDIT.md, or a direct census of the calamus/tessera
clones under `v2/`.

## Bottom line, before the arithmetic

There is no honest single scalar ROI: the axes do not commensurate.
What the numbers support is a split verdict.

- **B buys storage, record-kind fidelity, and supply-chain smallness
  at 43% of A's full-stack complexity score** (427 against 997).
  On our real corpus it holds the content in ≈15.6 MB where A's
  stores take ≈183 MB, and it is the only one of the two whose data
  model can hold 39 of the 42 record kinds in SCALE.md's inventory
  without pretending they are conversations.
- **A buys interactive performance, write behaviour at scale, an
  implemented gate, and reference correctness at 2.34× B's
  complexity score.** At working scale A wins ingest, mutation, and
  every query axis — most of them by orders of magnitude that a
  2.34× complexity discount does not dent.
- **The port's current ROI is archival, not operational.** Its one
  disqualifying cost — the cold-query cliff after a write — is also
  the cheapest thing in either variant to fix: incremental quad
  generation is priced below at +3–6 complexity points (≈1% of B's
  score) against a measured 196.3 s cliff. That fix moves the
  read-after-write boundary; it does not close the warm-latency gap
  (defect #102) or the write-path cost (defect #101's other half).

---

## 1. The instrument (STUDY.md §6.1, unchanged)

```
C = LOC_prod/100 + locked_packages_total + 50 × distinct_formats
    + 100 × (languages − 1)
```

with K1 (dependency weight: direct + locked packages), K2 (bespoke
machinery share — production LOC in modules whose stated purpose is
storage, serialization, indexing, migration, or locking of the
store), K3 (state surface: persisted artifacts and distinct formats),
K4 (production languages). Prior reports truncate the LOC component
to an integer before summing; this analysis follows that convention
and shows exact sums in parentheses where they differ.

### Accounting rules used here, stated once

1. **LOC_prod counts owned, maintained code, not dependency source.**
   This is the study's own basis: A's 967 never counted Oxigraph's or
   RocksDB's source lines; their weight enters through K1's package
   count. The same rule is applied to B: SQ's and libphext's *source*
   is not LOC-counted; their weight enters through SQ's package tree.
2. **calamus and tessera are the exception, and the task names them:**
   they are first-party stack crates whose source v1 could not see
   (private repos), so their LOC is added to A explicitly in §2.
   Oxigraph is a third-party dependency and stays package-counted,
   exactly as in the study.
3. **B's headline package count is SQ's host tree: 30 unique package
   names** (6 direct), per DEP-AUDIT.md §2/§4 — the operational
   surface on the Linux host. libphext is one of those 30 (and brings
   exactly one transitive package, xxhash-rust), so it is not counted
   twice. A lock-entry sensitivity is given in §3.3: SQ's shipped
   Cargo.lock carries 55 all-target/dev entries; scored that way B
   rises to 452 and no verdict in §4 flips.
4. **B's LOC basis is the v1 as-built census** (backend 1,661 +
   evaluator production 3,059), the basis the v1 score used. The v2
   errata fixes have since grown the backend files on disk to 1,807
   lines (ravel_backend.py 1,153 → 1,299); scored on current files,
   B's full-stack C is 428 instead of 427. The sensitivity is noted
   wherever the headline is used; it changes nothing downstream.
5. **Scope of the comparison is the scope V1 scored:** A's whole
   engine against B's backend system (store seam + routes +
   evaluator + its storage substrate). Layers above the seam that
   both variants share as consumers (the dialect adapters' *inputs*,
   the CLI contract, the writer's outputs) are not double-counted
   into B; B's own gaps above the seam (no gate, seam-level
   /sync + /import) are priced qualitatively in §4.7, because the
   instrument has no term for a missing feature.

---

## 2. Variant A — complexity, recomputed for the full stack

### 2.1 The v1-visible subtotal (STUDY.md §6.2)

| Component | Value | Basis |
|---|---:|---|
| LOC_prod | 18,622 (17,292 Rust + 1,330 Python lab) | STUDY §3.2 |
| K1 direct deps | Rust 14 (incl. calamus + tessera as git deps), Python core 0 | STUDY §3.5 |
| K1 locked packages | 240 (Cargo.lock) + 91 (uv.lock) = **331** | STUDY §3.5 |
| K2 bespoke machinery | **≥1,713 LOC = ≥9.9%** of production Rust (graph.rs 935, readdress.rs 379, outside.rs 399) | STUDY §6.2 |
| K3 state surface | 10 artifacts / **7 formats** per soul | STUDY §3.4 |
| K4 languages | 2 (Rust, Python) | STUDY §3.2 |
| **Complexity C** | **967** (= 186 + 331 + 350 + 100) | STUDY §6.2 |
| Cache-ability | 93 (state 100, work 86) | STUDY §6.2 |

### 2.2 What was invisible in v1: calamus and tessera themselves

Direct census of the clones at `v2/calamus` (HEAD `3d94fd0`, ravel's
Cargo.lock pin) and `v2/tessera` (HEAD `1142eb6`, ravel's Cargo.toml
rev) — V2-REPORT §1.1 for the pin verification.

| Crate | Source LOC (`src/`, incl. inline tests) | Tests / examples LOC | Runtime direct deps | Own Cargo.lock |
|---|---:|---:|---|---:|
| calamus 0.1.2 | **1,781** (bind 589, rewrite 450, names 310, ontology 228, walk 151, lib 53) | 337 / 25 | 3 — oxrdf, spargebra, oxttl (optional; ravel enables it via the `turtle` feature) | 71 entries, 70 unique names |
| tessera 0.1.1 | **1,260** (verify 713, keyring 286, access 200, lib 61) | 749 / 0 | 4 — base64, ed25519-dalek, serde, serde_json | 42 entries, 41 unique names |
| **Combined** | **3,041** | 1,086 / 25 | — | 113 entries (see below — **not additive**) |

**Their share of ravel's Cargo.lock — read carefully, because this is
where double-counting would sneak in:**

- calamus and tessera are already **2 of the 14 direct dependencies**
  in ravel's Cargo.toml (STUDY §3.5), and ravel's unified Cargo.lock
  (240 packages) already resolves their transitive closures. The
  full-stack locked total therefore **stays 331**; adding the crates'
  own standalone locks (71 + 42) would count the same packages a
  second time, and calamus's standalone lock additionally resolves
  its *dev*-dependency (oxigraph) tree, which ravel supplies itself.
- The certain share is **2 of 240 Cargo.lock package entries
  (0.83%)**. The exact *unique-to-them* transitive share inside the
  unified lock is **unmeasured**: the local study tree carries no
  copy of ravel's Cargo.lock to walk, and the instrument's rule is to
  say so rather than estimate. What can be said from the manifests:
  calamus's three runtime deps sit on packages ravel already carries
  directly (spargebra) or through Oxigraph (oxrdf, oxttl), so its
  unique contribution is close to itself alone; tessera's crypto
  closure (the ed25519-dalek family, ≈30 names in its standalone
  lock) is the genuinely additive-looking part, overlapping ravel's
  tree mainly at serde/serde_json/sha2/libc.
- **The delta v1 could not see is therefore LOC and machinery, not
  packages:** +3,041 LOC of first-party stack code (+16.3% over the
  v1-visible LOC), of which calamus's 310-line names.rs is not called
  by ravel at all (V2-REPORT §5.4: `calamus::names` is never invoked;
  ravel-called calamus ≈ 1,471 lines + the shared lib.rs).

**K2, updated.** STUDY's named ≥1,713 LOC was ravel-repo only.
calamus is, by its own crate description, store-query preparation —
the same K2 category the study counted outside.rs (399) under.
Crate-inclusive, named bespoke machinery rises to **≥3,494 LOC**;
ravel-called only (excluding names.rs), **≥3,184 LOC**. Against the
full Rust-only denominator (17,292 + 3,041 = 20,333), that is a
bespoke share of **17.2%** (15.7% called-only), against the 9.9%
v1 could see. tessera is not counted in K2: it is the gate, a
feature, not store compensation.

### 2.3 Full-stack total, Variant A

| Component | v1-visible | Full stack | Delta |
|---|---:|---:|---:|
| LOC_prod | 18,622 | **21,663** | +3,041 |
| Locked packages | 331 | **331** | 0 (already unified) |
| Distinct formats | 7 | **7** | 0 |
| Languages | 2 | **2** | 0 |
| **Complexity C** | **967** | **997** (exact sum 997.63) | **+30 points (+3.1%)** |

The finding in the delta: making the private repos public barely
moves A's *score* (+3.1%), because the instrument prices dependencies
by package count and the packages were always in the lock. What it
moves is the *owned-code* picture: 3,041 lines of stack-written
machinery — including a second, larger query-preparation engine
beside the one the study counted — were riding inside a number that
presented as a dependency count.

### 2.4 Operational moving parts, Variant A

- **Processes:** 1 persistent process (`raveld`, one per machine, the
  only writer) + transient `ravel` CLI invocations (which start the
  daemon if absent). Oxigraph/RocksDB is **in-process** — no store
  server, no per-query subprocess. (STUDY §1, §2.2, §3.4.)
- **Hosted surfaces:** 3 — the daemon HTTP API (14 routes + the W3C
  SPARQL route), the CLI command set, the Rust library seam.
  (STUDY §2.)
- **First-party components to version together:** 3 Rust crates
  (ravel, calamus, tessera) + the Python lab in ravel's repo.
- **State surface:** 10 persisted artifacts in 7 formats per soul,
  plus machine-level config + the self-capping daemon log.
  (STUDY §3.4.)

---

## 3. Variant B — complexity, recomputed for the full stack

### 3.1 The v1 port-system score (V1-REPORT, "Complexity and
cache-ability, recomputed with the study's instrument")

| Component | Value | Basis |
|---|---:|---|
| LOC_prod | 4,720 (Python backend 1,661 + evaluator Rust 3,059) | V1-REPORT complexity table |
| Locked packages | **0** (Python stdlib only; the evaluator adds no crates) | V1-REPORT |
| Distinct formats | 5 (phext, JSON, YAML, TTL, N-Quads wire) | V1-REPORT |
| Languages | 2 (Python, Rust) | V1-REPORT |
| **Complexity C** | **397** (= 47 + 0 + 250 + 100) | V1-REPORT |
| Backend alone | **216** (LOC 16 + packages 0 + formats 4×50 + languages 0) | V1-REPORT |
| Cache-ability | 100 at backend scope (state 100, work 100); an end-to-end rescore including unchanged above-seam layers keeps work at 86 → headline 93, same as A | V1-REPORT |

The evaluator's gross size, for reference: `src/sparql.rs` is 3,500
lines — 2,582 core, 373 regex engine, ~300 fringe, 545 tests — plus
the 104-line `phext_sparql.rs` binary (V1-REPORT, "Budget verdict").
The score's 3,059 is the production share (3,500 − 545 tests + 104
binary), on the study's LOC_prod basis.

The backend's 1,661 against its 1,200 budget, per component
(V1-REPORT, "Budget verdict"): store seam ~555 (budget 350 — the
projection layer had no line in the study's table), routes 241
(350), fixed queries ~70 (150), memory views ~368 (200 — verify() is
a full memory_relive port), glue ~430 (150 — the seam crosses a
process boundary: SQ REST client 131, NT term parser/serializer
~100, quad generation + evaluator client ~140, ontology preparation
~60).

### 3.2 What the v1 score treated as free: SQ + libphext

V1's "0 locked packages" is true of the code the port *authored* and
false of the system the port *runs on*. The honest full-stack count
(DEP-AUDIT.md §2, §4 — SQ v0.6.3 as published, Linux host tree):

- **SQ: 6 direct dependencies** — libphext, serde, serde_json,
  shared_memory, raw_sync, percent-encoding — **30 unique package
  names** in the host tree (33 name+version pairs). The weight is
  old, not new: the shared_memory + raw_sync IPC pair accounts for
  ~17 of the 32 dependency packages, both nix majors, and the entire
  rand stack (which exists so shared_memory can mint random segment
  names; SQ never calls rand). No direct dep has entered SQ's
  manifest since v0.5.5, and the follow-on minimization pass found
  zero removable (DEP-AUDIT.md §7 + addendum).
- **libphext 0.6.6** (the "0.6.4" floor floats under cargo install —
  DEP-AUDIT.md §1): 11 call-site refs in SQ, and the leanest dep in
  the tree — exactly one transitive package (xxhash-rust). It is
  already inside the 30 above.
- **State surface:** one phext per soul is the only persisted
  artifact the port adds (ingest receipts live inside it as derived
  scrolls; the generated N-Quads are a transient wire form, written
  to a temp file per query and discarded — backend/PHEXT.md,
  "Classification"; V2-REPORT §7.2 for the regeneration mechanism).
  The score's 5 formats stand as V1 counted them; only one of the
  five is a per-soul persisted format.

**Machinery relocation — the honest caveat to B's score.** B did not
make SPARQL machinery vanish; it *moved* it. The backend carries
~430 LOC of glue whose whole purpose is feeding an evaluator across
a process boundary (V1 budget table), and the evaluator itself is
3,059 production lines of query machinery now housed in
phext-lattice, where the instrument counts it as the port's LOC but
where it is also shared substrate with other consumers
(SPARQL-SUBSET.md §5). Symmetrically, A's score never counts
Oxigraph's evaluator source at all — it enters as packages. The
score therefore measures **owned complexity + dependency weight**,
not total executed code, and B's win on it is a win about who
maintains the machinery and how much supply chain rides along, not
a claim that less machinery exists in the world.

### 3.3 Full-stack total, Variant B

| Component | v1 port-system | Full stack | Delta |
|---|---:|---:|---:|
| LOC_prod | 4,720 | **4,720** (as-built basis; 4,866 on current post-v2 files) | 0 |
| Locked/unique packages | 0 | **30** (SQ host tree, incl. libphext) | +30 |
| Distinct formats | 5 | **5** | 0 |
| Languages | 2 | **2** | 0 |
| **Complexity C** | **397** | **427** (exact sum 427.2; 428 on current files) | **+30 points (+7.6%)** |

*Sensitivity:* on a strict lock-entry basis (SQ's shipped Cargo.lock
= 55 entries, all targets + dev closure — DEP-AUDIT.md §2), B scores
**452** and the A/B cost ratio moves from 2.34× to 2.21×. No verdict
in §4 changes under either basis.

**Headline pair for everything below: C_A = 997, C_B = 427.
B carries 42.8% of A's complexity score; A costs 2.335× B.**

### 3.4 Operational moving parts, Variant B

- **Processes:** 2 persistent — the Python server (`server.py`) and
  one SQ host process per data directory (`PhextStore` launches
  `sq host`; backend/phextio.py) — **plus one transient
  `phext-sparql` subprocess per query** (backend/ravel_backend.py
  `run_query`: full quad set written to a temp file, evaluator
  spawned, 30 s timeout). The native CLI remains as a transient
  consumer above the seam.
- **Hosted surfaces:** 4 internal — the port's HTTP route surface
  (the same 14 routes), the SQ REST host, the evaluator's line
  protocol, and the phext files themselves (addressed via SQ's toc).
  (backend/PHEXT.md, "What lives where".)
- **First-party components to version together:** 4 — the Python
  backend, phext-lattice (evaluator), SQ, libphext — across 4 repos,
  with a measured version-skew hazard already on record (V1-REPORT,
  "Honest gaps": the backend was written against SQ v0.6.0 semantics
  while ranch nodes carried v0.5.6).
- **State surface:** 1 persisted artifact per soul (the phext) +
  a global `souls.json` registry + in-memory caches (evaluator
  inputs, stats). (backend/PHEXT.md, "Running it".)

The split in one line: **B has the smaller code-and-package ledger
and the larger process topology; A has the larger ledger and the
smaller topology** (one daemon, everything else in-process).

---

## 4. Return per axis, complexity-normalized

### 4.0 The normalizer, defined once and used consistently

Raw performance ratios ignore that the variants cost different
amounts of complexity. Where an axis is a clean ratio, this section
also gives a **complexity-density index**:

- if B is *r*× better raw, B's density advantage = *r* × (C_A/C_B)
  = *r* × 2.335 — B delivers *r*× the return at 1/2.335 the cost;
- if A is *r*× better raw, A's density advantage = *r* × (C_B/C_A)
  = *r* / 2.335.

The index is a comparison device, not a physical unit; where an axis
is not a ratio (correctness, representability-as-kind, missing
features), the verdict is plain language and no index is fabricated.

### 4.1 Storage bytes — **B, overwhelmingly, at both scales**

| Scale | A (native) | B (phext) | Raw | Density |
|---|---:|---:|---:|---:|
| Toy (V2 §2) | 227,797–511,155 B apparent store (≈99% RocksDB fixed overhead at 190 quads) | **2,137 B** | B 107–239× | B 250–558× |
| Real corpus (V2 §7.1) | Σ ≈183 MB apparent stores (4.7–43× per item) | Σ ≈15.6 MB soul phexts (0.98–1.11× source) | B ≈11.7× | B ≈27× |

Plus the modelled estate from SCALE.md §2–§3: the same corpus in A's
full native shape (mirrors + projection + manifests) estimates to
≈313 MB of text form in 420 artifacts — against one phext file per
record-set. Storage is the axis where B's complexity discount and
raw win point the same way.

### 4.2 Query latency — **A, overwhelmingly, at both scales**

| Axis | Evidence (V2 §2, §7.2) | Raw | Density |
|---|---|---:|---:|
| Toy point lookup | A 0.0000793 s vs B 0.0054 s | A ~68× | A ~29× |
| Toy traversal 1-hop | A 0.0000268 s vs B 0.0056 s | A ~209× | A ~90× |
| Toy traversal 3-hop | A 0.0000280 s vs B 0.0110 s | A ~393× | A ~168× |
| Scale warm point | A ~20 µs vs B 0.055–0.22 s | A ~3,000–11,000× | A ~1,285–4,712× |
| Scale cold (first query after a write) | B: 0.7 s @ 240 quads → 23–26 s @ ~3.4k → 52–57 s @ ~7k → **196.3 s @ 15,044 quads**; A has no equivalent regeneration state (indexed store) | not a like-for-like ratio | A, categorically |

The mechanisms are architectural, not tuning: A evaluates in-process
over a warm indexed store; B spawns a subprocess and serializes the
full quad set across the boundary per query, and regenerates that
set from SQ scroll walks after any write (V2 §2 notes, §7.2). These
are now defects #102 (warm floor) and #100 (cold cliff) against
phext-lattice (DEFECTS-FILED.md).

### 4.3 Ingest — **split by scale: B at toy, A at working scale**

- Toy (V2 §2): B wins both sessions — sess-A ~1.6–1.8×, sess-B
  ~4.3–4.8× (density 3.7–11.2× for B). A's cost is dominated by the
  per-ingest RocksDB `flush()`; at 10² turns the flush *is* the cost.
- Scale (V2 §7.1–7.2): A wins **every item**, 2× (artifacts) to ~44×
  first-ingest and ~190× second-ingest (CYOA). Density: 0.86× on the
  smallest item — i.e. a per-complexity tie there — rising to ~81×
  for A. B's second ingest is consistently 2–4× its first (clear +
  rewrite of a now-full multi-MB hosted file) — defect #103,
  resolving feature: append-only ingest via true suffix append.

### 4.4 Mutation — **split by scale, and the scale side is A's rout**

- Toy (V2 §2): B wins the like-for-like pair ~1.5× (density ~3.5×).
- Scale (V2 §7.1–7.2): A wins every item, **7–180×** (density
  ~3–77×). One character costs B a full transcript re-ingest through
  per-scroll REST writes: **19–65 s** on the top items,
  indistinguishable from its no-change re-ingest, against A's
  0.19–1.91 s. V1's "exactly one scroll changes" remains true of the
  *bytes*, not of the *cost*. Defect #101; resolving feature:
  per-scroll quad deltas on write (see §5.1 for what that does and
  does not fix).

### 4.5 Representability — **B, categorically**

Of SCALE.md's 42-phext inventory, exactly **1 is transcript-shaped**
(life.phext), **2 have near-native homes** in A's model
(premises → the claim layer; emise → optmem, minus its curation
layer), and **39 are not representable without kind-loss** in the
turn/note/claim vocabulary: coordinate-keyed registers, book
geometry, snapshot identity, ledger arithmetic, addressed-but-
unlisted structure (V2 §7.3, SCALE.md §3). B's substrate holds all
42 as addressed records of their own kind — that is what a phext
*is*. Raw coverage 42/3 = 14×; density ≈33× for B. The caveat, for
honesty: this axis prices the *substrates* (phext vs ravel's record
model), not the port's projection, which is itself turn-shaped by
design — the port's win here is inherited from phext, and it is
real, because the port is the variant that runs on phext.

### 4.6 Dependency / supply-chain surface — **B**

- A: **331 locked packages** (240 Rust incl. two git deps that were
  unfetchable fleet-wide for the whole v1 pass — STUDY §3.3 — plus
  91 Python), 14 Rust direct deps.
- B: **30 unique packages** on the host tree (55 lock entries),
  6 direct, all in SQ; the authored code adds **zero** packages
  (Python stdlib; the evaluator is std-only Rust inside an existing
  crate — V1-REPORT).
- Raw: 11.0× fewer unique packages (6.0× on lock entries); density
  ≈26× (≈14× on lock entries) for B.
- The honest shadow: B's 30 are not weightless — ~17 of SQ's 32
  dependency packages hang off the 2021-era IPC pair, including a
  full rand stack SQ never calls (DEP-AUDIT.md §4). B's supply chain
  is small and *audited*; A's is large and, at the git-dep edge, was
  for a time *unbuildable* on this fleet. Both facts are ROI.

### 4.7 Correctness surface — **A, plainly; no index**

- **B's v2 record:** two confirmed errata, both verification escapes
  where the check shared the assumption — **C-1**: the ontology
  preparation derived empty name maps, so abstract-name rewriting
  (the entire outside-query surface) never fired in the shipped
  backend; **P-1**: recent listed timestamp-less turns (native 7,
  backend 8). Both fixed and bite-tested (harness 72/72; the fixed
  harness fails on exactly the 6 checks covering the two errata
  against the v1 backend — V2 §8). Plus: the **DISTINCT fan-out
  latent divergence** (crafted probe: native 1 row, evaluator 2;
  unreachable on ravel-projected data, open as defect #104), the
  **empty-string triple loss** (substrate limit, v1-disclosed,
  native-confirmed — V2 §3), and a **gate that does not exist** on
  B's side (127.0.0.1 bind, no token validation — V2 §9 item 3).
- **A's v2 record:** tessera **verified clean** — every contract
  point v1 asserted is implemented as asserted (V2 §4); calamus,
  after B's C-1 fix, mechanically and behaviourally identical to
  the port (V2 §5); the native projection *is* the third leg's
  reference, and ann_hash matched on all three sides (V2 §3).
- A's higher complexity bought, among other things, the surface
  that caught B's errors. B's lower complexity shipped with a
  verification hole exactly where its machinery was newest. That
  asymmetry is a real return on A's complexity and belongs in the
  ledger.

### 4.8 Build / cold start — **incomplete evidence; no winner claimed**

- A, measured (V2 §1.2): cold debug **229.37 s**, cold release
  **147.92 s** (369 crates compiled), warm no-op 0.44/0.15 s,
  incremental 2.10 s. Binaries: raveld 26.8 MB, ravel 13.5 MB.
- B, measured components only: SQ aligned-source build **12.68 s**
  (V2 §1.2); the backend is stdlib Python — no install step (v1's
  venv 1.72 s + `pip install -e .` 6.24 s were for ravel's own
  Python lab, not the backend — STUDY §3.3). The evaluator's
  isolated cold build time is **not in any report** — unmeasured —
  so a full-stack B cold-build total cannot be stated honestly.
- Operationally, B's cold *start* is heavier in topology (server →
  SQ host → per-query evaluator) even though its build inputs are
  lighter. Verdict: B almost certainly cheaper to build from cold,
  A cheaper to start and reason about; the axis is recorded as
  **unmeasured for B** rather than scored.

### 4.9 Cache-ability — **a wash the score overstates for B**

A: 93 (state 100, work 86 — the one non-deterministic stage is the
live-model relive writer, which both variants share above the seam).
B: 100 at backend scope, 93 end-to-end on the same basis (V1-REPORT).
The instrument's fractions do not capture *cache granularity*: B's
evaluator-input cache is whole-soul and single-flag invalidated
(§5.1), which is precisely where its measured cliff lives. No ROI
difference is claimed on this axis.

### 4.10 Axis summary

| Axis | Winner | Where |
|---|---|---|
| Storage bytes | **B** (density 27–558×) | both scales |
| Query latency | **A** (density 29×–4,712×; cold: categorical) | both scales |
| Ingest | **B** at toy (≤4.8× raw), **A** at scale (2–190× raw) | split |
| Mutation | **B** at toy (1.5× raw), **A** at scale (7–180× raw) | split |
| Representability | **B** (39/42 kinds A cannot hold without loss) | categorical |
| Supply chain | **B** (30 vs 331 packages) | — |
| Correctness / gate | **A** (B: 2 errata + latent divergence + no gate) | — |
| Build / cold start | unmeasured for B; A heavier to build, lighter to start | — |
| Owned-code complexity | **B** (427 vs 997) | — |
| Operational topology | **A** (1 persistent process, in-process store vs 2 + per-query subprocess) | — |

---

## 5. The two forward judgements

### 5.1 Incremental quad generation for B — the best complexity
purchase available in either variant

**The mechanism as it stands** (backend/ravel_backend.py): `Soul`
carries a single `_dirty` bool and a cached `_inputs` pair. Every
write path (ingest_transcript, append_note, upsert_summary) sets
`_dirty = True`; the next `_evaluator_inputs()` call regenerates
**everything** — `all_quads()` walks every transcript's scrolls
through SQ, reprojects spine + annotations, rebuilds the memory
graph and the recent window — serializes the whole set to N-Quads,
and `run_query` writes it to a temp file and spawns the evaluator.
That is the whole of defect #100's cliff (196.3 s at 15,044 quads,
≈13 ms/quad and superlinear — V2 §7.2) and half of defect #101.

**The fix, shaped by the code and by defects #100/#101:**

1. Partition the cached quad set at **per-event granularity** (not
   per-transcript: SCALE's largest items are effectively one
   transcript, so a transcript-level cache can retain the whole
   cliff). Spine + annotation quads for one event are already
   produced by pure functions (`spine_quads`, `annotation_quads`);
   partition keys are (graph, event_id), with separate partitions
   for the memory graph (per note/block) and the graph descriptions.
2. Cache the **timestamp index** (event_id → instant, graph) beside
   the partitions, so the recent window — the one genuinely global
   derivation — updates from metadata instead of a reprojection.
3. Writes update only the partitions they touch: ingest/append of
   one transcript replaces that transcript's event partitions
   (the events are already in memory at ingest — populating the
   cache costs no extra SQ reads); a note/summary write touches
   only the memory partition. `_evaluator_inputs()` then *joins*
   cached partitions instead of regenerating them.
4. Differential acceptance, in the shape the defects already carry:
   incremental output must equal full regeneration quad-for-quad on
   the SCALE items, and the bite test is a one-character mutation
   followed by a first query (defects #100/#101 acceptance shapes).

**Price (estimated from the code as it stands, labelled estimate):**
+300–600 production LOC in the backend — partition bookkeeping,
revision keys, the recent-index, cache-population at ingest — plus
~150–300 lines of tests (not in LOC_prod). **C impact: +3–6 points
(≈1% of B's 427).** No new packages, formats, languages, or
processes. A *persistent* (restart-surviving) scroll-native
QuadSource — defect #100's full feature shape — is a second,
separate purchase: roughly +200–400 LOC in lattice/SQ integration
and a new derived-state surface to keep honest; it should be bought
only if process-restart cold queries are a real workload, not as
part of this fix.

**Return:**

- **Cold-query axis: the cliff goes.** A first query after a write
  to one part of a soul stops paying for every other part. What
  remains is the join + temp-file serialization of the full set
  (O(quads) but at string-join speed, not SQ-walk speed) plus the
  evaluator's own scan — landing, by the shape of the warm numbers,
  in the warm-path order of magnitude (0.055–0.22 s at these sizes)
  rather than at 196 s. Labelled estimate from V2's warm/cold
  decomposition, not a measurement.
- **Mutation axis: only half goes — say it plainly.** Incremental
  generation removes the *reprojection* cost of a mutation and the
  post-mutation cold query. It does **not** reduce the measured
  19–65 s mutation itself, whose cost is the *write path*:
  `ingest_transcript` clears the region and rewrites every scroll
  through REST (V2 §7.2). Cutting that needs the companion change —
  coordinate-level write diffing (read the existing transcript,
  write only changed scrolls, clear only removed books), estimated
  +150–300 LOC — which is defect #101's full shape. Buying §5.1
  alone and claiming the mutation axis would be a false claim.
- **What it does not buy:** the warm floor (0.055–0.22 s vs ~20 µs —
  that is the subprocess + full-set serialization boundary, defect
  #102, a different purchase: a persistent evaluator session) and
  scale ingest (defect #103).

**Judgement:** at +3–6 points against a measured 196.3 s cliff and
the port's only categorical performance failure, incremental quad
generation has the highest return-per-complexity of any item on
either variant's table. It converts B from "unusable above ~10³
quads for read-after-write" to "warm-class reads at 10⁴ quads",
and it is the precondition for B competing in the mixed workload
at all (§6).

### 5.2 Variant A's irreducible floor — the verdict does not change

Grant A the most generous possible accidental-complexity removal:
the study's own estimate of substrate compensation in ravel's Rust
is ≈2,100–2,400 LOC (STUDY §1: store management and locking
workarounds, address migration, hand-rolled RDF emission, manifest
bookkeeping, the hand-maintained recent index, the projection
stamp). Delete *all* of it — more than a native-only refactor could
actually recover while keeping RocksDB — and:

- LOC floor: 18,622 − 2,100…2,400 = 16,222–16,522 for ravel, +
  3,041 of calamus/tessera (query preparation and the gate are
  *features*, not accidents; they survive any honest floor) =
  **19,263–19,563**.
- Packages floor: **331 stands.** Oxigraph/RocksDB, SPARQL, HTTP/TLS,
  and the crypto gate are the requirements themselves; no
  accidental-removal argument sheds their closures. (Excluding the
  Python lab would drop 91 packages, 1,330 LOC, and one language —
  C ≈ 769–772 — but that is a *scope* change, a runtime-only
  recount of a project whose census has always included the lab,
  not a floor of the same system.)
- **C floor ≈ 973–976** (LOC component 192–195 + 331 + 350 + 100):
  a ≈2% reduction from 997. The complexity ratio against B moves
  from 2.335× to ≈2.28×.

**Judgement:** A's ROI problem was never the 1,713 LOC of visible
substrate compensation — it is that its *requirements* (a mutable
indexed RDF store, full SPARQL, a crypto gate) are intrinsically
package- and machinery-heavy, while its *returns* (storage size,
record-kind fidelity) are capped by its data model. Deleting every
accidental line leaves every axis verdict in §4 exactly where it
stands: the normalized query/mutation advantages shrink by ~2%,
the storage and representability deficits are untouched because
they are model outcomes, not code outcomes. **The floor does not
rescue A's ROI; it confirms that A's complexity is mostly
essential to what A is.**

---

## 6. Synthesis — which variant wins under which workload

**Write-heavy conversation** (a live soul: frequent appends,
corrections, immediate reads): **A, decisively, today.** Its append
path, per-record mutation (0.19–1.91 s at scale), and indexed reads
are built for exactly this; B's write path (full re-ingest,
19–65 s) and post-write cold queries (up to 196.3 s) are
disqualifying at working scale. B's toy-scale ingest/mutation wins
are real but are fixed-overhead artefacts (RocksDB flush vs cheap
SQ writes at 10² turns) and invert by 10³–10⁴ quads. The one
exception: genuinely small, short-lived souls, where B's wins hold
and its files are simpler to keep.

**Read-heavy archive** (write once or rarely; preserve; read
occasionally): **B, decisively.** Storage density 27× per unit
complexity at corpus scale, one addressed text file per record-set
instead of a 10-artifact estate, every record kind preserved, zero
authored supply chain — and the read pattern (infrequent, batch,
tolerant of a 0.055–0.22 s warm lookup) never meets the cliff,
because the cliff is priced per *write*. The split inside
"read-heavy" matters: if the reads are high-rate interactive
SPARQL serving, that is not an archive workload, and A wins it on
§4.2 regardless of storage.

**Mixed soul lifetime** (the actual ravel workload: a soul that
grows for years, writes and reads interleaved, wakes from its own
history): **A today; B after §5.1, conditionally.** Today A's
operational profile is the only one that survives the mixture at
scale, and it also carries the implemented gate B lacks. After
incremental quad generation (+3–6 points) and the write-diff
companion (+150–300 LOC), B's remaining deficits are the warm
latency floor (~10³–10⁴× native, but 55–220 ms in absolute terms —
inside many interactive budgets) and the missing gate, against
wins in storage (11.7× raw at corpus scale), representability
(39 record kinds), and a 2.3× complexity discount. For a soul whose
value is *being kept* — attributable, inspectable, rebuildable —
that trade favours B. For a soul whose value is *being answered
quickly, by many readers, under a gate* — it favours A, and no
priced fix in this analysis changes that.

**The one-line ROI:** B is the better *store* per unit of
complexity by a wide and measured margin; A is the better *engine*
per unit of complexity by a wider one; and the distance between
the two statements is almost exactly one incremental-quad-cache
(+3–6 points) plus one write-diff (+150–300 LOC) wide.

---

## Sources

- STUDY.md — §1 (substrate-compensation estimate), §2 (surfaces),
  §3.2 (LOC census), §3.4 (persistence inventory), §3.5 (dependency
  chain), §6.1 (instrument), §6.2 (A's v1-visible score).
- V1-REPORT.md — complexity/cache-ability table (397 / backend-alone
  216), budget verdict (backend 1,661 per component; evaluator
  3,500 = 2,582 + 373 + ~300 + 545 tests, + 104 binary), honest
  gaps (SQ version skew, empty-string loss); errata notes C-1 / P-1
  (2026-11-24, v2 pass).
- V2-REPORT.md — §1.1 (pins), §1.2 (build times), §2 (toy
  side-by-side), §3 (third leg), §4 (tessera clean), §5 (calamus;
  §5.3 DISTINCT divergence; §5.4 names uncalled), §7 (SCALE gauge:
  §7.1 table, §7.2 readings, §7.3 representability), §8 (errata),
  §9 (honest gaps).
- SCALE.md — §1 (inventory: 42 phexts, 16,628,698 B, 9,513
  scrolls), §2 (18.79× measured factor; 10 artifacts / 7 formats),
  §3 (per-phext forcing + totals ≈313 MB / 420 artifacts).
- DEP-AUDIT.md — §1 (libphext 0.6.6 float), §2 (SQ counts: 6 direct,
  30 unique names, 33 name+version, 55 lock entries), §4 (IPC pair
  ≈17 of 32), §7 + addendum (zero removable).
- Direct census, this pass — `v2/calamus` and `v2/tessera` clones:
  src/ LOC by file, tests/examples LOC, Cargo.toml direct deps,
  Cargo.lock entry/unique-name counts.
- backend/PHEXT.md (moving parts, classification, running shape);
  backend/ravel_backend.py (`Soul._dirty` / `_evaluator_inputs` /
  `all_quads` / `run_query`, `ingest_transcript` write path);
  backend/phextio.py (`PhextStore` SQ host lifecycle).
- DEFECTS-FILED.md — phext-lattice issues #100 (cold-query cliff),
  #101 (mutation = full re-ingest), #102 (warm floor), #103
  (re-ingest asymmetry), #104 (DISTINCT fan-out).
