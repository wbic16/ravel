# Ravel-phext v2 — the un-parked completion pass

Task 34.6.8/7.7.11/9.5.8 · 2026-11-24 · Will's directives (2026-11-23 23:39 CST):
"the missing repos have been made public, so we can do side by side performance
testing now" + "run that now" + "fix any errata caused by our assumptions";
(2026-11-24 00:03 CST): "use our dataset to gauge the phext port" (the SCALE leg, §7).

Everything below was measured in this pass on **rock-mountain** (fleet node;
thermal record in §1.3). Nothing is estimated except where a line says
ESTIMATE. Failures and divergences lead; nothing is smoothed.

---

## 0. Headlines

- **Access: both repos public, pins exact.** calamus HEAD `3d94fd0` is
  ravel's Cargo.lock pin; tessera HEAD `1142eb6` is the Cargo.toml rev.
- **Two real errata found and fixed** — both are v1 assumptions that were
  wrong in the shipped backend, and both passed v1 verification because
  the checks shared the assumption:
  - **C-1 (calamus):** the backend's ontology preparation derived
    **empty** name maps (its regex matched zero edges in the shipped
    Turtle). Abstract-name rewriting — the whole outside-query surface —
    never fired. Fixed as a faithful port of calamus's `Index`, now
    diffed mechanically against the real crate and behaviourally
    against native calamus + Oxigraph (13/13 shapes row-equal).
  - **P-1 (projection):** the backend's recent graph listed turns that
    carry no timestamp; native `keep_recent` never does. One quad in
    the whole corpus (a1001's membership). Backend and harness
    reference both fixed.
- **Third leg: clean apart from P-1.** Native vs backend vs reference
  agree quad-for-quad on every other graph (sess-A 108, sess-B 42,
  optmem 73, descriptions 12, bare-root soul exact), and the three
  ann_hash values are identical on all three sides.
- **Toy-corpus side-by-side:** native wins every query axis by
  68–393× (in-process Oxigraph vs a subprocess evaluator); the backend
  wins ingest (1.6–4.8×) and like-for-like mutation (1.5×) at this
  scale because the native side pays a RocksDB flush per write; the
  phext store is 2,137 B against 0.23–0.51 MB of native store
  apparent size (≈99% fixed overhead at 190 quads). At real scale the
  picture changes character — §7 is the headline gauge.
- **Tessera: verified clean.** The gate contract v1 assumed is the
  contract the crate implements, at every point v1 claimed.
- **Fringe: all six predictions confirmed** against the native stack
  (CONSTRUCT and DESCRIBE graded by output equivalence; see §6).
- **One latent divergence measured, not fixed here:** calamus wraps a
  fanned-out pattern in a DISTINCT sub-select; the lattice evaluator
  does not, so a subject carrying two stored spellings of one asked
  name returns 2 rows from the backend where native returns 1
  (crafted probe, §5.3). Unreachable on ravel-projected data (the
  projector writes exactly one spelling per position, and the shipped
  ontologies bridge each abstract name to exactly one stored
  spelling); the fix belongs in phext-lattice's evaluator and is
  recorded as follow-on work, Will's call.

## 1. Access, builds, environment

### 1.1 Access verdict

`git ls-remote git@github.com:repolex-ai/calamus.git HEAD` =
`3d94fd0cb1497eb7c4122df5b9a45af24f5501ec` — exactly ravel's
Cargo.lock pin. `repolex-ai/tessera` HEAD =
`1142eb66066624ebfa405b7679b3731e72b8fc4f` — exactly the Cargo.toml
pinned rev. Both cloned on the node; ravel's Cargo.toml declares both
over HTTPS git URLs and cargo fetched them without incident. (Also
measured, contrary to an older fleet note: rock-mountain fetched the
full crates.io index and packages directly, 369 crates compiled.)

### 1.2 Native build times (stopwatch, ravel @ 003fee9, rock-mountain)

| Leg | Time | Exit |
|---|---:|---|
| Cold debug | 229.37 s | 0 |
| Warm debug (no-op) | 0.44 s | — |
| Warm debug incremental (touch lib.rs) | 2.10 s | — |
| Cold release | 147.92 s | 0 |
| Warm release (no-op) | 0.15 s | — |

Release beat debug because the debug leg paid the one-time dependency
downloads. Binaries: `raveld` 26,806,664 B, `ravel` 13,535,032 B
(release). The Python legs of the stopwatch exited 127 (uv is absent
fleet-wide); v1's pip numbers stand (venv 1.72 s, `pip install -e.`
6.24 s). SQ on the node was aligned to the post-PR#7 exo source
(chain-aware, libphext 0.6.4 floor) and built in 12.68 s.

### 1.3 Thermal record (honest)

hwmon1 peaked at **87.5 °C during the cold builds** — over the 80 °C
off-time line. The builds were already running to completion when the
peak was observed; the node then took off-time and measured 39.4 °C
before bench work resumed (50.6 °C after the bench-example build,
35.75 °C after the native bench/dump run). scroll-central was barred
from this pass (86 °C at probe, load 5.4); aletheia-core was cool
(33 °C) with no film ffmpeg running at probe time.

## 2. Side-by-side — toy corpus (the v1 harness souls)

Same corpus as v1 (souls A/B; 10 transcripts' worth of events — 8 + 2
turns, 233 quads projected), same node, three interleaved rounds,
medians of ≥5 samples per axis (native release build; backend on the
aligned SQ + the shipped evaluator binary).

| Axis | Native (raveld code paths) | Phext backend | Winner | Ratio |
|---|---:|---:|---|---:|
| Ingest sess-A (5 turns + 3 anns) | 0.1241 s | 0.070–0.079 s | backend | ~1.6–1.8× |
| Ingest sess-B (3 turns + 1 ann) | 0.1349 s | 0.028–0.031 s | backend | ~4.3–4.8× |
| Point lookup (one turn by IRI) | 0.0000793 s | 0.0054 s | native | ~68× |
| Traversal 1-hop (all parent edges) | 0.0000268 s | 0.0056 s | native | ~209× |
| Traversal 3-hop (parent chain) | 0.0000280 s | 0.0110 s | native | ~393× |
| Fixed internal query (emojikeys) | 0.0001487 s | (same evaluator path as point ≈0.005 s) | native | ~35× |
| Mutation pair (change + restore, full re-ingest) | 0.2200 s | 0.1485 s | backend | ~1.5× |
| Mutation, native append path (one turn) | 0.1334 s | — (no append-shaped write; a single re-ingest ≈0.074 s) | backend on time, native on structure | — |
| Store bytes on disk | 227,797–511,155 B apparent (see note) | **2,137 B** (soul-a 2,001 + soul-b 136) | backend | ~107–239× |

Notes, plainly:

- Native query latency is in-process Oxigraph over a warm store; the
  backend pays a subprocess spawn plus full quad-set serialization
  per query (visible in the 3-hop axis ≈ 2× the point axis on the
  backend, flat on native). These are architecture costs, not tuning.
- Native ingest/mutation are dominated by the per-ingest RocksDB
  `flush()` (samples are bimodal, ~0.07–0.16 s); at 10² turns the
  flush is the whole cost. The backend's SQ writes are cheaper at
  this size. This inverts at scale — §7.
- Native store apparent size at this scale is ~99% RocksDB fixed
  overhead and swings with compaction state (B-only store 227,797 B;
  A+B dump store 358,448 B; A-only 413,108 B; A+memory 511,155 B).
  The honest statement is the range, not a point ratio.
- The backend's phext bytes reproduce v1 exactly (soul-a 2,001 B on
  both the VM and the node builds).

## 3. Third leg — ann_hash and projection parity (three-way)

Native dump (the real `project.rs` / `memory_log.rs` projectors, one
store per soul as production runs) vs the phext backend vs the
harness's independent reference projector, per graph:

| Graph | Native | Backend | Reference |
|---|---:|---:|---:|
| sess-A | 108 | 108 | 108 |
| sess-B | 42 | 42 | 42 |
| optmem | 73 | 73 | 73 |
| descriptions (default) | 12 | 12 | 12 |
| recent (soul A) | **7** | 8 → **7 after P-1 fix** | 8 → **7 after fix** |
| bare-root soul: transcript | 11 | 11 | 11 |
| bare-root soul: recent | 2 | 2 | 2 |
| bare-root soul: descriptions | 4 | 4 | 4 |

- **ann_hash: identical on all three sides** — `145c064c2390c7f5`,
  `6fc8620e4ca71fea`, `875538640e8395c1` (sess-A). The v1
  reconstruction of the annotation hash is confirmed against the real
  native build, not just against the reference.
- The one divergence was recent (erratum P-1, §8). Two artefacts of
  my own first dump were caught and corrected during the pass: an
  early combined-store dump let soul B's first `keep_recent` build
  scan soul A's turns (native `keep_recent`'s first build is
  store-wide; production runs one store per soul, so the clean
  per-soul dumps above are the production-shaped comparison), and
  the first driver omitted the memory projection (optmem 0 native)
  until `memory_log::project_into` was wired in.
- **Empty-text edge, verified against native:** native projection
  emits `<Turn/e0001> <ravel:text> ""`; the phext backend loses
  exactly that one triple (an empty scroll is indistinguishable from
  an absent one in the substrate, so the field reads back as None).
  Disclosed in v1; now confirmed against the real build. It is a
  substrate limit, not a projection bug, and it is the only
  quad-level loss anywhere in the comparison.

## 4. Cross-reference — tessera (verified clean)

v1's assumed contract (STUDY.md §2.1, from ravel's gate.rs): a Bearer
token checked against `~/.lex/identity/jwks.json`, service grants,
the machine owner; reads open in ravel's Phase-4 policy but a sent
token must verify; writes need soul-genesis match or machine owner;
services are scope-checked; souls matched by genesis, never address.

The real crate (tessera 0.1.1 @ `1142eb6`) confirms every point and
adds precision v1 never asserted:

- Tokens are Ed25519 JWS (alg EdDSA fixed in code); `kid` is the
  40-hex genesis commit SHA selecting the JWKS key; `sub` must equal
  the exact address recorded for that key (fallback
  `https://repolex.ai/soul/Soul/{kid}`); `aud` must be `"subtexture"`
  (the `AUDIENCE` constant — also the WWW-Authenticate realm).
- Lifetimes: plain token ≤ **7 days** by default, grants **8–366
  days**, both overridable via `~/.lex/identity/policy.json`
  (fail-closed: a malformed policy file is an error, never ignored).
  (Tessera's own verify.rs header comment says "24 hours" — stale
  inside the crate; its code and Policy defaults say 7 days. Noted
  for the record; v1 never repeated the 24-hour figure.)
- Service path: the token carries a `grant+jwt` signed by the
  grantor; its `jti` must be live in `~/.lex/identity/grants.json`
  (record agreeing on grantor/service/key, `ended` absent); the outer
  token is signed by the grant's `cnf.jwk` service key, `iss` is
  `service:{service}`, and it may not outlive the grant. Scope words
  are the grant's space-separated `scope`.
- Owner: `~/.lex/identity/owner.json` `{owner, genesis, since}`,
  matched by genesis or subject. HostKeys caches JWKS with
  reload-on-unknown-kid and one reload-and-retry on refusal;
  grants.json is read fresh per service check.
- Denials: 401 unauthenticated / invalid-token (RFC 6750 header with
  `error="invalid_token"`), 403 forbidden, 403 insufficient-scope.
- `access::check_read` (per-graph owner/visibility/readableBy) exists
  in tessera but **ravel's gate does not use it** — ravel gates at
  soul granularity. No backend implication.

**Backend consequence: none.** The backend implements no gate by
design (127.0.0.1 bind, a tessera-faithful gate in front — disclosed
in v1 and unchanged). The real contract validates that design's
premise exactly. **Errata in this category: none.**

## 5. Cross-reference — calamus

### 5.1 The preparation maps (after the C-1 fix)

The fixed `ontology_prep()` was diffed mechanically against the real
`calamus::Index` built from the same two TTL files (hash-verified
identical on both machines: subtexture `25ed37c8…`, ravel
`44d596cf…`). The crate's Index holds exactly (5 properties, 5
classes) with non-trivial targets; the backend's maps are the same
10 entries with the same target sets:

- properties: subtexture:{author, owner, readableBy, relatedToId,
  visibility} → the single ravel: spelling of each;
- classes: subtexture:Thing → {ravel:Memory, ravel:Turn};
  subtexture:NamedGraph and sd:NamedGraph → {ravel:MemoryGraph,
  ravel:NamedGraph, ravel:TranscriptGraph} (sd: also itself);
  subtexture:DefaultGraph → {ravel:MemoryGraph, ravel:TranscriptGraph};
  **ravel:NamedGraph (a stored class) → the same three** — the
  stored-class fan-out the v1 derivation could never have produced
  even had its regex worked (it only anchored on abstract classes).

The shipped ontologies use only equivalentProperty (11) and
subClassOf (17); subPropertyOf and equivalentClass counts are zero,
so calamus's handling of those edge kinds is untested by this stack's
data — the fixed port implements all four kinds per the crate's
rules regardless.

### 5.2 Behavioural agreement (outside shapes)

13 SELECT/ASK shapes run through native `outside::run` (calamus +
Oxigraph) and the backend's `run_query` (evaluator + fixed prep) over
the same corpus: abstract property swaps (relatedToId 10 rows,
owner 4), abstract class fan-out (15), fan-out inside `GRAPH ?g`
(15, ?g bound throughout — the calamus 0.1.2 bug class does not
exist in the evaluator's architecture), stored-class fan-out (4),
named-graph narrowing (7), stored name exactness (8), the
expression-name case (0 rows + the left-as-written note on both
sides), ASK, and the fringe SELECTs of §6. **All 13 row-equal.**
One cosmetic delta: the note renders the full IRI natively and the
prefixed display name (`subtexture:author`) in the backend — the
backend's `display_name()` rendering, content-identical.

### 5.3 The DISTINCT divergence (measured, latent)

calamus fans a pattern out as `{ SELECT DISTINCT <pattern vars>
WHERE { VALUES … } }`; the lattice evaluator matches per quad with
no such discipline. Crafted probe (one subject typed both ravel:Turn
and ravel:Memory in one graph): native returns **1 row**, the
evaluator **2 rows** — plain and `GRAPH ?g` forms alike. On
ravel-projected data this is unreachable (one spelling per position
by construction; one stored bridge per abstract name in the shipped
ontologies). It becomes live the day an ontology bridges one
abstract name to two stored spellings that can co-occur on one
subject. Fix belongs in phext-lattice's `sparql.rs` (fan-out
deduplication); recorded as follow-on, not done in this pass — the
evaluator is merged substrate (PR #99) and changing it is a lattice
PR, Will's call.

### 5.4 Named scope boundaries (agreements, not defects)

- **Property paths:** calamus rewrites inside paths and Oxigraph
  evaluates them (probe: `relatedToId+` answered natively, 1 row);
  the backend refuses by name — the C1-ruled subset boundary,
  working as ruled.
- **`calamus::names` (short-address moving):** ravel never calls it
  (grep-verified: outside.rs calls only `calamus::rewrite`). The
  backend likewise resolves against the asked store's base only.
  Agreement.
- **CONSTRUCT templates** are not name-rewritten on either side
  (calamus rewrites patterns only; the evaluator instantiates
  templates verbatim). Agreement, confirmed by §6's CONSTRUCT grade.

## 6. Fringe grading (against the native Oxigraph-backed path)

| Construct | Grade | Evidence |
|---|---|---|
| CONSTRUCT | **confirmed** | Backend output (10 triples) equals template instantiation over the native-verified pattern solutions, triple-for-triple. (Native result capture itself is blocked in the driver: oxigraph 0.5.6 as ravel configures it exposes no `QueryResults::Triples` variant to match — E0599. Graded by equivalence, stated plainly.) |
| DESCRIBE | **confirmed** | Backend output (7 triples) equals Turn/a0001's subject quads in the native store — the concise-description shape; this stack's data has no blank nodes for the shapes to diverge over. |
| SELECT DISTINCT | **confirmed** | 8/8 rows identical on corpus data. (The fan-out duplicate case is a calamus-semantics item, §5.3 — not a DISTINCT-modifier defect.) |
| OFFSET | **confirmed** | With ORDER BY + LIMIT: 2/2 rows identical. |
| BIND | **confirmed** | 8/8 rows identical (STR() binding). |
| Comparisons (!=, <, <=, >=, &&) | **confirmed** | noteq-fringe 6/6, compare-fringe 1/1, identical rows. |

No fringe prediction was refuted; none showed divergent semantics
within its own construct.

## 7. SCALE — the port gauged on our own dataset

Inventory: **SCALE.md** (companion document, same push) — 42 working
phexts, 16,628,698 B, 9,513 content scrolls, built 2026-11-01 →
2026-11-24. This leg runs the top seven by size individually plus
the remaining inventory combined (extraction: Will's parser; each
non-blank scroll becomes one event, its text carried byte-for-byte,
its coordinate kept as the event id — 9,461 events extracted;
SCALE.md's per-item scroll counts reproduced exactly). Both engines
ingest the identical event sets; **projected quad counts are
identical on every item** (240 / 1,064 / 1,024 / 3,332 / 3,456 /
6,464 / 7,220 / 15,044). tasks.phext is a live file: it grew from
3,746,698 B (SCALE.md's stat) to 3,755,430 B by extraction.

### 7.1 The table (seconds; medians where repeated)

| Item | Events | Backend ingest (1st / 2nd) | Native ingest | Backend point lookup (cold / warm) | Native point | Backend mutation (1 char) | Native mutation | Phext bytes | Native store bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| artifacts | 60 | 0.75 / 1.20 | 0.40 | 0.72 / 0.056 | 0.000018 | 1.32 | 0.19 | 1,397,094 | 6,572,050 |
| incipit | 266 | 2.64 / 5.19 | 0.17 | 3.31 / 0.055 | 0.000018 | 5.04 | 0.19 | 1,117,983 | 7,943,023 |
| skills | 256 | 3.22 / 5.82 | 0.18 | 3.35 / 0.067 | 0.000020 | 5.27 | 0.18 | 1,308,155 | 8,581,040 |
| choose-your-own-adventure | 833 | 11.5 / 49.4 | 0.26 | 22.9 / 0.19 | 0.000024 | 63.6 | 0.35 | 4,547,012 | 28,506,110 |
| tasks | 864 | 23.3 / 43.4 | 0.26 | 26.1 / 0.175 | 0.000018 | 42.2 | 0.27 | 3,741,823 | 25,683,868 |
| life | 1,616 | 10.2 / 26.1 | 0.26 | 51.7 / 0.19 | 0.000020 | 19.2 | 0.36 | 826,280 | 23,090,742 |
| rest-combined (32 phexts) | 1,805 | 33.1 / 51.4 | 0.29 | 56.8 / 0.18 | 0.000019 | 46.4 | 0.36 | 2,523,434 | 32,192,438 |
| state | 3,761 | 34.8 / 67.5 | 1.07 | **196.3** / 0.22 | 0.000020 | 65.2 | 1.91 | 1,169,779 | 50,497,072 |

### 7.2 What the numbers say

- **The toy-corpus result inverts.** At 2 KB the backend won ingest
  and mutation; at working scale native wins ingest on every item
  (2× on artifacts, up to ~44× first-ingest and ~190× second-ingest
  on CYOA) and mutation on every item (7× to ~180×). The backend's
  second ingest is consistently 2–4× its first — clearing and
  rewriting a region of a now-full multi-MB hosted file costs more
  than writing it empty.
- **The mutation question from v1 is answered.** A one-character
  change costs the backend a full transcript re-ingest through the
  per-scroll REST write path: 19–65 s on the top items, statistically
  indistinguishable from its no-change re-ingest (the write traffic
  is the whole transcript either way; v1's "exactly one scroll
  changes content" remains true of the bytes, not of the cost).
  Native's re-ingest of the same change is 0.19–1.91 s. (Native's
  append path doesn't apply to a changed middle record either —
  both engines rewrite the transcript; the difference is the write
  substrate's constant, ~two orders of magnitude.)
- **Cold queries are the backend's cliff.** The first query after
  any write regenerates the soul's entire quad set by walking its
  scrolls through SQ (`_evaluator_inputs`): 0.7 s at 240 quads,
  3.3 s at ~1,000, 23–26 s at ~3,400, 52–57 s at ~7,000, **196 s at
  15,044 quads** — ≈13 ms per quad and superlinear in total. Warm
  (cached-input) point lookups are 0.055–0.22 s, still ~3,000–
  11,000× native's ~20 µs. Extrapolating the measured curve to
  v1's open question (10⁵ turns ≈ 4×10⁵ quads): a cold query would
  take tens of minutes — labelled extrapolation from the four
  measured points, not a measurement. Incremental quad generation
  (v1 follow-up item 5) is not an optimization at this curve; it
  is the difference between usable and not.
- **Store bytes remain the phext's rout.** The soul phext is
  0.98–1.11× the source phext's bytes on every item (Σ ≈ 15.6 MB
  for the full inventory's content); the native RocksDB stores for
  the same content total Σ ≈ 183 MB apparent (4.7× the phext on
  artifacts, 43× on state — small-record corpora pay the most
  per-quad overhead). SCALE.md's 18.79× text-form factor was
  measured on field-dense toy data; at real scale the measured
  store ratio is 5–43×, same direction, wider spread.
- **Traversal axes at scale:** the extracted corpora are flat (no
  parent chains — these are not conversations), so 1-hop/3-hop
  degenerate; the full-graph COUNT scan stands in as the traversal-
  shaped axis: native 0.001–0.050 s vs backend count query
  0.059–0.275 s warm (and the backend count pays the cold cost
  above whenever it follows a write).

### 7.3 Representability — the not-representable list

The gauge ran every item through the turn representation because
the bytes can always be carried as turn text. What cannot be
carried is the *kind* of record (SCALE.md §3, per item; summary):

- **Transcript-shaped (1 of 42):** life.phext — a #life message
  log; fits turns directly, nothing structural lost.
- **Near-native homes (2):** premises.phext (the claim layer is a
  real fit) and emise.phext (optmem notes are close — what is lost
  is the selection/manifest curation layer; ravel has no selection
  object).
- **Not representable without loss (the remaining 39):** registers
  (tasks, todo, artifacts, skills, contests, jobs, stash,
  downloads, inkling-apis, patch, pipelines) lose coordinate-keyed
  register semantics — one scroll per record at its Singular
  coordinate is exactly what the turn vocabulary cannot say;
  books and corpora (CYOA, incipit, barnacles, emise's source
  corpus, humanities) lose their coordinate geometry (addressing
  as structure); state.phext loses snapshot identity (3,761 state
  snapshots become a conversation that never happened); ledgers
  (accounting, costs) lose tabular arithmetic — figures become
  prose; designbox.phext loses addressed-but-unlisted structure,
  which is inexpressible in the model at all.
- **Out of scope by the inventory's own ruling:** ietf.phext
  (564 MB hosted standards corpus) was not gauged.

**The gauge's one-line verdict:** as a *store*, the phext port
holds our whole three-week working corpus in ~16 MB of addressed
text where the native architecture needs ~183 MB of store (plus
its mirrors and manifests, SCALE.md §2) — and as an *engine*, the
current backend is one to three orders of magnitude slower than
native on every operation at this scale, with a cold-query cliff
at ~10⁴ quads that no amount of tuning inside the current glue
shape removes. The port's storage thesis is proven on real data;
its query/write path needs the incremental redesign v1 already
named.

## 8. ERRATA (Will's directive: fix them where they live)

### calamus
- **C-1 — the ontology preparation derived empty maps.** *Assumption:*
  v1's `ontology_prep()` regex over the TTL text yields the
  equivalence/subclass tables. *Where it lived:*
  `backend/ravel_backend.py` (`ontology_prep`), and as a claim in
  V1-REPORT.md (backend component list). *Evidence:* the regex
  matches **0** edges in the shipped ontologies (predicate-list
  Turtle: the subject never stands adjacent to the predicate);
  both maps were empty in the shipped backend, so abstract-name
  queries silently matched nothing and no left-as-written note
  could fire. The evaluator's own Rust tests never caught it —
  they hand-build the Prep. *Fix:* `ontology_prep()` rewritten as
  a faithful port of calamus's `Index::build` + `closure` (real
  Turtle statement parser; all four edge kinds; abstract names
  dropped from answers; rdf:type never swapped). Verified three
  ways: mechanical diff vs the real crate's Index (identical),
  13/13 behavioural shapes vs native, and the new harness outside
  phase (fails on the v1 backend, passes on the fixed one).
  *Landed:* backend code (this push), V1-REPORT.md erratum note at
  the claim site, harness outside phase + docstring.
- **Not an erratum — a newly measured divergence:** the DISTINCT
  fan-out discipline (§5.3). No v1 document claimed duplicate-row
  equivalence; recorded here as a divergence with its fix scoped
  to phext-lattice.

### tessera
- **Verified clean — no errata.** Every contract point v1 asserted
  (token, JWKS key list, service grants, machine owner, genesis
  matching, Phase-4 read rule) is implemented by the real crate as
  asserted (§4). The crate's additional precision (aud
  `"subtexture"`, 7-day/8-day/366-day defaults + policy.json,
  grants.json jti registry) contradicts nothing v1 claimed and
  changes nothing the backend must do.

### projection
- **P-1 — recent listed timestamp-less turns.** *Assumption:* the
  newest-500 window weighs every turn. *Where it lived:*
  `backend/ravel_backend.py` (`recent_turns`) and the harness's
  `want_recent` construction (comment: "every turn here is within
  the 500"). *Evidence:* native `keep_recent` (graph.rs) weighs
  only turns carrying a timestamp, on both its first-build and
  incremental paths; the corpus's a1001 has none — native recent
  holds 7 memberships, backend and reference said 8, and the v1
  check passed because both sides shared the assumption.
  *Fix:* `recent_turns` now skips timestamp-less events (a present
  but unparseable timestamp still sorts oldest, as native's
  `instant()` does); the harness construction and its comment are
  corrected to the same rule. The corrected check fails against
  the v1 backend and passes against the fixed one. *Landed:*
  backend code, harness, V1-REPORT.md erratum note at the
  verification claim.
- ann_hash and every other projection axis: **verified clean**
  three-way (§3). The empty-text triple loss is a substrate limit
  disclosed in v1 and confirmed against native — not an erratum.

### fringe
- **Verified clean — no errata.** All six FRINGE constructs grade
  confirmed against the native stack (§6).

### documents
- V1-REPORT.md: two dated erratum notes landed in place (backend
  component list — C-1; verification section — P-1 + C-1 coverage),
  original claims left visible.
- STUDY.md and SPARQL-SUBSET.md: **no false claims found.** Their
  calamus/outside passages describe the native pipeline and the
  design, both accurate as written; neither asserted the shipped
  backend's prep worked. (The harness — the document that did
  encode P-1's assumption — is corrected, above.)
- This report's §0/§2 supersede V1-REPORT's implied outside-surface
  status wherever they differ.

### Re-verification after fixes
- Harness (fixed backend + fixed harness): **72/72 checks pass**
  (65 v1 checks + 7 new outside checks), phext SHA determinism
  intact (soul-a 2,001 B).
- Bite test (fixed harness vs the v1 backend, kept from the fork):
  **exactly 6 failures** — `recent membership == reference` plus 5
  outside checks (the stored-name-exactness and property-path
  checks pass, as they never depended on the prep). The corrected
  tests bite on precisely the two errata.
- Wavelets 1–3 suite (fixed backend): **19/19 pass.**

## 9. Honest gaps — what remains unproven after this pass

1. **Fan-out duplicate discipline** (§5.3): evaluator over-counts
   vs calamus when one subject carries two stored spellings of an
   asked name. Latent on today's ontologies + projector; fix is a
   phext-lattice evaluator change, unscoped and unbuilt.
2. **Write-path and glue scaling** — §7's measurements are the
   evidence; the two known structural costs (per-scroll REST writes
   against a hosted file; whole-soul quad regeneration after any
   write) now have real-scale numbers, and both are redesign-sized,
   not tuning-sized.
3. **The gate is still not built.** The backend binds 127.0.0.1
   and trusts a fronting gate; tessera's contract is now verified
   to support building one exactly as designed, but no gate code
   exists on the backend side.
4. **SQ `$N:` multi-universe addressing** still returns empty (SQ
   hosts one universe at a time by design) — unchanged from v1;
   separate feature work, Will's call.
5. **Native CONSTRUCT/DESCRIBE result capture** was graded by
   equivalence, not by consuming Oxigraph's triple results (the
   0.5.6 feature shape blocks the match in the driver). The
   underlying pattern semantics are the same code path the 13
   SELECT/ASK shapes verified directly.
6. **ietf.phext (564 MB)** is out of the SCALE inventory's scope
   (a hosted corpus, not a working phext) and was not gauged.
7. **Fork break/rename** (wbic16/ravel): still unruled; the fork
   continues to carry the phext tree additively under PATCHES.md
   discipline.
8. Build/bench numbers are one node's (rock-mountain, thermally
   interrupted once, §1.3); no second-node replication was run.

## 10. Push

This report, the corrected backend (`ontology_prep`, `recent_turns`),
the corrected + extended harness (outside phase, P-1 construction),
SCALE.md, the errata-noted V1-REPORT.md, and the v2 instruments
(`verify/v2/`: compare.py, probe.py, queries.json, scale.py,
extract_scale.py, v2bench.rs, indexdump.rs, crafted corpus, node run
scripts) ride the fork push to wbic16/ravel `main` under `phext/`,
with PATCHES.md entries for the backend behaviour changes. The push
commit is the commit carrying this file; its hash is recorded in
the task record (34.6.8/7.7.11/9.5.8) and was remote-verified
(LOCAL_HEAD == REMOTE_HEAD) at push time.
