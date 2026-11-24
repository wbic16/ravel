#!/usr/bin/env python3
"""SCALE leg — backend side. Runs ON the node.
For each extracted item (~/ravel-v2/scale/*.json): one soul, one transcript
holding every scroll as an event; measure ingest, store bytes, point lookup
and full count through the evaluator, and single-change mutation."""
import glob
import json
import os
import statistics
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
for cand in (os.path.join(HERE, "work"), os.path.join(HERE, "..")):
    sys.path.insert(0, os.path.join(cand, "backend"))

import ravel_backend as rb  # noqa: E402
from phextio import PhextStore  # noqa: E402

RAVEL = rb.RAVEL


def med(xs):
    return statistics.median(xs)


def main():
    port = 19010
    for path in sorted(glob.glob(os.path.expanduser("~/ravel-v2/scale/*.json"))):
        doc = json.load(open(path))
        item = doc["item"]
        events = doc["events"]
        port += 1
        data = os.path.expanduser(f"~/ravel-v2/scale-data-{item}")
        os.system(f"rm -rf {data}")
        store = PhextStore(data, port)
        store.start()
        try:
            soul = rb.Soul(store, "soul", rb.Names(f"https://repolex.ai/scale/{item}/", ""), None).open()
            xs = []
            for _ in range(2):
                t0 = time.time()
                soul.ingest_transcript("scale", events, [])
                xs.append(time.time() - t0)
            phext_bytes = os.path.getsize(os.path.join(data, "soul.phext"))
            mid = events[len(events) // 2]
            turn = soul.names.turn(mid["event_id"])
            q = f"SELECT ?text WHERE {{ <{turn}> <{RAVEL}text> ?text }}"
            lk = []
            for _ in range(3):
                t0 = time.time()
                res = soul.run_query(q)
                assert res["kind"] == "solutions" and len(res["rows"]) == 1, (item, res["kind"], len(res.get("rows", [])))
                lk.append(time.time() - t0)
            t0 = time.time()
            cnt = soul.run_query("SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } }")
            count_secs = time.time() - t0
            n_quads = cnt["rows"][0]["n"][1] if cnt["kind"] == "solutions" else "?"
            changed = [dict(e) for e in events]
            changed[len(events) // 2]["text"] = (changed[len(events) // 2].get("text") or "") + "!"
            t0 = time.time()
            soul.ingest_transcript("scale", changed, [])
            mut = time.time() - t0
            t0 = time.time()
            soul.ingest_transcript("scale", events, [])
            reing = time.time() - t0
            print(json.dumps({
                "item": item, "events": len(events), "source_bytes": doc.get("source_bytes"),
                "ingest_s": [round(x, 3) for x in xs], "phext_bytes": phext_bytes,
                "point_lookup_s": [round(x, 3) for x in lk], "count_quads": n_quads,
                "count_s": round(count_secs, 3), "mutation_one_change_s": round(mut, 3),
                "reingest_no_change_s": round(reing, 3),
            }), flush=True)
        finally:
            store.stop()


if __name__ == "__main__":
    main()
