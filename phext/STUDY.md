# Ravel — phext upgrade study (Phase 1)

Task 34.6.8/7.7.11/9.5.8 · Method: phext_upgrade skill · Source: wbic16/ravel @ `003fee97aaf97ba021e8dc33ac62eba0b0e6756f` (v0.12.1), read on its contents on rock-mountain, 2026-11-23. Nothing in the repo was modified. Claims are distilled from the project's own code, docs and invariants; anything not derivable that way is labelled **hypothesis**.

**Process note (evidence discipline):** the fresh clone tripped the runtime's execution-risk check, so the reading was done as plain text viewing (no repo code executed, no builds, no tests) until Will's direct build-timing ask (EE 395) plus his standing AGENTS.md rule ("wbic16 clones are auto-approved for building") authorized the build measurement in §3.3. The skill's `phext_audit.py` inventory was reproduced by equivalent metadata inventory (`du`/`find`/`wc` over the tree) rather than by running the script against the flagged tree; the numbers are the same kind the audit produces (file/extension census), and classification was done by reading, as the skill requires.

---

## 1. Plain summary

**What ravel is.** A transcript-graph engine (Rust, one crate, two binaries). The daemon `raveld` (one per machine, the only writer) watches the transcript files that Claude Code and Gemini/Antigravity write, imports claude.ai data exports, mirrors every transcript byte-for-byte into the owning agent's ("soul's") git repository, and projects the mirror into an RDF 1.2 graph in an Oxigraph (RocksDB) store beside it. The CLI `ravel` is a thin client of the daemon over a gated local HTTP API; it never opens a store itself. On top of the transcripts sits optmem — ravel's adaptation of Victor Taelin's OptMem: the soul's biography as an append-only log of one-line notes (≤280 bytes) merged pairwise up a binary tree, so the soul wakes with its whole history in at most 96 lines and can open any line down to the conversation turns it rests on. The transcript bytes are canonical; **the graph is a projection and can always be rebuilt** (README's own words — this is the fact the whole upgrade turns on).

**What its API promises.** Three surfaces, detailed in §2: (1) the daemon HTTP API — 14 routes, JSON in/out, plus a W3C SPARQL protocol route; this is the contract other stack tools (Syrinx, gitlexd, pand) already consume; (2) the CLI command set layered on it; (3) the Rust library whose store seam (`graph.rs`: open / ingest / append / query_rows / keep_recent) is where a backend swap actually happens. A phext-lattice backend keeps (1) and (2) byte-for-byte and re-implements (3)'s seam.

**Where the complexity lives.** 18,622 production lines (17,292 Rust + 1,330 Python lab) to maintain: a hand-rolled N-Triples serialization layer, a RocksDB store managed through a custom reader/writer turnstile that exists because of a measured Oxigraph read-beside-write failure mode, a whole store-migration tool (`readdress.rs`, 379 lines) that exists only because addresses are baked into a binary store, per-dialect ingest manifests with two different change-detection rules, a derived "recent turns" index maintained by hand, and a projection-stamp cache. Dependency weight: 240 locked Rust packages (14 direct, incl. two git deps) + 91 locked Python packages for a lab the engine grew out of. §3 has the census; §6 defines and computes the complexity / cache-ability score: **complexity 967 (score v1 convention), cache-ability 93 (state 100, work 86)**.

**Classification (§4).** The graph store's contents — Turn spine, annotations/claims, memory graph, recent list — are **Class A** (canonical phext) as the new backend: text records with stable identity, per-record appends, provenance value. The optmem logs are the strongest Class A in the project (append-only, position-addressed, already text). Transcript mirror bytes stay native as **Class C bridge** (they are other tools' formats, byte-for-byte by contract). The ontology TTL is Class C (a byte-identical copy is CI-checked against the git-lex kit). Manifests and the recent index are **Class B** derived views. Secrets/identity material, RocksDB internals, and the LLM writer's live calls are **Class D — refused**, named in §4.

**Draft coordinate map (§5).** One phext per soul (soul scoping is by store — ravel's own invariant), manifest at 1.1.1/1.1.1/1.1.1, axes derived from ravel's stable ids: transcript slot × turn slot × field lane for the spine; log position for notes; (lo, hi) block identity for merges. Draft only — Phase 2 builds from it after review.

**Honest estimate of what the lattice backend deletes.** Of the Rust machinery, ≈2,100–2,400 lines (12–14%) exist only to compensate for the substrate — store management and locking workarounds, address migration, hand-rolled RDF text emission, manifest bookkeeping, the hand-maintained recent index, the projection stamp — and are deleted outright when addresses *are* the storage. The dependency chain sheds Oxigraph/Spargebra and the RocksDB native build (2 of 14 direct deps; their transitive share is **unmeasured**). What does **not** shrink: the three dialect adapters (1,303 lines), the Event model and projection semantics, optmem's tree math, the relive pipeline, the gate, and the API surfaces themselves. The one place complexity can *grow* is the outside-SPARQL route: arbitrary SPARQL over scrolls is Phase 2's real design problem, stated plainly in §7 — the fixed internal query set is small and fully enumerated in §2.4, but the public SPARQL route is not a fixed set.

---

## 2. The API contract (what the new backend must keep)

### 2.1 Daemon HTTP API — `src/daemon/http.rs`, `router()` (all routes behind the gate, `src/daemon/gate.rs`)

| Route | Method | Handler | Needs (gate table, gate.rs) |
|---|---|---|---|
| `/ravel/health` | GET | `health` | open (stack-wide convention) |
| `/ravel/stores` | GET | `souls` | open (Syrinx discovers stores here) |
| `/ravel/sync` | POST | `sync_all` | machine owner |
| `/ravel/shutdown` | POST | `shutdown` | machine owner |
| `/ravel/{org}/{store}/sparql` | POST | `store_sparql` | read rules; W3C SPARQL protocol shape |
| `/ravel/{org}/{store}/query` | POST | `soul_query` | read rules; ravel's own JSON route (`{query}` in, rows out) |
| `/ravel/{org}/{store}/sync` | POST | `sync_one` | that soul or owner; service needs `ravel:write` |
| `/ravel/{org}/{store}/import` | POST | `import_one` | that soul or owner; service needs `ravel:write` |
| `/ravel/{org}/{store}/memory` | GET | `memory_view` | read rules (the biography/optmem index) |
| `/ravel/{org}/{store}/memory-progress` | GET | `memory_progress` | read rules |
| `/ravel/{org}/{store}/memory-check` | GET | `memory_check` | read rules |
| `/ravel/{org}/{store}/memory/{node}` | GET | `memory_open` | read rules (open a note/block by label) |
| `/ravel/{org}/{store}/health` | GET | `soul_health` | read rules (the `ravel status` diagnostic) |
| `/ravel/{org}/{store}/stats` | GET | `soul_stats` | read rules (store counts) |

Read rule (Phase 4 state, gate.rs): a token is not required yet; one that is sent must be good, and a service's grant must say `ravel:read`. Identity checking itself is tessera's (stack-shared crate): token, `~/.lex/identity/jwks.json` key list, service grants, machine owner. Default port **7881**, sync interval default **30 s** (`config.example.yml`, `src/daemon/config.rs`: `DaemonConfig { path, present, port, interval_secs, souls: Vec<PathBuf>, human: Option<String>, memory: Option<MemoryWriter { harness, model, reword }>, warnings }`).

### 2.2 CLI — `src/bin/ravel.rs` (thin client; starts the daemon when none runs; resolves a soul by cwd, 6-char genesis id, or path)

`ravel` (daemon status) ·
 `ravel status [soul]` · `ravel souls` · `ravel sync [soul] [dir]` · `ravel import [soul] <dir>` · `ravel query [soul] <sparql>` · `ravel query help` · `ravel biography [soul]` · `ravel optmem` · `ravel optmem wake [soul]` · `ravel optmem open [soul] <label>` (label = `#i` or `lo-hi`) · `ravel optmem recall <words…>` · `ravel optmem check [soul]`. And `raveld` (foreground) · `raveld status|restart|stop` (`src/bin/raveld.rs`).

### 2.3 Library surface (the swap seam)

The bins consume the library (`src/lib.rs`, 21 modules). The backend-relevant seam is the store layer, `src/graph.rs`:

- `open(path) -> Store` / `open_read_only(path) -> Store` — Oxigraph/RocksDB handles, plus the in-process reader/writer **turnstile** (`reading(dir)` / `writing(dir)`) that exists because a read-only RocksDB handle beside a writing one can error-loop unboundedly (measured incident, graph.rs header: an 84 GB runaway query, 2026-11-21).
- `ingest_transcript(store, events, anns, transcript_id, names) -> usize` — clear the transcript's named graph, load full spine + annotations, describe graph, `keep_recent`, flush.
- `ingest_annotations(...)` — the annotation-scoped variant (only referenced turns).
- `append_transcript(...)` — growth-only path: load new turns, clear nothing.
- `keep_recent(store, names, loaded)` — maintain the `recent` graph: newest 500 turns as `rdfs:member` statements.
- `query_rows(store, sparql) -> Vec<HashMap<String,String>>` and `query_emojikeys(store, Option<source_kind>)`.
- `drop_graph`, `describe_graph`, `transcript_graph_iri`.

Everything above this seam (sync orchestration, adapters, memory pipeline, HTTP handlers, gate) is backend-agnostic in structure: it speaks `Event`s, `Annotation`s and SPARQL strings. Everything below it is Oxigraph.

### 2.4 The fixed internal query set

Internal SPARQL is a closed set, enumerable from the code: the recent-graph bootstrap query (graph.rs), `query_emojikeys`' cross-graph claim query (graph.rs), the status/stats counts behind `/health` and `/stats` (http.rs `ask`), the memory check's existence queries (http.rs `memory_check`), and `ravel query help`'s ontology introspection. Arbitrary SPARQL enters only through the two outside routes (`/query`, `/sparql`), prepared by `outside.rs` + calamus (parse against base → ontology rewrite `subtexture:`→`ravel:` → default-graph-as-union). This split — fixed inside, arbitrary outside — is the load-bearing fact for §7.

---

## 3. Census

### 3.1 Tree inventory (audit-equivalent; working tree excl. `.git`, @ 003fee9)

76 files, 1,411,530 B. By extension: `.rs` 32 files / 713,341 B · `.lock` 2 / 380,361 B · `.md` 11 / 116,670 B · `.py` 18 / 81,574 B · `.ttl` 3 / 65,724 B · `.html` 1 / 25,450 B · `.txt` 1 / 15,061 B · `.yml` 3 / 10,119 B · `.toml` 2 / 3,040 B. Flagged per the audit's rules: structured-text stores — the runtime formats are generated per-soul (§3.4), not shipped in the repo; docs corpus — `docs/` 164,304 B (dated design/calibration documents, incl. a transcript ontology draft in Turtle); config — `config.example.yml`; binary/weights/media — none in the repo (Class D by absence).

### 3.2 LOC by component (wc -l, rock-mountain; includes inline unit tests — the inline-test share is **unmeasured**)

| Component | LOC | Role |
|---|---:|---|
| src/sync.rs | 2,990 | mirror + ingest orchestration, manifests, sibling slugs |
| src/memory_relive.rs | 1,643 | relive pipeline (note/merge generation via writer) |
| src/memory_extract.rs | 1,369 | memory reader: unread-stream computation |
| src/daemon/http.rs | 1,089 | the 14 HTTP routes |
| src/bin/ravel.rs | 973 | CLI client |
| src/graph.rs | 935 | **the store layer** (Oxigraph, turnstile, keep_recent) |
| src/memory_log.rs | 820 | optmem log files + memory-graph projection (hand-emitted NT) |
| src/agy.rs | 714 | Antigravity adapter |
| src/annotate.rs | 533 | annotation → NT (sha256 IRIs) |
| src/peers.rs | 504 | herdr sender-side peer matching |
| src/daemon/memory.rs | 493 | daemon memory scheduling |
| src/outside.rs | 399 | outside-query preparation (calamus) |
| src/writer.rs | 390 | writer harness contract |
| src/readdress.rs | 379 | **store address-migration tool** |
| src/project.rs | 356 | Event → NT projection (hand-emitted) |
| src/lib.rs | 335 | Event model + module map |
| src/daemon/config.rs | 328 | config |
| src/reader.rs | 310 | readers (emojikey) + Annotation type |
| src/claude_ai.rs | 296 | claude.ai adapter |
| src/adapter.rs | 293 | Claude Code adapter |
| src/memory.rs | 289 | optmem tree math (cover/pending) |
| src/names.rs | 243 | address construction |
| src/bin/raveld.rs | 236 | daemon binary |
| src/herdr.rs | 225 | herdr sender-line parsing |
| src/daemon/registry.rs | 194 | soul registry (genesis id) |
| src/client.rs | 190 | CLI→daemon HTTP client |
| src/daemon/gate.rs | 338 | authorization (tessera) |
| src/daemon/mod.rs | 352 | daemon core, sync loop |
| src/daemon/doing.rs | 60 | — |
| src/soul.rs | 16 | soul invariant, stated |
| **Production Rust** | **17,292** | (+195 tests/, +295 examples/) |
| python/src/weave (source) | 1,330 | the lab (monolog, readers, projection, store) |
| python/tests | 659 | |
| **Total production** | **18,622** | |

Dialect adapters (adapter + agy + claude_ai) = 1,303 lines. Named substrate-compensation modules (graph + readdress + outside) = 1,713 lines = 9.9% of production Rust, before the unmeasured shares inside sync.rs (manifest/mirror machinery) and the three NT emitters (project.rs, annotate.rs, memory_log.rs projection).

### 3.3 Build times — stopwatch receipts (Will's ask, EE 395; node: **rock-mountain**, rustc/cargo 1.94.1, 2026-11-23 22:05–22:07 CST)

**The Rust build did not run.** All five timed attempts failed at dependency fetch, before any compilation began:

| Attempt | Wall | Exit | Result |
|---|---:|---:|---|
| cold debug (`cargo clean` + `cargo build --locked`) | 11.56 s | 101 | fetch failure |
| warm debug, no-op re-run | 11.99 s | 101 | fetch failure |
| warm debug, incremental (touch lib.rs) | 12.24 s | 101 | fetch failure |
| cold release (`cargo clean` + `cargo build --release --locked`) | 12.17 s | 101 | fetch failure |
| warm release, no-op re-run | 11.19 s | 101 | fetch failure |

Cause, measured: cargo updates the crates.io index, then fails fetching the git dependency `https://github.com/repolex-ai/calamus` — "failed to receive HTTP 200 response: got 401". The same identity over SSH gets "Repository not found" for both `repolex-ai/calamus` and `repolex-ai/tessera`. No probed node (rock-mountain, aletheia-core, scroll-central, ni-knight) holds either repo in its cargo git cache. **These are fetch-failure receipts, not build times — no ravel build time exists on this fleet today.** Unblocking requires git access to the repolex-ai org for the building identity (Will's call; it is an access matter, not a code matter). Node temperature 35.5 °C before / 34.1 °C after (nothing heated; nothing built). Timing scripts and full logs: `~/ravel-build-times.txt` on rock-mountain; scripts kept at `~/workspace/ravel-upgrade/build-time.sh`, `build-time-py.sh`.

Python setup (same node, Python 3.12.3): the project's canonical tool **uv is absent** from rock-mountain and aletheia-core (checked `~/.local/bin` and PATH). Measured substitutes: `python3 -m venv` creation **1.72 s** (exit 0); `pip install -e .` for the core package (stdlib-only deps) **6.24 s** (exit 0); `import weave` **0.012 s**. The `--extra graph` (pyoxigraph) setup was not measured — **unmeasured**, because the uv route that defines it is unavailable on-node.

### 3.4 Persistence inventory (per soul repo, from README "What lives where" + code constants)

Under `<soul>/.ravel/_ignore/` (machine-local, gitignored):

| Artifact | Format | Written by | Mutability |
|---|---|---|---|
| `transcripts/claude-code/*.jsonl` | JSONL (harness bytes) | sync mirror | append / never-shrink |
| `transcripts/agy/<conversation>.jsonl` | JSONL | sync mirror | content-hash tracked; source rewrites in place |
| `transcripts/claude-ai/<export>/` | export files as downloaded | import | immutable once copied |
| `ingest-manifest.tsv` (+ `-agy`, `-claude-ai` variants) | TSV | sync | rewritten in place (small) |
| `optmem/memories.jsonl` | JSONL, one note per line | memory pipeline | **append-only, canonical** |
| `optmem/summaries.jsonl` | JSONL, one block per line | memory pipeline | append-only |
| `optmem/extracted.tsv` / `dropped.tsv` / `usage.tsv` | TSV | memory pipeline | append-only |
| `optmem/stream.json` | JSON | memory pipeline | rewritten (one number + stamp) |
| `optmem/projected.stamp` | text stamp | memory projection | rewritten |
| `oxigraph/` | RocksDB (Oxigraph) | graph layer | mutated in place; **fully derived** |

Plus machine-level: `~/.config/ravel/config.yml`, `raveld.log` (self-capping 100 MB → newest 50 MB). Ten persisted artifacts per soul, seven distinct formats (JSONL, TSV, JSON, stamp text, RocksDB binary, YAML, Turtle).

### 3.5 Dependency chain

Rust direct (14, Cargo.toml): oxigraph 0.5.6 (rdf-12), spargebra 0.4.6, serde, serde_json, chrono, anyhow, sha2, axum 0.8, tokio 1, reqwest 0.12 (blocking, rustls), serde_yaml, libc, **calamus** (git, repolex-ai), **tessera** (git, repolex-ai, pinned rev). Cargo.lock total: **240 packages**. Python: core runtime deps **0** (stdlib only); optional arms sentence-transformers / amrlib / pyoxigraph; uv.lock total **91 packages** (incl. dev). Ontology: `ravel.ttl` imports the Subtexture base (`subtexture.ttl` ships beside it); the kit copy in git-lex-kit-ravel must stay byte-identical (CI-checked).

---

## 4. Classification (phext_upgrade decision matrix; five tests applied per part)

| Part | Class | Why (from the project's own invariants) |
|---|---|---|
| Turn spine in the graph store (Turn nodes, parentTurn edges) | **A — canonical phext (new backend)** | Text records; identity = event_id, stable and validated; updates are per-transcript appends; readers want single turns without scanning (access test passes: optmem open, recent list, turn counts); provenance is the project's purpose. |
| Annotations / belief claims (reified triple terms) | **A** | Text; deterministic sha256 identity; anchored per (turn, span); append-only in practice (re-ingest overwrites identically); the claim/fact distinction is representable as scroll structure (claim lanes vs spine lanes, §5). |
| Memory graph (Memory nodes + relatedToId links) | **A** | A projection of Class A logs (below); as backend state it is per-record text with position identity. In the new backend it is not a second store — it is the logs' own coordinates read as a graph (§5), which is the minimization win. |
| optmem logs (memories.jsonl, summaries.jsonl) | **A — the strongest in the project** | Append-only text; identity is position (#i, #lo-hi) assigned by the format itself; one-line records; provenance (model, wording) carried per record. They already behave like scrolls written linearly. |
| optmem bookkeeping (extracted/dropped/usage TSVs, stream.json, projected.stamp) | **B — derived view** | Every one is recomputable from the logs + mirrors (extracted = turns cited/read; stamp = a cache key). In the backend they become derived scrolls or disappear into substrate metadata; they are never truth. |
| Ingest manifests (3 TSVs) | **B — derived view** | A cache of "what has been ingested, by size/hash". Rebuildable by scanning the mirrors. In the backend: substrate-side state (per-scroll hashes/sizes the lattice already keeps), not backend code. |
| Recent-turns graph (newest 500) | **B — derived view** | A sliding-window index, rebuilt from the spine by timestamp. Never stored twice as truth (the matrix's graph-archetype rule); the backend answers it as a query (§7 conflict C2 names the cost). |
| Transcript mirror bytes (3 dialects) | **C — bridge only** | Byte-for-byte copies are the contract with the harnesses and with git; other tools read them. The backend imports from them and can export them; it does not replace them. (Their *projection* is Class A above.) |
| claude.ai exports (as downloaded) | **C — bridge only** | External format with external users; imported, never watched; an older export must never roll a conversation back — an ingest rule, kept in the importer, not the backend. |
| Ontology files (ravel.ttl, subtexture.ttl) | **C — bridge only** | Text and addressable, but the kit copy must remain byte-identical (CI) — canonical home stays the TTL file; the backend reads it, never re-homes it. |
| Docs corpus (docs/, README) | **B — derived view candidate** | Knowledge layer; a phext materialization is legitimate but is not the backend's business in Phase 2. Noted, not scoped. |
| Daemon config (config.yml) | **C — bridge** | Machine configuration in the stack's shared shape; read as-is. |
| **Refused (Class D), named:** identity & secrets material (`~/.lex/identity/jwks.json`, session tokens, subscription credentials) | **D** | Secrets; phext gains nothing and risks everything. The gate reads them where they live. |
| RocksDB/Oxigraph store internals | **D** | The binary store is the thing being replaced; its internals have no phext representation and need none — the rebuild path (mirrors + logs) is the migration. |
| LLM writer calls (note/merge generation) | **D** | Live model I/O on the user's subscription; not state, not cacheable, not the backend's. Its *outputs* are Class A logs. |
| Hot query evaluation over the full store | **D (as state)** | Evaluation is work, not records; §7 prices what of it the backend must still do. |

Whole-project statement, as the skill requires: ravel's *records* phextize almost completely (the engine was already disciplined: canonical bytes, deterministic ids, append-only logs, derived graph). What refuses is exactly what should: secrets, the replaced store's internals, and live model I/O. A study that phextized those too would have failed its first test.

---

## 5. Coordinate map — DRAFT (derived from ravel's invariants; minimization-shaped)

Design rule for this map, per Will's directive (2026-11-23 22:08): **the backend is a thin mapping layer** — each API call resolves to coordinate reads/writes against the phext-lattice substrate; zero bespoke store, index, serialization, or traversal machinery in the backend. The map is therefore deliberately boring: it uses only structures the substrate already has (scrolls at coordinates, per-scroll update, enumeration of a region), and it stores each fact exactly once, at the coordinate its own identity names.

**One phext per soul** (soul scoping is by store — `soul.rs`; two souls' graphs are never mixed). Coordinates below are Library.Shelf.Series / Collection.Volume.Book / Chapter.Section.Scroll within that soul's phext. All components ≥ 1; slots are assigned by the registry scrolls and never reused, so existing addresses never move when the map grows.

- **Manifest** at `1.1.1/1.1.1/1.1.1`: format name + version, source commit (`003fee9…`, ravel 0.12.1), this map, the soul's root (`https://repolex.ai/{squad}/{user}/`), and the slot registries' locations.
- **Collection 1 — transcripts.** Volume = transcript slot (registry at `1.1.1/1.2.1/1.1.1`: transcript_id → slot, assigned at first ingest). Book = turn slot (the turn's position in its transcript's event order; registry-free — position is the order the adapter emitted, and event_id is stored in the identity lane for the reverse lookup). Chapter = lane: 1 identity (event_id, role, author, modelSetting, claimedSender* — one field per Section, value as Scroll), 2 timestamp (the federation anchor, own lane because queries key on it), 3 text (chunked by scroll if long), 4 spine (parent turn's coordinate — the edge is a scroll naming its target, per the graph archetype: presence/content = edge), 5 annotations (Section = reader slot, Scroll = one finding: span, origin, signal fields as sibling scrolls under the finding's Section block), 6 claims (the reified layer: a claim scroll names the spine/annotation scrolls it reifies, the reader, and its asserted/unasserted status — unasserted is the default and is structural, not a flag day).
- **Collection 2 — memory (optmem).** Volume 1 = the log: Book = position i+1 (note #i), Chapter 1 = the note (date, text, model, wording — field per Section), Chapter 2 = citations (one Scroll per cited turn, content = that turn's Collection-1 coordinate). Volume 2 = the tree: Book = lo+1, Chapter = span length (hi−lo), lanes as Volume 1 (text; halves = the two child coordinates, named in scrolls). Wake, open, recall, check are then reads over these coordinates plus the pure tree math (`memory.rs`, which the backend keeps as a library function, not machinery).
- **Collection 3 — derived views (Class B, rebuilt, never truth).** Volume 1 = ingest state per transcript (mirror size/hash as the substrate reports them — if the substrate's own per-scroll hashes suffice, this volume stays empty and the manifest says so). Volume 2 = the recent window *only if* §7 conflict C2 is decided for materialization; by default it does not exist.
- **No WAL volume, no seal volume, no index volume.** The single-phext-filesystem archetype's bookends are the substrate's business (SQ/libphext provide the store's own integrity); a backend that re-implements them has already failed minimization. If Phase 2 finds a substrate gap there, it is reported as a substrate gap, not papered over with backend code.

Growth rule: new record kinds take new Volumes/Books inside their Collection; new dialects take new transcript slots; nothing renumbers.

---

## 6. Complexity / cache-ability score (instrument defined before computation; Will's addition, 2026-11-23 22:02)

### 6.1 Definition (score v1 — small enough to reuse on the next phext_upgrade project)

**Complexity** — four measured components and one headline convention:
- **K1 dependency-chain weight:** direct dependencies and total locked packages, per ecosystem, from the manifests and lockfiles. (Chain *depth* is **unmeasured** unless a pass walks the lockfile graph; the rule is to say so.)
- **K2 bespoke-machinery share:** production LOC in modules whose stated purpose (their own module docs) is storage, serialization, indexing, migration, or locking of the store — i.e. lines that exist only because the substrate didn't — as a share of total production LOC. Measured at module granularity; name the modules; mark intra-module shares unmeasured rather than estimating them.
- **K3 state surface:** persisted artifacts per unit of deployment, and distinct persisted formats among them (census, §3.4).
- **K4 language count:** implementation languages in production (notation/data languages listed separately, not counted).
- **Headline (declared convention, weights stated so another repo scores comparably):** `C = LOC_prod/100 + locked_packages_total + 50 × distinct_formats + 100 × (languages − 1)`.

**Cache-ability** — two measured fractions; the headline is their mean, and the components always print beside it:
- **State cache-ability:** of the persisted artifacts (K3 census), the share that are either (i) immutable once written (append-only counts as immutable-per-record) or (ii) deterministic derivations of other persisted artifacts with a stated rebuild path. Counted per artifact, from the code's own rebuild claims.
- **Work cache-ability:** of the pipeline stages (enumerated from the code's own pass structure), the share that are deterministic transforms of their inputs (same input → same output, replayable/memoizable). A stage whose output is a live model call is not deterministic, however frozen its outputs later become.
- Any component that cannot be measured honestly is marked **unmeasured**, never estimated.

### 6.2 Ravel, computed (baseline, as-is @ 003fee9)

| Component | Value | Basis |
|---|---|---|
| K1 direct deps | Rust 14 (incl. 2 git), Python core 0 | Cargo.toml / pyproject.toml |
| K1 locked packages | 240 (Cargo.lock) + 91 (uv.lock) = **331** | lockfiles; depth **unmeasured** |
| K2 bespoke machinery | **≥1,713 LOC = ≥9.9%** of production Rust | named modules: graph.rs 935 (store/locking/index), readdress.rs 379 (store migration), outside.rs 399 (store query prep); additional shares inside sync.rs and the NT emitters **unmeasured** at module granularity |
| K3 state surface | 10 artifacts / **7 formats** per soul | §3.4 |
| K4 languages | **2** (Rust, Python) + notations (SPARQL, Turtle, YAML) | §3.2 |
| **Complexity headline C** | **967** (= 186.2 + 331 + 350 + 100) | convention above |
| State cache-ability | **100%** (10/10 artifacts) | mirrors + optmem logs immutable-per-record/append-only; manifests, stamp, stream.json, the entire Oxigraph store, recent + memory graphs all deterministic derivations with stated rebuild paths. One gap named: claude-code change detection keys on *size*, not content hash (agy keys on hash) — hashable in principle, not yet hashed |
| Work cache-ability | **86%** (6/7 stages) | mirror-skip (manifest-memoized), parse, project, read, extract, query — deterministic; **relive note/merge generation is a live model call — not deterministic**, its outputs freeze only once written |
| **Cache-ability headline** | **93** (mean of 100 and 86) | components above |

Reading: ravel is already an unusually cache-able system — its author built it as canonical bytes + deterministic derivations. Its complexity is concentrated where §1 said: substrate compensation and a 331-package chain, not in its data discipline.

---

## 7. Minimization analysis (Will's directive, 2026-11-23 22:08 — governs Phase 2)

Target shape: **a thin mapping layer and nothing more.** Each API call resolves to coordinate reads/writes against the phext-lattice substrate. If the lattice (SQ/libphext) already does it, the backend must not re-implement it. Every backend line carries its justification below: for each API behaviour, whether the substrate expresses it directly, and what backend mass it forces if not.

### 7.1 Behaviour-by-behaviour

| API behaviour (§2) | Substrate expresses it? | Backend mass forced |
|---|---|---|
| Open a soul's store | Yes — open the soul's phext (SQ/libphext) | ~0 (path/config glue) |
| Ingest transcript (full) | Yes — write spine/annotation scrolls per §5; clear = delete the transcript's region | mapping only: Event→scroll writes |
| Append to a grown transcript | Yes — append scrolls at the next turn slots | mapping only |
| Drop/replace one transcript | Yes — region delete + rewrite | ~0 beyond ingest mapping |
| Turn by id; transcript's turns in order | Yes — coordinate read; enumeration of a Book region | ~0 |
| Newest-500 recent list | **Partially** — enumeration + sort by the timestamp lane; the substrate has no cross-region time index | query-time sort in the adapter (no stored index): ~30–50 LOC, or materialize (conflict C2) |
| Memory wake / open / progress / check | Yes — reads over Collection 2 + the pure tree math (kept as-is, 289 LOC library, not backend machinery) | ~0 beyond mapping |
| Memory recall (word search over the log) | Partially — substrate text search if SQ exposes it; else a scan of Collection 2 (the log is small by design: notes ≤280 B) | 0 if SQ search; ~40 LOC scan otherwise |
| Status / stats counts | Yes — region enumeration + counts | ~20 LOC |
| Outside query, fixed internal set (§2.4) | Yes — each is a named region read + row shaping | ~15–25 LOC per query × ~6 queries |
| Outside SPARQL (arbitrary, `/query` + `/sparql`) | **No** — the substrate does not evaluate SPARQL | **the one big mass** — conflict C1 |
| Belief layer (claims unasserted by default) | Yes — structural (claim lanes, §5); filtering is row shaping | ~0 |
| Gate (who may ask) | Unchanged — tessera-side, not the backend's | 0 |
| Sync loop, mirroring, manifests | Mostly substrate-external (daemon's job, unchanged); manifest *state* becomes substrate per-scroll metadata | importer keeps its dialect logic (adapters stay) |
| Address migration (readdress) | **Deleted** — addresses are the storage; a root change is a manifest edit + scroll re-homing by the substrate's own tools, not a bespoke store copier | −379 LOC vs today |

### 7.2 Phase 2 minimization budget (the ceiling the build is judged against)

Derived from the API surface (§2), per-endpoint mapping cost + manifest/glue:

| Piece | Budget (LOC) |
|---|---:|
| Store seam implementation (§2.3 equivalents over SQ/libphext): open, ingest, append, drop, turn/transcript reads | 350 |
| HTTP route mapping: 14 routes × ~25 LOC (resolve soul → coordinate ops → JSON shaping) | 350 |
| Fixed internal query set (§2.4): ~6 queries × ~25 LOC | 150 |
| Memory views (wake/open/progress/check/recall) over Collection 2, reusing the tree-math library | 200 |
| Manifest, config glue, CLI import/export bridge (Class C) | 150 |
| **Backend ceiling (excl. C1)** | **1,200 LOC** |
| C1 allowance *if* Will rules for a SPARQL subset evaluator (see below) | +800, separately justified |

Against today's substrate-compensation mass (≥1,713 named LOC in the Rust engine, plus the Oxigraph/Spargebra dependency weight), a backend at or under 1,200 Python LOC — with zero store, index, serialization, or traversal machinery of its own — is the win condition. Under budget is a win to report; over budget needs a named reason per overrun, in the v1 comparison report.

### 7.3 Conflicts — named plainly for Will's ruling, not designed around

- **C1 — Arbitrary SPARQL vs minimization.** The `/query` and `/sparql` routes promise arbitrary SPARQL 1.2 (incl. triple terms, GRAPH, the calamus ontology rewrite). No phext substrate evaluates SPARQL. Keeping the promise verbatim forces a SPARQL evaluator into the backend (thousands of lines — the opposite of the directive) or a dependency on an external evaluator. Options for Will: (a) scope the routes to the subset the stack actually issues (measurable from Syrinx/pand usage — a Phase 2 evidence task), with out-of-subset queries refused by name; (b) keep Oxigraph as a *derived, disposable* query cache fed from the phext (Class B) — the API stays whole, the phext stays canonical, but a store-shaped dependency remains on the read path; (c) drop arbitrary SPARQL from the contract (a contract change — Will's alone). **The study does not choose.**
- **C2 — The recent-500 window.** Query-time sort per request repeats work the current design pays once per ingest. Minimization says compute at read; the panel workload that created `keep_recent` (a question every few seconds) says the cost lands on the hot path. Ruling needed only if Phase 2 measures the read-sort as too slow on a real store; the map (§5) defaults to *no materialized index*.
- **C3 — Byte-identical CLI status output.** `ravel status` output is a diagnostic other tooling may parse (**hypothesis** — no consumer was found in the repo). If a consumer exists, the backend must reproduce Oxigraph-era counts/wording exactly, which is glue mass (~50–100 LOC) that minimization would otherwise skip. Phase 2 should grep the stack for consumers before paying it.
- **C4 — The turnstile's failure mode.** The reader-beside-writer runaway was an Oxigraph/RocksDB behaviour. SQ has its own concurrency semantics; whether multi-reader/one-writer beside a live daemon is safe *there* is a substrate question Phase 2 must test, not assume — if it fails, the fix belongs in the substrate or the daemon's access pattern, not in backend locking machinery.

---

## 8. Forward notes (carried into Phase 2/3, per the score's terms)

1. **Phase 2 (Python adapter)** builds to §5's map under §7's budget (1,200 LOC ceiling excl. C1), as a parallel adapter beside the native path — native format, API and tests unchanged — with the skill's verification minimum: lossless roundtrip both directions (byte identity on the deterministic native side), the project's own edge cases (empty transcript, empty log, rewritten record with advanced version, self-referencing parent, maximum record counts from §2.5), and a mutation control (one record changes ⇒ exactly its scrolls change).
2. **The v1 comparison report recomputes §6's score identically** for the phext-lattice backend and prints the delta as a headline: complexity removed and cache-ability gained, in the same units, beside the backend's actual LOC against §7.2's budget.
3. **Phase 3 (Rust port evaluation)** inherits these facts: the seam is §2.3; the mass to beat is §7.2; the dependency question is whether a Rust backend can ride libphext/SQ without re-growing an Oxigraph-class chain (240-package baseline, K1). The evaluation is written against the Python backend's measured numbers, not against the Rust engine's.
4. **Open items for Will:** C1 (SPARQL scope) is the only ruling that changes Phase 2's shape; the repolex-org git access (§3.3) gates any future Rust-side build/test on this fleet; C2/C3 resolve inside Phase 2 on evidence.
5. **Standing constraints:** the ravel repo stays untouched until Will directs a contribution; reusable tooling built along the way publishes under the standing rules the session it is built.

*End of Phase 1 study. Deliverable: this file. Task 34.6.8/7.7.11/9.5.8 remains in progress — Phase 2 dispatches after review.*
