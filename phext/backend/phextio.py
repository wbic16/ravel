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
import os
import subprocess
import time
import urllib.request
import urllib.error

SQ = os.path.expanduser("~/.cargo/bin/sq")


class SqError(RuntimeError):
    pass


def _stem(name):
    return name[:-6] if name.endswith(".phext") else name


class PhextStore:
    """A directory of soul phexts behind one SQ host process."""

    def __init__(self, data_dir, port):
        self.dir = data_dir
        self.port = port
        self.base = f"http://127.0.0.1:{port}/api/v2"
        self.proc = None

    # -- lifecycle ------------------------------------------------------
    def start(self):
        os.makedirs(self.dir, exist_ok=True)
        # A stale .sq/ state dir can wedge a fresh host; this dir is ours.
        stale = os.path.join(self.dir, ".sq")
        if os.path.isdir(stale):
            import shutil

            shutil.rmtree(stale)
        self.proc = subprocess.Popen(
            [SQ, "host", str(self.port)],
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
        raise SqError(f"SQ host on port {self.port} did not come up")

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
    def _raw(self, method, suffix, body=None):
        req = urllib.request.Request(
            self.base + suffix,
            method=method,
            data=body.encode("utf-8") if body is not None else None,
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return resp.read().decode("utf-8")
        except urllib.error.URLError as e:
            raise SqError(f"SQ {method} {suffix}: {e}") from e

    # -- scroll operations --------------------------------------------------
    def read(self, name, coord):
        return self._raw("GET", f"/select?c={coord}&p={_stem(name)}")

    def exists(self, name, coord):
        return coord in self.toc(name)

    def write(self, name, coord, text):
        self._raw("POST", f"/update?c={coord}&p={_stem(name)}", text)

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
        """Empty every scroll under a coordinate prefix (SQ has no
        delete; an emptied scroll leaves the toc, which is the region)."""
        for coord in self.coords_with_prefix(name, prefix):
            self.write(name, coord, "")
