#!/usr/bin/env python3
"""v2 outside/fringe probe — backend side. Runs ON the node.
Builds soul A with the (fixed) backend, runs queries.json through
Soul.run_query, writes backend-results.jsonl + prep.txt (for the crafted
standalone-evaluator probe) in ~/ravel-v2/."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
for cand in (os.path.join(HERE, "work"), os.path.join(HERE, "..")):
    sys.path.insert(0, os.path.join(cand, "backend"))
    sys.path.insert(0, os.path.join(cand, "verify"))

import harness as h  # noqa: E402
import ravel_backend as rb  # noqa: E402
from phextio import PhextStore  # noqa: E402


def norm(res):
    if res["kind"] == "solutions":
        rows = []
        for r in res["rows"]:
            rows.append({v: rb.nt(t) for v, t in sorted(r.items())})
        rows.sort(key=lambda r: json.dumps(r, sort_keys=True))
        return {"kind": "solutions", "rows": rows, "notes": res["notes"]}
    if res["kind"] == "boolean":
        return {"kind": "boolean", "value": res["value"], "notes": res["notes"]}
    if res["kind"] == "triples":
        ts = sorted(f"{rb.nt(s)} {rb.nt(p)} {rb.nt(o)}" for s, p, o in res["triples"])
        return {"kind": "triples", "triples": ts, "notes": res["notes"]}
    return {"kind": res["kind"], "notes": res.get("notes", [])}


def main():
    queries = json.load(open(os.path.join(HERE, "queries.json")))
    data = os.path.expanduser("~/ravel-v2/probe-data")
    os.system(f"rm -rf {data}")
    store = PhextStore(data, 18996)
    store.start()
    out = open(os.path.expanduser("~/ravel-v2/backend-results.jsonl"), "w")
    try:
        soul = rb.Soul(store, "soul-a", rb.Names(h.ROOT_A, "spacegoat"), None).open()
        soul.ingest_transcript("sess-A", h.EVENTS_A1, h.ANNS_A1)
        soul.ingest_transcript("sess-B", h.EVENTS_A2, h.ANNS_A2)
        for n in h.NOTES_A:
            soul.append_note(n)
        for s in h.SUMMARIES_A:
            soul.upsert_summary(s)
        _nq, prep = soul._evaluator_inputs()
        open(os.path.expanduser("~/ravel-v2/prep.txt"), "w").write(prep)
        for q in queries:
            rec = {"id": q["id"]}
            try:
                rec.update(norm(soul.run_query(q["sparql"], q["base"])))
            except rb.Refused as e:
                rec.update({"refused": e.construct, "message": str(e)})
            except Exception as e:  # noqa: BLE001
                rec.update({"error": f"{type(e).__name__}: {e}"})
            out.write(json.dumps(rec, ensure_ascii=False) + "\n")
            out.flush()
    finally:
        out.close()
        store.stop()
    print("backend probe done")


if __name__ == "__main__":
    main()
