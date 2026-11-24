# Ravel — the SPARQL subset, determined; and where its evaluator should live

Task 34.6.8/7.7.11/9.5.8 · Evidence pass for Phase 2 · Source: wbic16/ravel @ `003fee9` (v0.12.1), the study clone on rock-mountain, read 2026-11-23 (EE ~396). Companion to STUDY.md (esp. §2.4, §7.3-C1). Nothing in the ravel repo was modified; nothing here is built — this is the determination and the placement design.

**C1 is RULED (Will, 2026-11-23 22:14 CST): option (a)** — the outside routes are scoped to a supported subset; out-of-subset queries are refused by name. This document is therefore a Phase 2 **build input**: the spec in §4 is what Phase 2 implements against, and the placement recommendation in §5 is Will's to rule on when this lands.

---

## 1. Plain summary

**Determination verdict: yes, easily.** Every SPARQL query ravel issues internally is a string literal or a `format!` template sitting in plain sight in six files; the whole internal surface enumerated in one pass (§2). The outside surface is bounded from inside the repo — ravel's own CLI is an outside issuer, `ravel query help` documents the dialect it promises users, and the preparation pipeline (outside.rs + calamus) characterizes the rest (§3). The one thing that resisted enumeration is named in §6: the third-party issuers (Syrinx, pand) are not present on any probed node, so their exact traffic is bounded by the documented dialect rather than measured. Refusal-by-name makes any such miss loud, never silent.

**The subset, in one table** (full spec with proving queries in §4):

| In | Proving use |
|---|---|
| SELECT (projected vars + expression aliases), ASK | every query in the repo |
| Basic graph patterns: triple patterns, `;`/`,` sugar, PREFIX, full IRIs, relative IRIs against a base | everywhere |
| GRAPH ?g and GRAPH \<iri\>, multi-GRAPH joins, default-graph-as-union dataset | stats, recall, emojikeys, help examples |
| OPTIONAL | turn text lookups, emojikeys |
| FILTER: `=`, `>`, `&&`, `\|\|`, `!`; STR, STRSTARTS, STRENDS, CONTAINS, LCASE, DATATYPE, REGEX (with flags), SUBSTR in expressions | stats, recall, verify, help |
| FILTER EXISTS / NOT EXISTS | verify, readdress report |
| Aggregates: COUNT(\*), COUNT(?x), COUNT(DISTINCT ?x), MIN, MAX; GROUP BY | stats, diagnose, recall |
| ORDER BY (ASC/DESC, expressions, multi-key), LIMIT | recent, stats, recall |
| VALUES (IRI lists) | memory verify's cited-turn check |
| Triple terms `<<( s p o )>>` as pattern object; triple terms bound as values | **query_emojikeys — the claim layer's core query** |

| Out (never issued anywhere in the repo — refused by name) |
|---|
| UNION, BIND, subqueries, MINUS, HAVING, OFFSET, SELECT DISTINCT, REDUCED, SERVICE, property paths |
| CONSTRUCT / DESCRIBE as query forms — handled by today's route layer but **never issued by any code or doc in the repo** (see §4.3 flag) |

**Placement recommendation (§5): fold the subset evaluator into phext-lattice** — as its own module behind a quad-source seam — and keep in ravel's backend only the ontology rewrite (calamus) and the HTTP result shaping. Estimated one-time substrate cost ~1,600–2,350 Rust LOC (estimate, labelled); ravel's backend then carries ~150–250 LOC of graph-view glue instead of an evaluator, inside the study's 1,200 LOC ceiling with room to spare. The honest counter-case is stated in §5.4. Per Will's standing instruction (2026-11-23 22:15), §5.5 is the first instalment of the fold evaluation: every candidate item, its target, and its reason; the evaluation completes with the v1 report.

---

## 2. Enumeration — the internal set (closed)

Method: grep sweeps over the clone for query-form keywords, `spargebra`/`SparqlParser`/`SparqlEvaluator` call sites, and `query_rows` callers, then full reads of every site. Production = reachable from `ravel`/`raveld` outside `#[cfg(test)]`. Every production query is a literal or a template whose only interpolations are IRIs, a FILTER clause, a LIMIT, or a VALUES list — no query is assembled from fragments beyond that, so the set below is complete, not sampled.

### 2.1 Production queries

| # | Site | Purpose | Constructs |
|---|---|---|---|
| I-1 | `src/graph.rs` `keep_recent` (~L478) | Bootstrap the recent graph: newest turns store-wide | SELECT; GRAPH ?g; BGP with `;`; ORDER BY DESC(?ts); LIMIT (interpolated, 1000) |
| I-2 | `src/graph.rs` `query_emojikeys` (~L579) | Every emojikey claim across all transcript graphs, via the reified claim | SELECT; PREFIX ×4; GRAPH ?g; **triple-term pattern** `?claim rdf:reifies <<( ?event ravel:exhibits "emojikey/harvest" )>>`; OPTIONAL ×2; FILTER(?sk = "…") (injected or absent); ORDER BY ?g ?ts |
| I-3 | `src/daemon/http.rs` `memory_open` (~L308) | One cited turn's ts/role/text for the memory view | SELECT template (ground subject IRI interpolated); GRAPH ?g; `;`; OPTIONAL; LIMIT 1 |
| I-4 | `src/daemon/http.rs` `soul_stats` (~L471–483) | The `/stats` counts: quads, graphs, subjects, turns, dated, earliest, latest, claims, top-20 predicates | 8 templates: COUNT(\*), COUNT(DISTINCT ?g/?s) over `GRAPH ?g { ?s ?p ?o }`; FILTER(STRSTARTS(STR(?s), "prefix")); FILTER(DATATYPE(?t) = \<xsd:dateTime\>); MIN(?t)/MAX(?t); COUNT over predicate `rdf:reifies` (triple terms bound as values); GROUP BY ?p + ORDER BY DESC(?n) + LIMIT 20 |
| I-5 | `src/sync.rs` `diagnose` (~L1973–1977) | `ravel status` store health: turn count, graph count, latest turn | COUNT(?s); COUNT(DISTINCT ?g); plain SELECT + ORDER BY DESC(?t) LIMIT 1 — run through a local evaluator, **not** the outside pipeline (stored spellings, by outside.rs's own rule) |
| I-6 | `src/memory_relive.rs` `verify` (~L1079) | Memory check: block edges pointing at no Memory node | COUNT(\*); FILTER(STRSTARTS(STR(?c), "…")); **FILTER NOT EXISTS** |
| I-7 | `src/memory_relive.rs` `verify` (~L1106) | Memory check: cited turns exist as Turn nodes | COUNT(DISTINCT ?t); **VALUES ?t { \<iri\> … }** (chunks of ≤200 IRIs); GRAPH ?g |
| I-8 | `src/readdress.rs` `turns()` (~L98) | Turn count for the address-migration tool | COUNT(DISTINCT ?t); GRAPH ?g. (The tool itself is deleted by the phext backend — STUDY §7.1 — but its query is part of today's internal set.) |

`memory_progress` and `memory_check` issue **no SPARQL of their own** (progress reads the log's own stream size; check delegates to `verify`, I-6/I-7). No other daemon, reader, or extract code issues queries — grep-verified.

### 2.2 Test-only internal queries (they bind the backend's own test suite)

| Site | Constructs beyond §2.1 |
|---|---|
| `src/memory_log.rs` (~L767, L807), `src/project.rs` (~L259) | ASK { \<iri\> a ?k } — ontology-consistency existence checks |
| `src/graph.rs` tests (~L787) | ASK with GRAPH ?g, `;`, FILTER(STRSTARTS(STR(?t), "…")) |
| `src/annotate.rs` tests (~L430, L491) | The emojikey triple-term pattern (as I-2); COUNT(\*) with **numeric** FILTER(?m > 5) over xsd:integer literals |
| `src/graph.rs` tests (~L898–932) | COUNT(DISTINCT ?g); FILTER(!STRENDS(STR(?g), "…")); per-graph dump `GRAPH <g> { ?s ?p ?o }`; ORDER BY ?p |
| `src/readdress.rs` tests/report (~L297–341) | FILTER **EXISTS** { GRAPH ?g { … } }; `;` chains of three; literal-object patterns (`memoryLevel 0`, FILTER(?l > 0)) |
| `src/outside.rs` tests (~L258–363) | The outside pipeline's conformance dialect — counted in §3.2 |

### 2.3 The Python lab (historical dialect — evidence of family, not of the current contract)

`python/src/weave/graph/store.py` passes caller SPARQL to pyoxigraph unchanged (SELECT only, per its docstring). Its concrete queries (`__main__.py` demo; `tests/test_graph.py`) use: BGP joins with `;`, ORDER BY, and one triple term **bound as a value** (`?claim rdf:reifies ?tt`). Same construct family as the Rust set; the lab's `weave:` namespace predates ravel. Counted as corroboration, not contract.

---

## 3. The outside surface — measured where reachable, bounded where not

Arbitrary SPARQL enters only via `/query` and `/sparql` (STUDY §2.4). Three in-repo evidence sources bound what "arbitrary" actually means in this stack:

### 3.1 First-party issuers (executed code, in the repo)

**The CLI's `optmem recall`** (`src/bin/ravel.rs` ~L513–521) POSTs three query templates to `/ravel/{id}/query` — i.e. ravel's own shipped client is an outside issuer, and its dialect is part of the subset's proof set:

- note search: GRAPH \<ravel/NamedGraph/optmem\> + BGP + injected FILTER + ORDER BY ?i
- top merge: same graph, FILTER(?l > 0), ORDER BY DESC(?l) ?a, LIMIT 1
- turn counts: GRAPH ?g, COUNT(DISTINCT ?u), MIN(STR(?ts)), MAX(STR(?ts))
- the injected FILTER is the richest expression in the codebase: `FILTER(CONTAINS(LCASE(?x), "term") && REGEX(?x, "(^|[^A-Za-z0-9_])term($|[^A-Za-z0-9_])", "i"))` — conjunction, LCASE, CONTAINS, REGEX with the case-insensitive flag.

### 3.2 The documented dialect (`ravel query help`, `src/bin/ravel.rs` ~L895–947)

The help text is the contract Syrinx-class consumers were written against (http.rs: the W3C route exists so "a standard client (and Syrinx) needs nothing ravel-specific"). Its three worked examples + one listing query use: PREFIX; `;` sugar; GROUP BY with COUNT and ORDER BY DESC; FILTER with `||` and STRSTARTS; SELECT-expression SUBSTR(?text, 1, 80); GRAPH ?g; GRAPH \<named\>; a **two-GRAPH join** (optmem note graph × turn graphs); ORDER BY + LIMIT. The pipeline's own conformance tests (outside.rs tests) add: FILTER(?p = prefixedName) (equality against a prefixed name — the case calamus deliberately leaves as written, with a note), COUNT forms with and without GRAPH, and relative-IRI resolution against the store root as base.

### 3.3 The preparation pipeline is itself a subset boundary

`outside::prepare`/`run_until` (`src/outside.rs` L48–95) defines what an outside query *is* today, independent of who sends it:

1. **Parse against a base** — the store's root IRI; relative addresses (`<ravel/Turn/x>`, `GRAPH <ravel/NamedGraph/optmem>`) resolve per-soul. Any parse error refuses the query outright ("never run half-prepared").
2. **Ontology rewrite (calamus)** — the parsed algebra is rewritten from the two shipped ontologies: `subtexture:` names find their `ravel:` stored spellings; abstract names are dropped from answers; names inside expressions are compared as written and reported as notes (http.rs surfaces them as `notes` / `x-ravel-notes`).
3. **Dataset rule** — a caller-written FROM / FROM NAMED is kept as written; otherwise the default graph becomes the **union of all named graphs** (ravel keeps everything in named graphs; without this the git-lex-shaped question returns nothing).

So the de facto outside subset = *whatever parses, survives the rewrite, and evaluates under a union-default dataset*. §4's spec is the constructive version of that boundary: the constructs the stack demonstrably uses, named so refusal can name the rest.

### 3.4 What could not be measured

No Syrinx, pand, or gitlexd checkout exists on rock-mountain (the node the study and this pass ran on), and no other node was swept for them in this pass. Their live traffic is therefore **unmeasured** — bounded by §3.1–3.3, not enumerated. This is the single gap in the determination, and the ruling's refusal-by-name is what makes it safe: a Syrinx query using an out-of-subset construct fails loudly with the construct named, at Phase 2 trial time, instead of silently returning wrong rows.

---

## 4. The subset spec (Phase 2 build input)

Conventions: a construct is **IN** only with a proving query from §2/§3. A construct used exactly once is flagged ◊ — a candidate cut, decided in Phase 2 on the stated alternative, never silently dropped. Everything not listed IN is **OUT**: refused at parse time, by name (per the ruling).

### 4.1 IN — query forms and structure

| Construct | Proved by |
|---|---|
| SELECT with projected variables | all |
| SELECT expression aliases `(expr AS ?v)`: aggregates, SUBSTR, STR | I-4, recall, help |
| ASK | §2.2 tests; the route's Boolean result form (http.rs `answer`) |
| PREFIX declarations; full IRIs; relative IRIs resolved against a base | I-2, §3.3 |
| Triple patterns: IRI/var subject, predicate, object; literal objects (plain, typed xsd:integer/dateTime, language-free); `a` | all |
| `;` and `,` sugar | all |
| GRAPH ?g (graph as variable, incl. in aggregates) | I-1, I-2, I-4 |
| GRAPH \<iri\> (named graph block) | recall, help, keep_recent's panel query |
| Multiple GRAPH blocks joined in one WHERE | help example 3 |
| No-GRAPH patterns under the union-default dataset (§3.3 rule 3); caller FROM/FROM NAMED honoured | outside.rs |
| OPTIONAL (left join), incl. several per pattern | I-2, I-3 |
| VALUES ?v { \<iri\> … } ◊ | I-7 (only use; alternative: ≤200 coordinate-existence reads in the verify path, no SPARQL — Phase 2 may cut VALUES if verify is re-expressed that way) |

### 4.2 IN — expressions, aggregates, solution modifiers

| Construct | Proved by |
|---|---|
| FILTER with `=`, `>` (numeric and string), `&&`, `\|\|`, unary `!` | I-2, recall, help, §2.2 |
| STR(?v) | I-4, I-6, recall |
| STRSTARTS / STRENDS / CONTAINS (on STR() or literals) | I-4, I-6, recall, help |
| LCASE | recall |
| DATATYPE(?v) = \<iri\> | I-4 |
| REGEX(?v, "pattern", "i") ◊ (one issuer: the CLI's recall — a shipped feature, so the flag is informational, not a cut recommendation) | recall |
| SUBSTR(?v, start, len) in SELECT expressions | help (×2) |
| FILTER EXISTS / FILTER NOT EXISTS ◊ (one production use each: I-6 NOT EXISTS; EXISTS in the readdress report/tests — the report dies with its tool, so EXISTS's production proof is thin; keep both: they are the same mechanism) | I-6, §2.2 |
| Aggregates: COUNT(\*), COUNT(?v), COUNT(DISTINCT ?v), MIN(?v), MAX(?v) — over vars and STR() of vars | I-4, I-5, recall, help |
| GROUP BY (single var in every observed use) | I-4, help |
| ORDER BY: ASC default, DESC(expr), multiple keys, keys not in the projection | I-1, I-4, recall |
| LIMIT (incl. interpolated) | everywhere |
| **Triple terms**: `<<( s p o )>>` as a triple-pattern object with inner vars (RDF 1.2 reification pattern); triple terms bound as ordinary values via a var object | I-2 (pattern — load-bearing, the claim layer's core query); I-4 + lab (as values) |

### 4.3 OUT — refused by name

UNION · BIND · subqueries (SELECT inside WHERE) · MINUS · HAVING · OFFSET · SELECT DISTINCT / REDUCED · SERVICE / federation · property paths (all forms: `/`, `^`, `*`, `+`, `?`, `|`, `!`) · functions beyond §4.2 (e.g. CONCAT, REPLACE, COALESCE, IF, BOUND, isIRI/isBlank/isLiteral, LANG beyond DATATYPE equality, arithmetic expressions) · SPARQL Update (the routes are read-only today; nothing issues an update).

**Flagged contract narrowing — CONSTRUCT / DESCRIBE.** Today's route layer *answers* these forms (http.rs `answer` maps Graph results to N-Triples; the `/sparql` doc comment advertises it), but **no query in the repo — production, test, CLI, help, or docs — ever issues one**. Under the ruling they are out-of-subset and refused by name in v1; if a consumer surfaces later, CONSTRUCT over the §4.1 pattern language is the natural first extension (its template is a BGP — the evaluator already walks those). Will should see this as the one place the scoped contract is visibly narrower than today's advertised route shape.

### 4.4 What the internal set needs from all this

Nothing beyond §4.1–4.2 — and Phase 2 doesn't have to route the internal set through the evaluator at all: STUDY §7.1 already prices the fixed internal queries as named region reads (~150 LOC). The evaluator exists for the two outside routes. (If it lands in phext-lattice, the internal set *may* later ride it for uniformity; that is an optimization, not a requirement, and C4's concurrency question in STUDY §7.3 applies to the substrate either way.)

---

## 5. Placement — the evaluator's home

### 5.1 The shape, wherever it lives

Three separable pieces, which today's outside.rs fuses:

1. **Parse + dataset semantics** — subset grammar, base-IRI resolution, union-default dataset. Generic SPARQL; identical for every consumer.
2. **Evaluation** — BGP matching, joins/left-joins, FILTER expressions, aggregates, ordering, over a *graph view*: an iterator of quads (graph, s, p, o) with named-graph enumeration. The view is the only project-specific surface.
3. **Preparation + presentation** — calamus's ontology rewrite (stack IP: subtexture↔ravel names), result shaping (W3C results JSON / N-Triples lines), concurrency seats and the 30 s cancellation policy (daemon policy).

### 5.2 Recommendation: pieces 1+2 fold into phext-lattice; piece 3 stays in ravel's backend

**Form:** a `sparql` module in the phext-lattice repo (its own crate in the workspace if the repo's layout prefers), exposing: the subset parser (refusal-by-name is native to it — an out-of-subset construct fails at the construct, named, rather than parsing fully and failing an audit later), the algebra, and an evaluator over a `QuadSource` trait (quad iteration + graph enumeration). phext-lattice's scroll/span machinery (libphext's span index — coordinates + byte ranges per scroll) is what a QuadSource walks; ravel's backend implements QuadSource over STUDY §5's coordinate map (GRAPH ?g ↔ transcript/memory regions; a triple ↔ a subject scroll's lane entries) in an estimated **150–250 LOC**.

**Why not spargebra:** depending on spargebra re-imports the term-model slice of the Oxigraph ecosystem the upgrade is shedding (STUDY §3.5), and it parses *full* SPARQL — refusal-by-name would need a second construct-audit pass over its algebra. A subset-native parser is smaller than the audit and refuses at the point of offence. (Estimate, labelled: subset parser ~450–700 LOC; algebra + evaluator ~800–1,200; expression evaluator ~300–450; **total ~1,600–2,350 Rust LOC, once, in the substrate** — against the study's +800 *Python* C1 allowance for a backend-local version, which bought less: one consumer, no reuse, and the mass inside the thing minimization is trying to shrink.)

**The calamus seam (the placement's one real integration cost, named):** calamus today rewrites *spargebra's* `Query`. A lattice-native algebra means the rewrite needs a lattice-algebra target: either calamus gains that target (repolex-side work, outside this upgrade), or the rewrite moves to the parsed algebra via a small adapter in ravel's backend (~150–300 LOC, estimate) — the subset algebra is small and closed, which is what makes the adapter bounded. Text-level pre-parse rewriting is **not** recommended: outside.rs's own notes show why (names inside expressions are compared as written — a text rewrite would corrupt exactly the case the pipeline is careful about).

**What ravel's backend keeps:** the prepare pipeline's rewrite step (calamus + adapter), the routes' result shaping, seats/cancellation — all HTTP/daemon policy, ~the outside.rs remainder. Its evaluator mass goes to zero; Phase 2's 1,200 LOC ceiling (STUDY §7.2) holds *without* spending the +800 C1 allowance inside the backend.

**What other phext-lattice consumers gain:** the SPARQL shape is the stack's shared query dialect, not ravel's private one — http.rs's own comment: the W3C route is "the same shape gitlexd and pand answer on, so a standard client (and Syrinx) needs nothing ravel-specific." git-lex's graphs are phext-shaped already (the recent-graph design cites git-lex's `NamedGraph/now` convention). Any lattice consumer whose scrolls carry graph structure gets the same evaluator over its own QuadSource, and the subset — being usage-proved — is the same subset their traffic already speaks (§3's evidence is stack evidence, not only ravel evidence).

### 5.3 Effect on the ravel backend's remaining mass

| Piece | Today (Rust) | With lattice evaluator |
|---|---|---|
| outside.rs (399) | parse (spargebra) + calamus rewrite + Oxigraph eval | calamus rewrite + algebra adapter + lattice call ≈ 200–300 (Python, Phase 2) |
| C1 allowance in backend budget | +800 LOC (STUDY §7.2) | **0** — replaced by QuadSource glue 150–250, inside the 1,200 ceiling |
| Fixed internal set | named queries via Oxigraph | unchanged plan: named region reads (~150, STUDY §7.1) |
| Dependency chain | oxigraph + spargebra (+ RocksDB native build) | shed entirely on the query path |

### 5.4 The honest counter-case (why it might stay in the backend)

- **One confirmed consumer family.** edit.phext.io / SQ have no SPARQL need today. If git-lex/pand never adopt the lattice evaluator, phext-lattice carries ~2k LOC for ravel alone — the same mass, worse housed, and in a substrate whose other citizens didn't ask for an RDF dialect.
- **Ravel-shaped creep.** The subset is defined by ravel's usage; housed in the substrate it will attract "just one more construct" pressure from exactly one direction. The spec's own rule is the guard (no construct without a proving query), but the guard is social, not mechanical.
- **Release coupling.** phext-lattice/libphext changes are Will-published (the libphext freeze etiquette: Will publishes himself). A ravel query fix that needs a substrate change waits on a substrate release; a backend-local evaluator iterates at ravel's speed.
- **Mitigation that keeps the recommendation standing:** ship it as a separable module/crate in the lattice repo, behind the QuadSource seam, so the fold is reversible — if adoption doesn't materialize, the crate lifts back out to the backend with the seam intact. The recommendation is for the lattice, with that reversibility as a condition.

### 5.5 Fold evaluation — instalment 1 (Will's standing instruction, 2026-11-23 22:15)

Per-item; folds land as PRs for Will's review; the libphext publication freeze stands (Will publishes libphext himself). This list completes with the v1 report, where anything else the build needed joins it.

| Item | Fold target | Reason |
|---|---|---|
| SPARQL subset parser + algebra + evaluator (§4) | **phext-lattice** (recommend; §5.2) | ≥3 stack consumers speak the dialect; backend minimization; closed, usage-proved subset keeps the substrate surface bounded |
| QuadSource seam (quad iteration + graph enumeration over scrolls) | **phext-lattice** (with the evaluator) | It is the evaluator's only project-facing surface; defining it once is what makes other consumers possible |
| Region/lane enumeration primitives the QuadSource needs | **phext-lattice**, riding libphext's span index; any gap found in Phase 2 → candidate **libphext-rs** fold (PR only) | Spans already give (coordinate, byte-range) per scroll; gaps, if any, are core-format concerns, not app concerns |
| Base-IRI resolution; default-graph-as-union dataset semantics | **phext-lattice** (part of the parser/evaluator) | Generic SPARQL dataset behaviour; every consumer needs it identically |
| Refusal-by-name errors (mechanism) | **phext-lattice** parser | Refusal at the construct is the mechanism the ruling requires; message wording stays the backend's |
| Ontology rewrite (calamus: subtexture↔ravel, abstract-name handling) | **stays in ravel's backend** | Stack ontology IP; meaningless outside the repolex ontologies; the substrate must not learn `subtexture:` |
| Result shaping (W3C results JSON, N-Triples lines), notes headers | **stays in ravel's backend** | HTTP route contract, not query semantics |
| Concurrency seats, 30 s limit, cancellation plumbing | **stays in ravel's backend** | Daemon policy (and STUDY C4's substrate-concurrency test is Phase 2's, wherever the evaluator lives) |
| The fixed internal query set | **stays in ravel's backend** (named region reads) | Cheaper as coordinate reads than as evaluated SPARQL; no reuse case elsewhere |

---

## 6. Determination cost — stated honestly

- **Effort:** one pass — six grep sweeps + full reads of every query site (~10 files) over one clone. The internal set (§2) is **complete**: production queries are literals/templates in six files (`graph.rs`, `daemon/http.rs`, `sync.rs`, `memory_relive.rs`, `readdress.rs`, plus the CLI as outside issuer in `bin/ravel.rs`); there is no query-construction machinery to hide in.
- **What resisted:** (1) third-party outside traffic (§3.4) — not present on the probed node; bounded, not measured. (2) Prose queries — the keep_recent doc comment and `query help` are documentation, counted as *documented dialect* (they are the consumer-facing contract), never as executed code. (3) Template interpolation — FILTER/VALUES/LIMIT values are injected at call sites; each injection site was read, so the templates' construct sets are exact, but the *values* are runtime data (e.g. recall's search term), which no static pass can enumerate — nor needs to: values don't add constructs.
- **Confidence:** the subset covers 100% of in-repo issued SPARQL (production + tests + CLI + help + lab) by construction. Residual risk concentrates entirely in §3.4's unmeasured third-party traffic, and the ruling's refusal-by-name converts that risk from silent wrong answers into named, fixable refusals.

*End of pass. Phase 2 builds against §4; §5.2's placement is Will's ruling to make.*
