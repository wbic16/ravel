"""raveld's HTTP route surface over the phext backend.

The 14 routes of daemon/http.rs, same shapes, backed by Souls on
phext scrolls. Deltas from the native daemon, named in PHEXT.md: the
tessera gate is not implemented (this binds 127.0.0.1 and answers
without a token); /sync and /import take seam-level payloads — events,
annotations, notes, summaries as JSON, exactly what the native sync
layer hands graph.rs — because the dialect adapters (Claude Code /
Antigravity / claude.ai export parsing) live above the seam and are
not part of the store swap; /health per soul reports store-level
findings, not sync::diagnose's mirror checks.
"""
import json
import os
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlparse

import ravel_backend as rb
from phextio import make_store

HERE = os.path.dirname(os.path.abspath(__file__))


def answer_json(out):
    """http.rs's answer(): W3C results JSON, or {"triples": [...]}."""
    if out["kind"] == "boolean":
        return {"head": {}, "boolean": out["value"]}
    if out["kind"] == "triples":
        return {"triples": [f"{rb.nt(s)} {rb.nt(p)} {rb.nt(o)} ." for s, p, o in out["triples"]]}
    bindings = []
    for row in out["rows"]:
        bindings.append({v: rb.term_json(row[v]) for v in out["vars"] if v in row})
    return {"head": {"vars": out["vars"]}, "results": {"bindings": bindings}}


def triples_text(out):
    return "".join(f"{rb.nt(s)} {rb.nt(p)} {rb.nt(o)} .\n" for s, p, o in out["triples"])


class Registry:
    def __init__(self, store, data_dir):
        self.store = store
        self.souls = {}  # (org, store) -> Soul
        self.seats = {}  # (org, store) -> Semaphore(4), the query seats
        cfg = os.path.join(data_dir, "souls.json")
        if os.path.exists(cfg):
            for entry in json.load(open(cfg)):
                self.add(entry)

    def add(self, entry):
        org, store_name = entry["org"], entry["store"]
        root = entry.get("root") or f"https://repolex.ai/{org}/{store_name}/"
        names = rb.Names(root, entry.get("user", store_name))
        sid = re.sub(r"[^A-Za-z0-9\-_]", "_", f"{org}--{store_name}")
        soul = rb.Soul(self.store, sid, names, entry.get("machine_owner"))
        soul.open()
        self.souls[(org, store_name)] = soul
        self.seats[(org, store_name)] = threading.Semaphore(4)
        return soul

    def get(self, org, store_name):
        return self.souls.get((org, store_name))


def apply_payload(soul, payload, replace):
    """The seam-level write surface /sync and /import share."""
    done = {}
    for t in payload.get("transcripts", []):
        done[t["id"]] = soul.ingest_transcript(
            t["id"], t.get("events", []), t.get("annotations", []), append=not replace
        )
    have = {n["id"] for n in soul.notes()}
    for n in payload.get("notes", []):
        if n["id"] not in have:
            soul.append_note(n)
    for s in payload.get("summaries", []):
        soul.upsert_summary(s)
    return done


def make_handler(reg, server):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *a):
            pass

        def _send(self, code, body, ctype="application/json", headers=None):
            data = body if isinstance(body, bytes) else body.encode()
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            for k, v in (headers or {}).items():
                self.send_header(k, v)
            self.end_headers()
            self.wfile.write(data)

        def _json(self, code, obj, headers=None):
            self._send(code, json.dumps(obj, ensure_ascii=False), headers=headers)

        def _body(self):
            n = int(self.headers.get("Content-Length") or 0)
            return self.rfile.read(n).decode("utf-8") if n else ""

        def _soul(self, org, store_name):
            soul = reg.get(org, store_name)
            if soul is None:
                self._json(404, {"error": f"no soul at {org}/{store_name}"})
            return soul

        def _run_query(self, org, store_name, sparql, sparql_route):
            soul = self._soul(org, store_name)
            if soul is None:
                return
            seat = reg.seats[(org, store_name)]
            if not seat.acquire(timeout=30):
                return self._json(503, {"error": "query seats busy"})
            try:
                out = soul.run_query(sparql)
            except rb.Refused as e:
                return self._json(400, {"error": str(e), "refused": e.construct})
            except rb.QueryError as e:
                return self._json(400, {"error": str(e)})
            finally:
                seat.release()
            notes = out.get("notes") or []
            if sparql_route:
                if out["kind"] == "triples":
                    headers = {"x-ravel-notes": " ;; ".join(notes)} if notes else None
                    return self._send(200, triples_text(out), "application/n-triples", headers)
                return self._json(200, answer_json(out), {"x-ravel-notes": " ;; ".join(notes)} if notes else None)
            body = answer_json(out)
            if notes:
                body["notes"] = notes
            return self._json(200, body)

        def do_GET(self):
            parts = [unquote(p) for p in urlparse(self.path).path.split("/") if p]
            if parts == ["ravel", "health"]:
                return self._json(200, {"ok": True, "backend": "phext", "souls": len(reg.souls)})
            if parts == ["ravel", "stores"]:
                return self._json(200, [
                    {"org": o, "store": s, "address": f"{o}/{s}", "root": soul.names.root}
                    for (o, s), soul in sorted(reg.souls.items())
                ])
            if len(parts) >= 4 and parts[0] == "ravel":
                org, store_name, rest = parts[1], parts[2], parts[3:]
                soul = self._soul(org, store_name)
                if soul is None:
                    return
                addr = f"{org}/{store_name}"
                if rest == ["memory"]:
                    v = rb.memory_view(soul)
                    v["store"] = addr
                    return self._json(200, v)
                if rest == ["memory-progress"]:
                    v = rb.memory_progress(soul)
                    v["store"] = addr
                    return self._json(200, v)
                if rest == ["memory-check"]:
                    v = rb.verify(soul)
                    if v["failures"]:
                        return self._json(200, {"store": addr, "check": "Failed", "failures": v["failures"]})
                    return self._json(200, {"store": addr, "check": "Verified"})
                if len(rest) == 2 and rest[0] == "memory":
                    try:
                        v = rb.memory_open(soul, rest[1])
                    except rb.QueryError as e:
                        return self._json(404, {"error": str(e)})
                    return self._json(200, v)
                if rest == ["health"]:
                    findings = rb.diagnose(soul)
                    return self._json(200, {
                        "store": addr,
                        "path": soul.file,
                        "attention": any(f["attention"] for f in findings),
                        "findings": findings,
                    })
                if rest == ["stats"]:
                    v = rb.soul_stats(soul)
                    v["store"] = addr
                    return self._json(200, v)
            return self._json(404, {"error": "no such route"})

        def do_POST(self):
            parts = [unquote(p) for p in urlparse(self.path).path.split("/") if p]
            if parts == ["ravel", "sync"]:
                payload = json.loads(self._body() or "{}")
                done = {}
                for addr, sub in payload.get("souls", {}).items():
                    org, _, store_name = addr.partition("/")
                    soul = self._soul(org, store_name)
                    if soul is not None:
                        done[addr] = apply_payload(soul, sub, replace=True)
                return self._json(200, {"synced": done})
            if parts == ["ravel", "shutdown"]:
                self._json(200, {"ok": True})
                threading.Thread(target=server.shutdown, daemon=True).start()
                return
            if len(parts) == 4 and parts[0] == "ravel":
                org, store_name, action = parts[1], parts[2], parts[3]
                if action == "sparql":
                    body = self._body()
                    if body.lstrip().startswith("{"):
                        body = json.loads(body).get("query", "")
                    return self._run_query(org, store_name, body, sparql_route=True)
                if action == "query":
                    body = json.loads(self._body() or "{}")
                    return self._run_query(org, store_name, body.get("query", ""), sparql_route=False)
                if action in ("sync", "import"):
                    soul = self._soul(org, store_name)
                    if soul is None:
                        return
                    payload = json.loads(self._body() or "{}")
                    done = apply_payload(soul, payload, replace=(action == "import"))
                    return self._json(200, {"store": f"{org}/{store_name}", "transcripts": done})
            return self._json(404, {"error": "no such route"})

    return Handler


def main():
    data = os.environ.get("RAVEL_PHEXT_DATA", os.path.join(HERE, "data"))
    port = int(os.environ.get("RAVEL_PORT", "7881"))
    sq_port = int(os.environ.get("RAVEL_SQ_PORT", "18991"))
    store = make_store(data, sq_port)
    store.start()
    reg = Registry(store, data)
    server = ThreadingHTTPServer(("127.0.0.1", port), make_handler(reg, server=None))
    server.RequestHandlerClass = make_handler(reg, server)
    print(f"ravel-phext backend on 127.0.0.1:{port}, data {data}, {len(reg.souls)} souls")
    try:
        server.serve_forever()
    finally:
        store.stop()


if __name__ == "__main__":
    main()
