# PHEXT.md — ravel on phext, v1

A parallel adapter for ravel (wbic16/ravel @ `003fee9`, v0.12.1): the
daemon's route surface and the `graph.rs` store seam, same API, backed
by one phext per soul. The native format and the native backend are
untouched; this tree is the swap candidate, not a replacement in place.

## What lives where

- `phextio.py` — SQ REST client + lifecycle. One `sq host` serves the
  data directory; phexts are addressed by stem name. SQ v0.6.0
  behaviour as probed (see the module docstring): no delete (clear =
  empty update), existence only via the toc, `sq share` is a daemon,
  not a registration step.
- `ravel_backend.py` — the seam (open / ingest_transcript /
  append_transcript / keep_recent / query_rows), the projection
  (project.rs / annotate.rs / herdr.rs semantics), the QuadSource glue
  to the lattice evaluator, the fixed internal queries, the memory
  views, and the memory_relive port (`verify`).
- `optmem.py` — the optmem tree math (cover / pending / check /
  parse_block), ported from memory.rs.
- `server.py` — the 14 daemon routes over the seam.
- `ontology/` — ravel's two shipped ontologies, read at runtime to
  derive the evaluator's preparation (equivalent properties, subclass
  fan-out), as calamus does from the same files.
- The evaluator itself is **not here**: it is a phext-lattice module
  (`src/sparql.rs`, PR https://github.com/wbic16/phext-lattice/pull/99),
  driven through its `phext-sparql` binary's line protocol
  (`PHEXT_SPARQL_BIN` locates the binary).

## Classification (per the study)

- **A (canonical, in the phext):** turn spine fields, annotations and
  their derived claims, the memory log (notes + summaries), graph
  descriptions and the recent-500 membership (the last two derived by
  the glue at query time — C2's default: no materialized index).
- **B (derived, disposable):** ingest receipts (Collection 3), the
  generated N-Quads the evaluator reads, the stats cache.
- **C (native format, untouched):** transcript mirror bytes, claude.ai
  exports, the ontology TTLs, config — all above the seam.
- **D (refused):** identity/secrets, store internals, live LLM writer
  calls — none enter the phext.

## Coordinate map, as built

Coordinates are `Collection.Volume.Book/Chapter.Section.Scroll/1.1.1`.

- **C1 V1 B1 Ch1 Sec1** — the manifest scroll: format name + version,
  source commit, the soul's root and user, the registry's location.
  **Ch1 Sec2** — the transcript registry: one scroll per transcript,
  scroll = slot + 1, content = transcript_id. (The study's fixed
  points, honoured literally: manifest `1.1.1/1.1.1/1.1.1`, registry
  `1.1.1/1.2.1/1.1.1` — they share volume 1; transcript volumes start
  at 2.)
- **C1 V(slot+2) B(position+1)** — one turn per book. Ch1 identity:
  Sec1 event_id, Sec2 role, Sec3 author (derived at ingest, with the
  ingesting machine's owner — never re-derived at read time), Sec4
  modelSetting, Sec5–9 the herdr claims (sender, pane, session, soul,
  user; present fields only). Ch2 timestamp. Ch3 text (one scroll;
  chunking deferred — see limits). Ch4 spine: Sec1 the parent turn's
  identity-scroll coordinate (a fast path), Sec2 the parent event_id
  (the identity the glue actually resolves by). Ch5 annotations:
  **Section = finding slot** (a refinement of the study draft's
  reader-slot sketch: a finding's fields must be individually
  addressable, and the reader is one of its fields) — scrolls 1 reader,
  2 event_type, 3 start, 4 end, 5 origin tag, 6 ts, 7+ one scroll per
  signal leg (`key\nkind\nlexical`, legs sorted by key). Ch6 claims:
  reserved, never stored — a claim is a pure function of its finding
  (the ann_hash), derived by the glue.
- **C2 V1 B(note+1)** — one memory note: Ch1 Sec1 id, Sec2 ts, Sec3
  text, Sec4 model; Ch2 one scroll per cited turn holding its
  Collection-1 identity coordinate (or `id:<turn_id>` when the turn is
  not in the store). **C2 V2 B(lo+1) Ch(span)** — one summary block:
  Sec1 text, Sec2 model, Sec3 written, Sec4 wording when present. A
  block's halves are derived from (lo, span) at projection, never
  stored.
- **C3 V(transcript volume) B1** — ingest receipts: Sec1 turn count,
  Sec2 annotation count, Sec3 quad count. Derived; rewritten at
  ingest.

## Running it

```
PHEXT_SPARQL_BIN=<path to phext-sparql> \
RAVEL_PHEXT_DATA=<data dir> RAVEL_PORT=7881 RAVEL_SQ_PORT=18991 \
python3 server.py
```

Souls are registered in `<data dir>/souls.json`
(`[{"org": ..., "store": ..., "root"?, "user"?, "machine_owner"?}]`).
`/sync` and `/import` take seam-level payloads — `{"transcripts":
[{"id", "events", "annotations"}], "notes": [...], "summaries":
[...]}` — exactly what the native sync layer hands `graph.rs`.

## Measured costs (backend-self, this VM, 2026-11-23)

Corpus: 2 souls, 10 turns, 4 annotations, 4 notes, 3 summaries,
233 quads. Ingest (2 transcripts, 8 turns + 4 annotations) 0.06–0.15 s;
point lookup 0.011 s; 1-hop traversal 0.009 s; 3-hop parent walk
0.017 s; emojikey claim query 0.18–0.28 s; stats battery (9 evaluator
calls) 0.09–0.11 s; store size 2,001 B of phext against 37,607 B of
native-format equivalent (N-Triples + JSONL) for the same soul.
Native-backend timings do not exist (ravel's Rust build is blocked
fleet-wide on repolex-ai git access); see V1-REPORT.md.

## Honest limits

- **Empty-string fields are not representable.** SQ's toc enumerates
  non-empty scrolls only, so a field stored empty is indistinguishable
  from an absent field. Measured divergence: an event with `text: ""`
  projects one `text ""` triple natively; here it projects none, and
  reads back as no-text. Every other field roundtrips exactly.
- **The tessera gate is not implemented.** The server binds
  127.0.0.1 and answers without a token; native reads currently need
  no token either, but a sent token is not validated here.
- **/sync and /import are seam-level.** The dialect adapters (Claude
  Code / Antigravity / claude.ai export parsing) live above the seam
  in the native stack and are not re-implemented; the routes accept
  the events/annotations those adapters produce.
- **Soul /health is store-level**, not sync::diagnose parity (the
  native checks run over the mirror, which is above the seam).
- **verify()'s cited-turn check** runs against ingested turn ids, not
  the native extract stream (which is also above the seam).
- **Text is one scroll per turn** (the study's chunking is deferred);
  very long turns make very long scrolls.
- **CONSTRUCT/DESCRIBE are answered** (the lattice fringe), where the
  core subset spec refuses them — the one place this backend is wider
  than the ruled subset, because the routes advertise them.
- Query seats: 4 concurrent per soul, evaluator timeout 30 s (the
  daemon's policy, kept backend-side).
