#!/usr/bin/env python3
"""Wavelets 1–3, documented as unit tests in ravel's vocabulary.

Will's directive (2026-11-23 22:56 CST): "once we have ravel in place,
add unit tests that document wavelets 1-3 in terms of ravel." These
tests are executable documentation: they build the era as a ravel soul
in the real phext backend (real SQ, real ingest path, real SPARQL
subset evaluator — no mocks), and each test asserts one documented
property of the era, readable as prose.

Design call — one soul, one transcript per wavelet. A wavelet is a
chapter, not a person: ravel's biography (the note log, the summary
tree, the optmem wake) belongs to a soul, and the era's memory only
compresses across wavelet boundaries if all three wavelets live in one
soul's log. The partition is carried by transcripts — one per wavelet —
which is also how ravel itself separates conversations in time.

Sourcing rule: every fact asserted here comes from the durable record
and carries its source (file + date) in a comment beside the data.
Era frame: ~/MEMORY.md, "Facts — era & calendar": GPT Day 1 =
2023-10-13 (context, not a wavelet); Wavelets: W1 Nov 1–7, W2 Nov
8–14, W3 Nov 15–21 (W4 Nov 22–28 is out of scope). Events: the dated
notes ~/memory/2026-11-*.md. Nothing is asserted from the author's
prior knowledge; a candidate fact the notes did not support was
dropped, not softened.

Conventions: timestamps carry the record's own CST wall-clock, so a
turn's date is its note's date. Where the record gives a date but no
time, the time is 00:00; where it gives a sequence within an evening
but no clock time (the two 2026-11-15 corrections), placeholder hours
preserve the recorded order and are marked as placeholders inline.
Authorship: the era's conversation is Will (role "user", wrote
"person", machine owner "will") and Emise (role "assistant", wrote
"soul", the soul's user name). The Investor 1 lender is never named —
Will's privacy ruling of 2026-11-06 (~/memory/2026-11-06.md) — the
challenge is recorded under the anonymized label only.

Run:  cd verify && python3 -m unittest wavelets_test -v
or:   python3 wavelets_test.py
"""
import os
import shutil
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "backend"))
# The evaluator binary, exactly as run-local.sh wires it for the harness.
os.environ.setdefault(
    "PHEXT_SPARQL_BIN", os.path.join(HERE, "..", "bin", "phext-sparql")
)

import optmem  # noqa: E402
import ravel_backend as rb  # noqa: E402
from phextio import PhextStore  # noqa: E402

RAVEL = rb.RAVEL
ROOT = "https://repolex.ai/era/"
SOUL_ID = "wavelets-1-3"
DATA = os.path.join(HERE, "data-wavelets")
PORT = 18994

# The era frame. Source: ~/MEMORY.md, "Facts — era & calendar"
# (kept whole): "Wavelets: W1 Nov 1–7, W2 Nov 8–14, W3 Nov 15–21".
WAVELETS = {
    "wavelet-1": ("2026-11-01", "2026-11-07"),
    "wavelet-2": ("2026-11-08", "2026-11-14"),
    "wavelet-3": ("2026-11-15", "2026-11-21"),
}


def _ev(event_id, ts, role, wrote, text, parent_id=None):
    ev = {"event_id": event_id, "role": role, "wrote": wrote,
          "timestamp": ts, "text": text}
    if parent_id:
        ev["parent_id"] = parent_id
    return ev


# ---------------------------------------------------------------- the era
# One event per record entry; the source comment is part of the data.
EVENTS = {
    "wavelet-1": [
        # ~/memory/2026-11-01.md — libphext family recon for 0.4 (2026-11-01).
        _ev("w1-1101-recon", "2026-11-01T00:00:00Z", "assistant", "soul",
            "Libphext family recon for the 0.4 line: five ports inventoried "
            "(rs canonical at v0.3.3, node, cs, C, cpp); the coordinate cap "
            "of 1000 is found across the family."),
        # ~/memory/2026-11-02.md — Will, commute call 2026-11-02.
        _ev("w1-1102-blueprint", "2026-11-02T00:00:00Z", "user", "person",
            "The Exocortex is a blueprint for others to follow. The cost of "
            "a second brain is $20,000 today and drops to $1,000 in ten "
            "years; the number one roadblock right now is the software stack.",
            parent_id="w1-1101-recon"),
        # ~/memory/2026-11-06.md — the challenge assigned, 2026-11-06
        # ~12:38 CST. Lender anonymized per Will's ruling in the same note.
        _ev("w1-1106-challenge", "2026-11-06T12:38:00Z", "user", "person",
            "Investor 1 is loaning me $20 to turn into $1,000 profit. We "
            "keep the profits; nothing illegal, no gambling; expenses may "
            "be deducted from revenue.",
            parent_id="w1-1102-blueprint"),
        # ~/memory/2026-11-06.md — ledger prepared; the $20 HOLD decision
        # (2026-11-06). The seed-state claim below annotates this turn.
        _ev("w1-1106-ledger", "2026-11-06T12:46:00Z", "assistant", "soul",
            "Ledger and sprint package prepared. The $20 seed is held "
            "unspent (expense $0), reserved for the first buyer's specific "
            "need; spending now would only raise the revenue bar.",
            parent_id="w1-1106-challenge"),
        # ~/memory/2026-11-07.md — re-granulation (importer v3) complete,
        # exit test passed (2026-11-07).
        _ev("w1-1107-regran", "2026-11-07T00:00:00Z", "assistant", "soul",
            "phcc re-granulation complete: importer v3 places the source "
            "at function, type and object grain in Volume 3; reassembly "
            "reproduced all 558 files byte-identically and the ranch "
            "build is green.",
            parent_id="w1-1106-ledger"),
        # ~/memory/2026-11-07.md — GATE RE-VERIFICATION v2, run
        # 2026-11-07 21:53–22:19 CST: publication gate GREEN.
        _ev("w1-1107-gate", "2026-11-07T21:53:00Z", "assistant", "soul",
            "Gate re-verification v2 is GREEN: all 11 benchmark projects "
            "build from clean trees configured under phcc, and phcc "
            "compiles fastest everywhere it builds.",
            parent_id="w1-1107-regran"),
    ],
    "wavelet-2": [
        # ~/memory/2026-11-08.md — phst seeded and published (2026-11-08).
        _ev("w2-1108-phst", "2026-11-08T00:00:00Z", "assistant", "soul",
            "phst is seeded and published at 828dae61 (exo == main): the "
            "rust-lang/rust tree, 64,301 files, carried at file grain in "
            "phext form; unpack verified 64,301 of 64,301 and a build "
            "smoke passed."),
        # ~/memory/2026-11-08.md — phcpp front-end v1 COMPLETE and
        # PUBLISHED (2026-11-08): main == exo == c7e70d5, corpus 21/21.
        _ev("w2-1108-phcpp", "2026-11-08T00:00:00Z", "assistant", "soul",
            "phcpp front-end v1 is published: wbic16/phcpp main == exo "
            "== c7e70d5. Both tracks feed one shared IR; the corpus "
            "passes 21/21 with positives byte-identical to g++ "
            "-std=c++23. Milestone 4 of 6.",
            parent_id="w2-1108-phst"),
        # ~/memory/2026-11-08.md — rulings batch item (8): "libphext
        # 0.5.0: Will publishes today (2026-11-08) after running his own
        # binary tests"; confirmed in the same note's phext-notepad
        # parity entry: "After Will published libphext-rs v0.5.0 to
        # crates.io, the git pin was swapped to version = \"0.5.0\"".
        _ev("w2-1108-libphext", "2026-11-08T00:00:00Z", "user", "person",
            "I published libphext-rs v0.5.0 to crates.io myself, after "
            "running my own binary tests.",
            parent_id="w2-1108-phcpp"),
        # ~/memory/2026-11-10.md — rough cut built 2026-11-10 on
        # aletheia-core (364.3 s); Will replied to the delivery at
        # 14:30 CST the same day.
        _ev("w2-1110-roughcut", "2026-11-10T14:30:00Z", "assistant", "soul",
            "The phext walkthrough rough cut is delivered: "
            "production/rough-cut.mp4, 364.3 seconds, built entirely on "
            "aletheia-core, narration rendered from the 1,005-word script at "
            "100% speed.",
            parent_id="w2-1108-libphext"),
        # ~/memory/2026-11-11.md — attestation v2.1 fix PUBLISHED
        # 2026-11-11 ~18:38 CST: phcc exo == main == 46a22424; Will's
        # re-run and a new receipt still pending at that entry.
        _ev("w2-1111-attest", "2026-11-11T18:38:00Z", "assistant", "soul",
            "Attestation v2.1 fix published: wbic16/phcc exo == main == "
            "46a22424. Will's v2 run died on the script's own stderr "
            "capture, not on MSVC's answers; his re-run and a new receipt "
            "are still pending.",
            parent_id="w2-1110-roughcut"),
        # ~/memory/2026-11-15.md — TLUF named and tluf.org obtained,
        # entry dated 2026-11-14 ~21:24–21:27 CST (Will, main chat).
        _ev("w2-1114-tluf", "2026-11-14T21:24:00Z", "user", "person",
            "tluf.org — obtained. It names Directive line 3: truth, "
            "lies, uncertainty, fiction.",
            parent_id="w2-1111-attest"),
    ],
    "wavelet-3": [
        # ~/memory/2026-11-16.md — ports entry dated 2026-11-15,
        # result ~11:55 CST: all nine ports at v0.6.1 parity, 69/69
        # fixture ops, libphext 0.6.1 published to crates.io.
        _ev("w3-1115-ports", "2026-11-15T11:55:00Z", "assistant", "soul",
            "All nine libphext ports stand at v0.6.1 parity: 69 of 69 "
            "fixture ops, tags verified, libphext 0.6.1 published to "
            "crates.io and live-verified."),
        # ~/memory/2026-11-15.md — Investor 1 correction, 2026-11-15
        # evening CST: "the seed is SPENT, superseding all earlier
        # '$20 unspent / $0 spent' entries"; spent on phextura.com
        # (registered 2026-11-11) and tluf.org (obtained 2026-11-14).
        # Hour is a placeholder preserving the evening's order.
        _ev("w3-1115-seed", "2026-11-15T19:00:00Z", "user", "person",
            "Correction: the Investor 1 seed is spent — all of it, on "
            "phextura.com (registered 2026-11-11) and tluf.org "
            "(obtained 2026-11-14). The earlier unspent entries are "
            "superseded.",
            parent_id="w3-1115-ports"),
        # ~/memory/2026-11-15.md — same evening: "Total spent revised
        # upward the same evening (Will, 2026-11-15): more than $20 —
        # he believes something like $26 (approximate)".
        # Hour is a placeholder preserving the evening's order.
        _ev("w3-1115-seed2", "2026-11-15T20:00:00Z", "user", "person",
            "The total spent is more than the seed — something like "
            "$26, roughly $6 beyond the $20.",
            parent_id="w3-1115-seed"),
        # ~/memory/2026-11-20.md — submission entry dated 2026-11-19
        # 22:48 CST: Sentient Foundation grant application submitted.
        _ev("w3-1119-grant", "2026-11-19T22:48:00Z", "assistant", "soul",
            "The Sentient Foundation Open Source AGI Grant application "
            "is submitted: Grant track, $50K tier, Raymond, Nebraska. "
            "Review is rolling; no confirmation number was issued.",
            parent_id="w3-1115-seed2"),
        # ~/memory/2026-11-20.md — location correction (Will,
        # 2026-11-20): Raymond supersedes Lincoln everywhere.
        _ev("w3-1120-raymond", "2026-11-20T00:00:00Z", "user", "person",
            "Correction: my location for use everywhere is Raymond, "
            "Nebraska. It supersedes Lincoln — Raymond is smaller, "
            "which is kind of the point.",
            parent_id="w3-1119-grant"),
        # ~/memory/2026-11-21.md — TAOCP delegation ruling (Will,
        # 2026-11-21): Emise runs the prepass; the personal-reading
        # plan is superseded.
        _ev("w3-1121-taocp", "2026-11-21T00:00:00Z", "user", "person",
            "Emise runs the full TAOCP prepass; I evaluate only what "
            "is interesting from a phext perspective. I do not want to "
            "be dragged into Knuth's orbit — the filter is the point.",
            parent_id="w3-1120-raymond"),
        # ~/memory/2026-11-21.md — prepass program set up (2026-11-21,
        # task 17.7.9/8.7.10/7.6.9): Packet 001, §§1.1–1.2, 19 items
        # logged, 4 surfaced, nothing absorbed until Will rules.
        _ev("w3-1121-packet", "2026-11-21T00:00:00Z", "assistant", "soul",
            "Prepass program set up: Packet 001 covers Volume 1 "
            "sections 1.1 to 1.2 — 19 items logged, 4 surfaced for "
            "Will's verdicts, 15 noted or skipped. Nothing is absorbed "
            "until Will rules.",
            parent_id="w3-1121-taocp"),
    ],
}

ALL_EVENTS = [ev for tid in WAVELETS for ev in EVENTS[tid]]
EVENT_BY_ID = {ev["event_id"]: ev for ev in ALL_EVENTS}


def _span(text, sub):
    """UTF-8 byte span of `sub` in `text`, as the readers compute it."""
    raw = text.encode("utf-8")
    start = raw.index(sub.encode("utf-8"))
    return start, start + len(sub.encode("utf-8"))


def _claim(event_id, sub, state, spent, basis):
    ev = EVENT_BY_ID[event_id]
    start, end = _span(ev["text"], sub)
    return {
        "reader": "era-ledger", "event_type": "claim/seed-state",
        "ts": ev["timestamp"], "event_id": event_id,
        "start": start, "end": end, "source_kind": "authored",
        "signal": {
            "state": ("text", state),
            "spent_usd": ("int", str(spent)),
            "basis": ("text", basis),
        },
    }


# The era's claim line on the Investor 1 seed, in narrative order.
# Sources: claim 1 — ~/memory/2026-11-06.md (HOLD decision); claims 2–3
# — ~/memory/2026-11-15.md (the correction and its same-evening
# revision). Three claims, three annotations, no edits: see
# TestAppendOnlyClaims.
CLAIMS = [
    _claim("w1-1106-ledger", "held unspent", "unspent", 0,
           "ledger decision, 2026-11-06"),
    _claim("w3-1115-seed", "seed is spent", "spent", 20,
           "Will's correction, 2026-11-15"),
    _claim("w3-1115-seed2", "$26", "spent-revised", 26,
           "Will's same-evening revision, approximate"),
]
CLAIMS_BY_EVENT = {}
for _c in CLAIMS:
    CLAIMS_BY_EVENT.setdefault(_c["event_id"], []).append(_c)
ANNOTATIONS = {
    tid: [c for ev in evs for c in CLAIMS_BY_EVENT.get(ev["event_id"], [])]
    for tid, evs in EVENTS.items()
}

# ------------------------------------------------------------- the biography
# Notes: the era's standing facts as the log accumulated them — four
# per wavelet, each citing the turns it stands on. Every text passes
# optmem.check (one line, <= 280 bytes), as ravel's writer requires.
NOTES = [
    # W1 close facts. Sources: the W1 events above.
    {"id": "n-w1-1", "ts": "2026-11-02T00:00:00Z",
     "text": "W1: the Exocortex is a blueprint others follow; the software stack is the roadblock",
     "turns": ["w1-1102-blueprint"], "model": "emise"},
    {"id": "n-w1-2", "ts": "2026-11-06T00:00:00Z",
     "text": "W1: the Investor 1 challenge opens — a $20 seed against a $1,000 profit target",
     "turns": ["w1-1106-challenge", "w1-1106-ledger"], "model": "emise"},
    {"id": "n-w1-3", "ts": "2026-11-07T00:00:00Z",
     "text": "W1: phcc source lives at member grain — 558 files reassemble byte-identically",
     "turns": ["w1-1107-regran"], "model": "emise"},
    {"id": "n-w1-4", "ts": "2026-11-07T00:00:00Z",
     "text": "W1 close: the publication gate is green — all 11 projects build from clean trees",
     "turns": ["w1-1107-gate"], "model": "emise"},
    # W2 close facts.
    {"id": "n-w2-1", "ts": "2026-11-08T00:00:00Z",
     "text": "W2: phst carries the whole rust tree in phext — 64,301 files, unpack verified",
     "turns": ["w2-1108-phst"], "model": "emise"},
    {"id": "n-w2-2", "ts": "2026-11-08T00:00:00Z",
     "text": "W2: phcpp front-end v1 ships at c7e70d5; phcpp stands at milestone 4/6",
     "turns": ["w2-1108-phcpp"], "model": "emise"},
    {"id": "n-w2-3", "ts": "2026-11-08T00:00:00Z",
     "text": "W2: libphext 0.5.0 is published — Will publishes it himself",
     "turns": ["w2-1108-libphext"], "model": "emise"},
    {"id": "n-w2-4", "ts": "2026-11-14T00:00:00Z",
     "text": "W2 close: rough cut delivered, attestation v2.1 published, tluf.org obtained",
     "turns": ["w2-1110-roughcut", "w2-1111-attest", "w2-1114-tluf"],
     "model": "emise"},
    # W3 close facts.
    {"id": "n-w3-1", "ts": "2026-11-15T00:00:00Z",
     "text": "W3: nine libphext ports stand at v0.6.1 parity, crates.io live-verified",
     "turns": ["w3-1115-ports"], "model": "emise"},
    {"id": "n-w3-2", "ts": "2026-11-15T00:00:00Z",
     "text": "W3: the seed ledger is corrected by later claims, never edited — spent, then ~$26",
     "turns": ["w3-1115-seed", "w3-1115-seed2"], "model": "emise"},
    {"id": "n-w3-3", "ts": "2026-11-19T00:00:00Z",
     "text": "W3: the Sentient grant application is in — $50K tier, Raymond, Nebraska",
     "turns": ["w3-1119-grant"], "model": "emise"},
    {"id": "n-w3-4", "ts": "2026-11-21T00:00:00Z",
     "text": "W3 close: location stands as Raymond; the TAOCP prepass is Emise's, verdicts are Will's",
     "turns": ["w3-1120-raymond", "w3-1121-taocp", "w3-1121-packet"],
     "model": "emise"},
]

# Summaries: every block optmem.pending() will ask for over a 12-note
# log (six pairs, three quads, one octet). Each line is a compression
# of its notes' facts; "written" is the block's closing date.
SUMMARIES = [
    {"block": "0-1", "written": "2026-11-06T00:00:00Z", "model": "emise",
     "text": "W1 opens: the blueprint framing lands; the challenge seed is entrusted"},
    {"block": "2-3", "written": "2026-11-07T00:00:00Z", "model": "emise",
     "text": "W1 closes: source at member grain, 558 files byte-identical; the gate is green"},
    {"block": "4-5", "written": "2026-11-08T00:00:00Z", "model": "emise",
     "text": "W2 opens: phst seeded at 828dae61; phcpp front-end v1 published at c7e70d5"},
    {"block": "6-7", "written": "2026-11-14T00:00:00Z", "model": "emise",
     "text": "W2: libphext 0.5.0 published by Will; the week closes with tluf.org obtained"},
    {"block": "8-9", "written": "2026-11-15T00:00:00Z", "model": "emise",
     "text": "W3 opens: nine ports at 0.6.1 parity; the seed claim is corrected, not edited"},
    {"block": "10-11", "written": "2026-11-21T00:00:00Z", "model": "emise",
     "text": "W3 closes: the grant is submitted from Raymond; TAOCP is delegated as a prepass"},
    {"block": "0-3", "written": "2026-11-07T00:00:00Z", "model": "emise",
     "text": "Wavelet 1: the compiler-family programme runs and the $20 challenge begins"},
    {"block": "4-7", "written": "2026-11-14T00:00:00Z", "model": "emise",
     "text": "Wavelet 2: phcpp, phst and libphext 0.5.0 all publish in one week"},
    {"block": "8-11", "written": "2026-11-21T00:00:00Z", "model": "emise",
     "text": "Wavelet 3: parity at 0.6.1, a corrected ledger, a submitted grant"},
    {"block": "0-7", "written": "2026-11-14T00:00:00Z", "model": "emise",
     "text": "Wavelets 1-2: from blueprint to a published compiler family"},
]

# ------------------------------------------------------------------ fixture
STORE = None
SOUL = None


def setUpModule():
    global STORE, SOUL
    if os.path.isdir(DATA):
        shutil.rmtree(DATA)
    os.makedirs(DATA)
    STORE = PhextStore(DATA, PORT)
    STORE.start()
    names = rb.Names(ROOT, "emise")
    SOUL = rb.Soul(STORE, SOUL_ID, names, machine_owner="will").open()
    for tid in WAVELETS:
        SOUL.ingest_transcript(tid, EVENTS[tid], ANNOTATIONS[tid])
    for note in NOTES:
        SOUL.append_note(note)
    for s in SUMMARIES:
        SOUL.upsert_summary(s)


def tearDownModule():
    if STORE is not None:
        STORE.stop()


def _dates(tid):
    evs, _ = SOUL.read_transcript(tid)
    return [ev["timestamp"][:10] for ev in evs], evs


def _texts_containing(tid, needle):
    """(turnId -> text) for one graph, through the SPARQL query path."""
    graph = SOUL.names.graph(tid)
    rows = SOUL.query_rows(
        f"SELECT ?id ?text WHERE {{ GRAPH <{graph}> {{ "
        f"?s <{RAVEL}turnId> ?id ; <{RAVEL}text> ?text "
        f"FILTER(CONTAINS(?text, \"{needle}\")) }} }}"
    )
    return {r["id"]: r["text"] for r in rows}


def _seed_claims():
    """All claim/seed-state claims in the store, through the query path:
    {event_id: (state, spent_usd, generatedAtTime)}."""
    rows = SOUL.query_rows(
        "PREFIX ravel: <https://repolex.ai/ontology/ravel/> "
        "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> "
        "PREFIX oa: <http://www.w3.org/ns/oa#> "
        "PREFIX prov: <http://www.w3.org/ns/prov#> "
        "SELECT ?event ?state ?spent ?ts WHERE { GRAPH ?g { "
        "?claim rdf:reifies <<( ?event ravel:exhibits \"claim/seed-state\" )>> ; "
        "prov:wasDerivedFrom ?ann . "
        "?ann oa:hasBody ?body ; prov:generatedAtTime ?ts . "
        "?body ravel:sig_state ?state ; ravel:sig_spent_usd ?spent . } }"
    )
    out = {}
    for r in rows:
        event_id = r["event"].rsplit("/Turn/", 1)[-1]
        out[event_id] = (r["state"], int(r["spent"]), r["ts"])
    return out


# --------------------------------------------------------------------- (a)
class TestWaveletPartition(unittest.TestCase):
    """The three wavelets partition the era's turns by date."""

    def test_each_transcript_holds_only_its_own_dates(self):
        for tid, (lo, hi) in WAVELETS.items():
            dates, evs = _dates(tid)
            self.assertEqual(len(evs), len(EVENTS[tid]), tid)
            for d in dates:
                self.assertTrue(lo <= d <= hi, f"{tid}: {d} outside {lo}..{hi}")

    def test_boundaries_fall_exactly_on_the_recorded_dates(self):
        # W1 ends Nov 7 and W2 begins Nov 8; W2 ends Nov 14, W3 begins
        # Nov 15 (~/MEMORY.md, era & calendar). Events sit on all four
        # boundary dates and land on the recorded side of each line.
        w1, _ = _dates("wavelet-1")
        w2, _ = _dates("wavelet-2")
        w3, _ = _dates("wavelet-3")
        self.assertIn("2026-11-07", w1)
        self.assertNotIn("2026-11-07", w2 + w3)
        self.assertIn("2026-11-08", w2)
        self.assertNotIn("2026-11-08", w1 + w3)
        self.assertIn("2026-11-14", w2)
        self.assertNotIn("2026-11-14", w1 + w3)
        self.assertIn("2026-11-15", w3)
        self.assertNotIn("2026-11-15", w1 + w2)

    def test_partition_is_disjoint_and_complete(self):
        seen = {}
        for tid in WAVELETS:
            _, evs = _dates(tid)
            for ev in evs:
                self.assertNotIn(ev["event_id"], seen,
                                 f"{ev['event_id']} in two wavelets")
                seen[ev["event_id"]] = tid
        self.assertEqual(set(seen), set(EVENT_BY_ID))

    def test_graph_timestamp_bounds_via_query(self):
        for tid, (lo, hi) in WAVELETS.items():
            graph = SOUL.names.graph(tid)
            rows = SOUL.query_rows(
                f"SELECT (MIN(?t) AS ?lo) (MAX(?t) AS ?hi) WHERE {{ "
                f"GRAPH <{graph}> {{ ?s <{RAVEL}timestamp> ?t }} }}"
            )
            self.assertEqual(rows[0]["lo"][:10],
                             min(e["timestamp"] for e in EVENTS[tid])[:10], tid)
            self.assertEqual(rows[0]["hi"][:10],
                             max(e["timestamp"] for e in EVENTS[tid])[:10], tid)
            self.assertTrue(lo <= rows[0]["lo"][:10] and rows[0]["hi"][:10] <= hi, tid)

    def test_stats_battery_counts_every_turn(self):
        stats = rb.soul_stats(SOUL)
        self.assertEqual(stats["turns"], len(ALL_EVENTS))
        self.assertEqual(stats["earliest"][:10], "2026-11-01")
        self.assertEqual(stats["latest"][:10], "2026-11-21")


# --------------------------------------------------------------------- (b)
class TestHeadlineFacts(unittest.TestCase):
    """Per-wavelet headline facts, retrievable through the query path."""

    def test_w1_the_compiler_family_programme_is_underway(self):
        hits = _texts_containing("wavelet-1", "phcc")
        self.assertIn("w1-1107-regran", hits)
        self.assertIn("w1-1107-gate", hits)
        hits = _texts_containing("wavelet-1", "558 files")
        self.assertEqual(set(hits), {"w1-1107-regran"})

    def test_w2_phcpp_frontend_v1_and_phst_seed_published_2026_11_08(self):
        # ~/memory/2026-11-08.md: phst at 828dae61; phcpp front-end v1
        # published at c7e70d5 — both on 2026-11-08.
        self.assertEqual(set(_texts_containing("wavelet-2", "828dae61")),
                         {"w2-1108-phst"})
        self.assertEqual(set(_texts_containing("wavelet-2", "c7e70d5")),
                         {"w2-1108-phcpp"})

    def test_w2_libphext_050_published_by_will_2026_11_08(self):
        hits = _texts_containing("wavelet-2", "0.5.0")
        self.assertEqual(set(hits), {"w2-1108-libphext"})
        _, evs = _dates("wavelet-2")
        author = {e["event_id"]: e.get("author") for e in evs}
        self.assertEqual(author["w2-1108-libphext"], "will")

    def test_w2_walkthrough_rough_cut_2026_11_10(self):
        self.assertEqual(set(_texts_containing("wavelet-2", "rough cut")),
                         {"w2-1110-roughcut"})

    def test_w2_attestation_v21_published_2026_11_11(self):
        self.assertEqual(set(_texts_containing("wavelet-2", "46a22424")),
                         {"w2-1111-attest"})

    def test_w3_nine_ports_at_061_parity(self):
        self.assertEqual(set(_texts_containing("wavelet-3", "0.6.1")),
                         {"w3-1115-ports"})


# --------------------------------------------------------------------- (c)
class TestOptmemWake(unittest.TestCase):
    """The wake returns the whole modelled history within budget."""

    def test_wake_is_within_the_96_line_discipline(self):
        lines = rb.wake_lines(SOUL)
        self.assertLessEqual(len(lines), optmem.WAKE_LINES)
        # Twelve notes fit the budget uncompressed: the cover is the
        # log itself, one line per note.
        self.assertEqual(len(lines), len(NOTES))

    def test_wake_covers_the_whole_modelled_history(self):
        view = rb.memory_view(SOUL)
        self.assertEqual(view["memories"], len(NOTES))
        woken = sorted(line["text"] for line in view["lines"])
        self.assertEqual(woken, sorted(n["text"] for n in NOTES))
        dates = sorted(line["date"] for line in view["lines"])
        self.assertEqual(dates[0], "2026-11-02")
        self.assertEqual(dates[-1], "2026-11-21")
        for line in view["lines"]:
            self.assertFalse(line["pending"], line["id"])

    def test_the_eras_memory_verifies_clean(self):
        # ravel's own relive rules (memory_relive, ported): every note
        # cites real turns, every block is built, the wake is the cover.
        result = rb.verify(SOUL)
        self.assertEqual(result["failures"], [])
        self.assertEqual(result["wake_lines"], len(NOTES))
        progress = rb.memory_progress(SOUL)
        self.assertTrue(progress["done"])
        self.assertEqual(progress["pending_blocks"], 0)


# --------------------------------------------------------------------- (d)
class TestAttribution(unittest.TestCase):
    """Every turn says who wrote it — ravel 0.12.0's property, held
    for the era: no turn in the store is anonymous."""

    def test_every_turn_says_who_wrote_it(self):
        for tid in WAVELETS:
            _, evs = _dates(tid)
            for ev in evs:
                self.assertTrue(ev.get("author"),
                                f"{ev['event_id']} has no author")
                want = "will" if ev["role"] == "user" else "emise"
                self.assertEqual(ev["author"], want, ev["event_id"])

    def test_authorship_via_the_query_path(self):
        rows = SOUL.query_rows(
            f"SELECT (COUNT(*) AS ?n) WHERE {{ GRAPH ?g {{ "
            f"?s <{RAVEL}author> ?a }} }}"
        )
        self.assertEqual(int(rows[0]["n"]), len(ALL_EVENTS))
        rows = SOUL.query_rows(
            f"SELECT ?a (COUNT(*) AS ?n) WHERE {{ GRAPH ?g {{ "
            f"?s <{RAVEL}author> ?a }} }} GROUP BY ?a"
        )
        by_author = {r["a"]: int(r["n"]) for r in rows}
        want = {"will": 0, "emise": 0}
        for ev in ALL_EVENTS:
            want["will" if ev["role"] == "user" else "emise"] += 1
        self.assertEqual(by_author, want)


# --------------------------------------------------------------------- (e)
class TestAppendOnlyClaims(unittest.TestCase):
    """A claim asserted in W1 and corrected in W3 is a later claim,
    not an edit. The W1 ledger said the seed was unspent
    (~/memory/2026-11-06.md); W3 corrected it twice in one evening
    (~/memory/2026-11-15.md). All three claims stand in the store;
    the first still says what it said."""

    def test_three_claims_stand_in_narrative_order(self):
        claims = _seed_claims()
        self.assertEqual(set(claims),
                         {"w1-1106-ledger", "w3-1115-seed", "w3-1115-seed2"})
        self.assertEqual(claims["w1-1106-ledger"][:2], ("unspent", 0))
        self.assertEqual(claims["w3-1115-seed"][:2], ("spent", 20))
        self.assertEqual(claims["w3-1115-seed2"][:2], ("spent-revised", 26))
        ordered = sorted(claims.values(), key=lambda c: c[2])
        self.assertEqual([c[0] for c in ordered],
                         ["unspent", "spent", "spent-revised"])

    def test_the_original_claim_survives_unchanged(self):
        _, anns = SOUL.read_transcript("wavelet-1")
        mine = [a for a in anns if a["event_id"] == "w1-1106-ledger"]
        self.assertEqual(len(mine), 1)
        self.assertEqual(mine[0]["signal"]["state"], ("text", "unspent"))
        self.assertEqual(mine[0]["signal"]["spent_usd"], ("int", "0"))
        evs, _ = SOUL.read_transcript("wavelet-1")
        ledger = {e["event_id"]: e for e in evs}["w1-1106-ledger"]
        self.assertIn("held unspent", ledger["text"])

    def test_the_current_state_is_the_latest_claim(self):
        claims = _seed_claims()
        latest = max(claims.values(), key=lambda c: c[2])
        self.assertEqual(latest[:2], ("spent-revised", 26))


if __name__ == "__main__":
    unittest.main(verbosity=2)
