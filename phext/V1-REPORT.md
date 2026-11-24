# Ravel phext backend — V1 comparison report

Task 34.6.8/7.7.11/9.5.8 · Phase 2 · 2026-11-23 · EE 404

## The plain summary

Ravel now has a working phext backend. The SPARQL subset evaluator
lives in phext-lattice as its own module behind a QuadSource seam —
**PR https://github.com/wbic16/phext-lattice/pull/99**, open for
Will's review, not merged. The Python backend in
`~/workspace/ravel-upgrade/backend/` implements ravel's store seam and
all 14 daemon routes over one phext per soul, and it roundtrips
losslessly on the verification corpus: every quad the native
projection would emit, the backend's glue emits — checked quad for
quad against an independently transcribed reference projector — and
the memory logs export byte-identical. One measured exception, named
below: an empty-string field cannot be told apart from an absent one
in a phext.

The complexity score, recomputed with the study's identical
instrument, falls from **967 to 397**; cache-ability rises from 93 to
100 at backend scope. The Python backend is **1,661 lines against the
1,200 budget** — over, with the reasons named per component below,
not absorbed silently.

**Native-backend timings remain unmeasurable.** Ravel's Rust build is
blocked fleet-wide: its git dependencies (repolex-ai/calamus,
repolex-ai/tessera) return 401 / "repository not found" from every
probed node. Every performance number in this report is therefore a
backend-self measurement or a structural comparison. No head-to-head
number was run, and none is claimed.

## What was built

1. **The subset evaluator** (`src/sparql.rs` in phext-lattice, PR
   #99): subset-native parser (a construct outside the spec fails to
   parse as a named refusal — structural, not a runtime check), the
   QuadSource trait with a Vec/N-Quads source, a small set-based
   regex engine for REGEX (std-only, no new dependencies), and the
   `phext-sparql` binary speaking a line protocol so non-Rust callers
   can drive it. 22 in-module tests; full lattice suite **224 passed,
   0 failed** on rock-mountain.
2. **The Python backend** (`backend/`: phextio.py, ravel_backend.py,
   optmem.py, server.py, ontology/, PHEXT.md): the seam (open /
   ingest_transcript / append_transcript / keep_recent / query_rows),
   the projection (spine, annotations, claims, authorship, herdr
   claims, the ann_hash), the ontology preparation derived from
   ravel's two shipped TTLs, the fixed internal queries run as
   specced through the evaluator, the memory views and the
   memory_relive port, and the 14 routes.

   > **Erratum (2026-11-24, v2 pass — V2-REPORT erratum C-1):** the
   > ontology preparation named above derived **empty** maps as
   > shipped: its line regex matched zero edges in the shipped
   > Turtle (the ontologies use predicate lists, so a subject never
   > stands next to its predicate), and the backend's abstract-name
   > rewriting therefore never fired. No v1 check caught it — the
   > harness issued no abstract-name query (see the verification
   > erratum below). The derivation has been rewritten as a faithful
   > port of calamus's `Index` and now matches the real calamus
   > mechanically (5 property + 5 class entries, diffed against the
   > crate itself) and behaviourally (13/13 outside SELECT/ASK shapes
   > row-equal with native calamus + Oxigraph).
3. **The verification harness** (`verify/harness.py`, outside the
   budget — it is the instrument): synthetic souls in ravel's own
   test style (the graph.rs transcript shapes, reader.rs's canonical
   emojikey texts verbatim, annotate.rs's typed signal legs), a
   reference projector transcribed separately from the same sources,
   and 65 checks. **All 65 pass**, twice, with an identical phext
   SHA-256 across runs (deterministic ingest).

## Verification — the gate

- **Quads, per graph, set equality** against the reference projector:
  sess-A (spine + 3 annotations incl. 3 reified claims), sess-B
  (spine + 1 tool-result annotation), the optmem graph (4 notes +
  3 blocks), the default graph (4 graph descriptions), the recent
  graph (8 memberships), and a bare-root soul's transcript — all
  exact, no missing, no extra.

  > **Errata (2026-11-24, v2 pass — V2-REPORT errata P-1 and C-1):**
  > (P-1) "the recent graph (8 memberships)" was exact only against
  > the harness's own reference: native `keep_recent` never lists a
  > turn that carries no timestamp, and the corpus's a1001 has none —
  > the native count is **7**. Backend and reference shared the wrong
  > assumption, so the check passed. Both are corrected; the
  > corrected check fails against the v1 backend and passes against
  > the fixed one. (C-1) The 65 checks never exercised the outside
  > (abstract-name) surface at all; a dedicated outside phase (7
  > checks) now covers it, and fails against the v1 backend on
  > exactly the C-1 defect. Three-way verification against the real
  > native build (this pass) otherwise confirms every quad count in
  > this section, including the ann_hash values.
- **Read-back:** every event field (role, timestamp, text, parent_id,
  model_setting, derived author) and every annotation field
  (including typed signal legs and byte spans) roundtrips exactly.
- **Memory logs:** memories.jsonl and summaries.jsonl export
  byte-identical to the canonical serialization of the inputs.
- **Mutation control:** changing one character of one turn's text and
  re-ingesting changes **exactly one scroll** — that turn's text
  scroll — in the whole phext.
- **Fixed queries:** the I-2 emojikey query returns all 3 keys with
  origins [authored, authored, tool_result]; the I-4 stats battery
  counts 8 turns / 4 claims; memory verify reports Verified with all
  4 cited turns in store.
- **HTTP surface:** all 14 routes exercised; UNION refused by name
  (400, construct named in the body); CONSTRUCT answered (fringe);
  an append through /sync lands (uncached count 8 → 9; the stats
  answer correctly stands from cache, as native's does).
- **Sizes:** soul A's phext is **2,001 B**; the native-format
  equivalent of the same soul (N-Triples projection + JSONL logs) is
  **37,607 B** — the phext stores each field once at its coordinate,
  where N-Triples repeats full IRIs on every triple. SHA-256 of the
  phext: `e183f352e2cfd2ce3016fe6b87c562d4dd7b7ef1e408d1a17412e4834ef0b14a`.

## The comparison table, as measured

Backend-self measurements on this VM (medians; corpus: 2 souls,
10 turns, 4 annotations, 4 notes, 3 summaries, 233 quads in soul A).
Native column: unmeasurable (see the summary) — structural notes
instead of invented numbers.

| Axis | Phext backend (measured) | Native backend |
|---|---|---|
| Ingest (2 transcripts, 8 turns, 4 annotations) | 0.06–0.15 s | unmeasurable; structurally: one RocksDB batch load per transcript |
| Point lookup (one turn by IRI) | 0.011 s | unmeasurable; an indexed store lookup |
| Traversal 1-hop (all parent edges) | 0.009 s | unmeasurable |
| Traversal multi-hop (3-step parent walk) | 0.017 s (3 queries; property paths are refused by design) | unmeasurable; same walk — no path queries are issued natively either |
| Single-record mutation | 0.19 s (whole-transcript re-ingest, both directions of the control) | unmeasurable; structurally cheaper per record (one graph reload) — the phext v1 re-ingests the transcript; the changed *scrolls* are exactly one |
| Store size (soul A) | 2,001 B phext | 37,607 B as NT + JSONL (the projection's text form; the RocksDB store itself is binary and unmeasured) |
| Roundtrip fidelity | **exact**, one named exception (empty-string fields) | — (native is the reference) |

Query latency shape: every query pays one evaluator process spawn
plus a full quad generation on first use after a write (cached
afterwards). The stats battery is 9 evaluator calls at ~0.09–0.11 s
total. This is a v1 on a small corpus; nothing here measures how the
glue's regenerate-on-write behaves at 10⁵ turns — named as the first
scaling question in Next steps.

## Complexity and cache-ability, recomputed with the study's instrument

Instrument (STUDY §6.1, unchanged):
`C = LOC_prod/100 + locked_packages + 50 × distinct_formats + 100 × (languages − 1)`;
cache-ability = mean(state fraction, work fraction).

| Component | Ravel (native) | Phext backend system |
|---|---|---|
| LOC_prod / 100 | 18,622 → 186 | 4,720 (Python 1,661 + evaluator Rust 3,059) → 47 |
| Locked packages | 331 | **0** (Python stdlib only; the evaluator adds no crates) |
| Distinct formats × 50 | 7 → 350 | 5 (phext, JSON, YAML, TTL, N-Quads wire) → 250 |
| Languages − 1 × 100 | 2 → 100 | 2 (Python, Rust) → 100 |
| **Complexity C** | **967** | **397** |
| Cache-ability: state | 100 | 100 (records + deterministic derivations only) |
| Cache-ability: work | 86 | 100 at backend scope (every pipeline stage deterministic; no writer/sync stages in scope) |
| **Cache-ability** | **93** | **100** (backend scope; an end-to-end rescore including the unchanged above-seam layers would keep work at 86, mean 93) |

The backend alone (excluding the shared evaluator) scores 216:
LOC 16 + packages 0 + formats 4 × 50 + languages 0.

## Budget verdict — 1,200 Python LOC

Measured by `wc -l` (the study's census measure); code-only lines in
parentheses. Total **1,661 (1,436)** vs 1,200 — an overrun of 461
(236 code-only), reasons per component:

| Component | Study §7.2 | Built | Verdict |
|---|---|---|---|
| Store seam | 350 | ~555 | **Over.** The study's table has no line for the projection layer (project.rs / annotate.rs / herdr.rs equivalents: spine, annotation + claim quads, ann_hash, authorship) — but the seam's ingest contract is "load full spine + annotations", so it is in the seam's price here. |
| Routes (server.py) | 350 | 241 (213) | Under. |
| Fixed queries | 150 | ~70 | Under (the I-4 battery is nine one-liners over query_rows; the I-6/I-7 shapes ride in verify). |
| Memory views | 200 | ~368 | **Over.** The 200 priced wake/open/progress; verify() is a complete memory_relive port (check / not_wrapped / a_memory / pending / dangling / cited-in-store) at ~120 lines, and optmem's cover/pending math is ~135. |
| Glue | 150 | ~430 | **Over.** The study priced an in-process QuadSource glue. The built seam crosses a process boundary: SQ REST client (131), NT term parser/serializer (~100), quad generation + evaluator client (~140), ontology preparation (~60). |

The evaluator, for its part: 3,500 lines in sparql.rs — 2,582 core,
373 regex engine, ~300 fringe (below), 545 tests — plus the 104-line
binary, against the determination's 1,600–2,350 estimate for the
evaluator alone. The overage is the regex engine, the N-Quads source,
and the wire-protocol binary, none of which the estimate scoped.

## Gap analysis — the dialect beyond the subset

Every construct outside the proven subset, classified. Evidence is
the subset determination's enumeration of issued queries, `ravel
query help`'s documented dialect, the routes' advertised surface,
and the calamus pipeline's behaviour.

**(i) Intentionally excluded — the refusal stands, prediction recorded.**
- **SPARQL Update** — load-bearing: the write path is ingest-shaped
  (transcript bytes canonical, the graph a rebuildable projection). A
  query-shaped write would break the projection invariant. All writes
  in the codebase flow through the seam's ingest/append functions.
- **SERVICE / federation** — load-bearing: souls are sovereign,
  per-soul addressed and gated; no issued query crosses a store.
- **Property paths** — the one recursive structure (the memory tree)
  is walked one level per call by design (zoom); traversals are
  issued as explicit 1-hop joins. Paths would also hide unbounded
  work inside the 30 s query seat.
- **Subqueries, MINUS, HAVING** — no issued instance; the one negation
  need is met by NOT EXISTS (I-6), post-aggregation filtering happens
  in Rust after rows return.
- **SELECT REDUCED** — its semantics are permissive by definition (an
  engine *may* dedupe); a scoped dialect cannot honour it honestly.
  Refused so no consumer builds on a maybe.
- **Blank nodes in patterns** — the projection mints IRIs for
  everything (annotations, claims, selectors); there is nothing for a
  blank-node pattern to match.
- **UNION** — no issued instance; the stack's questions are
  conjunctive over one soul's graphs. Prediction: this is the
  exclusion a future consumer feels first — recorded here so the
  prediction can be graded; the refusal stands until a real query
  needs it.

**(ii) Advertised but unimplemented — fixed in the fringe.**
- **CONSTRUCT / DESCRIBE** — the routes advertise them (answer()
  shapes triple answers for both), no code issues them, and the core
  subset as specced refuses them: the promise did not hold. In this
  build it does (fringe, below).
- **SELECT DISTINCT, SELECT \*** — implied by the W3C results shape
  the routes speak; issued by no internal query. Fringe.

**(iii) Never needed, no signal — fringe where adjacent, refused otherwise.**
- **OFFSET** — no issued instance (the recent window is LIMIT-only
  because keep_recent materializes it). Implemented as fringe anyway:
  paging is the obvious outside-consumer reach, and it is five lines
  next to LIMIT. *Prediction, labelled.*
- **BIND** — no issued instance; the documented dialect teaches
  `(expr AS ?x)` aliases, and BIND is the same need inside a group.
  Fringe. *Prediction, labelled.*
- **!=, <, <=, >=** — only `=` and `>` are issued; the comparison
  family is one operator table in every SPARQL text a consumer copies
  from. Fringe. *Prediction, labelled.*
- **IN, arithmetic expressions, CONCAT / COALESCE / BOUND and other
  functions, GROUP_CONCAT / SAMPLE** — no signal at all; VALUES
  covers the one issued membership need (I-7). Refused by name
  (functions by their name).

## The fringe, as built

In the lattice evaluator (PR #99), marked FRINGE in the code, each
with tests like the core: **CONSTRUCT** (template instantiation over
solutions), **DESCRIBE** (subject closure over the dataset — the
documented choice, since SPARQL leaves DESCRIBE's shape to the
engine), **SELECT \***, **SELECT DISTINCT**, **OFFSET**, **BIND**,
**!= / < / <= / >=**. Cost: ~300 of the evaluator's 2,955 production
lines (10%), inside the lattice — the Python budget is untouched.
The backend consequently answers CONSTRUCT/DESCRIBE on /query and
/sparql, the one place it is wider than the ruled core subset.

## Abstraction plan — what folds into phext-lattice

Per Will's directive (2026-11-23 22:25 CST): every generalizable
change, generalized into substrate form; ravel-specifics stay in the
backend. The evaluator PR is instance #1.

| # | Abstraction | Lattice home | Status |
|---|---|---|---|
| 1 | The subset SPARQL evaluator | `src/sparql.rs` | **In PR #99** |
| 2 | The QuadSource seam (trait + Vec/N-Quads source) | `src/sparql.rs` | **In PR #99.** The scroll-native QuadSource is **staged**: it wants a fixed graph-over-scrolls convention; ravel's coordinate map is one instance, and one instance is not a convention. Trigger: a second graph-over-scrolls consumer (git-lex is the candidate). |
| 3 | Base-IRI resolution + union-dataset semantics | `src/sparql.rs` (parser + DatasetView) | **In PR #99**, Prep-free and general |
| 4 | The refusal-by-name parser pattern (ParseError discipline) | `src/sparql.rs` | **In PR #99** as the working instance. Generalizing it into other lattice parsers (uml6d_diagram's, e.g.) is **staged** as a follow-on candidate: it changes another module's error surface and wants its own PR. |
| 5 | The ontology-preparation shape: name tables as data (Prep) | `src/sparql.rs` | **In PR #99.** The mechanism (equiv/subclass fan-out over caller-supplied tables) is substrate; the tables themselves are ravel's and stay in the backend, derived from its TTLs. |
| 6 | The regex engine | inside `src/sparql.rs` | **In PR #99.** Convergence with the lattice's existing search module is **staged**: two engines is a smell, but one build is not enough evidence to merge them. |
| 7 | The line protocol for non-Rust callers | `src/bin/phext_sparql.rs` | **In PR #99** as one binary's protocol. It graduates to a documented lattice surface when a second non-Rust consumer appears — **staged by that trigger**. |
| 8 | Region/lane enumeration over scrolls | SQ's toc (substrate, existing) | **No new work**: the build proved the toc is the enumeration primitive; the backend uses it for region clears and finding enumeration. A span-index API in libphext is **staged** — no gap was hit. |
| 9 | The Python SQ client (phextio: stem addressing, toc enumeration, clear-by-empty) | no home in the Rust lattice repo | **Staged**, Will's call: candidate home is SQ's own tooling. It encodes measured SQ behaviour any Python phext consumer needs. |
| — | Ontology rewrite content (subtexture↔ravel name tables) | stays in the backend | Ravel-specific by definition |
| — | Result shaping (W3C JSON, notes format) | stays in the backend | Ravel's route contract |
| — | Query seats (4/soul), 30 s limit | stays in the backend | Daemon policy |
| — | The fixed internal query set (I-1…I-8) | stays in the backend | Ravel's questions, run as specced |
| — | ann_hash, projection semantics, optmem math | stays in the backend | Ravel's content |

## Honest gaps

- **Empty-string fields** (measured, above): `text: ""` loses its one
  triple; reads back as absent. Root cause: SQ's toc enumerates
  non-empty scrolls only. Every other field roundtrips exactly.
- **ann_hash / projection byte-parity with the native build is
  unverified** — it cannot be, until repolex-ai git access lands.
  The backend and the harness's reference projector are two
  transcriptions of the same sources and agree; a third, native,
  reading is the missing leg.
- **The tessera gate is absent** (127.0.0.1, no token validation);
  /sync and /import are seam-level (dialect adapters stay above the
  seam, unimplemented); soul /health is store-level, not
  sync::diagnose parity; verify()'s cited-turn check runs against
  ingested ids, not the extract stream. All named in PHEXT.md.
- **SQ version skew:** the backend is written against SQ v0.6.0
  semantics (this VM). Ranch nodes carry v0.5.6, whose `sq share`
  daemonizes differently; align before any ranch deployment.
- **Text chunking deferred**: one scroll per turn text, per the
  study's map as built.
- **Scaling unmeasured**: the glue regenerates a soul's quads after
  any write (cached between writes). Fine at 10² turns; the design
  question at 10⁵ is open.

## Next steps

1. **Will merges PR #99** (his call; the PR is deliberately
   self-contained and removable).
2. **Wavelets 1–3 unit tests** (Will, 2026-11-23 22:56 CST — queued,
   not built in this pass): unit tests as executable documentation —
   Wavelets 1–3 (Nov 1–7, 8–14, 15–21 2026) modelled in ravel's
   vocabulary (souls, turns, transcripts, memory graph, optmem wake),
   the era's documented facts asserted in that model, living with the
   backend's test suite once the backend is in place.
3. **Native cross-check** of ann_hash and the projection when the
   repolex-ai git block clears — the one verification leg still owed.
4. **Scroll-native QuadSource** when a second graph-over-scrolls
   consumer exists (abstraction plan #2).
5. **Glue scaling**: incremental quad generation (per-transcript
   invalidation) before any soul with a large transcript lands.
