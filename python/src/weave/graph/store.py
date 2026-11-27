"""Load a projected monolog into the phext graph layer and query it.

Re-pointed 2026-11-27 (phext-only graph rewrite): this module used to
bind pyoxigraph. The lab now runs on the same store/evaluator as
everything else in ravel — the phext SPARQL-subset evaluator, reached
through its line-protocol process `phext-sparql` (phext/serve). The
store here is just the projected N-Triples held as a set of lines
(idempotent by deterministic IRIs, as before); each query writes the
lines to a temp .nq, runs the evaluator, and parses its tab-separated
SOLUTIONS back into plain dicts, so callers never touch term objects.

The binary is found at $PHEXT_SPARQL_BIN, else at the repo-relative
phext/serve/target/release/phext-sparql (build: `cargo build --release`
in phext/serve). `weave.graph.project` (the .nt emitter) is pure stdlib
and always importable; only load/query needs the binary.
"""

from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Iterator

from .project import monolog_to_ntriples


def sparql_bin() -> Path:
    env = os.environ.get("PHEXT_SPARQL_BIN")
    if env:
        return Path(env)
    # python/src/weave/graph/store.py -> repo root is parents[4].
    return Path(__file__).resolve().parents[4] / "phext" / "serve" / "target" / "release" / "phext-sparql"


def available() -> bool:
    return sparql_bin().is_file() and os.access(sparql_bin(), os.X_OK)


def _require_bin() -> Path:
    b = sparql_bin()
    if not b.is_file():
        raise ImportError(
            "weave.graph.store needs the phext-sparql evaluator binary —\n"
            f"    not found at {b}\n"
            "    build it with: cargo build --release (in phext/serve),\n"
            "    or point PHEXT_SPARQL_BIN at it.\n"
            "(the monolog hot path never needs it; the graph is a derived cache.)"
        )
    return b


class PhextGraphStore:
    """The projected monolog as a set of N-Triples lines."""

    def __init__(self) -> None:
        self.lines: set[str] = set()

    def __len__(self) -> int:
        return len(self.lines)

    def add_nt(self, nt: str) -> None:
        for line in nt.splitlines():
            line = line.strip()
            if line:
                self.lines.add(line)


def load_monolog(transcript_path: str | Path, *, store: PhextGraphStore | None = None):
    """Project a transcript's monolog to RDF 1.2 and load it.

    Returns a PhextGraphStore (or loads into the one passed in). Loading
    the same monolog twice adds nothing — the IRIs are deterministic
    and the store is a set.
    """
    _require_bin()
    if store is None:
        store = PhextGraphStore()
    store.add_nt("\n".join(monolog_to_ntriples(transcript_path)) + "\n")
    return store


def query(store: PhextGraphStore, sparql: str) -> Iterator[dict[str, Any]]:
    """Run a SELECT and yield each solution as a plain {var: python-value}
    dict; an unbound variable is omitted from its row."""
    b = _require_bin()
    with tempfile.TemporaryDirectory(prefix="weave-graph-") as td:
        nq = Path(td) / "quads.nq"
        rq = Path(td) / "query.rq"
        nq.write_text("\n".join(sorted(store.lines)) + "\n", encoding="utf-8")
        rq.write_text(sparql, encoding="utf-8")
        out = subprocess.run(
            [str(b), str(nq), str(rq), "https://weave.repolex.ai/"],
            capture_output=True,
            text=True,
        )
    lines = out.stdout.splitlines()
    if out.returncode != 0 or (lines and lines[0].startswith(("ERROR", "REFUSED"))):
        raise RuntimeError(f"phext-sparql: {out.stdout.strip()} {out.stderr.strip()}".strip())
    rows = [l for l in lines if not l.startswith("NOTE\t")]
    if not rows or not rows[0].startswith("SOLUTIONS"):
        return
    variables = rows[0].split("\t")[1:]
    for row in rows[1:]:
        fields = row.split("\t")
        out_row: dict[str, Any] = {}
        for var, field in zip(variables, fields):
            if field:
                out_row[var] = _py(field)
        yield out_row


def _py(field: str) -> Any:
    """One N-Triples-form term -> plain python value: an IRI's string, a
    literal's lexical form, a triple term's N-Triples text."""
    if field.startswith("<") and field.endswith(">"):
        return field[1:-1]
    if field.startswith('"'):
        # "lex"^^<dt> / "lex"@lang / "lex" — take the quoted part, with
        # the standard escapes undone only for \" and \\ (all the lab's
        # own literals need; the full form stays in the store).
        end = field.rindex('"')
        body = field[1:end]
        return body.replace('\\"', '"').replace("\\\\", "\\")
    return field
