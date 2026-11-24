#!/usr/bin/env python3
"""Verification harness for the ravel phext backend (outside the LOC
budget: it is the measuring instrument, not the measured code).

Corpus: synthetic souls in ravel's own test style — the graph.rs test
transcripts' shape, reader.rs's canonical emojikey texts verbatim, the
annotate.rs wave legs. The reference projector below is transcribed
from project.rs / annotate.rs / memory_log.rs semantics as documented
in STUDY.md and SPARQL-SUBSET.md; the backend's own projector is a
separate implementation of the same spec, and the two must agree quad
for quad. Roundtrip gates: events -> phext -> events, quads -> set
equality per graph, memory logs -> byte-identical JSONL.
"""
import hashlib
import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "backend"))

RAVEL = "https://repolex.ai/ontology/ravel/"
RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"
OA = "http://www.w3.org/ns/oa#"
PROV = "http://www.w3.org/ns/prov#"
XSD = "http://www.w3.org/2001/XMLSchema#"

FAILURES = []


def check(cond, label, detail=""):
    if cond:
        print(f"  ok  {label}")
    else:
        print(f"FAIL  {label}  {detail}")
        FAILURES.append(label)


# ---------------------------------------------------------------- corpus
ROOT_A = "https://repolex.ai/7r1pl3f0rc3/spacegoat/"
ROOT_B = "https://repolex.ai/"

EVENTS_A1 = [
    {
        "event_id": "u0001", "role": "user", "wrote": "person",
        "timestamp": "2026-07-01T09:00:00Z",
        "text": "--herdr v=1 from=will pane=main session=s1 user=willb--\nhello there",
        "text_provenance": [{"start": 0, "end": 66, "kind": "authored"}],
    },
    {
        "event_id": "a0001", "role": "assistant", "wrote": "soul", "model": "fable",
        "parent_id": "u0001", "timestamp": "2026-07-01T09:01:00Z",
        "text": "wrap up 🐐 [ME|🧠🎨8∠45]~[CONTENT|💻🧩9∠15]~[YOU|🎓🌱8∠35] done",
        "text_provenance": [{"start": 0, "end": 71, "kind": "authored"}],
    },
    {
        "event_id": "a0002", "role": "assistant", "wrote": "soul",
        "parent_id": "a0001", "timestamp": "2026-07-01T09:02:00Z",
        "text": "[me|🐐]~[content|⚙️🌊]~[you|🤝]",
        "model_setting": "deep",
        "text_provenance": [{"start": 0, "end": 30, "kind": "authored"}],
    },
    {
        "event_id": "u0002", "role": "user", "wrote": "person",
        "parent_id": "a0002", "timestamp": "2026-07-01T09:03:00Z",
        "text": "[lux] a peer line, unsigned",
        "text_provenance": [{"start": 0, "end": 27, "kind": "authored"}],
    },
    {
        "event_id": "x0001", "role": "user", "wrote": "nobody",
        "parent_id": "x0001", "timestamp": "2026-07-01T09:04:00Z",
        "text": "a self loop",
        "text_provenance": [{"start": 0, "end": 11, "kind": "authored"}],
    },
]
EVENTS_A2 = [
    {
        "event_id": "u1001", "role": "user", "wrote": "soul",
        "timestamp": "2026-07-02T08:00:00Z",
        "text": "log: [ME|🔧]~[CONTENT|⚙️]~[YOU|📈] end",
        "text_provenance": [{"start": 0, "end": 40, "kind": "tool_result"}],
    },
    {
        "event_id": "a1001", "role": "assistant", "wrote": "soul",
        "parent_id": "u1001", "timestamp": None, "text": None,
    },
    {
        "event_id": "a1002", "role": "assistant", "wrote": "nobody",
        "parent_id": "a1001", "timestamp": "2026-07-02T08:05:00Z",
        "text": "no key here",
    },
]
EVENTS_B1 = [
    {
        "event_id": "b0001", "role": "user", "wrote": "soul",
        "timestamp": "2026-06-01T00:00:00Z", "text": "bare root soul, first",
    },
    {
        "event_id": "b0002", "role": "assistant", "wrote": "soul",
        "parent_id": "b0001", "timestamp": "2026-06-01T00:01:00Z", "text": "second",
    },
]
NOTES_A = [
    {"id": "m0", "ts": "2026-07-01T10:00:00Z", "text": "spacegoat wrapped the GOAT task", "turns": ["a0001"], "model": "fable"},
    {"id": "m1", "ts": "2026-07-01T11:00:00Z", "text": "peer lines carry no author", "turns": ["u0002"], "model": "fable"},
    {"id": "m2", "ts": "2026-07-02T09:00:00Z", "text": "tool results harvest keys too", "turns": ["u1001"], "model": "fable"},
    {"id": "m3", "ts": "2026-07-02T10:00:00Z", "text": "self loops are legal spine", "turns": ["x0001"], "model": "fable"},
]
SUMMARIES_A = [
    {"block": "0-1", "text": "GOAT wrapped; peer lines unauthored", "model": "fable", "written": "2026-07-02T12:00:00Z"},
    {"block": "2-3", "text": "keys in tool results; self loops legal", "model": "fable", "written": "2026-07-02T12:01:00Z"},
    {"block": "0-3", "text": "the whole two days, compressed", "model": "fable", "written": "2026-07-02T12:02:00Z", "wording": "tight"},
]


# ------------------------------------------------- the emojikey reader
_KEY = re.compile(r"\[ME\|([^\]]*)\]~\s*\[CONTENT\|([^\]]*)\]~\s*\[YOU\|([^\]]*)\]", re.IGNORECASE)


def emojikey_read(events):
    """reader.rs's emojikey_read, ported: byte spans, voice = role."""
    out = []
    for e in events:
        text = e.get("text")
        if not text:
            continue
        raw = text.encode("utf-8")
        for m in _KEY.finditer(text):
            start = len(text[: m.start()].encode("utf-8"))
            end = len(text[: m.end()].encode("utf-8"))
            kind = "authored"
            for span in e.get("text_provenance", []):
                if span["start"] <= start < span["end"]:
                    kind = span["kind"]
            out.append({
                "reader": "emojikey", "event_type": "emojikey/harvest",
                "ts": e.get("timestamp"), "event_id": e["event_id"],
                "start": start, "end": end, "source_kind": kind,
                "signal": {
                    "raw": ("text", text[m.start():m.end()]),
                    "me": ("text", m.group(1)),
                    "content": ("text", m.group(2)),
                    "you": ("text", m.group(3)),
                    "voice": ("text", e["role"]),
                },
            })
    return out


WAVE = {
    "reader": "wave", "event_type": "wave/peak", "ts": "2026-07-01T09:01:00Z",
    "event_id": "a0001", "start": 0, "end": 5, "source_kind": "authored",
    "signal": {"magnitude": ("int", "8"), "score": ("decimal", "0.5"), "label": ("text", "peak")},
}
ANNS_A1 = emojikey_read(EVENTS_A1) + [WAVE]
ANNS_A2 = emojikey_read(EVENTS_A2)


# ------------------------------------------------- reference projector
def esc(s):
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")


def L(v, dt=None):
    out = f'"{esc(v)}"'
    return out + (f"^^<{dt}>" if dt else "")


def I(v):
    return f"<{v}>"


def ref_ann_hash(tid, ann):
    h = hashlib.sha256()
    h.update(tid.encode()); h.update(b"\0")
    h.update(ann["reader"].encode()); h.update(b"\0")
    h.update(ann["event_type"].encode()); h.update(b"\0")
    h.update(ann["event_id"].encode()); h.update(b"\0")
    h.update(int(ann["start"]).to_bytes(8, "little"))
    h.update(int(ann["end"]).to_bytes(8, "little"))
    h.update(b"\0")
    h.update((ann.get("source_kind") or "").encode())
    tags = {"text": 0, "int": 1, "decimal": 2}
    for key in sorted(ann["signal"]):
        kind, lexical = ann["signal"][key]
        h.update(b"\0"); h.update(key.encode()); h.update(b"\1")
        h.update(bytes([tags[kind]])); h.update(lexical.encode())
    return h.digest()[:8].hex()


def ref_safe(s):
    return re.sub(r"[^A-Za-z0-9\-_]", "_", s)


_HERDR = re.compile(r"^--herdr v=1((?: [a-z_]+=[^ ]+)*)--$")


def ref_claims(text):
    if not text:
        return None
    m = _HERDR.match(text.split("\n", 1)[0])
    if not m:
        return None
    out = {}
    for pair in m.group(1).split():
        k, _, v = pair.partition("=")
        out[k] = v
    return out or None


def ref_author(ev, root_user, machine_owner):
    claims = ref_claims(ev.get("text"))
    if claims and claims.get("user"):
        return claims["user"]
    wrote = ev.get("wrote", "soul")
    if wrote == "soul":
        soul = (claims or {}).get("soul") or root_user
        if not soul:
            return None
        return f"{soul}-on-{ev['model']}" if ev.get("model") else soul
    if wrote == "person":
        first = (ev.get("text") or "").split("\n", 1)[0]
        if re.match(r"^\[[A-Za-z0-9\-_]{1,40}\]$", first) or re.match(
            r"^\[[A-Za-z0-9\-_]{1,40} -> [A-Za-z0-9\-_]{1,40}\]$", first
        ):
            return None
        return machine_owner
    return None


def ref_spine(root, user, graph, ev, machine_owner):
    """project.rs project_nt -> set of NT lines (no graph term)."""
    turn = f"{root}ravel/Turn/{ev['event_id']}"
    out = set()

    def add(p, o):
        out.add(f"{I(turn)} {I(p)} {o} .")

    add(RDF_TYPE, I(RAVEL + "Turn"))
    add(RAVEL + "turnId", L(ev["event_id"]))
    if ev.get("role"):
        add(RAVEL + "role", L(ev["role"]))
    if ev.get("parent_id"):
        add(RAVEL + "parentTurn", I(f"{root}ravel/Turn/{ev['parent_id']}"))
    if ev.get("timestamp"):
        add(RAVEL + "timestamp", L(ev["timestamp"], XSD + "dateTime"))
    if ev.get("text") is not None:
        add(RAVEL + "text", L(ev["text"]))
    if ev.get("model_setting"):
        add(RAVEL + "modelSetting", L(ev["model_setting"]))
    author = ref_author(ev, user, machine_owner)
    if author:
        add(RAVEL + "author", L(author))
    if ev.get("role") == "user":
        claims = ref_claims(ev.get("text"))
        if claims:
            for key, pred in [("from", "claimedSender"), ("pane", "claimedSenderPane"),
                              ("session", "claimedSenderSession"), ("soul", "claimedSenderSoul"),
                              ("user", "claimedSenderUser")]:
                if claims.get(key):
                    add(RAVEL + pred, L(claims[key]))
    return out


def ref_annotations(root, graph, tid, ann):
    base = f"{root}ravel/"
    h = ref_ann_hash(tid, ann)
    ann_iri = f"{base}Annotation/{h}"
    out = set()

    def add(s, p, o):
        out.add(f"{I(s)} {I(p)} {o} .")

    det = f"{base}Detector/{ref_safe(ann['reader'])}"
    add(det, RDF_TYPE, I(PROV + "SoftwareAgent"))
    add(det, RAVEL + "readerName", L(ann["reader"]))
    add(ann_iri, RDF_TYPE, I(OA + "Annotation"))
    add(ann_iri, PROV + "wasAttributedTo", I(det))
    add(ann_iri, RAVEL + "detectionType", L(ann["event_type"]))
    if ann.get("ts"):
        add(ann_iri, PROV + "generatedAtTime", L(ann["ts"], XSD + "dateTime"))
    if ann.get("source_kind"):
        add(ann_iri, RAVEL + "textOrigin", L(ann["source_kind"]))
    target, selector, body = f"{ann_iri}/target", f"{ann_iri}/selector", f"{ann_iri}/body"
    add(ann_iri, OA + "hasTarget", I(target))
    add(target, OA + "hasSource", I(f"{base}Transcript/{ref_safe(tid)}"))
    add(target, OA + "hasSelector", I(selector))
    add(selector, RDF_TYPE, I(OA + "TextPositionSelector"))
    add(selector, OA + "start", L(str(int(ann["start"])), XSD + "integer"))
    add(selector, OA + "end", L(str(int(ann["end"])), XSD + "integer"))
    event = f"{root}ravel/Turn/{ann['event_id']}"
    add(target, RAVEL + "atTurn", I(event))
    add(ann_iri, PROV + "used", I(event))
    add(ann_iri, OA + "hasBody", I(body))
    dts = {"int": XSD + "integer", "decimal": XSD + "decimal"}
    for key in sorted(ann["signal"]):
        kind, lexical = ann["signal"][key]
        add(body, RAVEL + "sig_" + ref_safe(key), L(lexical, dts.get(kind)))
    claim = f"{base}Claim/{h}"
    add(claim, REIFIES, f"<<( {I(event)} {I(RAVEL + 'exhibits')} {L(ann['event_type'])} )>>")
    add(claim, PROV + "wasDerivedFrom", I(ann_iri))
    add(claim, RAVEL + "detectorName", L(ann["reader"]))
    add(claim, RAVEL + "detectionType", L(ann["event_type"]))
    return out


def ref_memory(root, notes, summaries):
    """memory_log.rs project_nt -> NT lines for the optmem graph."""
    out = set()

    def add(s, p, o):
        out.add(f"{I(s)} {I(p)} {o} .")

    def node_id(lo, hi):
        return notes[lo]["id"] if hi - lo == 1 else f"{lo}-{hi - 1}"

    for i, n in enumerate(notes):
        m = f"{root}ravel/Memory/{n['id']}"
        add(m, RDF_TYPE, I(RAVEL + "Memory"))
        add(m, RAVEL + "memoryId", L(n["id"]))
        add(m, RAVEL + "memoryLevel", L("0", XSD + "integer"))
        add(m, RAVEL + "logStart", L(str(i), XSD + "integer"))
        add(m, RAVEL + "logEnd", L(str(i), XSD + "integer"))
        add(m, RAVEL + "memoryText", L(n["text"]))
        add(m, RAVEL + "windowStart", L(n["ts"], XSD + "dateTime"))
        add(m, RAVEL + "windowEnd", L(n["ts"], XSD + "dateTime"))
        add(m, RAVEL + "memoryModel", L(n["model"]))
        for t in n["turns"]:
            add(m, RAVEL + "relatedToId", I(f"{root}ravel/Turn/{t}"))
    by_block = {s["block"]: s for s in summaries}
    for bid in sorted(by_block):
        s = by_block[bid]
        a, b = bid.split("-")
        lo, hi = int(a), int(b) + 1
        if hi > len(notes):
            continue
        m = f"{root}ravel/Memory/{bid}"
        add(m, RDF_TYPE, I(RAVEL + "Memory"))
        add(m, RAVEL + "memoryId", L(bid))
        add(m, RAVEL + "memoryLevel", L(str((hi - lo).bit_length() - 1), XSD + "integer"))
        add(m, RAVEL + "logStart", L(str(lo), XSD + "integer"))
        add(m, RAVEL + "logEnd", L(str(hi - 1), XSD + "integer"))
        add(m, RAVEL + "memoryText", L(s["text"]))
        add(m, RAVEL + "windowStart", L(notes[lo]["ts"], XSD + "dateTime"))
        add(m, RAVEL + "windowEnd", L(notes[hi - 1]["ts"], XSD + "dateTime"))
        add(m, RAVEL + "memoryModel", L(s["model"]))
        mid = (lo + hi) // 2
        add(m, RAVEL + "relatedToId", I(f"{root}ravel/Memory/{node_id(lo, mid)}"))
        add(m, RAVEL + "relatedToId", I(f"{root}ravel/Memory/{node_id(mid, hi)}"))
    return out


def ref_descriptions(root, user, graphs):
    """graph.rs's graph descriptions, in the default graph."""
    out = set()
    for gid, cls in graphs:
        g = f"{root}ravel/NamedGraph/{gid}"
        out.add(f"{I(g)} {I(RDF_TYPE)} {I(RAVEL + cls)} .")
        out.add(f"{I(g)} {I(RAVEL + 'visibility')} {L('private')} .")
        if user:
            out.add(f"{I(g)} {I(RAVEL + 'owner')} {I(f'{root}soul/Soul/{user}')} .")
    return out


# ------------------------------------------------------------------ run
def backend_quads_by_graph(soul):
    """The backend's glue output, as {graph: set of NT lines}, plus the
    default graph under key None."""
    import ravel_backend as rb

    out = {}
    for g, s, p, o in soul.all_quads():
        line = f"{rb.nt(s)} {rb.nt(p)} {rb.nt(o)} ."
        out.setdefault(g, set()).add(line)
    return out


def scroll_snapshot(store, fname):
    snap = {}
    for c in store.toc(fname):
        snap[c] = store.read(fname, c)
    return snap


def seam_phase():
    import ravel_backend as rb
    from phextio import PhextStore

    print("== seam phase ==")
    data = os.environ["HARNESS_DATA1"]
    store = PhextStore(data, int(os.environ["HARNESS_SQ1"]))
    store.start()
    timings = {}
    try:
        names_a = rb.Names(ROOT_A, "spacegoat")
        soul_a = rb.Soul(store, "soul-a", names_a, machine_owner=None).open()
        t0 = time.time()
        soul_a.ingest_transcript("sess-A", EVENTS_A1, ANNS_A1)
        soul_a.ingest_transcript("sess-B", EVENTS_A2, ANNS_A2)
        timings["ingest_a_secs"] = time.time() - t0
        names_b = rb.Names(ROOT_B, "")
        soul_b = rb.Soul(store, "soul-b", names_b, machine_owner=None).open()
        soul_b.ingest_transcript("only", EVENTS_B1, [])
        for n in NOTES_A:
            soul_a.append_note(n)
        for s in SUMMARIES_A:
            soul_a.upsert_summary(s)

        # --- quad-for-quad, per graph
        got = backend_quads_by_graph(soul_a)
        g_a = f"{ROOT_A}ravel/NamedGraph/sess-A"
        g_b = f"{ROOT_A}ravel/NamedGraph/sess-B"
        g_m = f"{ROOT_A}ravel/NamedGraph/optmem"
        want_a = set()
        for ev in EVENTS_A1:
            want_a |= ref_spine(ROOT_A, "spacegoat", g_a, ev, None)
        for a in ANNS_A1:
            want_a |= ref_annotations(ROOT_A, g_a, "sess-A", a)
        want_b = set()
        for ev in EVENTS_A2:
            want_b |= ref_spine(ROOT_A, "spacegoat", g_b, ev, None)
        for a in ANNS_A2:
            want_b |= ref_annotations(ROOT_A, g_b, "sess-B", a)
        check(got.get(g_a) == want_a, "sess-A quads == reference",
              f"\n  missing: {sorted(want_a - got.get(g_a, set()))[:4]}\n  extra: {sorted(got.get(g_a, set()) - want_a)[:4]}")
        check(got.get(g_b) == want_b, "sess-B quads == reference",
              f"\n  missing: {sorted(want_b - got.get(g_b, set()))[:4]}\n  extra: {sorted(got.get(g_b, set()) - want_b)[:4]}")
        want_m = ref_memory(ROOT_A, NOTES_A, SUMMARIES_A)
        check(got.get(g_m) == want_m, "memory quads == reference",
              f"\n  missing: {sorted(want_m - got.get(g_m, set()))[:4]}\n  extra: {sorted(got.get(g_m, set()) - want_m)[:4]}")
        want_desc = ref_descriptions(ROOT_A, "spacegoat", [
            ("sess-A", "TranscriptGraph"), ("sess-B", "TranscriptGraph"),
            ("optmem", "MemoryGraph"), ("recent", "NamedGraph"),
        ])
        check(got.get(None) == want_desc, "graph descriptions == reference",
              f"\n  missing: {sorted(want_desc - got.get(None, set()))[:4]}\n  extra: {sorted(got.get(None, set()) - want_desc)[:4]}")
        # recent membership: every turn here is within the 500.
        want_recent = {
            f"{I(f'{ROOT_A}ravel/NamedGraph/recent')} {I('http://www.w3.org/2000/01/rdf-schema#member')} {I(f'{ROOT_A}ravel/Turn/{e['event_id']}')} ."
            for e in EVENTS_A1 + EVENTS_A2
        }
        g_r = f"{ROOT_A}ravel/NamedGraph/recent"
        check(got.get(g_r) == want_recent, "recent membership == reference")
        got_b = backend_quads_by_graph(soul_b)
        want_b1 = set()
        for ev in EVENTS_B1:
            want_b1 |= ref_spine(ROOT_B, "", f"{ROOT_B}ravel/NamedGraph/only", ev, None)
        check(got_b.get(f"{ROOT_B}ravel/NamedGraph/only") == want_b1, "bare-root soul quads == reference",
              f"\n  missing: {sorted(want_b1 - got_b.get(f'{ROOT_B}ravel/NamedGraph/only', set()))[:4]}\n  extra: {sorted(got_b.get(f'{ROOT_B}ravel/NamedGraph/only', set()) - want_b1)[:4]}")

        # --- read-back fidelity
        evs, anns = soul_a.read_transcript("sess-A")
        check([e["event_id"] for e in evs] == [e["event_id"] for e in EVENTS_A1], "read-back order")
        by_id = {e["event_id"]: e for e in evs}
        for orig in EVENTS_A1:
            got_ev = by_id[orig["event_id"]]
            for field in ("role", "timestamp", "text", "parent_id", "model_setting"):
                check(got_ev.get(field) == orig.get(field), f"read-back {orig['event_id']}.{field}",
                      f"{got_ev.get(field)!r} != {orig.get(field)!r}")
            check(got_ev.get("author") == ref_author(orig, "spacegoat", None),
                  f"read-back {orig['event_id']}.author", f"{got_ev.get('author')!r}")
        check(len(anns) == len(ANNS_A1), "read-back annotation count", f"{len(anns)} != {len(ANNS_A1)}")
        want_anns = sorted((json.dumps(a, sort_keys=True, default=str) for a in ANNS_A1))
        got_anns = sorted((json.dumps({
            "reader": a["reader"], "event_type": a["event_type"], "ts": a["ts"],
            "event_id": a["event_id"], "start": a["start"], "end": a["end"],
            "source_kind": a["source_kind"],
            "signal": {k: list(v) for k, v in a["signal"].items()},
        }, sort_keys=True, default=str) for a in anns))
        want_anns_n = [json.dumps({
            "reader": a["reader"], "event_type": a["event_type"], "ts": a["ts"],
            "event_id": a["event_id"], "start": a["start"], "end": a["end"],
            "source_kind": a["source_kind"],
            "signal": {k: list(v) for k, v in a["signal"].items()},
        }, sort_keys=True, default=str) for a in ANNS_A1]
        check(sorted(want_anns_n) == got_anns, "read-back annotations == originals")

        # --- memory logs byte-identical
        notes = soul_a.notes()
        mem_lines = [
            json.dumps({"id": n["id"], "ts": n["ts"], "text": n["text"], "turns": n["turns"], "model": n["model"]},
                       ensure_ascii=False, separators=(",", ":"))
            for n in notes
        ]
        want_lines = [
            json.dumps({"id": n["id"], "ts": n["ts"], "text": n["text"], "turns": n["turns"], "model": n["model"]},
                       ensure_ascii=False, separators=(",", ":"))
            for n in NOTES_A
        ]
        check(mem_lines == want_lines, "memories.jsonl export byte-identical")
        sums = soul_a.summaries()
        sum_lines = []
        for s in sorted(sums, key=lambda s: SUMMARIES_A.index(next(x for x in SUMMARIES_A if x["block"] == s["block"]))):
            d = {"block": s["block"], "text": s["text"], "model": s["model"], "written": s["written"]}
            if "wording" in s:
                d["wording"] = s["wording"]
            sum_lines.append(json.dumps(d, ensure_ascii=False, separators=(",", ":")))
        want_sums = []
        for s in SUMMARIES_A:
            d = {"block": s["block"], "text": s["text"], "model": s["model"], "written": s["written"]}
            if "wording" in s:
                d["wording"] = s["wording"]
            want_sums.append(json.dumps(d, ensure_ascii=False, separators=(",", ":")))
        check(sum_lines == want_sums, "summaries.jsonl export byte-identical")

        # --- fixed queries through the evaluator
        t0 = time.time()
        rows = soul_a.query_rows(rb.__dict__.get("EMOJIKEYS", "") or (
            "PREFIX ravel: <https://repolex.ai/ontology/ravel/> "
            "PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> "
            "PREFIX oa: <http://www.w3.org/ns/oa#> "
            "PREFIX prov: <http://www.w3.org/ns/prov#> "
            "SELECT ?g ?sk ?me ?content ?you ?ts WHERE { GRAPH ?g { "
            "?claim rdf:reifies <<( ?event ravel:exhibits \"emojikey/harvest\" )>> ; "
            "prov:wasDerivedFrom ?ann . ?ann oa:hasBody ?body . "
            "OPTIONAL { ?ann ravel:textOrigin ?sk } OPTIONAL { ?ann prov:generatedAtTime ?ts } "
            "?body ravel:sig_me ?me ; ravel:sig_content ?content ; ravel:sig_you ?you . } } "
            "ORDER BY ?g ?ts"
        ))
        timings["emojikey_query_secs"] = time.time() - t0
        check(len(rows) == 3, "emojikey query returns all three keys", f"{len(rows)} rows")
        kinds = sorted(r.get("sk", "") for r in rows)
        check(kinds == ["authored", "authored", "tool_result"], "emojikey origins", str(kinds))
        t0 = time.time()
        stats = rb.soul_stats(soul_a)
        timings["stats_battery_secs"] = time.time() - t0
        check(stats["turns"] == 8, "stats turns", str(stats["turns"]))
        check(stats["claims"] == 4, "stats claims", str(stats["claims"]))
        v = rb.verify(soul_a)
        check(v["failures"] == [], "memory verify clean", str(v["failures"]))
        check(v["turns_in_store"] == 4, "verify cited turns in store", str(v["turns_in_store"]))

        # --- mutation control: one record changes, exactly its scrolls do
        before = scroll_snapshot(store, "soul-a.phext")
        changed_events = [dict(e) for e in EVENTS_A1]
        changed_events[1] = dict(changed_events[1], text=changed_events[1]["text"] + "!")
        soul_a.ingest_transcript("sess-A", changed_events, ANNS_A1)
        after = scroll_snapshot(store, "soul-a.phext")
        changed = {c for c in set(before) | set(after) if before.get(c) != after.get(c)}
        # a0001 is book 2 of volume 2 (sess-A is slot 0); its text scroll:
        text_scroll = "1.2.2/3.1.1/1.1.1"
        check(changed == {text_scroll}, "mutation touches exactly the text scroll",
              f"changed: {sorted(changed)}")
        soul_a.ingest_transcript("sess-A", EVENTS_A1, ANNS_A1)  # restore

        # --- sizes and hashes
        phext_bytes = os.path.getsize(os.path.join(data, "soul-a.phext"))
        native_nt = sum(len("\n".join(sorted(s)) + "\n") for s in
                        [want_a, want_b, want_m, want_desc] if s)
        native_logs = sum(len(x) + 1 for x in want_lines + want_sums)
        digest = hashlib.sha256(open(os.path.join(data, "soul-a.phext"), "rb").read()).hexdigest()
        print(f"  sizes: phext {phext_bytes} B; native NT {native_nt} B + logs {native_logs} B")
        print(f"  soul-a.phext sha256 {digest}")
        timings["phext_bytes"] = phext_bytes
        timings["native_equiv_bytes"] = native_nt + native_logs
        return timings
    finally:
        store.stop()


def http_phase():
    import urllib.request
    import urllib.error

    print("== http phase ==")
    base = f"http://127.0.0.1:{os.environ['HARNESS_PORT']}/ravel"
    timings = {}

    def call(method, path, body=None, raw=False):
        data = None
        headers = {}
        if body is not None:
            data = (body.encode() if isinstance(body, str) else body) if raw else json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(base + path, data=data, method=method, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                return resp.status, resp.read().decode(), dict(resp.headers)
        except urllib.error.HTTPError as e:
            return e.code, e.read().decode(), dict(e.headers)

    code, body, _ = call("GET", "/health")
    check(code == 200 and json.loads(body)["ok"], "GET /ravel/health")
    code, body, _ = call("GET", "/stores")
    stores = json.loads(body)
    check(any(s["address"] == "7r1pl3f0rc3/spacegoat" for s in stores), "GET /ravel/stores lists soul A")

    payload = {"transcripts": [
        {"id": "sess-A", "events": EVENTS_A1, "annotations": ANNS_A1},
        {"id": "sess-B", "events": EVENTS_A2, "annotations": ANNS_A2},
    ], "notes": NOTES_A, "summaries": SUMMARIES_A}
    # annotations carry tuples in-process; on the wire they are lists.
    payload = json.loads(json.dumps(payload))
    t0 = time.time()
    code, body, _ = call("POST", "/7r1pl3f0rc3/spacegoat/import", payload)
    timings["http_import_secs"] = time.time() - t0
    check(code == 200, "POST import", body[:200])

    code, body, _ = call("GET", "/7r1pl3f0rc3/spacegoat/stats")
    stats = json.loads(body)
    check(stats["turns"] == 8 and stats["claims"] == 4, "GET stats counts", body[:200])
    code, body, _ = call("POST", "/7r1pl3f0rc3/spacegoat/query", {"query":
        "SELECT ?me WHERE { GRAPH ?g { ?b <https://repolex.ai/ontology/ravel/sig_me> ?me } }"})
    rows = json.loads(body)["results"]["bindings"]
    check(len(rows) == 3, "POST /query rows", body[:200])
    code, body, headers = call("POST", "/7r1pl3f0rc3/spacegoat/sparql",
                               "ASK { <https://repolex.ai/7r1pl3f0rc3/spacegoat/ravel/Turn/a0001> a <https://repolex.ai/ontology/ravel/Turn> }",
                               raw=True)
    check(json.loads(body).get("boolean") is True, "POST /sparql ASK")
    code, body, _ = call("POST", "/7r1pl3f0rc3/spacegoat/query", {"query":
        "SELECT ?s WHERE { ?s ?p ?o } UNION { ?s ?p ?o }"})
    check(code == 400 and "UNION" in body, "UNION refused by name over HTTP", body[:200])
    code, body, _ = call("POST", "/7r1pl3f0rc3/spacegoat/sparql",
                         "CONSTRUCT { ?s ?p ?o } WHERE { GRAPH ?g { ?s ?p ?o } }", raw=True)
    check(code == 200 and body.count("\n") > 40, "CONSTRUCT (fringe) answers triples", f"{body.count(chr(10))} lines")
    code, body, _ = call("GET", "/7r1pl3f0rc3/spacegoat/memory")
    view = json.loads(body)
    check(view["memories"] == 4 and len(view["lines"]) == 4, "GET memory wake = four note lines (log under budget)", body[:200])
    code, body, _ = call("GET", "/7r1pl3f0rc3/spacegoat/memory/0-3")
    opened = json.loads(body)
    check(opened["kind"] == "block" and len(opened["children"]) == 2, "GET memory/0-3 zooms")
    code, body, _ = call("GET", "/7r1pl3f0rc3/spacegoat/memory/0")
    opened = json.loads(body)
    check(opened["kind"] == "memory" and opened["turns"] and opened["turns"][0]["turn"] == "a0001",
          "GET memory/0 opens the note with its turn", body[:300])
    code, body, _ = call("GET", "/7r1pl3f0rc3/spacegoat/memory-check")
    check(json.loads(body)["check"] == "Verified", "GET memory-check Verified", body[:200])
    code, body, _ = call("GET", "/7r1pl3f0rc3/spacegoat/memory-progress")
    prog = json.loads(body)
    check(prog["done"] is True and prog["pending_blocks"] == 0, "GET memory-progress done", body[:200])
    code, body, _ = call("GET", "/7r1pl3f0rc3/spacegoat/health")
    health = json.loads(body)
    check(health["attention"] is False, "GET soul health quiet", body[:300])
    # append through /sync grows the store by one turn
    extra = {"event_id": "a0003", "role": "assistant", "wrote": "soul",
             "parent_id": "a0002", "timestamp": "2026-07-01T09:05:00Z", "text": "one more"}
    code, body, _ = call("POST", "/7r1pl3f0rc3/spacegoat/sync",
                         {"transcripts": [{"id": "sess-A", "events": [extra], "annotations": []}]})
    check(code == 200, "POST sync append")
    # Stats answers stand for 300 s in the native daemon too (the kept
    # answer is not invalidated by a sync), so the append is asserted
    # through an uncached query, and the stand itself is asserted.
    code, body, _ = call("POST", "/7r1pl3f0rc3/spacegoat/query", {"query":
        "SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } "
        "FILTER(STRSTARTS(STR(?s), \"https://repolex.ai/7r1pl3f0rc3/spacegoat/ravel/Turn/\")) }"})
    n = json.loads(body)["results"]["bindings"][0]["n"]["value"]
    check(n == "9", "append landed (uncached count)", body[:200])
    code, body, _ = call("GET", "/7r1pl3f0rc3/spacegoat/stats")
    check(json.loads(body)["counted_secs_ago"] >= 0, "stats stand-for cache answers", body[:120])
    return timings


def main():
    timings = {}
    timings.update(seam_phase())
    timings.update(http_phase())
    print("== timings (backend-self, this node) ==")
    for k, v in timings.items():
        print(f"  {k}: {v}")
    if FAILURES:
        print(f"\n{len(FAILURES)} FAILURES: {FAILURES}")
        return 1
    print("\nall checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
