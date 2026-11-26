"""SQ REST client + phext lifecycle for the ravel v1 backend.

One `sq host <port>` process serves every soul phext in the data
directory over /api/v2, addressed by stem name (p=soul-a reads and
writes the file soul-a.phext). Behaviour measured against SQ v0.6.0
(libphext 0.3.3), probed 2026-11-23/24:

- `sq share <file>` is not a registration step: it starts a per-file
  daemon on port 18999 and does not exit. `sq host` is the HTTP
  front-end and needs no shares.
- GET  /api/v2/select?c=<coord>&p=<stem>  -> the scroll's text; a
  missing coordinate answers 200 with an EMPTY body, so existence is
  only ever decided by the table of contents.
- GET  /api/v2/toc?p=<stem>               -> "* <coord>: <title>" lines,
  one per non-empty scroll: the region enumeration primitive.
- POST /api/v2/update?c=<coord>&p=<stem>  raw body replaces the scroll.
- POST /api/v2/insert?c=<coord>&p=<stem>  raw body appends to the scroll.
- There is no delete: POST /api/v2/delete answers 404. Clearing a
  scroll is an update with an empty body; an emptied scroll leaves
  the toc.
- API writes persist into the .phext file on disk. A phext file the
  host has not seen yet is picked up by name on first use.
"""
import base64
import json
import os
import subprocess
import time
import urllib.request
import urllib.error
from contextlib import contextmanager
from urllib.parse import quote

SQ = os.path.expanduser("~/.cargo/bin/sq")
# phext-serve lives in this repo since the 2026-11-26 de-slop
# (phext/serve/, relocated from phext-lattice): default to its build.
HOLO_DEFAULT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "serve", "target", "release", "phext-serve",
    )
)


class SqError(RuntimeError):
    pass


def _stem(name):
    return name[:-6] if name.endswith(".phext") else name


class _StoreBase:
    """The transport surface the two stores share: one host process
    per data directory, serving every soul phext over /api/v2 by stem
    name. Subclasses differ only in the process they launch, their
    request timeout/label, and their write path (immediate vs the
    holo batch/txn machinery)."""

    _timeout = 30
    _label = "SQ"

    def __init__(self, data_dir, port):
        self.dir = data_dir
        self.port = port
        self.base = f"http://127.0.0.1:{port}/api/v2"
        self.proc = None

    # -- lifecycle ------------------------------------------------------
    def _launch(self, argv, label):
        os.makedirs(self.dir, exist_ok=True)
        self.proc = subprocess.Popen(
            argv,
            cwd=self.dir,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        deadline = time.time() + 20
        while time.time() < deadline:
            try:
                self._raw("GET", "/toc?p=__ping__")
                return
            except Exception:
                time.sleep(0.25)
        raise SqError(f"{label} on port {self.port} did not come up")

    def stop(self):
        if self.proc is not None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
            self.proc = None

    def ensure_file(self, name):
        """Create the phext file if absent (one delimiter byte); the
        host picks it up by stem name on first use."""
        path = os.path.join(self.dir, name)
        if not os.path.exists(path):
            with open(path, "wb") as fh:
                fh.write(b"\x01")

    # -- raw HTTP ---------------------------------------------------------
    def _raw(self, method, suffix, body=None, headers=None):
        return self._raw_full(method, suffix, body=body, headers=headers)[1]

    def _raw_full(self, method, suffix, body=None, headers=None):
        req = urllib.request.Request(
            self.base + suffix,
            method=method,
            data=body.encode("utf-8") if isinstance(body, str) else body,
            headers=headers or {},
        )
        try:
            with urllib.request.urlopen(req, timeout=self._timeout) as resp:
                return {k.lower(): v for k, v in resp.headers.items()}, resp.read().decode("utf-8")
        except urllib.error.URLError as e:
            raise SqError(f"{self._label} {method} {suffix}: {e}") from e

    # -- scroll operations --------------------------------------------------
    def read(self, name, coord):
        return self._raw("GET", f"/select?c={coord}&p={_stem(name)}")

    def exists(self, name, coord):
        return coord in self.toc(name)

    def toc(self, name):
        """coord -> title for every non-empty scroll in the phext."""
        out = {}
        for line in self._raw("GET", f"/toc?p={_stem(name)}").splitlines():
            line = line.strip()
            if line.startswith("*") and ":" in line:
                coord, title = line[1:].split(":", 1)
                out[coord.strip()] = title.strip()
        return out

    def coords_with_prefix(self, name, prefix):
        return sorted(c for c in self.toc(name) if c.startswith(prefix))

    def clear_region(self, name, prefix):
        """Empty every scroll under a coordinate prefix (neither host
        has a delete; an emptied scroll leaves the toc, which is the
        region)."""
        for coord in self.coords_with_prefix(name, prefix):
            self.write(name, coord, "")


class PhextStore(_StoreBase):
    """A directory of soul phexts behind one SQ host process."""

    def start(self):
        # A stale .sq/ state dir can wedge a fresh host; this dir is ours.
        stale = os.path.join(self.dir, ".sq")
        if os.path.isdir(stale):
            import shutil

            shutil.rmtree(stale)
        self._launch([SQ, "host", str(self.port)], "SQ host")

    def write(self, name, coord, text):
        self._raw("POST", f"/update?c={coord}&p={_stem(name)}", text)

    @contextmanager
    def batch(self, name):
        """Transport-neutral batch: SQ has no txn primitive, so writes
        stay immediate (a no-op passthrough with identical semantics)."""
        yield self


class HoloStore(_StoreBase):
    """Same public interface as PhextStore, against phext-serve.

    phext-serve is SQ-compatible on select/update/insert/toc and adds
    /txn (atomic multi-write), /seq + /updates (commit feed), /quads
    (maintained in-memory quad set) and /query (in-process SPARQL).
    On disk a soul is `<stem>.phext` + `<stem>.wal.phext` (WAL)."""

    _timeout = 120
    _label = "holo"

    def __init__(self, data_dir, port):
        super().__init__(data_dir, port)
        self.bin = os.environ.get("RAVEL_HOLO_BIN", HOLO_DEFAULT)
        self._batch_depth = 0
        self._batch_writes = {}  # (name, coord) -> text, insertion-ordered

    # -- lifecycle ------------------------------------------------------
    def start(self):
        self._launch([self.bin, str(self.port)], "phext-serve")

    def write(self, name, coord, text):
        if self._batch_depth:
            self._batch_writes[(name, coord)] = text
            return
        self._raw("POST", f"/update?c={coord}&p={_stem(name)}", text)

    # -- holo-only primitives ---------------------------------------------
    def txn(self, name, writes):
        """writes: iterable of (coord, text). One atomic commit."""
        payload = json.dumps({"writes": [[c, t] for c, t in writes]})
        out = self._raw("POST", f"/txn?p={_stem(name)}", payload,
                        headers={"Content-Type": "application/json"})
        return json.loads(out)["seq"]

    def seq(self, name):
        return json.loads(self._raw("GET", f"/seq?p={_stem(name)}"))["seq"]

    def updates(self, name, since):
        """List of {seq, coords} entries, or {"truncated": True}."""
        return json.loads(self._raw("GET", f"/updates?p={_stem(name)}&since={int(since)}"))

    def quads(self, name, payload):
        out = self._raw("POST", f"/quads?p={_stem(name)}", json.dumps(payload),
                        headers={"Content-Type": "application/json"})
        return json.loads(out)["quads"]

    def query(self, name, sparql, base, prep_text):
        """-> (exit:int, body:str); exit mirrors the phext-sparql bin."""
        headers = {"X-Prep-B64": base64.b64encode(prep_text.encode("utf-8")).decode("ascii")}
        hdrs, body = self._raw_full(
            "POST", f"/query?p={_stem(name)}&base={quote(base, safe='')}", sparql, headers=headers)
        return int(hdrs.get("x-exit", "2")), body

    @contextmanager
    def batch(self, name):
        """Buffer write() calls; flush as ONE /txn on outermost exit.
        Nested batches join the outer one. On exception the buffer is
        discarded (the txn never happens)."""
        outermost = self._batch_depth == 0
        if outermost:
            self._batch_writes = {}
        self._batch_depth += 1
        try:
            yield self
        except Exception:
            self._batch_depth -= 1
            if outermost:
                self._batch_writes = {}
            raise
        self._batch_depth -= 1
        if outermost:
            writes, self._batch_writes = self._batch_writes, {}
            if writes:
                self.txn(name, [(coord, text) for (_n, coord), text in writes.items()])


def make_store(data_dir, port):
    """HoloStore when RAVEL_TRANSPORT=holo, else the SQ PhextStore."""
    if os.environ.get("RAVEL_TRANSPORT") == "holo":
        return HoloStore(data_dir, port)
    return PhextStore(data_dir, port)
