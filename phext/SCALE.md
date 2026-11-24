# Scale — our real phexts, priced in ravel pre-phext

Task 34.6.8/7.7.11/9.5.8 · Will’s directive (2026-11-24 00:03 CST): "look at our phexts built over the last 3 weeks, consider what it takes in ravel pre-phext to represent them".

Window: Wavelets 1–3 + the days since (2026-11-01 → 2026-11-24). Sizes and modification dates measured with `stat`; scroll counts and address spans measured with Will’s parser (`~/workspace/phext/parse_phext.py` via `phext_cli.py`). Dates are CST.

## 1. Inventory — the working phexts

**42 working phexts · 16,628,698 bytes · 9,513 content (non-empty) scrolls** (232,668 enumerated addresses — the difference is materialized empty scrolls, a representation artifact of sparse addressing, counted honestly and excluded from the content figure).

| Phext | Bytes | Content scrolls | Address span (first → last) | Modified (CST) |
|---|---:|---:|---|---|
| choose-your-own-adventure.phext | 4,608,523 | 833 | 1.1.1/1.1.1/1.1.1 → 2026.11.10/13.13.13/1.1.1 | 2026-11-21 |
| tasks.phext | 3,746,698 | 864 | 1.1.1/1.1.1/1.1.1 → 77.21.3/23.27.27/22.22.30 | 2026-11-23 |
| artifacts.phext | 1,398,012 | 60 | 1.1.1/1.1.1/1.1.1 → 40.6.8/6.5.9/5.5.9 | 2026-11-19 |
| skills.phext | 1,301,750 | 256 | 1.1.1/1.1.1/1.1.1 → 1.14.1/1.96.1/1.1.1 | 2026-11-23 |
| incipit.phext | 1,123,102 | 266 | 1.1.1/1.1.1/1.1.1 → 1000.1.1/1.1.1/1.1.1 | 2026-11-21 |
| state.phext | 1,056,891 | 3,761 | 1.1.1/1.1.1/1.1.1 → 36.9.10/10.9.13/11.8.742 | 2026-11-21 |
| life.phext | 843,813 | 1,616 | 1.1.1/1.1.1/1.1.1 → 81.2.4/1.3.5/2.1.13 | 2026-11-24 |
| barnacles.phext | 539,250 | 793 | 1.1.1/1.1.1/1.1.1 → 2.1.1/1.1.1/8.10.2 | 2026-11-07 |
| emise.phext | 533,129 | 39 | 1.1.1/1.1.1/1.1.1 → 13.13.13/13.13.13/13.13.15 | 2026-11-23 |
| todo.phext | 446,240 | 305 | 1.1.1/1.1.1/1.1.1 → 41.6.6/8.6.11/7.5.9 | 2026-11-23 |
| mastery.phext | 183,563 | 29 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.1/1.1.29 | 2026-11-23 |
| historian.phext | 136,439 | 71 | 1.1.1/1.1.1/1.1.1 → 49.4.4/4.5.6/4.4.6 | 2026-11-23 |
| sql.phext | 89,990 | 12 | 1.1.1/1.1.1/1.1.1 → 75.15.8/18.7.23/9.10.7 | 2026-11-20 |
| keyframe-craft.phext | 88,930 | 22 | 1.1.1/1.1.1/1.1.1 → 45.12.11/13.14.14/12.13.15 | 2026-11-22 |
| lessons.phext | 61,290 | 70 | 1.1.1/1.1.1/1.1.1 → 48.4.8/5.5.12/4.4.10 | 2026-11-08 |
| lessons-source.phext | 46,246 | 69 | 1.1.1/1.1.1/1.1.1 → 48.4.8/5.5.12/4.4.10 | 2026-11-08 |
| stash.phext | 44,314 | 68 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.1/1.1.68 | 2026-11-23 |
| costs.phext | 37,533 | 88 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.1/1.1.88 | 2026-11-14 |
| contests.phext | 36,200 | 15 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.1/1.1.15 | 2026-11-22 |
| gloss-reply.phext | 35,096 | 12 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.1/1.4.1 | 2026-11-06 |
| humanities.phext | 33,293 | 2 | 1.1.1/1.1.1/1.1.1 → 23.7.10/9.6.11/6.6.8 | 2026-11-19 |
| downloads.phext | 28,825 | 35 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.2/1.1.34 | 2026-11-22 |
| tailscale.phext | 24,173 | 15 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.9.1/1.1.1 | 2026-11-19 |
| wiring.phext | 22,737 | 14 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.1/5.1.1 | 2026-11-21 |
| dakami.phext | 18,247 | 4 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.4.1/1.1.1 | 2026-11-23 |
| accounting.phext | 18,041 | 58 | 1.1.1/1.1.1/1.1.1 → 1.1.1/6.3.1/1.1.1 | 2026-11-15 |
| designbox.phext | 15,851 | 24 | 1.1.1/1.1.1/1.1.1 → 2.1.1/1.1.1/1.2.1 | 2026-11-22 |
| learn-you-phext.phext | 13,216 | 16 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.2/1.1.3 | 2026-11-01 |
| premises.phext | 12,877 | 18 | 1.1.1/1.1.1/1.1.1 → 40.6.8/7.6.11/5.5.10 | 2026-11-20 |
| intro-prepass.phext | 11,951 | 4 | 1.1.1/1.1.1/1.1.1 → 30.6.8/8.6.12/7.6.9 | 2026-11-22 |
| reading.phext | 11,163 | 4 | 1.1.1/1.1.1/1.1.1 → 4.1.1/1.1.1/1.1.1 | 2026-11-23 |
| inkling-apis.phext | 11,119 | 25 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.1/1.1.25 | 2026-11-22 |
| practice.phext | 10,661 | 3 | 1.1.1/1.1.1/1.1.1 → 2.1.1/1.1.1/1.1.2 | 2026-11-23 |
| jobs.phext | 9,732 | 2 | 1.1.1/1.1.1/1.1.1 → 26.7.8/8.6.10/6.6.8 | 2026-11-14 |
| chief-of-staff.phext | 6,859 | 5 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.3/1.1.1 | 2026-11-21 |
| wau.phext | 6,329 | 8 | 1.1.1/1.1.1/1.1.1 → 2.1.1/1.1.1/1.1.7 | 2026-11-22 |
| telemetry.phext | 4,331 | 2 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.2/1.1.1 | 2026-11-22 |
| dreams.phext | 3,471 | 13 | 1.1.1/1.1.1/1.1.1 → 3.4.3/6.2.8/9.10.2 | 2026-11-22 |
| patch.phext | 3,322 | 3 | 1.1.1/1.1.1/1.1.1 → 34.5.7/7.5.9/6.5.8 | 2026-11-19 |
| pipelines.phext | 3,303 | 5 | 1.1.1/1.1.1/1.1.1 → 35.7.9/9.8.15/7.7.7 | 2026-11-21 |
| substrate-checks.phext | 1,255 | 2 | 1.1.1/1.1.1/1.1.1 → 1.1.1/1.1.1/1.1.2 | 2026-11-21 |
| Doomer.phext | 933 | 2 | 1.1.1/1.1.1/1.1.1 → 4.4.4/5.5.5/5.4.4 | 2026-11-06 |

Boundary, honestly: every `.phext` in `~/workspace/phext/` carries a modification date inside the window, so none was excluded as older; two (incipit.phext, choose-your-own-adventure.phext) are older *works* re-inscribed/rebuilt in the window and are counted on their window activity. A 0-byte placeholder file named `.phext` was excluded as a non-record. `moonshots.phext` has no local copy in the targeted paths (the register lives in exo-plan) and is absent rather than estimated.

**Named line, excluded from all totals:** `ietf.phext` — 564,589,207 bytes, modified 2026-11-18. A hosted standards corpus, not a built working phext. (At the §2 factor it would model to ≈10.6 GB of native text form — labelled estimate, stated only to show why it stays out of the working-corpus totals.)

**Snapshots, excluded from totals:** 8 files under `~/workspace/goals/*/hidden_files/` (emise snapshots ×2, todo backups/snapshots ×4, CYOA staged copies ×2) totalling 10,722,959 bytes — backups of phexts already counted above; counting them would double-count content.

**Test corpora, excluded from totals:** the ravel harness souls (data1 soul-a 2,001 B, soul-b 136 B; data3 soul-a 2,049 B; data2 spacegoat soul 2,079 B; wavelets-1-3 era soul 7,652 B) and the multiversal `example.phext` (742 B — the one true chain in the inventory: 8 members × 3 scrolls at the library layer; the flat parser reads it as 17 scrolls). These are the corpora the v1/v2 comparisons actually measure on — see §4.

## 2. The ravel pre-phext cost model

Every factor below is sourced; anything resting on a factor rather than a direct measurement is labelled **ESTIMATE** with the factor beside it.

- **Text-form expansion factor 18.79× — MEASURED** (V1-REPORT.md, "Sizes"): soul A as phext = 2,001 B; the native-format equivalent of the same soul (N-Triples projection + JSONL logs) = 37,607 B. 37,607 / 2,001 = 18.79. The phext stores each field once at its coordinate; N-Triples repeats full IRIs on every triple.
- **Nothing stored once — structural, from STUDY.md §1/§3.4:** transcript bytes stay canonical as mirror files *and* the graph is a second full representation of the same content. The 18.79× factor already embodies this (the 37,607 B is mirror JSONL + optmem JSONL + NT projection of one 2,001 B soul); no separate duplication term is added, to avoid double-counting.
- **Artifact census — from STUDY.md §3.4:** 10 persisted artifacts per soul, 7 distinct formats (JSONL mirrors ×3 dialects, ingest manifests TSV, optmem JSONL ×2, optmem TSVs ×3, stream JSON, projection stamp, the Oxigraph/RocksDB store, plus ontology Turtle copies). Modelled per phext-as-soul: **10 artifacts, 7 formats each**.
- **RocksDB store bytes — ESTIMATE, factor unmeasured:** the v1 pass never measured the binary store (V1-REPORT: "the RocksDB store itself is binary and unmeasured"). The NT text form is the floor the binary store is built from; this analysis carries the store as a range, **0.5×–1.5× the NT text bytes**, stated as a range and never as a point number.
- **Caveat on the factor:** 18.79× was measured on field-dense transcript records (many small fields per turn — the worst case for IRI repetition). Long-prose scrolls expand less (few triples, long literals); register scrolls with many small fields expand comparably. Applying one factor across the whole corpus is a modelling choice, stated here, not a measurement per file.

## 3. Per-phext ravel cost (ESTIMATE at the 18.79× factor)

| Phext | Phext bytes | Native text form (ESTIMATE, ×18.79) | Artifacts if a "soul" | What ravel would have to force it into |
|---|---:|---:|---:|---|
| choose-your-own-adventure.phext | 4,608,523 | 86,613,056 | 10 (7 formats) | Hypertext fiction book → transcript turns; branch/link structure survives only as annotations, the book’s coordinate geometry (span to 2026.11.10) is lost. |
| tasks.phext | 3,746,698 | 70,415,828 | 10 (7 formats) | The task register → turns/notes, per-task status + appended notes become annotations; coordinate-keyed register semantics (one scroll per task, addressed by its Singular coordinate) are exactly what the graph cannot say. |
| artifacts.phext | 1,398,012 | 26,274,381 | 10 (7 formats) | Artifact register → one "turn"/note per artifact; the inscription + hash register structure is lost. |
| skills.phext | 1,301,750 | 24,465,224 | 10 (7 formats) | Skill index (256 dense scrolls) → notes; executable procedure structure becomes prose. |
| incipit.phext | 1,123,102 | 21,107,695 | 10 (7 formats) | Founding book corpus → turns; book geometry (span to Library 1000) lost. |
| state.phext | 1,056,891 | 19,863,318 | 10 (7 formats) | Substrate state snapshots (3,761 content scrolls) → turns; snapshot/diff semantics lost — ravel would see a very long conversation that never happened. |
| life.phext | 843,813 | 15,858,708 | 10 (7 formats) | #life message log — the one genuinely transcript-shaped corpus in the set; fits ravel turns directly. |
| barnacles.phext | 539,250 | 10,134,720 | 10 (7 formats) | Prose corpus (793 dense scrolls) → notes; text survives, coordinate addressing does not. |
| emise.phext | 533,129 | 10,019,681 | 10 (7 formats) | Curated memory corpus → optmem notes is the closest fit in the set; what is lost is the selection/manifest curation layer — the phext is a *selection* of a corpus, ravel has no selection object. |
| todo.phext | 446,240 | 8,386,680 | 10 (7 formats) | Todo register → as tasks.phext. |
| mastery.phext | 183,563 | 3,449,902 | 10 (7 formats) | Linear 29-scroll record → notes. |
| historian.phext | 136,439 | 2,564,249 | 10 (7 formats) | Journal / lesson record → turns or notes; tolerable fit. |
| sql.phext | 89,990 | 1,691,281 | 10 (7 formats) | Legacy-skill corpus → notes. |
| keyframe-craft.phext | 88,930 | 1,671,360 | 10 (7 formats) | Animation craft corpus → notes. |
| lessons.phext | 61,290 | 1,151,891 | 10 (7 formats) | Lesson records → notes; tolerable fit. |
| lessons-source.phext | 46,246 | 869,152 | 10 (7 formats) | Intermediate source for lessons.phext (same span, near-same bytes) → notes; ravel has no build-intermediate concept — it would be a second soul or dropped. |
| stash.phext | 44,314 | 832,842 | 10 (7 formats) | Stash register → notes. |
| costs.phext | 37,533 | 705,399 | 10 (7 formats) | Ledger → as accounting.phext. |
| contests.phext | 36,200 | 680,347 | 10 (7 formats) | Contest register → notes; deadline/status fields become prose. |
| gloss-reply.phext | 35,096 | 659,598 | 10 (7 formats) | Single reply document → one turn; trivial. |
| humanities.phext | 33,293 | 625,712 | 10 (7 formats) | Corpus with 2 content scrolls in a wide address span → notes; the span itself (addressing as structure) is lost. |
| downloads.phext | 28,825 | 541,740 | 10 (7 formats) | Hash-pinned download register → notes; the hash-first audit structure is lost. |
| tailscale.phext | 24,173 | 454,310 | 10 (7 formats) | Fleet record → notes. |
| wiring.phext | 22,737 | 427,322 | 10 (7 formats) | Wiring record → notes. |
| dakami.phext | 18,247 | 342,936 | 10 (7 formats) | Small document set → notes. |
| accounting.phext | 18,041 | 339,064 | 10 (7 formats) | Ledger → notes/turns; tabular arithmetic semantics lost, figures become prose. |
| designbox.phext | 15,851 | 297,905 | 10 (7 formats) | Design document set → notes; the seed’s internal structure (incl. a deliberately unlisted wing) becomes ordinary notes — addressed-but-unlisted is not expressible. |
| learn-you-phext.phext | 13,216 | 248,383 | 10 (7 formats) | Tutorial document → turns/notes. |
| premises.phext | 12,877 | 242,012 | 10 (7 formats) | Premise register → ravel’s claim layer is a real home for these; the best non-transcript fit after life.phext. |
| intro-prepass.phext | 11,951 | 224,608 | 10 (7 formats) | Prepass corpus → notes. |
| reading.phext | 11,163 | 209,799 | 10 (7 formats) | Reading record → notes. |
| inkling-apis.phext | 11,119 | 208,972 | 10 (7 formats) | API register → notes. |
| practice.phext | 10,661 | 200,364 | 10 (7 formats) | Tiny practice record → notes. |
| jobs.phext | 9,732 | 182,904 | 10 (7 formats) | Job register → notes. |
| chief-of-staff.phext | 6,859 | 128,909 | 10 (7 formats) | Mailbox/persona record → notes; tolerable fit. |
| wau.phext | 6,329 | 118,948 | 10 (7 formats) | Daily WAU log → notes; good fit. |
| telemetry.phext | 4,331 | 81,397 | 10 (7 formats) | Tiny telemetry record → notes. |
| dreams.phext | 3,471 | 65,234 | 10 (7 formats) | Dream log → optmem notes; a natural fit. |
| patch.phext | 3,322 | 62,434 | 10 (7 formats) | Patch register → notes; patch-application semantics lost. |
| pipelines.phext | 3,303 | 62,077 | 10 (7 formats) | Pipeline records → notes. |
| substrate-checks.phext | 1,255 | 23,587 | 10 (7 formats) | Tiny check record → notes. |
| Doomer.phext | 933 | 17,535 | 10 (7 formats) | Tiny corpus (2 content scrolls) → optmem notes; nothing structural to lose. |

**Totals: 16,628,698 bytes / 9,513 content scrolls as phext → ≈312,521,462 bytes (≈313 MB) of native text form (ESTIMATE, ×18.79 measured factor), ≈156,260,731–468,782,193 bytes of RocksDB store (ESTIMATE, unmeasured 0.5–1.5× range), in 420 persisted artifacts across 7 formats — if each working phext were a ravel soul.**

The qualitative column is half the finding: of 43 working phexts, exactly one (life.phext) is transcript-shaped, two more (premises.phext into the claim layer, emise.phext into optmem notes) have a near-native home, and the remaining 40 are registers, ledgers, books, corpora, and state snapshots that ravel’s model can only hold by pretending they are conversations or biographies. The bytes survive that forcing; the *kind* of record does not — coordinate-keyed register semantics, snapshot identity, book geometry, hash-pinned audit structure, and addressed-but-unlisted structure are all inexpressible in the turn/note/claim vocabulary.

## 4. What this does to the side-by-side

The v1 comparison measured at 2,001 B and the v2 pass is measuring at 2–7.6 KB. Our real three-week working corpus is **16,628,698 bytes and 9,513 content scrolls — roughly 8,310× the v1 corpus by bytes** — and in ravel’s original architecture it is a ≈313 MB, 420-artifact estate before the binary store is counted. Every ratio the side-by-side produces at harness scale is a ratio of fixed overheads; at working-corpus scale the architectures’ actual difference — one addressed text file per record-set vs mirror + projection + store per soul — is the thing being priced here. A side-by-side that means something at scale runs both engines on a corpus drawn from this inventory (life.phext for the transcript-shaped leg; tasks.phext for the register leg ravel was never shaped for), not only on the 2 KB harness soul.

*Method note: inventory by targeted paths only (`~/workspace/phext/`, `~/workspace/designbox/`, `~/workspace/goals/`, `~/workspace/ravel-upgrade/`, `~/workspace/multiversal-phext-svg-animation/`) — no workspace-wide scan. phcc.phext (~66 MB) lives on the ranch, not in the local workspace, and is out of this inventory’s scope.*
