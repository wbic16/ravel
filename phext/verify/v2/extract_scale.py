#!/usr/bin/env python3
"""Extract SCALE-leg corpora from the real working phexts (SCALE.md inventory).
Each content (non-empty) scroll becomes one ravel event: text carried
byte-for-byte, coordinate kept as the event id (slashes -> dots).
Writes one JSON per item into ~/workspace/ravel-upgrade/v2/scale/."""
import json
import os
import sys

sys.path.insert(0, os.path.expanduser("~/workspace/phext"))
from parse_phext import parse  # noqa: E402

ITEMS = [
    "choose-your-own-adventure.phext",
    "tasks.phext",
    "artifacts.phext",
    "skills.phext",
    "incipit.phext",
    "state.phext",
    "life.phext",
]
OUT = os.path.expanduser("~/workspace/ravel-upgrade/v2/scale")
os.makedirs(OUT, exist_ok=True)

manifest = []
for fname in ITEMS:
    path = os.path.join(os.path.expanduser("~/workspace/phext"), fname)
    scrolls = parse(path)
    events = []
    for coord, text in scrolls:
        if not text.strip():
            continue
        events.append({
            "event_id": coord.replace("/", "."),
            "parent_id": None,
            "role": "user",
            "wrote": "nobody",
            "timestamp": None,
            "text": text,
        })
    item = fname.removesuffix(".phext")
    with open(os.path.join(OUT, item + ".json"), "w") as f:
        json.dump({"item": item, "source_bytes": os.path.getsize(path), "events": events}, f, ensure_ascii=False)
    manifest.append((item, os.path.getsize(path), len(events), sum(len(e["text"]) for e in events)))
    print(f"{item}: {len(events)} events, {sum(len(e['text']) for e in events)} text chars")

# the remaining inventory items, combined into one aggregate soul
import glob
rest = []
for path in sorted(glob.glob(os.path.expanduser("~/workspace/phext/*.phext"))):
    fname = os.path.basename(path)
    if fname in ITEMS or fname == "ietf.phext":
        continue
    for coord, text in parse(path):
        if not text.strip():
            continue
        rest.append({
            "event_id": (fname.removesuffix(".phext") + ":" + coord).replace("/", "."),
            "parent_id": None,
            "role": "user",
            "wrote": "nobody",
            "timestamp": None,
            "text": text,
        })
with open(os.path.join(OUT, "rest-combined.json"), "w") as f:
    json.dump({"item": "rest-combined", "source_bytes": None, "events": rest}, f, ensure_ascii=False)
print(f"rest-combined: {len(rest)} events from {len(glob.glob(os.path.expanduser('~/workspace/phext/*.phext'))) - len(ITEMS) - 1} phexts")
