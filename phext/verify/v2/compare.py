#!/usr/bin/env python3
"""v2 third leg + backend ingest timings + clean store sizes.
Runs ON the node (needs the staged backend, SQ, evaluator).
Compares, per soul per graph: native dump vs phext backend vs harness reference.
"""
import os
import re
import statistics
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
for cand in (os.path.join(HERE, "work"), os.path.join(HERE, "..")):
    sys.path.insert(0, os.path.join(cand, "backend"))
    sys.path.insert(0, os.path.join(cand, "verify"))

import harness as h  # noqa: E402
import ravel_backend as rb  # noqa: E402
from phextio import PhextStore  # noqa: E402

MEMBER = "http://www.w3.org/2000/01/rdf-schema#member"


def parse_native(path):
    graphs = {}
    for line in open(path):
        line = line.rstrip("\n")
        if not line:
            continue
        terms, i = [], 0
        for _ in range(4):
            while i < len(line) and line[i] == " ":
                i += 1
            if line.startswith("DEFAULT", i):
                terms.append(None)
                i += len("DEFAULT")
                continue
            t, i = rb.parse_nt_term(line, i)
            terms.append(t)
        s, p, o, g = terms
        gk = g[1] if g and g[0] == "iri" else None
        graphs.setdefault(gk, set()).add((s, p, o))
    return graphs


def nt_lines_to_tuples(lines):
    out = set()
    for line in lines:
        terms, i = [], 0
        for _ in range(3):
            while i < len(line) and line[i] == " ":
                i += 1
            t, i = rb.parse_nt_term(line, i)
            terms.append(t)
        out.add(tuple(terms))
    return out


def ref_graphs_a():
    g_a = f"{h.ROOT_A}ravel/NamedGraph/sess-A"
    g_b = f"{h.ROOT_A}ravel/NamedGraph/sess-B"
    g_m = f"{h.ROOT_A}ravel/NamedGraph/optmem"
    g_r = f"{h.ROOT_A}ravel/NamedGraph/recent"
    want = {}
    sa = set()
    for ev in h.EVENTS_A1:
        sa |= h.ref_spine(h.ROOT_A, "spacegoat", g_a, ev, None)
    for a in h.ANNS_A1:
        sa |= h.ref_annotations(h.ROOT_A, g_a, "sess-A", a)
    want[g_a] = nt_lines_to_tuples(sa)
    sb = set()
    for ev in h.EVENTS_A2:
        sb |= h.ref_spine(h.ROOT_A, "spacegoat", g_b, ev, None)
    for a in h.ANNS_A2:
        sb |= h.ref_annotations(h.ROOT_A, g_b, "sess-B", a)
    want[g_b] = nt_lines_to_tuples(sb)
    want[g_m] = nt_lines_to_tuples(h.ref_memory(h.ROOT_A, h.NOTES_A, h.SUMMARIES_A))
    want[None] = nt_lines_to_tuples(h.ref_descriptions(h.ROOT_A, "spacegoat", [
        ("sess-A", "TranscriptGraph"), ("sess-B", "TranscriptGraph"),
        ("optmem", "MemoryGraph"), ("recent", "NamedGraph"),
    ]))
    want[g_r] = nt_lines_to_tuples({
        f"<{g_r}> <{MEMBER}> <{h.ROOT_A}ravel/Turn/{e['event_id']}> ."
        for e in h.EVENTS_A1 + h.EVENTS_A2
    })
    return want


def ref_graphs_b():
    g_o = f"{h.ROOT_B}ravel/NamedGraph/only"
    g_r = f"{h.ROOT_B}ravel/NamedGraph/recent"
    sb1 = set()
    for ev in h.EVENTS_B1:
        sb1 |= h.ref_spine(h.ROOT_B, "", g_o, ev, None)
    want = {g_o: nt_lines_to_tuples(sb1)}
    want[g_r] = nt_lines_to_tuples({
        f"<{g_r}> <{MEMBER}> <{h.ROOT_B}ravel/Turn/{e['event_id']}> ."
        for e in h.EVENTS_B1
    })
    want[None] = nt_lines_to_tuples(h.ref_descriptions(h.ROOT_B, "", [
        ("only", "TranscriptGraph"), ("recent", "NamedGraph"),
    ]))
    return want


def fmt(q):
    return f"{rb.nt(q[0])} {rb.nt(q[1])} {rb.nt(q[2])}"


def compare(label, native, backend, ref):
    print(f"\n== {label} ==")
    keys = sorted(set(native) | set(backend) | set(ref), key=lambda k: str(k))
    ndiff = 0
    for g in keys:
        n, b = native.get(g, set()), backend.get(g, set())
        has_ref = g in ref
        r = ref.get(g, set())
        ok = n == b and (not has_ref or b == r)
        if not ok:
            ndiff += 1
        print(f"{'OK ' if ok else 'DIFF'} graph={g} native={len(n)} backend={len(b)} ref={len(r) if has_ref else 'n/a'}")
        if n != b:
            for q in sorted(n - b, key=fmt)[:8]:
                print(f"   native-only: {fmt(q)}")
            for q in sorted(b - n, key=fmt)[:8]:
                print(f"   backend-only: {fmt(q)}")
        if has_ref and b != r and n == b:
            for q in sorted(r - b, key=fmt)[:8]:
                print(f"   ref-only: {fmt(q)}")
            for q in sorted(b - r, key=fmt)[:8]:
                print(f"   backend-not-ref: {fmt(q)}")
    return ndiff


def ann_ids(qs):
    out = set()
    for s, p, o in qs:
        for t in (s, p, o):
            if t[0] == "iri":
                m = re.search(r"Annotation/([0-9a-f]{16})", t[1])
                if m:
                    out.add(m.group(1))
    return out


def main():
    # clean per-soul native dumps (one store per soul, as production runs)
    native_a = parse_native(os.path.expanduser("~/ravel-v2/native-dump2.nq"))
    native_b = parse_native(os.path.expanduser("~/ravel-v2/native-dumpb.nq"))

    data = os.path.expanduser("~/ravel-v2/compare-data")
    os.system(f"rm -rf {data}")
    store = PhextStore(data, 18995)
    store.start()
    try:
        soul_a = rb.Soul(store, "soul-a", rb.Names(h.ROOT_A, "spacegoat"), None).open()

        def timed_ingest(tid, evs, anns, n=7):
            soul_a.ingest_transcript(tid, evs, anns)
            xs = []
            for _ in range(n):
                t0 = time.time()
                soul_a.ingest_transcript(tid, evs, anns)
                xs.append(time.time() - t0)
            return statistics.median(xs)

        print(f"backend ingest_sessA median {timed_ingest('sess-A', h.EVENTS_A1, h.ANNS_A1):.4f}s")
        print(f"backend ingest_sessB median {timed_ingest('sess-B', h.EVENTS_A2, h.ANNS_A2):.4f}s")
        for n in h.NOTES_A:
            soul_a.append_note(n)
        for s in h.SUMMARIES_A:
            soul_a.upsert_summary(s)
        soul_b = rb.Soul(store, "soul-b", rb.Names(h.ROOT_B, ""), None).open()
        soul_b.ingest_transcript("only", h.EVENTS_B1, [])
        backend_a, backend_b = {}, {}
        for g, s, p, o in soul_a.all_quads():
            backend_a.setdefault(g, set()).add((s, p, o))
        for g, s, p, o in soul_b.all_quads():
            backend_b.setdefault(g, set()).add((s, p, o))
        pa = os.path.join(data, "soul-a.phext")
        pb = os.path.join(data, "soul-b.phext")
        print(f"backend store bytes: soul-a {os.path.getsize(pa)} soul-b {os.path.getsize(pb)}")
    finally:
        store.stop()

    d = compare("soul A", native_a, backend_a, ref_graphs_a())
    d += compare("soul B", native_b, backend_b, ref_graphs_b())
    ga = f"{h.ROOT_A}ravel/NamedGraph/sess-A"
    print("\nann ids sess-A native :", sorted(ann_ids(native_a.get(ga, set()))))
    print("ann ids sess-A backend:", sorted(ann_ids(backend_a.get(ga, set()))))
    print("ann ids sess-A ref    :", sorted(ann_ids(ref_graphs_a().get(ga, set()))))
    print(f"\ngraphs with any diff: {d}")


if __name__ == "__main__":
    main()
