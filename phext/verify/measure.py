#!/usr/bin/env python3
"""Backend-self measurements for the V1 comparison table + the
empty-string edge probe. Run after harness.py's corpus builders."""
import os
import statistics
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "backend"))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import EVENTS_A1, EVENTS_A2, ANNS_A1, ANNS_A2, NOTES_A, SUMMARIES_A, ROOT_A  # noqa: E402


def timed(fn, n=5):
    samples = []
    for _ in range(n):
        t0 = time.time()
        fn()
        samples.append(time.time() - t0)
    return statistics.median(samples)


def main():
    import ravel_backend as rb
    from phextio import make_store

    data = os.environ["MEASURE_DATA"]
    store = make_store(data, int(os.environ["MEASURE_SQ"]))
    store.start()
    try:
        soul = rb.Soul(store, "soul-a", rb.Names(ROOT_A, "spacegoat"), None).open()
        soul.ingest_transcript("sess-A", EVENTS_A1, ANNS_A1)
        soul.ingest_transcript("sess-B", EVENTS_A2, ANNS_A2)
        for n in NOTES_A:
            soul.append_note(n)
        for s in SUMMARIES_A:
            soul.upsert_summary(s)
        turn = f"{ROOT_A}ravel/Turn/a0001"
        point = (f"SELECT ?ts ?role ?text WHERE {{ GRAPH ?g {{ <{turn}> <{rb.RAVEL}timestamp> ?ts ; "
                 f"<{rb.RAVEL}role> ?role . OPTIONAL {{ <{turn}> <{rb.RAVEL}text> ?text }} }} }} LIMIT 1")
        print(f"point lookup (one turn by IRI):      {timed(lambda: soul.query_rows(point)):.4f}s median")
        one_hop = (f"SELECT ?t ?p WHERE {{ GRAPH ?g {{ ?t <{rb.RAVEL}parentTurn> ?p }} }}")
        print(f"traversal 1-hop (all parent edges):  {timed(lambda: soul.query_rows(one_hop)):.4f}s median")

        def three_hops():
            cur = turn
            for _ in range(3):
                rows = soul.query_rows(
                    f"SELECT ?p WHERE {{ GRAPH ?g {{ <{cur}> <{rb.RAVEL}parentTurn> ?p }} }}")
                if not rows:
                    break
                cur = rows[0]["p"]
            return cur

        print(f"traversal 3-hop (parent chain walk): {timed(three_hops):.4f}s median")

        def mutate():
            changed = [dict(e) for e in EVENTS_A1]
            changed[1] = dict(changed[1], text=changed[1]["text"] + "!")
            soul.ingest_transcript("sess-A", changed, ANNS_A1)
            soul.ingest_transcript("sess-A", EVENTS_A1, ANNS_A1)

        print(f"single-record mutation (re-ingest):  {timed(mutate, 3):.4f}s median")

        # The empty-string edge: an event whose text is "" projects a
        # `text ""` triple natively; the phext store cannot hold an
        # empty scroll distinctly from an absent one (toc enumerates
        # non-empty scrolls only). Measure the exact divergence.
        edge = [{"event_id": "e0001", "role": "assistant", "wrote": "nobody",
                 "timestamp": "2026-07-03T00:00:00Z", "text": ""}]
        soul.ingest_transcript("edge", edge, [])
        evs, _ = soul.read_transcript("edge")
        print(f"empty-text probe: read-back text = {evs[0].get('text')!r} (native: '' with a text triple)")
        rows = soul.query_rows(
            f"SELECT ?o WHERE {{ GRAPH ?g {{ <{ROOT_A}ravel/Turn/e0001> <{rb.RAVEL}text> ?o }} }}")
        print(f"empty-text probe: text triples in store = {len(rows)} (native: 1, the empty literal)")
    finally:
        store.stop()


if __name__ == "__main__":
    main()
