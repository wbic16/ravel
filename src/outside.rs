//! A query that arrives from outside: `ravel query` and raveld's query
//! routes. It is prepared here before the store sees it.
//!
//! Three things happen, in the original pipeline's order (parse,
//! rewrite, evaluate):
//!
//!   - it is parsed against a base, so a short address resolves;
//!   - the ontology rewrite is applied — as DATA, not as an algebra
//!     rewrite: the phext evaluator takes the calamus Index's closure
//!     as a `Prep` (equivalent/sub properties and classes, abstract
//!     names dropped), derived from the two shipped ontologies by the
//!     purpose-built Turtle reader in `crate::ttl`. This is the Rust
//!     seat of the phext backend's `ontology_prep()` port (V2-REPORT:
//!     13/13 outside shapes row-equal with the old calamus pipeline);
//!   - a query that names no dataset sees every graph in the store —
//!     the phext evaluator's dataset rule (named-graph spec section 12).
//!
//! ravel's own working queries never come through here. They name stored
//! spellings and must not change, or slow down, when an ontology gains
//! a name.

use crate::graph::{CancellationToken, QueryResults, Store};
use anyhow::{anyhow, Result};
use phext_serve::sparql::{collect_left, parse_query, Prep, Query};
use std::sync::OnceLock;

// The base an outside query is read against is the root of the store being
// asked, `https://repolex.ai/{squad}/{user}/` (`Names::root`). So
// `<ravel/Turn/x>` and `GRAPH <ravel/NamedGraph/optmem>` mean that store's
// own, and one saved query works on every soul.

/// The prefix of the names no store holds; the rewrite drops them from
/// answers.
pub const ABSTRACT: &str = "https://repolex.ai/ontology/subtexture/";

const SUBTEXTURE_TTL: &str = include_str!("../ontology/subtexture/subtexture.ttl");
const RAVEL_TTL: &str = include_str!("../ontology/ravel/ravel.ttl");

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The ontology preparation, built once from the shipped ontologies:
/// the calamus closure as evaluator data (a faithful port of the phext
/// backend's `ontology_prep()` / `_calamus_closure()`).
pub fn prep() -> &'static Prep {
    static PREP: OnceLock<Prep> = OnceLock::new();
    PREP.get_or_init(|| {
        use crate::ttl::{triples, TtlTerm};
        let mut prefixes = std::collections::HashMap::from([
            ("owl".to_string(), "http://www.w3.org/2002/07/owl#".to_string()),
            ("rdfs".to_string(), "http://www.w3.org/2000/01/rdf-schema#".to_string()),
        ]);
        // (predicate, is_property, both_ways)
        let edge_kinds: std::collections::HashMap<&str, (bool, bool)> =
            std::collections::HashMap::from([
                ("http://www.w3.org/2000/01/rdf-schema#subPropertyOf", (true, false)),
                ("http://www.w3.org/2002/07/owl#equivalentProperty", (true, true)),
                ("http://www.w3.org/2000/01/rdf-schema#subClassOf", (false, false)),
                ("http://www.w3.org/2002/07/owl#equivalentClass", (false, true)),
            ]);
        let mut prop_edges: std::collections::HashMap<String, std::collections::HashSet<String>> =
            Default::default();
        let mut class_edges: std::collections::HashMap<String, std::collections::HashSet<String>> =
            Default::default();
        for text in [RAVEL_TTL, SUBTEXTURE_TTL] {
            for (s, p, o) in triples(text, &mut prefixes) {
                let (TtlTerm::Iri(s), TtlTerm::Iri(p), TtlTerm::Iri(o)) = (s, p, o) else {
                    continue;
                };
                let Some(&(is_prop, both_ways)) = edge_kinds.get(p.as_str()) else {
                    continue;
                };
                let edges = if is_prop { &mut prop_edges } else { &mut class_edges };
                edges.entry(o.clone()).or_default().insert(s.clone());
                edges.entry(s.clone()).or_default();
                if both_ways {
                    edges.entry(s).or_default().insert(o);
                }
            }
        }
        // calamus ontology.rs closure(): for every name, everything
        // reachable from it (itself included), abstract names dropped,
        // in address order. A name whose stored answer is only itself
        // is left out (asked as written).
        let closure =
            |edges: &std::collections::HashMap<String, std::collections::HashSet<String>>| {
                let mut out: std::collections::HashMap<String, Vec<String>> = Default::default();
                for start in edges.keys() {
                    let mut seen: std::collections::HashSet<&str> = Default::default();
                    let mut todo = vec![start.as_str()];
                    while let Some(name) = todo.pop() {
                        if seen.insert(name) {
                            if let Some(next) = edges.get(name) {
                                todo.extend(next.iter().map(String::as_str));
                            }
                        }
                    }
                    let mut stored: Vec<String> = seen
                        .iter()
                        .filter(|x| !x.starts_with(ABSTRACT))
                        .map(|x| x.to_string())
                        .collect();
                    stored.sort();
                    if stored.is_empty() || stored == vec![start.clone()] {
                        continue;
                    }
                    out.insert(start.clone(), stored);
                }
                out
            };
        let mut prop_equiv = closure(&prop_edges);
        prop_equiv.remove(RDF_TYPE); // calamus: rdf:type is never swapped
        let class_subs = closure(&class_edges);
        Prep {
            prop_equiv,
            class_subs,
            abstract_prefixes: vec![ABSTRACT.to_string()],
        }
    })
}

/// What preparation found: the names inside expressions that have other
/// stored spellings and were left as written (the old Report's
/// `left_as_written`, computed by the evaluator's own collector).
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub left_as_written: Vec<String>,
}

/// Parse against `base` and compute the preparation. Any error means
/// the query is refused, never run half-prepared. Out-of-subset
/// constructs are refused BY NAME by the phext parser (the subset is
/// the contract; see `phext/SPARQL-SUBSET.md`).
pub fn prepare(text: &str, base: &str) -> Result<(Query, Report)> {
    let query = parse_query(text, base).map_err(|e| anyhow!("{}", e.message))?;
    let mut left = Vec::new();
    collect_left(&query, prep(), &mut left);
    Ok((query, Report { left_as_written: left }))
}

/// Prepare and run. The results borrow nothing from the query.
pub fn run(store: &Store, text: &str, base: &str) -> Result<(QueryResults, Report)> {
    run_until(store, text, base, None)
}

/// [`run`], with a token the caller can use to stop the work part-way.
/// The token is the phext counterpart of the native evaluator's
/// cancellation token: the evaluator's hot loops poll its flag
/// cooperatively and stop with an error (see
/// `phext_serve::sparql::evaluate_with_stop`). The daemon's 30 s seat
/// policy cancels this token from its timer, and a caller hanging up
/// cancels it on drop — both stop the work, as before.
pub fn run_until(
    store: &Store,
    text: &str,
    base: &str,
    stop: Option<CancellationToken>,
) -> Result<(QueryResults, Report)> {
    let (_query, report) = prepare(text, base)?;
    let results = store.evaluate_union(text, base, prep(), None, stop.as_ref())?;
    Ok((results, report))
}

/// Prepare and run with an explicit cooperative deadline (the other
/// half of the native 30 s policy: a hard stop even if nobody cancels).
pub fn run_deadline(
    store: &Store,
    text: &str,
    base: &str,
    deadline: Option<std::time::Instant>,
) -> Result<(QueryResults, Report)> {
    let (_query, report) = prepare(text, base)?;
    let results = store.evaluate_union(text, base, prep(), deadline, None)?;
    Ok((results, report))
}

/// What a caller should be told about names left as written, one line each.
pub fn notes(report: &Report) -> Vec<String> {
    report
        .left_as_written
        .iter()
        .map(|n| {
            format!(
                "{} has other stored spellings but is compared as written here \
                 (inside an expression); it will most likely match nothing",
                n.as_str()
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{NamedNode, QueryResults};

    /// A writer and a reader over one store directory, as raveld runs them:
    /// the writer opens read-write, adds a turn, flushes and closes, again
    /// and again; the reader opens read-only and runs the query, again and
    /// again. `RAVEL_TRIAL_STORE` must be a scratch COPY: it is written to.
    #[test]
    #[ignore]
    fn real_reader_beside_writer() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let dir = std::env::var("RAVEL_TRIAL_STORE").unwrap();
        assert!(dir.contains("scratchpad"), "a scratch copy only");
        let q = std::fs::read_to_string(std::env::var("RAVEL_TRIAL_QUERY").unwrap()).unwrap();
        let base = std::env::var("RAVEL_TRIAL_BASE").unwrap();
        let secs: u64 = std::env::var("RAVEL_TRIAL_SECS").map(|s| s.parse().unwrap()).unwrap_or(60);
        let done = Arc::new(AtomicBool::new(false));
        let writer = {
            let (dir, base, done) = (dir.clone(), base.clone(), done.clone());
            std::thread::spawn(move || {
                let mut n = 0u64;
                while !done.load(Ordering::Relaxed) {
                    // `RAVEL_TRIAL_HOLD=1`: take turns at the store, as raveld does.
                    let _alone = std::env::var("RAVEL_TRIAL_HOLD")
                        .is_ok()
                        .then(|| crate::graph::writing(std::path::Path::new(&dir)));
                    let store = crate::graph::open(std::path::Path::new(&dir)).unwrap();
                    let g = format!("{base}ravel/NamedGraph/trial");
                    let ts = crate::time::Instant::now().to_rfc3339_millis_z();
                    let nt = format!(
                        "<{base}ravel/Turn/trial-{n}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://repolex.ai/ontology/ravel/Turn> .\n\
                         <{base}ravel/Turn/trial-{n}> <https://repolex.ai/ontology/ravel/timestamp> \"{ts}\"^^<http://www.w3.org/2001/XMLSchema#dateTime> .\n\
                         <{base}ravel/Turn/trial-{n}> <https://repolex.ai/ontology/ravel/role> \"assistant\" .\n\
                         <{base}ravel/Turn/trial-{n}> <https://repolex.ai/ontology/ravel/text> \"trial line {n}\" .\n\
                         <{base}ravel/Turn/trial-{n}> <https://repolex.ai/ontology/ravel/parentTurn> <{base}ravel/Turn/trial-{}> .\n",
                        n.saturating_sub(1)
                    );
                    store.load_nt(&nt, Some(&g)).unwrap();
                    store.flush().unwrap();
                    drop(store);
                    n += 1;
                }
                n
            })
        };
        let t0 = std::time::Instant::now();
        let (mut runs, mut worst, mut failed) = (0u32, 0f64, 0u32);
        while t0.elapsed().as_secs() < secs {
            let t = std::time::Instant::now();
            let _hold = std::env::var("RAVEL_TRIAL_HOLD")
                .is_ok()
                .then(|| crate::graph::reading(std::path::Path::new(&dir)));
            match crate::graph::open_read_only(std::path::Path::new(&dir)) {
                Ok(store) => match run(&store, &q, &base) {
                    Ok((QueryResults::Solutions(sols), _)) => {
                        let rows = sols.filter(|s| s.is_ok()).count();
                        let took = t.elapsed().as_secs_f64();
                        if took > 3.0 {
                            eprintln!("SLOW: {took:.1}s, {rows} rows, at {:.0}s", t0.elapsed().as_secs_f64());
                        }
                        worst = worst.max(took);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        failed += 1;
                        eprintln!("QUERY FAILED: {e:#}");
                    }
                },
                Err(e) => {
                    failed += 1;
                    eprintln!("OPEN FAILED: {e:#}");
                }
            }
            runs += 1;
        }
        done.store(true, Ordering::Relaxed);
        let written = writer.join().unwrap();
        eprintln!("DONE: {runs} queries, worst {worst:.2}s, {failed} failed, {written} turns written");
    }

    /// One query against a real store, read-only, timed. `RAVEL_TRIAL_STORE`
    /// is a phext store directory, `RAVEL_TRIAL_QUERY` a file, `RAVEL_TRIAL_BASE`
    /// the store's root address.
    #[test]
    #[ignore]
    fn real_query() {
        let dir = std::env::var("RAVEL_TRIAL_STORE").unwrap();
        let q = std::fs::read_to_string(std::env::var("RAVEL_TRIAL_QUERY").unwrap()).unwrap();
        let base = std::env::var("RAVEL_TRIAL_BASE").unwrap();
        let t = std::time::Instant::now();
        let store = crate::graph::open_read_only(std::path::Path::new(&dir)).unwrap();
        eprintln!("OPENED in {:.2}s", t.elapsed().as_secs_f64());
        // `RAVEL_TRIAL_CANCEL_MS`: cancel the query that long after it starts,
        // to show what a cancel does to spareval (see daemon/http.rs `answer`).
        let stop = crate::graph::CancellationToken::new();
        if let Ok(ms) = std::env::var("RAVEL_TRIAL_CANCEL_MS") {
            let (stop, ms) = (stop.clone(), ms.parse::<u64>().unwrap());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                stop.cancel();
                eprintln!("CANCELLED at {ms} ms");
            });
        }
        let (r, _) = run_until(&store, &q, &base, Some(stop)).unwrap();
        let (mut n, mut errs) = (0usize, 0usize);
        if let QueryResults::Solutions(sols) = r {
            for s in sols {
                match s {
                    Ok(_) => n += 1,
                    Err(_) => errs += 1,
                }
            }
        }
        eprintln!("ROWS {n}, ERRORS {errs} in {:.2}s", t.elapsed().as_secs_f64());
    }

    /// On a copy of a real soul (`RAVEL_TRIAL_REPO`, in a scratchpad): list
    /// its newest turns for the first time, then time "the newest 40 turns"
    /// asked of that list and asked of the whole store.
    #[test]
    #[ignore]
    fn real_recent() {
        let repo = std::path::PathBuf::from(std::env::var("RAVEL_TRIAL_REPO").unwrap());
        assert!(repo.to_string_lossy().contains("scratchpad"), "a copy, never a soul");
        let names = crate::names::Names::for_soul(&repo).unwrap();
        let store = crate::graph::open(repo.join(crate::sync::STORE_SUBDIR)).unwrap();
        for i in 1..=2 {
            let t = std::time::Instant::now();
            crate::graph::keep_recent(&store, &names, &[]).unwrap();
            eprintln!("LISTED (call {i}) in {:.3}s", t.elapsed().as_secs_f64());
        }
        store.flush().unwrap();
        let ask = |what: &str, q: &str| {
            let t = std::time::Instant::now();
            let (r, _) = run(&store, q, names.root()).unwrap();
            let mut first = String::new();
            let mut n = 0;
            if let QueryResults::Solutions(sols) = r {
                for s in sols {
                    let s = s.unwrap();
                    if n == 0 {
                        first = s.get("ts").map(|t| t.to_string()).unwrap_or_default();
                    }
                    n += 1;
                }
            }
            eprintln!("{what}: {n} rows, newest {first}, in {:.3}s", t.elapsed().as_secs_f64());
        };
        let p = "PREFIX ravel: <https://repolex.ai/ontology/ravel/> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>";
        let rest = "?t ravel:timestamp ?ts ; ravel:role ?role . OPTIONAL { ?t ravel:text ?text } } ORDER BY DESC(?ts) LIMIT 40";
        ask("OF THE LIST", &format!("{p} SELECT ?t ?ts ?role ?text WHERE {{ GRAPH <ravel/NamedGraph/recent> {{ ?l rdfs:member ?t }} {rest}"));
        ask("OF THE WHOLE STORE", &format!("{p} SELECT ?t ?ts ?role ?text WHERE {{ ?t a ravel:Turn . {rest}"));
        ask("OF THE LIST again", &format!("{p} SELECT ?t ?ts ?role ?text WHERE {{ GRAPH <ravel/NamedGraph/recent> {{ ?l rdfs:member ?t }} {rest}"));
    }

    const DATA: &str = r#"
<https://repolex.ai/ravel/Turn/t1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://repolex.ai/ontology/ravel/Turn> <https://repolex.ai/ravel/NamedGraph/a> .
<https://repolex.ai/ravel/Turn/t1> <https://repolex.ai/ontology/ravel/role> "user" <https://repolex.ai/ravel/NamedGraph/a> .
<https://repolex.ai/ravel/Turn/t2> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://repolex.ai/ontology/ravel/Turn> <https://repolex.ai/ravel/NamedGraph/b> .
<https://repolex.ai/ravel/Memory/0-1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://repolex.ai/ontology/ravel/Memory> <https://repolex.ai/ravel/NamedGraph/memory-v1> .
<https://repolex.ai/ravel/Memory/0-1> <https://repolex.ai/ontology/ravel/relatedToId> <https://repolex.ai/ravel/Turn/t1> <https://repolex.ai/ravel/NamedGraph/memory-v1> .
"#;

    fn store() -> Store {
        let s = Store::new().unwrap();
        s.load_nq(DATA).unwrap();
        s
    }

    fn count(store: &Store, q: &str) -> usize {
        match run(store, q, crate::names::BARE_ROOT).unwrap().0 {
            QueryResults::Solutions(s) => s.map(|r| r.unwrap()).count(),
            _ => panic!("expected rows"),
        }
    }

    const P: &str = "PREFIX subtexture: <https://repolex.ai/ontology/subtexture/> \
                     PREFIX ravel: <https://repolex.ai/ontology/ravel/> ";

    #[test]
    fn a_query_with_no_graph_sees_every_graph() {
        let s = store();
        assert_eq!(count(&s, &format!("{P} SELECT ?t WHERE {{ ?t a ravel:Turn }}")), 2);
    }

    #[test]
    fn a_base_class_finds_turns_and_memory_once_each() {
        let s = store();
        let q = format!("{P} SELECT ?x WHERE {{ ?x a subtexture:Thing }}");
        assert_eq!(count(&s, &q), 3);
    }

    #[test]
    fn a_base_property_finds_the_stored_spelling() {
        let s = store();
        let q = format!("{P} SELECT ?m ?t WHERE {{ ?m subtexture:relatedToId ?t }}");
        assert_eq!(count(&s, &q), 1);
    }

    #[test]
    fn a_short_address_resolves_against_the_base() {
        let s = store();
        let q = format!("{P} SELECT ?r WHERE {{ <ravel/Turn/t1> ravel:role ?r }}");
        assert_eq!(count(&s, &q), 1);
        let q = format!("{P} SELECT ?t WHERE {{ GRAPH <ravel/NamedGraph/b> {{ ?t a ravel:Turn }} }}");
        assert_eq!(count(&s, &q), 1);
    }

    #[test]
    fn a_named_graph_still_narrows_the_question() {
        let s = store();
        let q = format!("{P} SELECT ?t WHERE {{ GRAPH ?g {{ ?t a subtexture:Thing }} }}");
        assert_eq!(count(&s, &q), 3);
    }

    /// calamus 0.1.2: a fanned-out name inside GRAPH ?g used to come back
    /// with ?g empty and no error.
    #[test]
    fn a_fanned_out_name_keeps_its_graph() {
        let s = store();
        let q = format!("{P} SELECT ?g ?t WHERE {{ GRAPH ?g {{ ?t a subtexture:Thing }} }}");
        let QueryResults::Solutions(sols) = run(&s, &q, crate::names::BARE_ROOT).unwrap().0 else {
            panic!("expected rows")
        };
        let rows: Vec<_> = sols.map(|r| r.unwrap()).collect();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.get("g").is_some()), "a row lost its graph");
    }

    #[test]
    fn a_name_inside_a_filter_is_reported() {
        let q = format!("{P} SELECT ?s WHERE {{ ?s ?p ?o FILTER(?p = subtexture:relatedToId) }}");
        let (_, report) = prepare(&q, crate::names::BARE_ROOT).unwrap();
        assert_eq!(notes(&report).len(), 1);
    }

    #[test]
    fn a_query_that_does_not_parse_is_refused() {
        assert!(prepare("SELECT WHERE", crate::names::BARE_ROOT).is_err());
    }

    /// Not a test of correctness: the price of the union view and the
    /// rewrite on a real store. `RAVEL_REAL_STORE=<soul>/.ravel/_ignore/store
    /// cargo test --release real_store -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_store_timings() {
        let dir = std::env::var("RAVEL_REAL_STORE").expect("RAVEL_REAL_STORE");
        let s = crate::graph::open_read_only(std::path::Path::new(&dir)).unwrap();
        for (label, q) in [
            ("turns, GRAPH ?g", format!("{P} SELECT (COUNT(?t) AS ?n) WHERE {{ GRAPH ?g {{ ?t a ravel:Turn }} }}")),
            ("turns, no GRAPH", format!("{P} SELECT (COUNT(?t) AS ?n) WHERE {{ ?t a ravel:Turn }}")),
            ("Thing, no GRAPH", format!("{P} SELECT (COUNT(?t) AS ?n) WHERE {{ ?t a subtexture:Thing }}")),
            ("latest 20 turns, no GRAPH", format!("{P} SELECT ?t ?ts WHERE {{ ?t a ravel:Turn ; ravel:timestamp ?ts }} ORDER BY DESC(?ts) LIMIT 20")),
            ("latest 20 turns, GRAPH ?g", format!("{P} SELECT ?t ?ts WHERE {{ GRAPH ?g {{ ?t a ravel:Turn ; ravel:timestamp ?ts }} }} ORDER BY DESC(?ts) LIMIT 20")),
            ("relatedToId base name", format!("{P} SELECT (COUNT(*) AS ?n) WHERE {{ ?m subtexture:relatedToId ?t }}")),
        ] {
            let t = std::time::Instant::now();
            let (r, _) = run(&s, &q, crate::names::BARE_ROOT).unwrap();
            let out = match r {
                QueryResults::Solutions(sols) => {
                    let rows: Vec<_> = sols.map(|r| r.unwrap()).collect();
                    if rows.len() == 1 { rows[0].iter().map(|(_, v)| v.to_string()).collect::<Vec<_>>().join(" ") } else { format!("{} rows", rows.len()) }
                }
                _ => String::new(),
            };
            println!("{:>8.3}s  {label}: {out}", t.elapsed().as_secs_f64());
        }
    }

    /// calamus drops abstract names from every answer, so a stored one would
    /// be hidden. Nothing ravel projects may use one.
    #[test]
    fn the_projection_writes_no_subtexture_name() {
        for src in [
            include_str!("project.rs"),
            include_str!("memory_log.rs"),
            include_str!("graph.rs"),
        ] {
            let code: String = src
                .lines()
                .take_while(|l| !l.contains("#[cfg(test)]"))
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !code.contains("ontology/subtexture/"),
                "a writer names a subtexture: address"
            );
        }
    }
}
