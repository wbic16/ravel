"""The ravel store seam over one phext per soul.

Implements the seam graph.rs exposes (open / ingest_transcript /
append_transcript / keep_recent / query_rows) plus the fixed internal
queries and memory views the daemon's routes are built from, backed by
phext scrolls on the coordinate map in PHEXT.md and by the phext-lattice
SPARQL subset evaluator (the phext-sparql binary) behind a QuadSource
glue that projects the scrolls to quads on demand.
"""
import hashlib
import json
import os
import re
import subprocess
import tempfile
import time as _time
from datetime import datetime, timezone

import optmem

RAVEL = "https://repolex.ai/ontology/ravel/"
RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"
RDFS_MEMBER = "http://www.w3.org/2000/01/rdf-schema#member"
OA = "http://www.w3.org/ns/oa#"
PROV = "http://www.w3.org/ns/prov#"
XSD = "http://www.w3.org/2001/XMLSchema#"
ABSTRACT = "https://repolex.ai/ontology/subtexture/"
EVALUATOR = os.environ.get(
    "PHEXT_SPARQL_BIN",
    os.path.normpath(
        os.path.join(
            os.path.dirname(os.path.abspath(__file__)),
            "..", "serve", "target", "debug", "phext-sparql",
        )
    ),
)
ONTOLOGY_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ontology")


def coord(lib, vol, book, ch, sec, scroll):
    return f"{lib}.{vol}.{book}/{ch}.{sec}.{scroll}/1.1.1"


def safe(s):
    return re.sub(r"[^A-Za-z0-9\-_]", "_", s)


def parse_instant(s):
    """RFC3339 -> epoch milliseconds, or None when unparseable."""
    if not s:
        return None
    try:
        dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    except ValueError:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return int(dt.timestamp() * 1000)


def local_date(ts):
    try:
        dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
        return dt.date().isoformat()
    except (ValueError, AttributeError):
        return (ts or "")[:10]


# -- terms ----------------------------------------------------------------
# A term is ("iri", v) | ("lit", v, datatype|None, lang|None) |
# ("triple", (s, p, o)). Plain literals carry datatype None.

def iri(v):
    return ("iri", v)


def lit(v, dt=None):
    return ("lit", v, dt, None)


def nt_escape(s):
    return (
        s.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


def nt(t):
    if t[0] == "iri":
        return f"<{t[1]}>"
    if t[0] == "triple":
        s, p, o = t[1]
        return f"<<( {nt(s)} {nt(p)} {nt(o)} )>>"
    _, v, dt, lang = t
    out = f'"{nt_escape(v)}"'
    if lang:
        out += f"@{lang}"
    elif dt and dt != XSD + "string":
        out += f"^^<{dt}>"
    return out


def term_value(t):
    return t[1]


def term_json(t):
    """http.rs's term_json: a literal always carries its datatype."""
    if t[0] == "iri":
        return {"type": "uri", "value": t[1]}
    if t[0] == "triple":
        s, p, o = t[1]
        return {"type": "triple", "value": {"subject": term_json(s), "predicate": term_json(p), "object": term_json(o)}}
    _, v, dt, lang = t
    out = {"type": "literal", "value": v}
    if lang:
        out["xml:lang"] = lang
    else:
        out["datatype"] = dt or (XSD + "string")
    return out


_NT_ESCAPES = {"n": "\n", "r": "\r", "t": "\t", '"': '"', "\\": "\\"}


def parse_nt_term(s, i=0):
    """Parse one term of evaluator output at s[i:]; return (term, next)."""
    if s.startswith("<<(", i):
        j = i + 3
        while s[j] == " ":
            j += 1
        subj, j = parse_nt_term(s, j)
        while s[j] == " ":
            j += 1
        pred, j = parse_nt_term(s, j)
        while s[j] == " ":
            j += 1
        obj, j = parse_nt_term(s, j)
        while s[j] == " ":
            j += 1
        assert s.startswith(")>>", j), s
        return ("triple", (subj, pred, obj)), j + 3
    if s[i] == "<":
        j = s.index(">", i)
        return ("iri", s[i + 1 : j]), j + 1
    assert s[i] == '"', s[i:]
    j = i + 1
    buf = []
    while s[j] != '"':
        if s[j] == "\\":
            buf.append(_NT_ESCAPES[s[j + 1]])
            j += 2
        else:
            buf.append(s[j])
            j += 1
    j += 1
    dt = lang = None
    if s.startswith("^^<", j):
        k = s.index(">", j)
        dt, j = s[j + 3 : k], k + 1
    elif j < len(s) and s[j] == "@":
        k = j + 1
        while k < len(s) and (s[k].isalnum() or s[k] == "-"):
            k += 1
        lang, j = s[j + 1 : k], k
    return ("lit", "".join(buf), dt, lang), j


# -- names, claims, authorship ---------------------------------------------
class Names:
    def __init__(self, root, user):
        self.root = root
        self.user = user

    @classmethod
    def from_repo_yml(cls, text):
        squad = re.search(r"(?m)^\s*squad:\s*(\S+)\s*$", text)
        user = re.search(r"(?m)^\s*username:\s*(\S+)\s*$", text)
        if squad and user:
            return cls(f"https://repolex.ai/{squad.group(1)}/{user.group(1)}/", user.group(1))
        return cls("https://repolex.ai/", user.group(1) if user else "")

    def turn(self, event_id):
        return f"{self.root}ravel/Turn/{event_id}"

    def graph(self, gid):
        return f"{self.root}ravel/NamedGraph/{gid}"

    def memory(self, mid):
        return f"{self.root}ravel/Memory/{mid}"

    @property
    def owner(self):
        return f"{self.root}soul/Soul/{self.user}" if self.user else None


_HERDR = re.compile(r"^--herdr v=1((?: [a-z_]+=[^ ]+)*)--$")
_SLUG = re.compile(r"^[A-Za-z0-9\-_]+$")


def herdr_claims(text):
    """The `--herdr v=1 k=v...--` first line, as a claims dict or None."""
    if not text:
        return None
    m = _HERDR.match(text.split("\n", 1)[0])
    if not m:
        return None
    claims = {}
    for pair in m.group(1).split():
        k, _, v = pair.partition("=")
        if k == "soul" and not re.fullmatch(r"[0-9a-f]{40}", v):
            continue
        if k == "user" and not _SLUG.match(v):
            continue
        claims[k] = v
    return claims or None


_PEER = re.compile(
    r"^\[([A-Za-z0-9\-_]{1,40})\]$|^\[([A-Za-z0-9\-_]{1,40}) -> ([A-Za-z0-9\-_]{1,40})\]$"
)


def unsigned_peer(text):
    return bool(text) and bool(_PEER.match(text.split("\n", 1)[0]))


def derive_author(ev, names, machine_owner):
    """project.rs's author(): who a turn is attributed to, or None."""
    claims = herdr_claims(ev.get("text"))
    if claims and claims.get("user"):
        return claims["user"]
    wrote = ev.get("wrote", "soul")
    if wrote == "soul":
        soul = (claims or {}).get("soul") or names.user
        if not soul:
            return None
        model = ev.get("model")
        return f"{soul}-on-{model}" if model else soul
    if wrote == "person":
        if unsigned_peer(ev.get("text")):
            return None
        return machine_owner
    return None


# -- annotations -------------------------------------------------------------
_KIND_TAG = {"text": 0, "int": 1, "decimal": 2}
_KIND_DT = {"int": XSD + "integer", "decimal": XSD + "decimal"}


def ann_hash(transcript_id, ann):
    """annotate.rs's ann_hash: first 8 bytes of SHA-256, hex."""
    h = hashlib.sha256()

    def put(b):
        h.update(b if isinstance(b, bytes) else b.encode())

    put(transcript_id)
    put(b"\0")
    put(ann["reader"])
    put(b"\0")
    put(ann["event_type"])
    put(b"\0")
    put(ann["event_id"])
    put(b"\0")
    put(int(ann["start"]).to_bytes(8, "little"))
    put(int(ann["end"]).to_bytes(8, "little"))
    put(b"\0")
    put(ann.get("source_kind") or "")
    for key in sorted(ann["signal"]):
        kind, lexical = ann["signal"][key]
        put(b"\0")
        put(key)
        put(b"\1")
        put(bytes([_KIND_TAG[kind]]))
        put(lexical)
    return h.digest()[:8].hex()


def annotation_quads(names, base, graph, transcript_id, ann):
    """annotate.rs's annotation_nt + claim_nt, as (g, s, p, o) quads."""
    h = ann_hash(transcript_id, ann)
    ann_iri = iri(f"{base}Annotation/{h}")
    claim = iri(f"{base}Claim/{h}")
    detector = iri(f"{base}Detector/{safe(ann['reader'])}")
    transcript = iri(f"{base}Transcript/{safe(transcript_id)}")
    target = iri(f"{base}Annotation/{h}/target")
    selector = iri(f"{base}Annotation/{h}/selector")
    body = iri(f"{base}Annotation/{h}/body")
    event = iri(names.turn(ann["event_id"]))
    q = []

    def add(s, p, o):
        q.append((graph, s, p, o))

    add(detector, iri(RDF_TYPE), iri(PROV + "SoftwareAgent"))
    add(detector, iri(RAVEL + "readerName"), lit(ann["reader"]))
    add(ann_iri, iri(RDF_TYPE), iri(OA + "Annotation"))
    add(ann_iri, iri(PROV + "wasAttributedTo"), detector)
    add(ann_iri, iri(RAVEL + "detectionType"), lit(ann["event_type"]))
    if ann.get("ts"):
        add(ann_iri, iri(PROV + "generatedAtTime"), lit(ann["ts"], XSD + "dateTime"))
    if ann.get("source_kind"):
        add(ann_iri, iri(RAVEL + "textOrigin"), lit(ann["source_kind"]))
    add(ann_iri, iri(OA + "hasTarget"), target)
    add(target, iri(OA + "hasSource"), transcript)
    add(target, iri(OA + "hasSelector"), selector)
    add(selector, iri(RDF_TYPE), iri(OA + "TextPositionSelector"))
    add(selector, iri(OA + "start"), lit(str(int(ann["start"])), XSD + "integer"))
    add(selector, iri(OA + "end"), lit(str(int(ann["end"])), XSD + "integer"))
    add(target, iri(RAVEL + "atTurn"), event)
    add(ann_iri, iri(PROV + "used"), event)
    add(ann_iri, iri(OA + "hasBody"), body)
    for key in sorted(ann["signal"]):
        kind, lexical = ann["signal"][key]
        add(body, iri(RAVEL + "sig_" + safe(key)), lit(lexical, _KIND_DT.get(kind)))
    exhibits = ("triple", (event, iri(RAVEL + "exhibits"), lit(ann["event_type"])))
    add(claim, iri(REIFIES), exhibits)
    add(claim, iri(PROV + "wasDerivedFrom"), ann_iri)
    add(claim, iri(RAVEL + "detectorName"), lit(ann["reader"]))
    add(claim, iri(RAVEL + "detectionType"), lit(ann["event_type"]))
    return q


def spine_quads(names, graph, ev, machine_owner):
    """project.rs's project_nt for one event, as (g, s, p, o) quads."""
    turn = iri(names.turn(ev["event_id"]))
    q = []

    def add(p, o):
        q.append((graph, turn, iri(p), o))

    add(RDF_TYPE, iri(RAVEL + "Turn"))
    add(RAVEL + "turnId", lit(ev["event_id"]))
    if ev.get("role"):
        add(RAVEL + "role", lit(ev["role"]))
    if ev.get("parent_id"):
        add(RAVEL + "parentTurn", iri(names.turn(ev["parent_id"])))
    if ev.get("timestamp"):
        add(RAVEL + "timestamp", lit(ev["timestamp"], XSD + "dateTime"))
    if ev.get("text") is not None:
        add(RAVEL + "text", lit(ev["text"]))
    if ev.get("model_setting"):
        add(RAVEL + "modelSetting", lit(ev["model_setting"]))
    # A stored event carries its author (derived once, at ingest, with
    # the ingesting machine's owner); a fresh event derives it here.
    author = ev["author"] if "author" in ev else derive_author(ev, names, machine_owner)
    if author:
        add(RAVEL + "author", lit(author))
    if ev.get("role") == "user":
        claims = herdr_claims(ev.get("text"))
        if claims:
            for key, pred in [
                ("from", "claimedSender"),
                ("pane", "claimedSenderPane"),
                ("session", "claimedSenderSession"),
                ("soul", "claimedSenderSoul"),
                ("user", "claimedSenderUser"),
            ]:
                if claims.get(key):
                    add(RAVEL + pred, lit(claims[key]))
    return q


# -- the ontology preparation (outside.rs's calamus step, as data) --------
_PREP = None


_RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
_EDGE_KINDS = {
    "http://www.w3.org/2000/01/rdf-schema#subPropertyOf": ("prop", False),
    "http://www.w3.org/2002/07/owl#equivalentProperty": ("prop", True),
    "http://www.w3.org/2000/01/rdf-schema#subClassOf": ("class", False),
    "http://www.w3.org/2002/07/owl#equivalentClass": ("class", True),
}


def _ttl_tokens(text):
    """Tokenize Turtle: IRIs, string literals, names, structural ; , . [ ] ( ).
    Comments (# to end of line) are dropped outside strings and IRIs."""
    toks, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c in " \t\r\n":
            i += 1
        elif c == "#":
            while i < n and text[i] != "\n":
                i += 1
        elif c == "<":
            j = text.index(">", i)
            toks.append(("iri", text[i + 1:j]))
            i = j + 1
        elif c in "\"'":
            q = c * 3
            if text.startswith(q, i):
                j = text.index(q, i + 3)
                toks.append(("lit", text[i + 3:j]))
                i = j + 3
            else:
                j = i + 1
                buf = []
                while text[j] != c:
                    if text[j] == "\\":
                        buf.append(text[j:j + 2])
                        j += 2
                    else:
                        buf.append(text[j])
                        j += 1
                toks.append(("lit", "".join(buf)))
                i = j + 1
        elif c in ";,.[]()":
            toks.append((c, c))
            i += 1
        elif c == "@":
            j = i + 1
            while j < n and (text[j].isalnum() or text[j] in "-_"):
                j += 1
            toks.append(("dir", text[i + 1:j]))
            i = j
        else:
            j = i
            while j < n and text[j] not in " \t\r\n;,.,[]()<>\"'#":
                j += 1
            if j == i:  # a lone '.' ends a name token above; guard anyway
                j = i + 1
            toks.append(("word", text[i:j]))
            i = j
    return toks


def _ttl_triples(text, prefixes):
    """Parse Turtle statements (subject + predicate-object lists) and return
    (prefixes, [(s, p, o)]) with every term expanded; only triples whose
    subject and object are named nodes are kept (calamus's build filter)."""
    toks = _ttl_tokens(text)
    out = []
    pos = 0

    def expand(tok):
        kind, val = tok
        if kind == "iri":
            return val
        if kind == "word":
            if val == "a":
                return _RDF_TYPE
            if ":" in val:
                p, local = val.split(":", 1)
                return prefixes.get(p, p + ":") + local
            return val
        return None

    def skip_block(pos, open_t, close_t):
        depth = 0
        while pos < len(toks):
            if toks[pos][0] == open_t:
                depth += 1
            elif toks[pos][0] == close_t:
                depth -= 1
                if depth == 0:
                    return pos + 1
            pos += 1
        return pos

    while pos < len(toks):
        kind, val = toks[pos]
        if kind == "dir":  # @prefix p: <iri> .
            if val in ("prefix", "base") and pos + 2 < len(toks):
                if val == "prefix":
                    prefixes[toks[pos + 1][1].rstrip(":")] = toks[pos + 2][1]
                pos += 3
                if pos < len(toks) and toks[pos][0] == ".":
                    pos += 1
            else:
                pos += 1
            continue
        subj = expand(toks[pos])
        pos += 1
        while pos < len(toks):  # predicate-object list
            if toks[pos][0] == ".":
                pos += 1
                break
            if toks[pos][0] == ";":
                pos += 1
                continue
            pred = expand(toks[pos])
            pos += 1
            while pos < len(toks):  # object list
                if toks[pos][0] == "[":
                    pos = skip_block(pos, "[", "]")
                    obj = None
                elif toks[pos][0] == "(":
                    pos = skip_block(pos, "(", ")")
                    obj = None
                else:
                    obj = expand(toks[pos])
                    pos += 1
                if subj is not None and pred is not None and obj is not None:
                    out.append((subj, pred, obj))
                if pos < len(toks) and toks[pos][0] == ",":
                    pos += 1
                    continue
                break
            if pos < len(toks) and toks[pos][0] == ";":
                continue
            if pos < len(toks) and toks[pos][0] == ".":
                pos += 1
            break
    return prefixes, out


def _calamus_closure(edges, abstract_prefixes):
    """calamus ontology.rs closure(): for every name, everything reachable
    from it (itself included), abstract names dropped, in address order.
    A name whose stored answer is only itself is left out (asked as written)."""
    out = {}
    for start in edges:
        seen, todo = set(), [start]
        while todo:
            name = todo.pop()
            if name not in seen:
                seen.add(name)
                todo.extend(edges.get(name, ()))
        stored = sorted(x for x in seen if not any(x.startswith(p) for p in abstract_prefixes))
        if not stored or stored == [start]:
            continue
        out[start] = stored
    return out


def ontology_prep():
    """Derive the evaluator's Prep from ravel's two shipped ontologies.

    A faithful port of calamus's Index (ontology.rs): a property is
    satisfied by itself, its sub-properties and its equivalents (in either
    direction, transitively); a class likewise with subClassOf /
    equivalentClass. Abstract (subtexture:) names are dropped from every
    answer. Replaces the v1 derivation, whose line regex matched zero
    edges in the shipped Turtle (the ontologies use predicate lists, so
    the subject never stands next to the predicate) and silently produced
    empty maps — see V2-REPORT erratum C-1."""
    global _PREP
    if _PREP is not None:
        return _PREP
    prefixes = {"owl": "http://www.w3.org/2002/07/owl#", "rdfs": "http://www.w3.org/2000/01/rdf-schema#"}
    prop_edges, class_edges = {}, {}
    for fname in ("ravel.ttl", "subtexture.ttl"):
        text = open(os.path.join(ONTOLOGY_DIR, fname)).read()
        prefixes, triples = _ttl_triples(text, prefixes)
        for s, p, o in triples:
            kind = _EDGE_KINDS.get(p)
            if kind is None:
                continue
            edges = prop_edges if kind[0] == "prop" else class_edges
            edges.setdefault(o, set()).add(s)
            edges.setdefault(s, set())
            if kind[1]:  # equivalents point both ways
                edges.setdefault(s, set()).add(o)
    prop_equiv = _calamus_closure(prop_edges, [ABSTRACT])
    class_subs = _calamus_closure(class_edges, [ABSTRACT])
    prop_equiv.pop(_RDF_TYPE, None)  # calamus: rdf:type is never swapped
    _PREP = (prop_equiv, class_subs)
    return _PREP


class Refused(Exception):
    def __init__(self, construct, message):
        super().__init__(message)
        self.construct = construct


class QueryError(Exception):
    pass


def display_name(iri_str):
    if iri_str.startswith(ABSTRACT):
        return "subtexture:" + iri_str[len(ABSTRACT):]
    if iri_str.startswith(RAVEL):
        return "ravel:" + iri_str[len(RAVEL):]
    return iri_str


# -- one soul -----------------------------------------------------------------
def parse_coord(c):
    a, b, _ = c.split("/")
    lib, vol, book = a.split(".")
    ch, sec, scroll = b.split(".")
    return tuple(int(x) for x in (lib, vol, book, ch, sec, scroll))


class Soul:
    def __init__(self, store, soul_id, names, machine_owner=None):
        self.store = store
        self.id = soul_id
        self.names = names
        self.machine_owner = machine_owner
        self.file = f"{soul_id}.phext"
        self._turn_coord = {}  # event_id -> identity scroll coordinate
        self._coord_turn = {}  # identity scroll coordinate -> event_id
        self._dirty = True
        self._inputs = None  # cached (nquads, prep) for the evaluator
        self._transcript_cache = {}  # tid -> (events, annotations)
        # holo bridge state (unused in SQ mode)
        self._holo_synced = False
        self._holo_seq = 0
        self._holo_sections = None  # section key -> set of NT lines
        self._holo_needs_replace = False

    @property
    def _is_holo(self):
        return hasattr(self.store, "quads") and hasattr(self.store, "query")

    def _invalidate_tid(self, tid):
        self._transcript_cache.pop(tid, None)

    # -- open ------------------------------------------------------------
    def open(self):
        self.store.ensure_file(self.file)
        if not self.store.exists(self.file, coord(1, 1, 1, 1, 1, 1)):
            self.store.write(
                self.file,
                coord(1, 1, 1, 1, 1, 1),
                json.dumps(
                    {
                        "format": "ravel-phext",
                        "version": 1,
                        "source": "ravel 0.12.1 @ 003fee97aaf97ba021e8dc33ac62eba0b0e6756f",
                        "root": self.names.root,
                        "user": self.names.user,
                        "registry": "1.1.1/1.2.1/1.1.1",
                    }
                ),
            )
        for _tid, slot in self.registry():
            vol = slot + 2
            for book in self._books(vol):
                c = coord(1, vol, book, 1, 1, 1)
                eid = self.store.read(self.file, c)
                if eid:
                    self._turn_coord[eid] = c
                    self._coord_turn[c] = eid
        return self

    def registry(self):
        out = []
        for c in self.store.toc(self.file):
            lib, vol, book, ch, sec, scroll = parse_coord(c)
            if (lib, vol, book, ch, sec) == (1, 1, 1, 1, 2):
                out.append((self.store.read(self.file, c), scroll - 1))
        return sorted(out, key=lambda x: x[1])

    def _slot_of(self, tid, create=False):
        reg = self.registry()
        for t, slot in reg:
            if t == tid:
                return slot
        if not create:
            return None
        slot = len(reg)
        self.store.write(self.file, coord(1, 1, 1, 1, 2, slot + 1), tid)
        return slot

    def _books(self, vol, lib=1):
        books = set()
        for c in self.store.toc(self.file):
            l2, v2, book, ch, sec, scroll = parse_coord(c)
            if (l2, v2) == (lib, vol):
                books.add(book)
        return sorted(books)

    # -- the store seam: ingest / append ----------------------------------
    def ingest_transcript(self, tid, events, annotations, append=False):
        with self.store.batch(self.file):
            result = self._ingest_transcript_impl(tid, events, annotations, append=append)
        self._holo_eager_sync({tid})
        return result

    def _primed_transcript(self, events, annotations):
        """The (events, annotations) read_transcript would return for a
        transcript written from these inputs — _write_event's scrolls
        read back through _read_event / _read_annotations, field for
        field: the author is the derived author as stored (None when no
        author scroll was written); an optional field survives only when
        a non-empty scroll was written for it; claims carry only the
        five stored keys; annotation signals stand in sorted-key order
        as (kind, lexical) tuples; annotations are grouped under their
        event, in event order (orphans are never written, so never
        read back)."""
        by_event = {}
        for a in annotations:
            by_event.setdefault(a["event_id"], []).append(a)
        evs, anns = [], []
        for ev in events:
            stored = {
                "event_id": ev["event_id"],
                "author": derive_author(ev, self.names, self.machine_owner) or None,
            }
            for key in ("role", "model_setting", "timestamp", "text", "parent_id"):
                if ev.get(key):
                    stored[key] = ev[key]
            if ev.get("role") == "user":
                claims_in = herdr_claims(ev.get("text")) or {}
                claims = {
                    k: claims_in[k]
                    for k in ("from", "pane", "session", "soul", "user")
                    if claims_in.get(k)
                }
                if claims:
                    stored["claims"] = claims
            evs.append(stored)
            for a in by_event.get(ev["event_id"], []):
                signal = {}
                for key in sorted(a["signal"]):
                    kind, lexical = a["signal"][key]
                    signal[key] = (kind, lexical)
                anns.append(
                    {
                        "reader": a["reader"] or None,
                        "event_type": a["event_type"] or None,
                        "event_id": a["event_id"],
                        "start": int(a["start"]),
                        "end": int(a["end"]),
                        "source_kind": a.get("source_kind") or None,
                        "ts": a.get("ts") or None,
                        "signal": signal,
                    }
                )
        return evs, anns

    def _ingest_transcript_impl(self, tid, events, annotations, append=False):
        slot = self._slot_of(tid, create=True)
        vol = slot + 2
        # Holo priming for the append path needs the pre-write read-back
        # form; capture it before any scroll of this transcript moves.
        old_events, old_anns = (
            self.read_transcript(tid) if (append and self._is_holo) else ([], [])
        )
        if append:
            start = len(self._books(vol))
        else:
            self.store.clear_region(self.file, f"1.{vol}.")
            start = 0
            self._turn_coord = {
                k: v for k, v in self._turn_coord.items() if not v.startswith(f"1.{vol}.")
            }
            self._coord_turn = {v: k for k, v in self._turn_coord.items()}
        by_event = {}
        for a in annotations:
            by_event.setdefault(a["event_id"], []).append(a)
        for i, ev in enumerate(events):
            c = coord(1, vol, start + i + 1, 1, 1, 1)
            self._turn_coord[ev["event_id"]] = c
            self._coord_turn[c] = ev["event_id"]
        quads = 0
        for i, ev in enumerate(events):
            anns = by_event.get(ev["event_id"], [])
            self._write_event(vol, start + i, ev, anns)
            quads += len(spine_quads(self.names, self.names.graph(tid), ev, self.machine_owner))
            for a in anns:
                quads += len(annotation_quads(self.names, f"{self.names.root}ravel/", self.names.graph(tid), tid, a))
        for sec, val in ((1, len(events)), (2, len(annotations)), (3, quads)):
            self.store.write(self.file, coord(3, vol, 1, 1, sec, 1), str(val))
        if self._is_holo:
            # Prime, don't invalidate: the cache now holds exactly what
            # read_transcript would read back for this write, so the
            # eager sync (and every later derivation) needs no re-read.
            primed_events, primed_anns = self._primed_transcript(events, annotations)
            if append:
                primed_events = old_events + primed_events
                primed_anns = old_anns + primed_anns
            self._transcript_cache[tid] = (primed_events, primed_anns)
        else:
            self._invalidate_tid(tid)
        self._dirty = True
        return {"turns": len(events), "annotations": len(annotations), "quads": quads}

    def _write_event(self, vol, pos, ev, anns):
        f, W = self.file, self.store.write
        b = pos + 1

        def put(ch, sec, scroll, text):
            if text:
                W(f, coord(1, vol, b, ch, sec, scroll), text)

        put(1, 1, 1, ev["event_id"])
        put(1, 2, 1, ev.get("role"))
        put(1, 3, 1, derive_author(ev, self.names, self.machine_owner))
        put(1, 4, 1, ev.get("model_setting"))
        if ev.get("role") == "user":
            claims = herdr_claims(ev.get("text")) or {}
            for i, key in enumerate(("from", "pane", "session", "soul", "user")):
                put(1, 5 + i, 1, claims.get(key))
        put(2, 1, 1, ev.get("timestamp"))
        put(3, 1, 1, ev.get("text"))
        if ev.get("parent_id"):
            put(4, 1, 1, self._turn_coord.get(ev["parent_id"]))
            put(4, 2, 1, ev["parent_id"])
        for j, ann in enumerate(anns):
            sec = j + 1
            put(5, sec, 1, ann["reader"])
            put(5, sec, 2, ann["event_type"])
            put(5, sec, 3, str(int(ann["start"])))
            put(5, sec, 4, str(int(ann["end"])))
            put(5, sec, 5, ann.get("source_kind"))
            put(5, sec, 6, ann.get("ts"))
            for k, key in enumerate(sorted(ann["signal"])):
                kind, lexical = ann["signal"][key]
                put(5, sec, 7 + k, f"{key}\n{kind}\n{lexical}")

    # -- reading the store back ---------------------------------------------
    def read_transcript(self, tid):
        if tid in self._transcript_cache:
            return self._transcript_cache[tid]
        slot = self._slot_of(tid)
        if slot is None:
            return [], []
        vol = slot + 2
        events, annotations = [], []
        for book in self._books(vol):
            ev = self._read_event(vol, book)
            if ev is None:
                continue
            events.append(ev)
            annotations.extend(self._read_annotations(vol, book))
        self._transcript_cache[tid] = (events, annotations)
        return events, annotations

    def _read_event(self, vol, book):
        f, R = self.file, self.store.read

        def get(ch, sec, scroll=1):
            return R(f, coord(1, vol, book, ch, sec, scroll)) or None

        eid = get(1, 1)
        if not eid:
            return None
        ev = {"event_id": eid, "author": get(1, 3)}
        for key, val in (
            ("role", get(1, 2)),
            ("model_setting", get(1, 4)),
            ("timestamp", get(2, 1)),
            ("text", get(3, 1)),
            ("parent_id", get(4, 2)),
        ):
            if val is not None:
                ev[key] = val
        claims = {}
        for i, key in enumerate(("from", "pane", "session", "soul", "user")):
            if (v := get(1, 5 + i)) is not None:
                claims[key] = v
        if claims:
            ev["claims"] = claims
        return ev

    def _read_annotations(self, vol, book):
        f = self.file
        secs = set()
        for c in self.store.toc(f):
            lib, v2, b2, ch, sec, scroll = parse_coord(c)
            if (lib, v2, b2, ch) == (1, vol, book, 5):
                secs.add(sec)
        out = []
        for sec in sorted(secs):
            R = self.store.read

            def get(scroll):
                return R(f, coord(1, vol, book, 5, sec, scroll)) or None

            ann = {
                "reader": get(1),
                "event_type": get(2),
                "event_id": self._coord_turn.get(coord(1, vol, book, 1, 1, 1)),
                "start": int(get(3) or 0),
                "end": int(get(4) or 0),
                "source_kind": get(5),
                "ts": get(6),
                "signal": {},
            }
            scroll = 7
            while (leg := get(scroll)) is not None:
                key, kind, lexical = leg.split("\n", 2)
                ann["signal"][key] = (kind, lexical)
                scroll += 1
            out.append(ann)
        return out

    # -- memory: notes and summaries -----------------------------------------
    def notes(self):
        out = []
        for book in self._books(1, lib=2):
            R = self.store.read

            def get(ch, sec, scroll=1):
                return R(self.file, coord(2, 1, book, ch, sec, scroll)) or None

            if get(1, 1) is None:
                continue
            note = {"id": get(1, 1), "ts": get(1, 2) or "", "text": get(1, 3) or "", "model": get(1, 4) or ""}
            turns = []
            sec = 1
            while (c := get(2, sec)) is not None:
                turns.append(self._coord_turn.get(c) or (c[3:] if c.startswith("id:") else c))
                sec += 1
            note["turns"] = turns
            out.append(note)
        return out

    def append_note(self, note):
        with self.store.batch(self.file):
            result = self._append_note_impl(note)
        self._holo_eager_sync(set())
        return result

    def _append_note_impl(self, note):
        book = len(self._books(1, lib=2)) + 1
        W = self.store.write
        for sec, val in ((1, note["id"]), (2, note["ts"]), (3, note["text"]), (4, note["model"])):
            if val:
                W(self.file, coord(2, 1, book, 1, sec, 1), val)
        for j, tid in enumerate(note["turns"]):
            W(self.file, coord(2, 1, book, 2, j + 1, 1), self._turn_coord.get(tid) or f"id:{tid}")
        self._dirty = True

    def summaries(self):
        out = []
        toc = self.store.toc(self.file)
        for c in toc:
            lib, vol, book, ch, sec, scroll = parse_coord(c)
            if (lib, vol, sec, scroll) == (2, 2, 1, 1):
                R = self.store.read
                get = lambda s2: R(self.file, coord(2, 2, book, ch, s2, 1)) or None  # noqa: E731
                s = {"block": optmem.block_id(book - 1, book - 1 + ch),
                     "text": get(1) or "", "model": get(2) or "", "written": get(3) or ""}
                if get(4) is not None:
                    s["wording"] = get(4)
                out.append(s)
        return out

    def upsert_summary(self, s):
        with self.store.batch(self.file):
            result = self._upsert_summary_impl(s)
        self._holo_eager_sync(set())
        return result

    def _upsert_summary_impl(self, s):
        span = optmem.parse_block(s["block"])
        if span is None:
            raise ValueError(f"bad block id {s['block']!r}")
        lo, hi = span
        W = self.store.write
        for sec, val in ((1, s["text"]), (2, s["model"]), (3, s["written"]), (4, s.get("wording"))):
            if val:
                W(self.file, coord(2, 2, lo + 1, hi - lo, sec, 1), val)
        self._dirty = True

    def built(self):
        return optmem.built_blocks(self.summaries(), len(self.notes()))

    # -- the QuadSource glue ---------------------------------------------------
    def _desc_quads(self, quads, gname, cls):
        g = iri(self.names.graph(gname))
        quads.append((None, g, iri(RDF_TYPE), iri(RAVEL + cls)))
        quads.append((None, g, iri(RAVEL + "visibility"), lit("private")))
        if self.names.owner:
            quads.append((None, g, iri(RAVEL + "owner"), iri(self.names.owner)))

    def _transcript_quads(self, tid):
        quads = []
        self._desc_quads(quads, tid, "TranscriptGraph")
        events, annotations = self.read_transcript(tid)
        gname = self.names.graph(tid)
        base = f"{self.names.root}ravel/"
        for ev in events:
            quads += spine_quads(self.names, gname, ev, self.machine_owner)
        for a in annotations:
            quads += annotation_quads(self.names, base, gname, tid, a)
        return quads

    def _global_quads(self):
        """Notes/memory + recent-turns quads: the non-transcript section.
        Depends on every transcript (recent), so any transcript change
        re-derives it in holo mode."""
        quads = []
        notes = self.notes()
        if notes:
            self._desc_quads(quads, "optmem", "MemoryGraph")
            quads += self._memory_quads(notes)
        recent = self.recent_turns()
        if recent:
            self._desc_quads(quads, "recent", "NamedGraph")
            rg = self.names.graph("recent")
            for t in recent:
                quads.append((rg, iri(rg), iri(RDFS_MEMBER), iri(t)))
        return quads

    def all_quads(self):
        if self._is_holo and self._holo_synced and not getattr(self, "_holo_in_sync", False):
            self._holo_poll_cache()
        quads = []
        for tid, _slot in self.registry():
            quads += self._transcript_quads(tid)
        quads += self._global_quads()
        return quads

    def _memory_quads(self, notes):
        g = self.names.graph("optmem")
        sums = {s["block"]: s for s in self.summaries()}
        q = []

        def add(node, p, o):
            q.append((g, node, iri(p), o))

        def node_id(lo, hi):
            return notes[lo]["id"] if hi - lo == 1 else optmem.block_id(lo, hi)

        for i, n in enumerate(notes):
            m = iri(self.names.memory(n["id"]))
            add(m, RDF_TYPE, iri(RAVEL + "Memory"))
            add(m, RAVEL + "memoryId", lit(n["id"]))
            add(m, RAVEL + "memoryLevel", lit("0", XSD + "integer"))
            add(m, RAVEL + "logStart", lit(str(i), XSD + "integer"))
            add(m, RAVEL + "logEnd", lit(str(i), XSD + "integer"))
            add(m, RAVEL + "memoryText", lit(n["text"]))
            add(m, RAVEL + "windowStart", lit(n["ts"], XSD + "dateTime"))
            add(m, RAVEL + "windowEnd", lit(n["ts"], XSD + "dateTime"))
            add(m, RAVEL + "memoryModel", lit(n["model"]))
            for t in n["turns"]:
                add(m, RAVEL + "relatedToId", iri(self.names.turn(t)))
        for bid in sorted(sums):
            span = optmem.parse_block(bid)
            if span is None or span[1] > len(notes):
                continue
            lo, hi = span
            s = sums[bid]
            m = iri(self.names.memory(bid))
            add(m, RDF_TYPE, iri(RAVEL + "Memory"))
            add(m, RAVEL + "memoryId", lit(bid))
            add(m, RAVEL + "memoryLevel", lit(str((hi - lo).bit_length() - 1), XSD + "integer"))
            add(m, RAVEL + "logStart", lit(str(lo), XSD + "integer"))
            add(m, RAVEL + "logEnd", lit(str(hi - 1), XSD + "integer"))
            add(m, RAVEL + "memoryText", lit(s["text"]))
            add(m, RAVEL + "windowStart", lit(notes[lo]["ts"], XSD + "dateTime"))
            add(m, RAVEL + "windowEnd", lit(notes[hi - 1]["ts"], XSD + "dateTime"))
            add(m, RAVEL + "memoryModel", lit(s["model"]))
            mid = (lo + hi) // 2
            add(m, RAVEL + "relatedToId", iri(self.names.memory(node_id(lo, mid))))
            add(m, RAVEL + "relatedToId", iri(self.names.memory(node_id(mid, hi))))
        return q

    def recent_turns(self, limit=500):
        """keep_recent, derived: the newest `limit` turns store-wide.

        Native keep_recent (graph.rs) only ever weighs turns that carry a
        timestamp: a turn with none is never a candidate, on either the
        first-build or the incremental path. A turn whose timestamp string
        is present but unparseable sorts oldest (instant() -> i64::MIN)
        yet remains a candidate. v1 listed timestamp-less turns too —
        see V2-REPORT erratum P-1."""
        best = {}
        for tid, _slot in self.registry():
            events, _ = self.read_transcript(tid)
            for ev in events:
                if ev.get("timestamp") is None:
                    continue
                t = self.names.turn(ev["event_id"])
                when = parse_instant(ev.get("timestamp"))
                when = when if when is not None else -(2**63)
                if t not in best or when > best[t]:
                    best[t] = when
        ordered = sorted(best.items(), key=lambda kv: (kv[1], kv[0]), reverse=True)
        return [t for t, _w in ordered[:limit]]

    # -- the evaluator ----------------------------------------------------------
    @staticmethod
    def _quad_line(q):
        g, s, po, o = q
        line = f"{nt(s)} {nt(po)} {nt(o)}"
        if g is not None:
            line += f" <{g}>"
        return line + " ."

    def _prep_text(self):
        prop_equiv, class_subs = ontology_prep()
        prep = []
        for asked, stored in prop_equiv.items():
            for st in stored:
                prep.append(f"equiv\t{asked}\t{st}")
        for asked, stored in class_subs.items():
            for st in stored:
                prep.append(f"subclass\t{asked}\t{st}")
        prep.append(f"abstract\t{ABSTRACT}")
        return "\n".join(prep) + "\n"

    def _evaluator_inputs(self):
        if not self._dirty and self._inputs is not None:
            return self._inputs
        lines = [self._quad_line(q) for q in self.all_quads()]
        self._inputs = ("\n".join(lines) + "\n", self._prep_text())
        self._dirty = False
        return self._inputs

    def _parse_protocol(self, lines, exit_code):
        """The phext-sparql line protocol, shared by the subprocess path
        and the phext-serve /query path (identical bytes, X-Exit header
        standing in for the process exit code)."""
        if exit_code == 3 and lines and lines[0].startswith("REFUSED\t"):
            _r, construct, message = lines[0].split("\t", 2)
            raise Refused(construct, message)
        if exit_code != 0:
            raise QueryError(lines[0] if lines else f"evaluator exit {exit_code}")
        notes, i = [], 0
        prop_equiv, class_subs = ontology_prep()
        known = set(prop_equiv) | set(class_subs)
        while i < len(lines) and lines[i].startswith("NOTE\t"):
            name = lines[i][5:]
            if name in known:
                notes.append(
                    f"{display_name(name)} has other stored spellings but is compared as written "
                    "here (inside an expression); it will most likely match nothing"
                )
            i += 1
        head = lines[i] if i < len(lines) else ""
        if head.startswith("SOLUTIONS"):
            vars_ = head.split("\t")[1:] if "\t" in head else []
            rows = []
            for line in lines[i + 1:]:
                row = {}
                for v, field in zip(vars_, line.split("\t")):
                    if field:
                        row[v] = parse_nt_term(field)[0]
                rows.append(row)
            return {"kind": "solutions", "vars": vars_, "rows": rows, "notes": notes}
        if head.startswith("BOOLEAN"):
            return {"kind": "boolean", "value": head.split("\t")[1] == "true", "notes": notes}
        if head == "TRIPLES":
            triples = []
            for line in lines[i + 1:]:
                s, j = parse_nt_term(line)
                po, j = parse_nt_term(line, j + 1)
                o, _j = parse_nt_term(line, j + 1)
                triples.append((s, po, o))
            return {"kind": "triples", "triples": triples, "notes": notes}
        raise QueryError(f"evaluator: unexpected output {head!r}")

    # -- holo bridge sync ---------------------------------------------------
    def _coord_tids(self, c, reg):
        """Map a changed scroll coordinate to transcript ids it can
        affect: {tid} for a transcript region, set() for the notes /
        summaries region (global section only), None for 'unknown /
        registry' (affects everything). reg: slot -> tid."""
        try:
            lib, vol, _book, _ch, _sec, _scroll = parse_coord(c)
        except Exception:
            return None
        if lib == 1 and vol == 1:
            return None
        if lib in (1, 3) and vol >= 2:
            tid = reg.get(vol - 2)
            return {tid} if tid is not None else None
        if lib == 2:
            return set()
        return None

    def _collect_external(self):
        """Poll the bridge commit feed once. Returns None (no change),
        ("full", seq), or ("diff", seq, affected_tids, global_hit).
        Invalidates the transcript caches of affected tids."""
        seq = self.store.seq(self.file)
        if seq == self._holo_seq:
            return None
        if seq < self._holo_seq:
            self._transcript_cache.clear()
            return ("full", seq)
        upd = self.store.updates(self.file, self._holo_seq)
        if isinstance(upd, dict):  # {"truncated": true}
            self._transcript_cache.clear()
            return ("full", seq)
        reg = {slot: tid for tid, slot in self.registry()}
        affected, global_hit = set(), False
        for entry in upd:
            for c in entry.get("coords", []):
                tids = self._coord_tids(c, reg)
                if tids is None:
                    self._transcript_cache.clear()
                    return ("full", seq)
                if tids:
                    affected |= tids
                else:
                    global_hit = True
        for tid in affected:
            self._invalidate_tid(tid)
        return ("diff", seq, affected, global_hit)

    def _holo_poll_cache(self):
        """all_quads()'s external-change poll (holo, after first sync):
        invalidate touched transcripts; the next run_query re-syncs the
        bridge quad set with a full replace."""
        got = self._collect_external()
        if got is not None:
            self._holo_needs_replace = True
            self._holo_seq = got[1]

    def _derive_sections(self):
        """section key -> set of NT lines, plus the lines in all_quads
        order. Section keys are tids, plus '__global__'."""
        sections, ordered = {}, []
        self._holo_in_sync = True
        try:
            for tid, _slot in self.registry():
                lines = [self._quad_line(q) for q in self._transcript_quads(tid)]
                sections[tid] = set(lines)
                ordered += lines
            glines = [self._quad_line(q) for q in self._global_quads()]
            sections["__global__"] = set(glines)
            ordered += glines
        finally:
            self._holo_in_sync = False
        return sections, ordered

    def _holo_post_diff(self, fresh):
        """POST the add/remove diff between fresh section line-sets and
        the last-sent sets, adopt the fresh sets, and drop sections of
        transcripts that have vanished from the registry."""
        add, remove = [], []
        for k, new in fresh.items():
            old = self._holo_sections.get(k, set())
            add += sorted(new - old)
            remove += sorted(old - new)
            self._holo_sections[k] = new
        live = {tid for tid, _s in self.registry()}
        for k in list(self._holo_sections):
            if k != "__global__" and k not in live:
                remove += sorted(self._holo_sections.pop(k))
        if add or remove:
            payload = {}
            if add:
                payload["add"] = add
            if remove:
                payload["remove"] = remove
            self.store.quads(self.file, payload)

    def _holo_eager_sync(self, tids):
        """Holo only: move the quad derivation for our own just-committed
        write to write time, so the next query pays only the seq poll
        (a no-op) plus the bridge /query. Derives the affected
        transcripts from their (primed) read caches plus the global
        section, posts the diff, and advances _holo_seq past our own
        commit. Commits in the feed that are not ours (an external
        writer touching another transcript) keep the poll path's
        behaviour: that transcript's cache is dropped and it alone is
        re-derived, here at write time. On any failure the sync state
        is marked for a full replace and the error propagates; the
        write itself has already committed."""
        if not self._is_holo:
            return
        try:
            if not self._holo_synced or self._holo_needs_replace:
                self._holo_sync()
                return
            seq = self.store.seq(self.file)
            if seq != self._holo_seq:
                upd = self.store.updates(self.file, self._holo_seq)
                if isinstance(upd, dict):  # truncated feed: re-derive all
                    for tid in list(self._transcript_cache):
                        if tid not in tids:
                            self._invalidate_tid(tid)
                    self._holo_needs_replace = True
                    self._holo_sync()
                    return
                reg = {slot: tid for tid, slot in self.registry()}
                extra = set()
                for entry in upd:
                    for c in entry.get("coords", []):
                        ct = self._coord_tids(c, reg)
                        if ct is None:  # registry / unknown: re-derive all
                            for tid in list(self._transcript_cache):
                                if tid not in tids:
                                    self._invalidate_tid(tid)
                            self._holo_needs_replace = True
                            self._holo_sync()
                            return
                        extra |= ct
                for tid in extra - set(tids):
                    self._invalidate_tid(tid)
                tids = set(tids) | extra
            self._holo_in_sync = True
            try:
                fresh = {}
                for tid in tids:
                    fresh[tid] = {self._quad_line(q) for q in self._transcript_quads(tid)}
                fresh["__global__"] = {self._quad_line(q) for q in self._global_quads()}
            finally:
                self._holo_in_sync = False
            self._holo_post_diff(fresh)
            self._holo_seq = seq
        except Exception:
            self._holo_needs_replace = True
            raise

    def _holo_sync(self):
        """Keep the bridge's maintained quad set equal to this soul's
        projected quads: full replace on first use / truncation, else a
        per-tid add/remove diff driven by the commit feed."""
        if not self._holo_synced or self._holo_needs_replace:
            seq0 = self.store.seq(self.file)
            sections, ordered = self._derive_sections()
            self.store.quads(self.file, {"replace": ordered})
            self._holo_sections = sections
            self._holo_seq = seq0
            self._holo_synced = True
            self._holo_needs_replace = False
            return
        got = self._collect_external()
        if got is None:
            return
        if got[0] == "full":
            self._holo_needs_replace = True
            self._holo_sync()
            return
        _kind, seq, affected, _global_hit = got
        # Any change can move the global section (recent turns derive
        # from all transcripts), so it is re-derived on every diff.
        self._holo_in_sync = True
        try:
            fresh = {}
            for tid in affected:
                fresh[tid] = {self._quad_line(q) for q in self._transcript_quads(tid)}
            fresh["__global__"] = {self._quad_line(q) for q in self._global_quads()}
        finally:
            self._holo_in_sync = False
        self._holo_post_diff(fresh)
        self._holo_seq = seq

    def run_query(self, sparql, base=None):
        if self._is_holo:
            self._holo_sync()
            exit_code, body = self.store.query(
                self.file, sparql, base or self.names.root, self._prep_text())
            return self._parse_protocol(body.splitlines(), exit_code)
        nq, prep = self._evaluator_inputs()
        with tempfile.TemporaryDirectory() as td:
            qp, rp, pp = (os.path.join(td, n) for n in ("q.nq", "q.rq", "prep.txt"))
            open(qp, "w").write(nq)
            open(rp, "w").write(sparql)
            open(pp, "w").write(prep)
            proc = subprocess.run(
                [EVALUATOR, qp, rp, base or self.names.root, pp],
                capture_output=True, text=True, timeout=30,
            )
        return self._parse_protocol(proc.stdout.splitlines(), proc.returncode)

    def query_rows(self, sparql):
        """graph.rs's query_rows: variable -> plain value, per row."""
        out = self.run_query(sparql)
        if out["kind"] == "boolean":
            return [{"boolean": out["value"]}]
        if out["kind"] != "solutions":
            raise QueryError("query_rows wants a SELECT or ASK")
        return [{v: term_value(t) for v, t in row.items()} for row in out["rows"]]


# -- fixed queries and views (the daemon's route building blocks) -----------

_DT = "FILTER(DATATYPE(?t) = <http://www.w3.org/2001/XMLSchema#dateTime>)"
_G = "GRAPH ?g { ?s ?p ?o }"


def _scalar(soul, q):
    rows = soul.query_rows(q)
    if not rows:
        return ""
    return next(iter(rows[0].values()), "")


def soul_stats(soul):
    """http.rs's soul_stats: the I-4 battery, run as specced."""
    cache = getattr(soul, "_stats_cache", None)
    if cache and _time.time() - cache[0] < 300:
        v = dict(cache[1])
        v["counted_secs_ago"] = int(_time.time() - cache[0])
        return v
    agg = lambda expr: _scalar(soul, f"SELECT ({expr} AS ?n) WHERE {{ {_G} }}")  # noqa: E731
    num = lambda s: int(s) if str(s).isdigit() else 0  # noqa: E731
    turn_prefix = f"{soul.names.root}ravel/Turn/"
    v = {
        "store": soul.id,
        "path": soul.file,
        "quads": num(agg("COUNT(*)")),
        "graphs": num(agg("COUNT(DISTINCT ?g)")),
        "subjects": num(agg("COUNT(DISTINCT ?s)")),
        "turns": num(_scalar(soul, f'SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE {{ GRAPH ?g {{ ?s ?p ?o }} FILTER(STRSTARTS(STR(?s), "{turn_prefix}")) }}')),
        "dated_turns": num(_scalar(soul, f"SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE {{ GRAPH ?g {{ ?s ?ts ?t }} {_DT} }}")),
        "earliest": _scalar(soul, f"SELECT (MIN(?t) AS ?m) WHERE {{ GRAPH ?g {{ ?s ?p ?t }} {_DT} }}"),
        "latest": _scalar(soul, f"SELECT (MAX(?t) AS ?m) WHERE {{ GRAPH ?g {{ ?s ?p ?t }} {_DT} }}"),
        "claims": num(_scalar(soul, f"SELECT (COUNT(*) AS ?n) WHERE {{ GRAPH ?g {{ ?s <{REIFIES}> ?tt }} }}")),
    }
    top = soul.run_query("SELECT ?p (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } } GROUP BY ?p ORDER BY DESC(?n) LIMIT 20")
    v["top_predicates"] = [
        {"predicate": f"<{term_value(r['p'])}>", "n": num(term_value(r["n"]))} for r in top["rows"]
    ]
    v["counted_secs_ago"] = 0
    soul._stats_cache = (_time.time(), v)
    return v


def node_line(notes, sums_map, lo, hi):
    """memory_log.rs's node_line: one wake/zoom line."""
    first, last = notes[lo], notes[hi - 1]
    if hi - lo == 1:
        return {
            "id": first["id"], "label": f"#{lo}", "level": 0,
            "from": first["ts"], "to": first["ts"], "text": first["text"],
            "date": local_date(first["ts"]), "date_to": local_date(first["ts"]),
            "pending": False,
        }
    bid = optmem.block_id(lo, hi)
    s = sums_map.get(bid)
    text, pending = (s["text"], False) if s else (f"(#{bid} not compressed yet)", True)
    return {
        "id": bid, "label": f"#{bid}", "level": (hi - lo).bit_length() - 1,
        "from": first["ts"], "to": last["ts"], "text": text,
        "date": local_date(first["ts"]), "date_to": local_date(last["ts"]),
        "pending": pending,
    }


def wake_lines(soul):
    notes = soul.notes()
    sums_map = {s["block"]: s for s in soul.summaries()}
    return [node_line(notes, sums_map, lo, hi) for lo, hi in optmem.cover(len(notes))]


def memory_view(soul):
    return {
        "store": soul.id,
        "memories": len(soul.notes()),
        "summaries": len(soul.summaries()),
        "lines": wake_lines(soul),
    }


def memory_open(soul, node):
    notes = soul.notes()
    sums_map = {s["block"]: s for s in soul.summaries()}
    span = optmem.parse_block(node) if isinstance(node, str) else None
    if span is not None:
        lo, hi = span
        mid = (lo + hi) // 2
        children = [
            node_line(notes, sums_map, a, min(z, len(notes)))
            for a, z in ((lo, mid), (mid, hi))
            if a < len(notes)
        ]
        return {"node": node, "kind": "block", "children": children}
    pos = None
    if isinstance(node, int):
        pos = node
    else:
        for i, n in enumerate(notes):
            if n["id"] == node or str(i) == node:
                pos = i
                break
    if pos is None or pos >= len(notes):
        raise QueryError(f"no memory at {node!r}")
    n = notes[pos]
    turns = []
    for tid in n["turns"]:
        tiri = soul.names.turn(tid)
        rows = soul.query_rows(
            f"SELECT ?ts ?role ?text WHERE {{ GRAPH ?g {{ <{tiri}> <{RAVEL}timestamp> ?ts ; "
            f"<{RAVEL}role> ?role . OPTIONAL {{ <{tiri}> <{RAVEL}text> ?text }} }} }} LIMIT 1"
        )
        if rows:
            r = rows[0]
            turns.append({"turn": tid, "ts": r.get("ts", ""), "role": r.get("role", ""), "text": r.get("text", "")})
    return {
        "node": node,
        "kind": "memory",
        "position": pos,
        "memory": {"id": n["id"], "ts": n["ts"], "text": n["text"], "turns": n["turns"], "model": n["model"]},
        "turns": turns,
    }


def memory_progress(soul):
    turns = sum(len(soul.read_transcript(t)[0]) for t, _ in soul.registry())
    pend = optmem.pending(len(soul.notes()), soul.built())
    return {
        "store": soul.id,
        "turns": turns,
        "turns_counted": turns,
        "turns_read": turns,
        "memories": len(soul.notes()),
        "blocks": len(soul.summaries()),
        "pending_blocks": len(pend),
        "done": not pend,
    }


# -- verify (memory_relive.rs, ported) ---------------------------------------
_META = [
    "compression budget", "haven't compressed", "have not compressed",
    "compressions remain", "raise the limit", "remain in memory unchanged",
    "not compressing", "no line written", "unsaved range", "already saved as",
]


def _not_wrapped(line):
    t = line.lstrip()
    if t[:1] in ("{", "["):
        try:
            v = json.loads(line)
            if isinstance(v, (dict, list)):
                return "That is JSON. Give the memory itself: one plain line of text."
        except ValueError:
            pass
    return None


def _a_memory(line, block=None):
    low = line.lower()
    if any(m in low for m in _META):
        return "That is about the task, not a memory. There is no budget to save: write the merged line itself."
    if block is not None:
        t = low.lstrip()
        if t.startswith(f"#{block}") or t.startswith("compressed #") or t.startswith("compressed:"):
            return "Give the line itself, without its number or a label."
    return None


def _text_failures(text, block, where, failures):
    _t, err = optmem.check(text)
    if err:
        failures.append(f"{where}: {err}")
        return
    for err in (_not_wrapped(text), _a_memory(text, block)):
        if err:
            failures.append(f"{where}: {err}")
            return


def verify(soul):
    notes, sums = soul.notes(), soul.summaries()
    failures = []
    for i, n in enumerate(notes):
        _text_failures(n["text"], None, f"note {i} ({n['id']})", failures)
        if not n["turns"]:
            failures.append(f"note {i} ({n['id']}) cites no turn")
    for s in sums:
        span = optmem.parse_block(s["block"])
        if span is None:
            failures.append(f"summary {s['block']}: not an aligned block")
            continue
        if span[1] > len(notes):
            failures.append(f"summary {s['block']} reaches past the log ({len(notes)} notes)")
        _text_failures(s["text"], s["block"], f"summary {s['block']}", failures)
    pend = optmem.pending(len(notes), soul.built())
    if pend:
        failures.append(f"{len(pend)} blocks still pending")
    own = set(soul._turn_coord)
    cited = []
    for n in notes:
        for t in n["turns"]:
            cited.append(t)
            if t not in own:
                failures.append(f"note {n['id']} cites turn {t}, not one of this soul's turns")
    lines = wake_lines(soul)
    spans = optmem.cover(len(notes))
    if len(lines) != len(spans):
        failures.append("the wake view is not the cover")
    dangling = _scalar(
        soul,
        f"SELECT (COUNT(*) AS ?n) WHERE {{ GRAPH ?g {{ ?m <{RAVEL}relatedToId> ?c . "
        f"FILTER(STRSTARTS(STR(?c), \"{soul.names.root}ravel/Memory/\")) "
        f"FILTER NOT EXISTS {{ ?c a <{RAVEL}Memory> }} }} }}",
    )
    if dangling.isdigit() and int(dangling) > 0:
        failures.append(f"{dangling} memory edges point at nodes that are not there")
    in_store = None
    if cited:
        iris = sorted({soul.names.turn(t) for t in cited if t in own})
        total = 0
        for i in range(0, len(iris), 200):
            chunk = " ".join(f"<{x}>" for x in iris[i:i + 200])
            n = _scalar(
                soul,
                f"SELECT (COUNT(DISTINCT ?t) AS ?n) WHERE {{ VALUES ?t {{ {chunk} }} GRAPH ?g {{ ?t a <{RAVEL}Turn> }} }}",
            )
            total += int(n) if n.isdigit() else 0
        in_store = total
        if total != len(iris):
            failures.append(f"only {total} of {len(iris)} cited turns are in the store")
    return {
        "notes": len(notes),
        "blocks": len(sums),
        "turn_links": len(cited),
        "wake_lines": len(lines),
        "nt_triples": len(soul._memory_quads(notes)),
        "turns_in_store": in_store,
        "failures": failures,
    }


def diagnose(soul):
    """Backend-level health findings (sync::diagnose parity is a named
    gap: the native checks run over the mirror, this runs over the
    store). Findings: {"attention": bool, "line": str}."""
    findings = [{"attention": False, "line": f"{len(soul.registry())} transcripts in the registry"}]
    for tid, _slot in soul.registry():
        events, anns = soul.read_transcript(tid)
        if not events:
            findings.append({"attention": True, "line": f"transcript {tid} holds no turns"})
        else:
            findings.append({"attention": False, "line": f"transcript {tid}: {len(events)} turns, {len(anns)} annotations"})
    if soul.notes():
        v = verify(soul)
        findings += [{"attention": True, "line": f"memory: {f}"} for f in v["failures"]]
        if not v["failures"]:
            findings.append({"attention": False, "line": f"memory verifies: {v['notes']} notes, {v['blocks']} blocks"})
    return findings
