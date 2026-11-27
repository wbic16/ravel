//! The persistent graph: annotations from many sessions accumulate into
//! one queryable store — the "see graphs of sessions" arm.
//!
//! THE PHEXT GRAPH LAYER (2026-11-27, Will's direction: "rewrite the graph
//! layer in terms of phext only"). The store is the phext store: the
//! `phext-serve` library's `VecSource` (holo-phext's quad set, in-process)
//! plus its SPARQL-subset evaluator — the same evaluator and store the
//! phext backend seats behind the daemon's HTTP contract. One evaluator,
//! one store, two seats: the phext-serve binary and this daemon.
//! Persistence is text: the store directory holds one N-Quads file
//! (`store.nq`), rewritten whole by [`Store::flush`]. Addresses are text,
//! so a store can be read, diffed and re-addressed as text (see
//! `crate::readdress`). Oxigraph/RocksDB left the tree with this rewrite;
//! a pre-rewrite RocksDB store directory is NOT read — the graph is a
//! rebuildable projection of the transcripts, and sync re-ingests it.
//!
//! The public types in this module (`Store`, `Quad`, `Term`, `NamedNode`,
//! `Literal`, `QueryResults`, …) keep the shapes the rest of the crate
//! already speaks, over phext terms (`phext_serve::sparql::Term`).
//!
//! Three w4r3z-flagged gotchas are handled here, on purpose:
//!
//! 1. **Idempotency on re-ingest.** Annotation IRIs are deterministic
//!    (sha256 over identity, `annotate` mod), so re-ingesting the same
//!    transcript overwrites the same triples rather than duplicating. We
//!    ALSO clear the transcript's named graph before reload, so a
//!    *removed* key (edited source) doesn't leave a ghost.
//!
//! 2. **Named-graph read-wrapping.** Each transcript's annotations live
//!    in their OWN named graph `ravel:graph/<transcript_id>` (a
//!    per-session provenance unit — droppable, re-ingestable atomically).
//!    A naked default-graph query will NOT see them; every query here
//!    wraps in `GRAPH ?g { … }`.
//!
//! 3. **Same registry / partition.** The partition string
//!    (`https://repolex.ai/ravel/Turn/` from the soul adapter) is threaded
//!    through unchanged; the engine never invents it. Soul scoping is BY
//!    STORE, not by subject — see `soul.rs`.

use crate::annotate::annotation_nt;
use crate::names::Names;
use crate::project::project_nt;
use crate::reader::Annotation;
use crate::{Event, RAVEL_NS};
use anyhow::{Context, Result};
use phext_serve::sparql::{
    evaluate_with_stop_view, parse_query, Literal as PLiteral, Prep, QuadSource, Results,
    Term as PTerm, Triple as PTriple, VecSource,
};
use std::path::{Path, PathBuf};

// ───────────────────────────── terms ─────────────────────────────

/// An IRI. The phext term model's only node kind besides literals and
/// triple terms: ravel's projectors emit deterministic IRIs, never blank
/// nodes (census gap #29 is latent, not live).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NamedNode(String);

impl NamedNode {
    pub fn new(iri: impl Into<String>) -> Result<Self> {
        let iri = iri.into();
        if iri.is_empty() || iri.contains(|c: char| c.is_whitespace() || c == '<' || c == '>') {
            anyhow::bail!("invalid IRI: {iri:?}");
        }
        Ok(Self(iri))
    }
    pub fn new_unchecked(iri: impl Into<String>) -> Self {
        Self(iri.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn into_string(self) -> String {
        self.0
    }
    pub fn as_ref(&self) -> NamedNodeRef<'_> {
        NamedNodeRef(&self.0)
    }
}

impl std::fmt::Display for NamedNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{}>", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NamedNodeRef<'a>(&'a str);

impl<'a> NamedNodeRef<'a> {
    pub fn new_unchecked(iri: &'a str) -> Self {
        Self(iri)
    }
    pub fn as_str(&self) -> &str {
        self.0
    }
    pub fn into_owned(self) -> NamedNode {
        NamedNode(self.0.to_string())
    }
}

impl From<NamedNodeRef<'_>> for NamedNode {
    fn from(r: NamedNodeRef<'_>) -> Self {
        r.into_owned()
    }
}

/// A literal: lexical form, optional datatype, optional language.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Literal(PLiteral);

impl Literal {
    pub fn new_simple_literal(value: impl Into<String>) -> Self {
        Self(PLiteral::simple(&value.into()))
    }
    pub fn new_typed_literal(value: impl Into<String>, datatype: NamedNodeRef<'_>) -> Self {
        Self(PLiteral::typed(&value.into(), datatype.as_str()))
    }
    pub fn value(&self) -> &str {
        &self.0.value
    }
    pub fn language(&self) -> Option<&str> {
        self.0.lang.as_deref()
    }
    pub fn datatype(&self) -> Datatype {
        Datatype(self.0.datatype_str().to_string())
    }
}

/// A literal's datatype IRI (simple literals report xsd:string).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Datatype(String);

impl Datatype {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", PTerm::Literal(self.0.clone()).to_nt())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NamedOrBlankNode {
    NamedNode(NamedNode),
    BlankNode(String),
}

impl NamedOrBlankNode {
    pub fn as_str(&self) -> &str {
        match self {
            NamedOrBlankNode::NamedNode(n) => n.as_str(),
            NamedOrBlankNode::BlankNode(b) => b,
        }
    }
}

impl std::fmt::Display for NamedOrBlankNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NamedOrBlankNode::NamedNode(n) => write!(f, "{n}"),
            NamedOrBlankNode::BlankNode(b) => write!(f, "_:{b}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedOrBlankNodeRef<'a> {
    NamedNode(NamedNodeRef<'a>),
    BlankNode(&'a str),
}

impl<'a> From<NamedNodeRef<'a>> for NamedOrBlankNodeRef<'a> {
    fn from(n: NamedNodeRef<'a>) -> Self {
        NamedOrBlankNodeRef::NamedNode(n)
    }
}

impl<'a> NamedOrBlankNodeRef<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            NamedOrBlankNodeRef::NamedNode(n) => n.as_str(),
            NamedOrBlankNodeRef::BlankNode(b) => b,
        }
    }
}

/// An RDF term: a node, a literal, or a triple term (RDF 1.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    NamedNode(NamedNode),
    BlankNode(String),
    Literal(Literal),
    Triple(Box<Triple>),
}

impl Term {
    /// The plain value: an IRI's string, a literal's lexical form.
    pub fn value(&self) -> String {
        term_value(self)
    }
    pub fn to_nt(&self) -> String {
        to_pterm(self).to_nt()
    }
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_nt())
    }
}

/// A triple (asserted, or as a triple term's content).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Triple {
    pub subject: NamedOrBlankNode,
    pub predicate: NamedNode,
    pub object: Term,
}

impl Triple {
    pub fn new(subject: NamedOrBlankNode, predicate: NamedNode, object: Term) -> Self {
        Self {
            subject,
            predicate,
            object,
        }
    }
}

impl std::fmt::Display for Triple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} {}", self.subject, self.predicate, self.object)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TermRef<'a> {
    NamedNode(NamedNodeRef<'a>),
    BlankNode(&'a str),
    Literal(&'a Literal),
    Triple(&'a Triple),
}

impl<'a> From<NamedNodeRef<'a>> for TermRef<'a> {
    fn from(n: NamedNodeRef<'a>) -> Self {
        TermRef::NamedNode(n)
    }
}

/// A graph name: a named graph, or the default graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GraphName {
    NamedNode(NamedNode),
    DefaultGraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphNameRef<'a> {
    NamedNode(NamedNodeRef<'a>),
    DefaultGraph,
}

impl GraphNameRef<'_> {
    /// The graph selector for the phext source: None = the default graph.
    fn selector(&self) -> Option<&str> {
        match self {
            GraphNameRef::NamedNode(n) => Some(n.as_str()),
            GraphNameRef::DefaultGraph => None,
        }
    }
}

/// One statement: a triple in a graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Quad {
    pub subject: NamedOrBlankNode,
    pub predicate: NamedNode,
    pub object: Term,
    pub graph_name: GraphName,
}

impl Quad {
    pub fn new(
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        object: Term,
        graph_name: GraphName,
    ) -> Self {
        Self {
            subject,
            predicate,
            object,
            graph_name,
        }
    }
}

impl std::fmt::Display for Quad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.graph_name {
            GraphName::NamedNode(g) => {
                write!(
                    f,
                    "{} {} {} {}",
                    self.subject, self.predicate, self.object, g
                )
            }
            GraphName::DefaultGraph => {
                write!(f, "{} {} {}", self.subject, self.predicate, self.object)
            }
        }
    }
}

// ── conversions to and from phext terms ──

pub fn to_pterm(t: &Term) -> PTerm {
    match t {
        Term::NamedNode(n) => PTerm::Iri(n.as_str().to_string()),
        // The phext term model has no blank nodes; a blank node reaching
        // here is refused by name rather than silently re-homed.
        Term::BlankNode(b) => PTerm::Iri(format!("_:blank-node-refused-{b}")),
        Term::Literal(l) => PTerm::Literal(l.0.clone()),
        Term::Triple(t) => PTerm::Triple(Box::new(PTriple {
            s: to_pterm(&Term::NamedNode(match &t.subject {
                NamedOrBlankNode::NamedNode(n) => n.clone(),
                NamedOrBlankNode::BlankNode(b) => {
                    NamedNode::new_unchecked(format!("_:blank-node-refused-{b}"))
                }
            })),
            p: PTerm::Iri(t.predicate.as_str().to_string()),
            o: to_pterm(&t.object),
        })),
    }
}

pub fn from_pterm(t: &PTerm) -> Term {
    match t {
        PTerm::Iri(i) => Term::NamedNode(NamedNode::new_unchecked(i.clone())),
        PTerm::Literal(l) => Term::Literal(Literal(l.clone())),
        PTerm::Triple(t) => Term::Triple(Box::new(Triple {
            subject: match from_pterm(&t.s) {
                Term::NamedNode(n) => NamedOrBlankNode::NamedNode(n),
                other => NamedOrBlankNode::NamedNode(NamedNode::new_unchecked(other.value())),
            },
            predicate: match from_pterm(&t.p) {
                Term::NamedNode(n) => n,
                other => NamedNode::new_unchecked(other.value()),
            },
            object: from_pterm(&t.o),
        })),
    }
}

fn subject_pterm(s: &NamedOrBlankNode) -> PTerm {
    match s {
        NamedOrBlankNode::NamedNode(n) => PTerm::Iri(n.as_str().to_string()),
        NamedOrBlankNode::BlankNode(b) => PTerm::Iri(format!("_:blank-node-refused-{b}")),
    }
}

fn from_pquad(q: &phext_serve::sparql::Quad) -> Quad {
    Quad {
        subject: match from_pterm(&q.s) {
            Term::NamedNode(n) => NamedOrBlankNode::NamedNode(n),
            other => NamedOrBlankNode::NamedNode(NamedNode::new_unchecked(other.value())),
        },
        predicate: match from_pterm(&q.p) {
            Term::NamedNode(n) => n,
            other => NamedNode::new_unchecked(other.value()),
        },
        object: from_pterm(&q.o),
        graph_name: match &q.graph {
            Some(g) => GraphName::NamedNode(NamedNode::new_unchecked(g.clone())),
            None => GraphName::DefaultGraph,
        },
    }
}

fn to_pquad(q: &Quad) -> phext_serve::sparql::Quad {
    phext_serve::sparql::Quad {
        graph: match &q.graph_name {
            GraphName::NamedNode(g) => Some(g.as_str().to_string()),
            GraphName::DefaultGraph => None,
        },
        s: subject_pterm(&q.subject),
        p: PTerm::Iri(q.predicate.as_str().to_string()),
        o: to_pterm(&q.object),
    }
}

// ───────────────────────────── query results ─────────────────────────────

/// One solution row: variable → term (absent = unbound).
#[derive(Debug, Clone)]
pub struct Solution(std::collections::HashMap<String, Term>);

impl Solution {
    pub fn get(&self, var: &str) -> Option<&Term> {
        self.0.get(var)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Term)> {
        self.0.iter()
    }
}

/// SELECT solutions: the projected variables, then rows in answer order.
pub struct Solutions {
    vars: Vec<String>,
    rows: std::vec::IntoIter<Result<Solution>>,
}

impl Solutions {
    pub fn variables(&self) -> &[String] {
        &self.vars
    }
}

impl Iterator for Solutions {
    type Item = Result<Solution>;
    fn next(&mut self) -> Option<Self::Item> {
        self.rows.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.rows.size_hint()
    }
}

/// What a query produced — the phext evaluator's three outcomes.
pub enum QueryResults {
    Solutions(Solutions),
    Boolean(bool),
    /// CONSTRUCT / DESCRIBE triples.
    Graph(std::vec::IntoIter<Result<Triple>>),
}

/// A cooperative cancellation handle for query evaluation: the phext
/// counterpart of the native evaluator's cancellation token. Cancelling
/// sets the deadline to now; the evaluator's hot loops poll it
/// (`phext_serve::sparql::check_deadline`) and stop with an error.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
    /// The flag the evaluator polls (see `evaluate_with_stop`).
    pub fn flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.cancelled.clone()
    }
}

// ───────────────────────────── the store ─────────────────────────────

/// The file inside a store directory holding the store as N-Quads text.
/// Its presence is what says "a store lives here" (the role the RocksDB
/// `CURRENT` file played before the rewrite).
pub const STORE_FILE: &str = "store.nq";

/// The phext graph store: a `VecSource` in memory, a `store.nq` file on
/// disk when opened from a directory. Interior-locked, so readers and
/// the writer in one process share one store (the turnstile below still
/// coordinates the daemon's open/write passes, as before).
pub struct Store {
    src: std::sync::RwLock<VecSource>,
    file: Option<PathBuf>,
}

impl Store {
    pub fn new() -> Result<Self> {
        Ok(Self {
            src: std::sync::RwLock::new(VecSource::new()),
            file: None,
        })
    }

    /// Open (or create) the persistent store at directory `path`.
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("create store directory {}", path.display()))?;
        Self::load(path)
    }

    /// Open the store at directory `path` without creating it.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        if !path.is_dir() {
            anyhow::bail!("no store directory at {}", path.display());
        }
        Self::load(path)
    }

    fn load(dir: &Path) -> Result<Self> {
        let file = dir.join(STORE_FILE);
        let src = if file.exists() {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("read {}", file.display()))?;
            VecSource::from_nquads(&text)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .with_context(|| format!("parse {}", file.display()))?
        } else {
            VecSource::new()
        };
        Ok(Self {
            src: std::sync::RwLock::new(src),
            file: Some(file),
        })
    }

    /// Write the store back to its file, whole and atomically (a temp
    /// file renamed over the old one). A no-op for an in-memory store.
    pub fn flush(&self) -> Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        let text = self.src.read().unwrap().to_nquads();
        let tmp = file.with_extension("nq.tmp");
        std::fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, file)
            .with_context(|| format!("put the store at {}", file.display()))?;
        Ok(())
    }

    pub fn insert(&self, q: &Quad) -> Result<bool> {
        Ok(self.src.write().unwrap().insert(to_pquad(q)))
    }

    pub fn remove(&self, q: &Quad) -> Result<bool> {
        Ok(self.src.write().unwrap().remove(&to_pquad(q)))
    }

    pub fn clear_graph(&self, graph: GraphNameRef<'_>) -> Result<()> {
        self.src.write().unwrap().clear_graph(graph.selector());
        Ok(())
    }

    /// Every quad matching the pattern (`None` = wildcard in a term
    /// position; `None` graph = every graph). The public store-iteration
    /// seam of the phext layer, surfaced from the evaluator's own
    /// `QuadSource::quads_matching`.
    pub fn quads_for_pattern(
        &self,
        s: Option<NamedOrBlankNodeRef<'_>>,
        p: Option<NamedNodeRef<'_>>,
        o: Option<TermRef<'_>>,
        g: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<Quad>> {
        let s_t = s.map(|s| match s {
            NamedOrBlankNodeRef::NamedNode(n) => PTerm::Iri(n.as_str().to_string()),
            NamedOrBlankNodeRef::BlankNode(b) => PTerm::Iri(format!("_:blank-node-refused-{b}")),
        });
        let p_t = p.map(|p| PTerm::Iri(p.as_str().to_string()));
        let o_t = o.map(|o| match o {
            TermRef::NamedNode(n) => PTerm::Iri(n.as_str().to_string()),
            TermRef::Literal(l) => PTerm::Literal(l.0.clone()),
            TermRef::BlankNode(b) => PTerm::Iri(format!("_:blank-node-refused-{b}")),
            TermRef::Triple(t) => to_pterm(&Term::Triple(Box::new((*t).clone()))),
        });
        let graph: Option<Option<String>> = g.map(|g| g.selector().map(str::to_string));
        let quads = self.src.read().unwrap().quads_for_pattern(
            s_t.as_ref(),
            p_t.as_ref(),
            o_t.as_ref(),
            graph.as_ref().map(|g| g.as_deref()),
        );
        quads.into_iter().map(|q| Ok(from_pquad(&q)))
    }

    /// Load N-Triples text into `graph` (None = the default graph).
    pub fn load_nt(&self, text: &str, graph: Option<&str>) -> Result<()> {
        let parsed = VecSource::from_nquads(text).map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut src = self.src.write().unwrap();
        for mut q in parsed.quads {
            if q.graph.is_none() {
                q.graph = graph.map(str::to_string);
            }
            src.insert(q);
        }
        Ok(())
    }

    /// Load N-Quads text (each line names its own graph).
    pub fn load_nq(&self, text: &str) -> Result<()> {
        let parsed = VecSource::from_nquads(text).map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut src = self.src.write().unwrap();
        for q in parsed.quads {
            src.insert(q);
        }
        Ok(())
    }

    /// Load Turtle text into `graph`, through the purpose-built reader
    /// in `crate::ttl` (the shipped ontologies and fixtures; not a
    /// general Turtle parser — census gap #24).
    pub fn load_ttl(&self, text: &str, graph: Option<&str>) -> Result<()> {
        let mut prefixes = std::collections::HashMap::new();
        let triples = crate::ttl::triples(text, &mut prefixes);
        let mut src = self.src.write().unwrap();
        for (s, p, o) in triples {
            let (crate::ttl::TtlTerm::Iri(s), crate::ttl::TtlTerm::Iri(p)) = (s, p) else {
                continue;
            };
            let o = match o {
                crate::ttl::TtlTerm::Iri(i) => PTerm::Iri(i),
                crate::ttl::TtlTerm::Literal {
                    value,
                    datatype,
                    lang,
                } => PTerm::Literal(PLiteral {
                    value,
                    datatype,
                    lang,
                }),
            };
            src.insert(phext_serve::sparql::Quad {
                graph: graph.map(str::to_string),
                s: PTerm::Iri(s),
                p: PTerm::Iri(p),
                o,
            });
        }
        Ok(())
    }

    /// The whole store as N-Quads text.
    pub fn dump_nquads(&self) -> String {
        self.src.read().unwrap().to_nquads()
    }

    /// Every quad in the store, in store order.
    pub fn iter(&self) -> impl Iterator<Item = Result<Quad>> {
        let quads = self.src.read().unwrap().quads.clone();
        quads.into_iter().map(|q| Ok(from_pquad(&q)))
    }

    pub fn len(&self) -> Result<usize> {
        Ok(self.src.read().unwrap().len())
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.src.read().unwrap().is_empty())
    }

    /// The store's named graphs (the default graph is not listed).
    pub fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> {
        let names = self.src.read().unwrap().graph_names();
        names
            .into_iter()
            .map(|g| Ok(NamedOrBlankNode::NamedNode(NamedNode::new_unchecked(g))))
    }

    pub fn contains_named_graph(&self, graph: &NamedNode) -> Result<bool> {
        Ok(self
            .src
            .read()
            .unwrap()
            .contains_named_graph(graph.as_str()))
    }

    /// Evaluate a SPARQL query (the phext subset) over this store.
    /// `prep` carries the ontology rewrite as data; internal callers
    /// pass `Prep::default()`. `deadline` is the cooperative
    /// cancellation counterpart (see [`CancellationToken`]).
    pub fn evaluate(
        &self,
        text: &str,
        base: &str,
        prep: &Prep,
        deadline: Option<std::time::Instant>,
    ) -> Result<QueryResults> {
        self.evaluate_stoppable(text, base, prep, deadline, None)
    }

    /// [`Store::evaluate`], with a caller-held cancellation token.
    pub fn evaluate_stoppable(
        &self,
        text: &str,
        base: &str,
        prep: &Prep,
        deadline: Option<std::time::Instant>,
        stop: Option<&CancellationToken>,
    ) -> Result<QueryResults> {
        self.evaluate_view(text, base, prep, deadline, stop, false)
    }

    /// [`Store::evaluate`] under the outside dataset rule: a query that
    /// names no dataset sees the union of every graph.
    pub fn evaluate_union(
        &self,
        text: &str,
        base: &str,
        prep: &Prep,
        deadline: Option<std::time::Instant>,
        stop: Option<&CancellationToken>,
    ) -> Result<QueryResults> {
        self.evaluate_view(text, base, prep, deadline, stop, true)
    }

    fn evaluate_view(
        &self,
        text: &str,
        base: &str,
        prep: &Prep,
        deadline: Option<std::time::Instant>,
        stop: Option<&CancellationToken>,
        union: bool,
    ) -> Result<QueryResults> {
        let query = parse_query(text, base).map_err(|e| anyhow::anyhow!("{}", e.message))?;
        let src = self.src.read().unwrap();
        let outcome = evaluate_with_stop_view(
            &query,
            &*src,
            prep,
            deadline,
            stop.map(|t| t.flag()),
            !union,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(match outcome.results {
            Results::Solutions { vars, rows } => QueryResults::Solutions(Solutions {
                vars,
                rows: rows
                    .into_iter()
                    .map(|b| {
                        Ok(Solution(
                            b.into_iter().map(|(v, t)| (v, from_pterm(&t))).collect(),
                        ))
                    })
                    .collect::<Vec<_>>()
                    .into_iter(),
            }),
            Results::Boolean(b) => QueryResults::Boolean(b),
            Results::Triples(ts) => QueryResults::Graph(
                ts.into_iter()
                    .map(|t| {
                        Ok(Triple {
                            subject: match from_pterm(&t.s) {
                                Term::NamedNode(n) => NamedOrBlankNode::NamedNode(n),
                                other => NamedOrBlankNode::NamedNode(NamedNode::new_unchecked(
                                    other.value(),
                                )),
                            },
                            predicate: match from_pterm(&t.p) {
                                Term::NamedNode(n) => n,
                                other => NamedNode::new_unchecked(other.value()),
                            },
                            object: from_pterm(&t.o),
                        })
                    })
                    .collect::<Vec<_>>()
                    .into_iter(),
            ),
        })
    }
}

/// Evaluate `text` over `store` with no ontology preparation and no
/// deadline — the internal-query seam the rest of the crate uses
/// (ravel's own working queries name stored spellings and are never
/// rewritten; see `crate::outside`).
pub fn eval_query(store: &Store, text: &str) -> Result<QueryResults> {
    store.evaluate(text, "", &Prep::default(), None)
}

/// [`eval_query`] under the union dataset rule (the outside rule).
pub fn eval_query_union(store: &Store, text: &str) -> Result<QueryResults> {
    store.evaluate_union(text, "", &Prep::default(), None, None)
}

/// Open (or create) the persistent store at `path`.
pub fn open(path: impl AsRef<Path>) -> Result<Store> {
    Store::open(path.as_ref())
        .with_context(|| format!("open phext store at {}", path.as_ref().display()))
}

/// The named graph for one transcript's annotations. Lives under the
/// instance base (`…/ravel/NamedGraph/<id>`) — NOT the
/// ontology namespace, which is vocabulary-only.
pub fn transcript_graph_iri(names: &Names, transcript_id: &str) -> String {
    names.graph(&safe(transcript_id))
}

/// What a named graph is, said in the store's unnamed graph (named-graph
/// spec section 12.2). ravel's two classes both sit under
/// `subtexture:DefaultGraph`, which is why a question with no GRAPH sees
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphKind {
    Transcript,
    Memory,
    /// The list of the newest turns. A plain `ravel:NamedGraph`: it is no
    /// transcript, and holds no statement a transcript graph also holds.
    Recent,
}

impl GraphKind {
    pub fn class(self) -> String {
        match self {
            GraphKind::Transcript => format!("{RAVEL_NS}TranscriptGraph"),
            GraphKind::Memory => format!("{RAVEL_NS}MemoryGraph"),
            GraphKind::Recent => format!("{RAVEL_NS}NamedGraph"),
        }
    }
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The statements that describe one graph: its class, its owner when the
/// store has one, and `private`. The subject is the graph's own address.
/// Every graph says its owner, so a reader that checks access never has to
/// guess whose a transcript is.
pub fn description(names: &Names, graph: &NamedNode, kind: GraphKind) -> Vec<Quad> {
    let s = NamedOrBlankNode::NamedNode(graph.clone());
    let p = |name: &str| NamedNode::new_unchecked(format!("{RAVEL_NS}{name}"));
    let mut out = vec![
        Quad::new(
            s.clone(),
            NamedNode::new_unchecked(RDF_TYPE),
            Term::NamedNode(NamedNode::new_unchecked(kind.class())),
            GraphName::DefaultGraph,
        ),
        Quad::new(
            s.clone(),
            p("visibility"),
            Term::Literal(Literal::new_simple_literal("private")),
            GraphName::DefaultGraph,
        ),
    ];
    if let Some(owner) = names.owner() {
        out.push(Quad::new(
            s,
            p("owner"),
            Term::NamedNode(NamedNode::new_unchecked(owner)),
            GraphName::DefaultGraph,
        ));
    }
    out
}

/// Write a graph's description. Safe to repeat.
pub fn describe_graph(
    store: &Store,
    names: &Names,
    graph: &NamedNode,
    kind: GraphKind,
) -> Result<()> {
    for q in description(names, graph, kind) {
        store.insert(&q)?;
    }
    Ok(())
}

/// Empty a graph and take its description away with it: a graph that holds
/// nothing must not still be announced.
pub fn drop_graph(store: &Store, graph: &NamedNode) -> Result<()> {
    store.clear_graph(GraphNameRef::NamedNode(graph.as_ref()))?;
    let said: Vec<Quad> = store
        .quads_for_pattern(
            Some(NamedOrBlankNodeRef::NamedNode(graph.as_ref())),
            None,
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .collect::<Result<_, _>>()?;
    for q in &said {
        store.remove(q)?;
    }
    Ok(())
}

// ───────────────── readers and the writer never overlap ─────────────────
//
// The turnstile survives the phext rewrite as process-level coordination:
// raveld's sync passes (writers) and query passes (readers) still take
// turns per store directory, so a half-ingested transcript graph is never
// read mid-pass. Its original failure mode — two RocksDB handles over one
// directory, whose error-looping iterator grew raveld to 84 GB on
// 2026-11-21 — cannot occur in the phext store (one in-process quad set,
// text on disk), and the coordinator is kept because the discipline it
// enforces (many readers, or one writer alone) is still the right one.

#[derive(Default)]
struct Turnstile {
    state: std::sync::Mutex<TurnstileState>,
    changed: std::sync::Condvar,
}

#[derive(Default)]
struct TurnstileState {
    readers: usize,
    writing: bool,
    writers_waiting: usize,
}

fn turnstile(dir: &Path) -> std::sync::Arc<Turnstile> {
    static ALL: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, std::sync::Arc<Turnstile>>>,
    > = std::sync::OnceLock::new();
    let key = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    ALL.get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry(key)
        .or_default()
        .clone()
}

/// Held by a reader of the store at `dir`; released when dropped.
pub struct Reading(std::sync::Arc<Turnstile>);
/// Held by the writer of the store at `dir`; released when dropped.
pub struct Writing(std::sync::Arc<Turnstile>);

/// Wait until no writer has the store at `dir` (or is waiting for it), then
/// hold it for reading alongside any other readers.
pub fn reading(dir: &Path) -> Reading {
    let t = turnstile(dir);
    {
        let mut s = t.state.lock().unwrap();
        while s.writing || s.writers_waiting > 0 {
            s = t.changed.wait(s).unwrap();
        }
        s.readers += 1;
    }
    Reading(t)
}

/// Wait until nobody has the store at `dir`, then hold it alone. A waiting
/// writer goes ahead of readers that arrive after it.
pub fn writing(dir: &Path) -> Writing {
    let t = turnstile(dir);
    {
        let mut s = t.state.lock().unwrap();
        s.writers_waiting += 1;
        while s.writing || s.readers > 0 {
            s = t.changed.wait(s).unwrap();
        }
        s.writers_waiting -= 1;
        s.writing = true;
    }
    Writing(t)
}

impl Drop for Reading {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().readers -= 1;
        self.0.changed.notify_all();
    }
}

impl Drop for Writing {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().writing = false;
        self.0.changed.notify_all();
    }
}

/// Open the store read-only. The path must already exist.
pub fn open_read_only(path: impl AsRef<Path>) -> Result<Store> {
    Store::open_read_only(path.as_ref())
        .with_context(|| format!("open read-only phext store at {}", path.as_ref().display()))
}

/// Ingest one transcript's Turn nodes AND its annotations into its named
/// graph, idempotently. Clears the transcript's named graph first (so a
/// dropped key doesn't ghost), then loads the freshly-projected
/// N-Triples INTO that named graph. Returns the triple count now in
/// that graph. (Full rationale in the pre-rewrite history: the Turn
/// nodes carry the federation TIME anchor the annotations point at.)
pub fn ingest_annotations(
    store: &Store,
    events: &[Event],
    anns: &[Annotation],
    transcript_id: &str,
    names: &Names,
) -> Result<usize> {
    let graph_iri = transcript_graph_iri(names, transcript_id);
    let graph = NamedNode::new(&graph_iri).context("transcript graph IRI")?;
    let partition = names.turn_partition();

    store.clear_graph(GraphNameRef::NamedNode(graph.as_ref()))?;

    let referenced: std::collections::HashSet<&str> =
        anns.iter().map(|a| a.event_id.as_str()).collect();
    let anchor_events: Vec<Event> = events
        .iter()
        .filter(|e| referenced.contains(e.event_id.as_str()))
        .cloned()
        .collect();

    let mut nt = project_nt(&anchor_events, &partition, &crate::project::Who::of(names))?;
    nt.push_str(&annotation_nt(anns, transcript_id, names)?);
    store.load_nt(&nt, Some(&graph_iri))?;
    describe_graph(store, names, &graph, GraphKind::Transcript)?;
    store.flush()?;

    Ok(store
        .quads_for_pattern(
            None,
            None,
            None,
            Some(GraphNameRef::NamedNode(graph.as_ref())),
        )
        .count())
}

/// Ingest a transcript's FULL SPINE (every Turn node) plus its annotations
/// into its named graph, idempotently. Same shape as
/// [`ingest_annotations`], but it does NOT filter events to just the
/// reader-anchored ones — the whole conversation lands in the graph.
pub fn ingest_transcript(
    store: &Store,
    events: &[Event],
    anns: &[Annotation],
    transcript_id: &str,
    names: &Names,
) -> Result<usize> {
    let graph_iri = transcript_graph_iri(names, transcript_id);
    let graph = NamedNode::new(&graph_iri).context("transcript graph IRI")?;
    let partition = names.turn_partition();

    store.clear_graph(GraphNameRef::NamedNode(graph.as_ref()))?;

    let mut nt = project_nt(events, &partition, &crate::project::Who::of(names))?;
    nt.push_str(&annotation_nt(anns, transcript_id, names)?);
    store.load_nt(&nt, Some(&graph_iri))?;
    describe_graph(store, names, &graph, GraphKind::Transcript)?;
    keep_recent(store, names, events)?;
    store.flush()?;

    Ok(store
        .quads_for_pattern(
            None,
            None,
            None,
            Some(GraphNameRef::NamedNode(graph.as_ref())),
        )
        .count())
}

/// Add turns to a transcript graph that already holds the earlier ones.
/// Nothing is cleared; a turn's address is made from its id, so loading
/// one that is already there changes nothing.
pub fn append_transcript(
    store: &Store,
    events: &[Event],
    anns: &[Annotation],
    transcript_id: &str,
    names: &Names,
) -> Result<()> {
    let graph_iri = transcript_graph_iri(names, transcript_id);
    let graph = NamedNode::new(&graph_iri).context("transcript graph IRI")?;
    let mut nt = project_nt(
        events,
        &names.turn_partition(),
        &crate::project::Who::of(names),
    )?;
    nt.push_str(&annotation_nt(anns, transcript_id, names)?);
    store.load_nt(&nt, Some(&graph_iri))?;
    describe_graph(store, names, &graph, GraphKind::Transcript)?;
    keep_recent(store, names, events)?;
    store.flush()?;
    Ok(())
}

/// How many turns the `recent` graph lists.
pub const RECENT_TURNS: usize = 500;

const RDFS_MEMBER: &str = "http://www.w3.org/2000/01/rdf-schema#member";

/// A turn's time as a number, for telling newer from older whatever zone
/// or precision it was written in. A time that cannot be read sorts oldest.
fn instant(ts: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|t| t.timestamp_millis())
        .unwrap_or(i64::MIN)
}

/// Keep `{root}ravel/NamedGraph/recent` listing the store's newest
/// [`RECENT_TURNS`] turns, given the turns just loaded. It holds one
/// statement a turn, `<the graph> rdfs:member <turn>`, and nothing else —
/// a turn's own statements stay only in its transcript's graph, so no
/// ordinary question sees a turn twice.
pub fn keep_recent(store: &Store, names: &Names, loaded: &[Event]) -> Result<()> {
    let graph = NamedNode::new(names.recent_graph()).context("recent graph IRI")?;
    let g = GraphNameRef::NamedNode(graph.as_ref());
    let member = NamedNodeRef::new_unchecked(RDFS_MEMBER);
    let ts_p = NamedNode::new_unchecked(format!("{RAVEL_NS}timestamp"));
    let described = store
        .quads_for_pattern(
            Some(NamedOrBlankNodeRef::NamedNode(graph.as_ref())),
            Some(NamedNodeRef::new_unchecked(RDF_TYPE)),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .is_some();
    if described && loaded.is_empty() {
        return Ok(());
    }
    let mut cand: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut listed: std::collections::HashSet<String> = std::collections::HashSet::new();
    if described {
        for q in store.quads_for_pattern(None, Some(member), None, Some(g)) {
            let q = q?;
            let Term::NamedNode(t) = q.object else {
                continue;
            };
            let when = store
                .quads_for_pattern(
                    Some(NamedOrBlankNodeRef::NamedNode(t.as_ref())),
                    Some(ts_p.as_ref()),
                    None,
                    None,
                )
                .filter_map(|q| q.ok())
                .map(|q| instant(&term_value(&q.object)))
                .max();
            listed.insert(t.as_str().to_string());
            if let Some(when) = when {
                cand.insert(t.into_string(), when);
            }
        }
    } else {
        let q = format!(
            "SELECT ?t ?ts WHERE {{ GRAPH ?g {{ ?t a <{RAVEL_NS}Turn> ; <{RAVEL_NS}timestamp> ?ts }} }} \
             ORDER BY DESC(?ts) LIMIT {}",
            RECENT_TURNS * 2
        );
        if let QueryResults::Solutions(rows) = eval_query(store, &q)? {
            for row in rows {
                let row = row?;
                if let (Some(Term::NamedNode(t)), Some(ts)) = (row.get("t"), row.get("ts")) {
                    cand.insert(t.as_str().to_string(), instant(&term_value(ts)));
                }
            }
        }
    }
    for e in loaded {
        if let Some(ts) = &e.timestamp {
            cand.insert(names.turn(&e.event_id), instant(ts));
        }
    }
    let mut newest: Vec<(i64, String)> = cand.into_iter().map(|(t, w)| (w, t)).collect();
    newest.sort_by(|a, b| b.cmp(a));
    newest.truncate(RECENT_TURNS);
    let keep: std::collections::HashSet<&str> = newest.iter().map(|(_, t)| t.as_str()).collect();
    let quad = |turn: &str| {
        Quad::new(
            NamedOrBlankNode::NamedNode(graph.clone()),
            member.into_owned(),
            Term::NamedNode(NamedNode::new_unchecked(turn)),
            GraphName::NamedNode(graph.clone()),
        )
    };
    for gone in listed.iter().filter(|t| !keep.contains(t.as_str())) {
        store.remove(&quad(gone))?;
    }
    for new in keep.iter().filter(|t| !listed.contains(**t)) {
        store.insert(&quad(new))?;
    }
    describe_graph(store, names, &graph, GraphKind::Recent)?;
    Ok(())
}

/// A cross-session query result row: one harvested emojikey with provenance.
#[derive(Debug, Clone)]
pub struct EmojikeyHit {
    pub transcript: String,
    pub source_kind: String,
    pub me: String,
    pub content: String,
    pub you: String,
    pub ts: String,
}

/// Run a SELECT and return each row as variable → plain value (an IRI's
/// string, a literal's lexical form). For callers that want values, not
/// RDF terms.
pub fn query_rows(
    store: &Store,
    q: &str,
) -> Result<Vec<std::collections::HashMap<String, String>>> {
    let QueryResults::Solutions(solutions) = eval_query(store, q)? else {
        anyhow::bail!("expected SELECT solutions");
    };
    let mut rows = Vec::new();
    for sol in solutions {
        let sol = sol?;
        rows.push(
            sol.iter()
                .map(|(v, t)| (v.clone(), term_value(t)))
                .collect(),
        );
    }
    Ok(rows)
}

/// Cross-session query: every emojikey annotation in the store, read back
/// THROUGH the triple term, across ALL transcript named graphs.
pub fn query_emojikeys(store: &Store, only_source_kind: Option<&str>) -> Result<Vec<EmojikeyHit>> {
    let filter = match only_source_kind {
        Some(sk) => format!(r#"FILTER(?sk = "{sk}")"#),
        None => String::new(),
    };
    let q = format!(
        r#"
        PREFIX ravel: <{RAVEL_NS}>
        PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
        PREFIX oa: <http://www.w3.org/ns/oa#>
        PREFIX prov: <http://www.w3.org/ns/prov#>
        SELECT ?g ?sk ?me ?content ?you ?ts WHERE {{
            GRAPH ?g {{
                ?claim rdf:reifies <<( ?event ravel:exhibits "emojikey/harvest" )>> ;
                       prov:wasDerivedFrom ?ann .
                ?ann oa:hasBody ?body .
                OPTIONAL {{ ?ann ravel:textOrigin ?sk }}
                OPTIONAL {{ ?ann prov:generatedAtTime ?ts }}
                ?body ravel:sig_me ?me ; ravel:sig_content ?content ; ravel:sig_you ?you .
                {filter}
            }}
        }}
        ORDER BY ?g ?ts
        "#
    );
    let QueryResults::Solutions(solutions) = eval_query(store, &q)? else {
        anyhow::bail!("expected SELECT solutions");
    };
    let mut hits = Vec::new();
    for sol in solutions {
        let sol = sol?;
        let get = |k: &str| sol.get(k).map(term_value).unwrap_or_default();
        let g = get("g");
        let transcript = g
            .rsplit_once("/NamedGraph/")
            .map_or(g.as_str(), |(_, id)| id)
            .to_string();
        hits.push(EmojikeyHit {
            transcript,
            source_kind: get("sk"),
            me: get("me"),
            content: get("content"),
            you: get("you"),
            ts: get("ts"),
        });
    }
    Ok(hits)
}

/// Unwrap a term's plain value (an IRI's string, a literal's lexical form).
pub fn term_value(t: &Term) -> String {
    match t {
        Term::NamedNode(n) => n.as_str().to_string(),
        Term::Literal(l) => l.value().to_string(),
        other => other.to_nt(),
    }
}

fn safe(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {

    #[test]
    fn readers_share_a_store_and_a_writer_has_it_alone() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let dir = std::env::temp_dir().join(format!("ravel-turnstile-{}", std::process::id()));
        let (inside, overlaps) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        // Two readers at once is allowed.
        let (a, b) = (super::reading(&dir), super::reading(&dir));
        drop((a, b));
        let mut threads = Vec::new();
        for k in 0..8 {
            let (dir, inside, overlaps) = (dir.clone(), inside.clone(), overlaps.clone());
            threads.push(std::thread::spawn(move || {
                for _ in 0..200 {
                    if k % 2 == 0 {
                        let _w = super::writing(&dir);
                        // A writer must find nobody inside, and nobody may enter.
                        if inside.fetch_add(1000, Ordering::SeqCst) != 0 {
                            overlaps.fetch_add(1, Ordering::SeqCst);
                        }
                        std::thread::yield_now();
                        inside.fetch_sub(1000, Ordering::SeqCst);
                    } else {
                        let _r = super::reading(&dir);
                        if inside.fetch_add(1, Ordering::SeqCst) >= 1000 {
                            overlaps.fetch_add(1, Ordering::SeqCst);
                        }
                        std::thread::yield_now();
                        inside.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(overlaps.load(Ordering::SeqCst), 0);
    }
    use super::*;
    use crate::reader::emojikey_read;
    use crate::{Event, SourceKind, TextSpan};

    /// The newest turns are listed, the list slides, and listing them does
    /// not make any turn come back twice from an ordinary question.
    #[test]
    fn the_recent_graph_lists_the_newest_turns_and_doubles_nothing() {
        let store = Store::new().unwrap();
        let names = Names::new("7r1pl3f0rc3", "spacegoat").unwrap();
        let line = |i: usize| {
            format!(
                r#"{{"type":"user","uuid":"u{i:04}","timestamp":"2026-11-10T{:02}:{:02}:{:02}Z","message":{{"role":"user","content":"turn {i}"}}}}"#,
                i / 3600,
                (i / 60) % 60,
                i % 60
            )
        };
        let events = |r: std::ops::Range<usize>| {
            crate::adapter::parse_transcript(&r.map(line).collect::<Vec<_>>().join("\n")).unwrap()
        };
        ingest_transcript(&store, &events(0..400), &[], "s1", &names).unwrap();
        let n = |q: &str| query_rows(&store, q).unwrap().len();
        let recent = names.recent_graph();
        let listed = format!("SELECT ?t WHERE {{ GRAPH <{recent}> {{ ?l <{RDFS_MEMBER}> ?t }} }}");
        assert_eq!(n(&listed), 400);
        // A second transcript, newer: the list slides to the newest 500.
        append_transcript(&store, &events(400..700), &[], "s2", &names).unwrap();
        assert_eq!(n(&listed), RECENT_TURNS);
        let ends = |order: &str| {
            let q = format!(
                "SELECT ?t WHERE {{ GRAPH <{recent}> {{ ?l <{RDFS_MEMBER}> ?t }} GRAPH ?g {{ ?t <{RAVEL_NS}timestamp> ?ts }} }} ORDER BY {order} LIMIT 1"
            );
            query_rows(&store, &q).unwrap()[0]["t"].clone()
        };
        assert!(ends("DESC(?ts)").ends_with("/ravel/Turn/u0699"));
        assert!(ends("?ts").ends_with("/ravel/Turn/u0200"));
        // Every turn is still stored once: asked of all graphs together
        // (a query with no GRAPH sees the union of every graph — the
        // phext evaluator's dataset rule).
        let QueryResults::Solutions(rows) = eval_query_union(
            &store,
            &format!("SELECT ?t WHERE {{ ?t a <{RAVEL_NS}Turn> }}"),
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(rows.count(), 700);
        // It is described as a plain named graph, private, with its owner.
        let said: Vec<String> =
            query_rows(&store, &format!("SELECT ?o WHERE {{ <{recent}> ?p ?o }}"))
                .unwrap()
                .into_iter()
                .map(|r| r["o"].clone())
                .collect();
        assert!(said.contains(&format!("{RAVEL_NS}NamedGraph")), "{said:?}");
        assert!(said.contains(&"private".to_string()));
        assert!(said.iter().any(|o| o.ends_with("/soul/Soul/spacegoat")));
    }

    fn ev(id: &str, text: &str, kind: SourceKind) -> Event {
        let span = TextSpan {
            start: 0,
            end: text.len(),
            kind,
        };
        Event {
            event_id: id.into(),
            parent_id: None,
            role: "assistant".into(),
            timestamp: Some("2026-07-01T00:00:00Z".into()),
            text: Some(text.into()),
            text_provenance: vec![span],
            thinking: None,
            model: None,
            model_setting: None,
            wrote: crate::Wrote::Nobody,
            actions: Vec::new(),
            outcomes: Vec::new(),
        }
    }

    #[test]
    fn triple_term_survives_named_graph_and_reingest_is_idempotent() {
        let store = Store::new().unwrap(); // in-memory is the same API as on-disk
        let evs = vec![ev(
            "e1",
            "[ME|🧠]~[CONTENT|💻]~[YOU|🎓]",
            SourceKind::Authored,
        )];
        let anns = emojikey_read(&evs);
        assert_eq!(anns.len(), 1);

        // ingest twice — idempotent: triple count stable, no duplication.
        let n1 = ingest_annotations(&store, &evs, &anns, "sess-A", &crate::names::Names::bare())
            .unwrap();
        let n2 = ingest_annotations(&store, &evs, &anns, "sess-A", &crate::names::Names::bare())
            .unwrap();
        assert!(n1 > 0);
        assert_eq!(n1, n2, "re-ingest must not change the graph (idempotent)");

        // the referenced Turn NODE must be present with its time anchor — else
        // the federation join has the anchor edge but not time (the
        // dangling-Turn bug this ingest fixes).
        let turn_q = format!(
            r#"PREFIX ravel: <{RAVEL_NS}>
               PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
               ASK {{ GRAPH ?g {{ ?t rdf:type ravel:Turn ; ravel:timestamp ?ts .
                      FILTER(STRSTARTS(STR(?t), "https://repolex.ai/ravel/Turn/")) }} }}"#
        );
        let ask = eval_query(&store, &turn_q).unwrap();
        assert!(
            matches!(ask, QueryResults::Boolean(true)),
            "the Turn node with its timestamp must be in the graph"
        );

        // GOTCHA #2: the triple term must be readable THROUGH the named graph.
        let hits = query_emojikeys(&store, None).unwrap();
        assert_eq!(
            hits.len(),
            1,
            "one emojikey, read back through GRAPH + triple term"
        );
        assert_eq!(hits[0].transcript, "sess-A");
        assert_eq!(hits[0].source_kind, "authored");
        assert_eq!(hits[0].me, "🧠");

        // provenance filter: authored-only keeps it; tool_result-only drops it.
        assert_eq!(query_emojikeys(&store, Some("authored")).unwrap().len(), 1);
        assert_eq!(
            query_emojikeys(&store, Some("tool_result")).unwrap().len(),
            0
        );
    }

    #[test]
    fn two_sessions_accumulate_and_are_separable() {
        let store = Store::new().unwrap();
        let eva = vec![ev("e1", "[ME|a]~[CONTENT|b]~[YOU|c]", SourceKind::Authored)];
        let evb = vec![ev(
            "e2",
            "[ME|x]~[CONTENT|y]~[YOU|z]",
            SourceKind::ToolResult,
        )];
        let a = emojikey_read(&eva);
        let b = emojikey_read(&evb);
        ingest_annotations(&store, &eva, &a, "sess-A", &crate::names::Names::bare()).unwrap();
        ingest_annotations(&store, &evb, &b, "sess-B", &crate::names::Names::bare()).unwrap();

        let all = query_emojikeys(&store, None).unwrap();
        assert_eq!(all.len(), 2, "two sessions accumulate in one store");
        // separable by named graph → by transcript
        let transcripts: Vec<_> = all.iter().map(|h| h.transcript.as_str()).collect();
        assert!(transcripts.contains(&"sess-A") && transcripts.contains(&"sess-B"));
        // and by provenance across sessions
        assert_eq!(query_emojikeys(&store, Some("authored")).unwrap().len(), 1);
    }

    #[test]
    fn full_spine_stores_every_turn_not_just_annotated_ones() {
        // The whole point of ingest_transcript vs ingest_annotations: a session
        // with turns that NO reader touched still lands in the graph. Here two of
        // three turns carry no emojikey — annotation-scoped ingest would drop
        // them; full-spine keeps all three.
        let store = Store::new().unwrap();
        let evs = vec![
            ev("e1", "just some prose, no key here", SourceKind::Authored),
            ev("e2", "[ME|🧠]~[CONTENT|💻]~[YOU|🎓]", SourceKind::Authored),
            ev("e3", "more prose, still no key", SourceKind::Authored),
        ];
        let anns = emojikey_read(&evs);
        assert_eq!(anns.len(), 1, "only the middle turn has an emojikey");

        ingest_transcript(
            &store,
            &evs,
            &anns,
            "sess-full",
            &crate::names::Names::bare(),
        )
        .unwrap();

        // ALL THREE turns are present as spine nodes (not just the annotated one).
        let count_q = format!(
            r#"PREFIX ravel: <{RAVEL_NS}>
               SELECT (COUNT(?t) AS ?n) WHERE {{ GRAPH ?g {{ ?t a ravel:Turn }} }}"#
        );
        let res = eval_query(&store, &count_q).unwrap();
        if let QueryResults::Solutions(mut s) = res {
            let row = s.next().unwrap().unwrap();
            let n = row.get("n").unwrap().to_string();
            assert!(n.contains('3'), "all 3 spine turns must be stored, got {n}");
        } else {
            panic!("expected solutions");
        }

        // and the emojikey annotation still rides on top — spine + belief coexist.
        assert_eq!(query_emojikeys(&store, None).unwrap().len(), 1);
    }

    /// Two souls' stores once held exactly 2,500 transcript graphs each
    /// (goodlux asked, 2026-11-20, whether that was a ceiling). It is not:
    /// a store on disk takes 2,600 and reports every one.
    #[test]
    fn a_store_has_no_ceiling_at_2500_graphs() {
        let dir = std::env::temp_dir().join(format!("ravel-2600-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = open(&dir).unwrap();
        let names = Names::new("7r1pl3f0rc3", "spacegoat").unwrap();
        for i in 0..2600 {
            let evs = vec![ev(&format!("t{i}"), "hello", SourceKind::Authored)];
            ingest_transcript(&store, &evs, &[], &format!("sess-{i}"), &names).unwrap();
        }
        let n = |q: &str| query_rows(&store, q).unwrap()[0]["n"].clone();
        // 2,600 transcripts and the list of the newest turns.
        assert_eq!(
            n("SELECT (COUNT(DISTINCT ?g) AS ?n) WHERE { GRAPH ?g { ?s ?p ?o } }"),
            "2601"
        );
        assert_eq!(n("SELECT (COUNT(DISTINCT ?g) AS ?n) WHERE { ?g <https://repolex.ai/ontology/ravel/visibility> ?v }"), "2601");
        assert_eq!(store.named_graphs().count(), 2601);
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_graph_is_described_while_it_holds_something_and_not_after() {
        let store = Store::new().unwrap();
        let names = Names::new("7r1pl3f0rc3", "spacegoat").unwrap();
        let evs = vec![ev("only", "hello", SourceKind::Authored)];
        ingest_transcript(&store, &evs, &[], "sess-1", &names).unwrap();
        ingest_transcript(&store, &evs, &[], "sess-1", &names).unwrap();
        let said = |store: &Store| {
            // (The list of the newest turns is described too; not what
            // this test is about.)
            query_rows(store, "SELECT ?g ?p ?o WHERE { ?g ?p ?o FILTER(!STRENDS(STR(?g), '/NamedGraph/recent')) } ORDER BY ?p").unwrap()
        };
        // Three statements in the unnamed graph, once, however often it is ingested.
        let rows = said(&store);
        assert_eq!(rows.len(), 3, "{rows:?}");
        let g = "https://repolex.ai/7r1pl3f0rc3/spacegoat/ravel/NamedGraph/sess-1";
        assert!(rows.iter().all(|r| r["g"] == g));
        let objects: Vec<&str> = rows.iter().map(|r| r["o"].as_str()).collect();
        assert!(objects.contains(&"https://repolex.ai/ontology/ravel/TranscriptGraph"));
        assert!(objects.contains(&"private"));
        assert!(objects.contains(&"https://repolex.ai/7r1pl3f0rc3/spacegoat/soul/Soul/spacegoat"));
        // The turn is under the soul's own root.
        let t = query_rows(
            &store,
            "SELECT ?t WHERE { GRAPH ?g { ?t a <https://repolex.ai/ontology/ravel/Turn> } }",
        )
        .unwrap();
        assert_eq!(
            t[0]["t"],
            "https://repolex.ai/7r1pl3f0rc3/spacegoat/ravel/Turn/only"
        );

        drop_graph(&store, &NamedNode::new(g).unwrap()).unwrap();
        assert!(
            said(&store).is_empty(),
            "a dropped graph is no longer announced"
        );
        let left = query_rows(
            &store,
            &format!("SELECT ?s WHERE {{ GRAPH <{g}> {{ ?s ?p ?o }} }}"),
        )
        .unwrap();
        assert!(left.is_empty());
    }
}
