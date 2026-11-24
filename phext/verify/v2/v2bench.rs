//! v2bench — native-side driver for the ravel-phext v2 comparison pass.
//! Lives in examples/ of the study clone; not part of ravel.
//! Modes:
//!   bench <corpus.json> <storedir>  — timing samples (JSON lines on stdout)
//!   dump  <corpus.json> <storedir> <out.nq> — ingest once, dump all quads
use ravel::graph;
use ravel::names::Names;
use ravel::reader::{Annotation, SignalValue};
use ravel::{Event, SourceKind, TextSpan, Wrote};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::time::Instant;

fn ev(j: &Value) -> Event {
    let prov = j
        .get("text_provenance")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .map(|s| TextSpan {
                    start: s["start"].as_u64().unwrap() as usize,
                    end: s["end"].as_u64().unwrap() as usize,
                    kind: match s["kind"].as_str().unwrap_or("authored") {
                        "tool_result" => SourceKind::ToolResult,
                        _ => SourceKind::Authored,
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    Event {
        event_id: j["event_id"].as_str().unwrap().to_string(),
        parent_id: j.get("parent_id").and_then(|v| v.as_str()).map(str::to_string),
        role: j["role"].as_str().unwrap().to_string(),
        timestamp: j.get("timestamp").and_then(|v| v.as_str()).map(str::to_string),
        text: j.get("text").and_then(|v| v.as_str()).map(str::to_string),
        text_provenance: prov,
        thinking: None,
        model: j.get("model").and_then(|v| v.as_str()).map(str::to_string),
        model_setting: j.get("model_setting").and_then(|v| v.as_str()).map(str::to_string),
        wrote: match j.get("wrote").and_then(|v| v.as_str()) {
            Some("person") => Wrote::Person,
            Some("soul") => Wrote::Soul,
            _ => Wrote::Nobody,
        },
        actions: Vec::new(),
        outcomes: Vec::new(),
    }
}

fn ann(j: &Value) -> Annotation {
    let mut signal = BTreeMap::new();
    for (k, v) in j["signal"].as_object().unwrap() {
        let (kind, val) = (v[0].as_str().unwrap(), &v[1]);
        signal.insert(
            k.clone(),
            match kind {
                "int" => SignalValue::Int(
                    val.as_i64()
                        .unwrap_or_else(|| val.as_str().unwrap().parse().unwrap()),
                ),
                "decimal" => SignalValue::Decimal(val.as_str().unwrap().to_string()),
                _ => SignalValue::Text(val.as_str().unwrap().to_string()),
            },
        );
    }
    Annotation {
        reader: j["reader"].as_str().unwrap().to_string(),
        event_type: j["event_type"].as_str().unwrap().to_string(),
        ts: j.get("ts").and_then(|v| v.as_str()).map(str::to_string),
        event_id: j["event_id"].as_str().unwrap().to_string(),
        start: j["start"].as_u64().unwrap() as usize,
        end: j["end"].as_u64().unwrap() as usize,
        source_kind: match j.get("source_kind").and_then(|v| v.as_str()) {
            Some("tool_result") => Some(SourceKind::ToolResult),
            Some(_) => Some(SourceKind::Authored),
            None => None,
        },
        signal,
    }
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (mode, corpus_path, store_dir) = (&args[1], &args[2], &args[3]);
    let corpus: Value = serde_json::from_str(&std::fs::read_to_string(corpus_path)?)?;

    if mode == "scale" {
        // SCALE leg: one real-world item (events extracted from a working
        // phext's scrolls) ingested as a single transcript; same axes as bench.
        let names = Names::new("7r1pl3f0rc3", "spacegoat")?;
        let root = names.root().to_string();
        let ravel_ns = "https://repolex.ai/ontology/ravel/";
        let events: Vec<Event> = corpus["events"].as_array().unwrap().iter().map(ev).collect();
        let _ = std::fs::remove_dir_all(store_dir);
        let store = graph::open(store_dir)?;
        let mut xs = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            graph::ingest_transcript(&store, &events, &[], "scale", &names)?;
            xs.push(t.elapsed().as_secs_f64());
        }
        println!("{{\"axis\":\"ingest\",\"median_s\":{},\"samples\":{:?}}}", median(xs.clone()), xs);
        let mid = &events[events.len() / 2];
        let turn = format!("{root}ravel/Turn/{}", mid.event_id);
        let point = format!("SELECT ?text WHERE {{ GRAPH ?g {{ <{turn}> <{ravel_ns}text> ?text }} }}");
        let mut xs = Vec::new();
        for _ in 0..7 {
            let t = Instant::now();
            let rows = graph::query_rows(&store, &point)?;
            assert_eq!(rows.len(), 1);
            xs.push(t.elapsed().as_secs_f64());
        }
        println!("{{\"axis\":\"point_lookup\",\"median_s\":{},\"samples\":{:?}}}", median(xs.clone()), xs);
        let mut xs = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            let rows = graph::query_rows(&store, "SELECT ?s ?p ?o WHERE { GRAPH ?g { ?s ?p ?o } }")?;
            xs.push(t.elapsed().as_secs_f64());
            if xs.len() == 1 {
                println!("{{\"axis\":\"quad_count\",\"rows\":{}}}", rows.len());
            }
        }
        println!("{{\"axis\":\"full_scan\",\"median_s\":{},\"samples\":{:?}}}", median(xs.clone()), xs);
        let mut changed = events.clone();
        let mi = events.len() / 2;
        changed[mi].text = changed[mi].text.as_ref().map(|t| format!("{t}!"));
        let t = Instant::now();
        graph::ingest_transcript(&store, &changed, &[], "scale", &names)?;
        println!("{{\"axis\":\"mutation_reingest_one_change\",\"s\":{}}}", t.elapsed().as_secs_f64());
        let t = Instant::now();
        graph::ingest_transcript(&store, &events, &[], "scale", &names)?;
        println!("{{\"axis\":\"reingest_no_change\",\"s\":{}}}", t.elapsed().as_secs_f64());
        store.flush()?;
        drop(store);
        let mut total = 0u64;
        for b in walkdir(store_dir) {
            total += b;
        }
        println!("{{\"axis\":\"store_bytes_apparent\",\"bytes\":{total}}}");
        return Ok(());
    }
    let evs = |k: &str| -> Vec<Event> {
        corpus[k].as_array().unwrap().iter().map(ev).collect()
    };
    let anns = |k: &str| -> Vec<Annotation> {
        corpus[k].as_array().unwrap().iter().map(ann).collect()
    };
    let (a1, a2, b1) = (evs("EVENTS_A1"), evs("EVENTS_A2"), evs("EVENTS_B1"));
    let (an1, an2) = (anns("ANNS_A1"), anns("ANNS_A2"));
    let names = Names::new("7r1pl3f0rc3", "spacegoat")?;
    let root = names.root().to_string();
    let ravel_ns = "https://repolex.ai/ontology/ravel/";

    if mode == "dumpa" || mode == "edge" {
        let store = graph::open(store_dir)?;
        if mode == "dumpa" {
            graph::ingest_transcript(&store, &a1, &an1, "sess-A", &names)?;
            graph::ingest_transcript(&store, &a2, &an2, "sess-B", &names)?;
        } else {
            let mut e = a1[0].clone();
            e.event_id = "e0001".to_string();
            e.parent_id = None;
            e.role = "assistant".to_string();
            e.wrote = Wrote::Nobody;
            e.timestamp = Some("2026-07-03T00:00:00Z".to_string());
            e.text = Some(String::new());
            e.text_provenance = Vec::new();
            graph::ingest_transcript(&store, &[e], &[], "edge", &names)?;
            for q in store.iter() {
                let q = q?;
                println!("{} {} {} {} .", q.subject, q.predicate, q.object, q.graph_name);
            }
        }
        store.flush()?;
        drop(store);
        let mut total = 0u64;
        for b in walkdir(store_dir) {
            total += b;
        }
        println!("{{\"mode\":\"{mode}\",\"store_bytes_apparent\":{total}}}");
        return Ok(());
    }

    if mode == "dump2" || mode == "dumpb" {
        // Clean per-soul dumps (one store per soul, as production raveld runs):
        // dump2 = soul A transcripts + native optmem projection; dumpb = soul B alone.
        let out_path = &args[4];
        let store = graph::open(store_dir)?;
        if mode == "dump2" {
            graph::ingest_transcript(&store, &a1, &an1, "sess-A", &names)?;
            graph::ingest_transcript(&store, &a2, &an2, "sess-B", &names)?;
            let repo = std::path::PathBuf::from(format!("{store_dir}-repo"));
            let logdir = repo.join(".ravel/_ignore/optmem");
            std::fs::create_dir_all(&logdir)?;
            let mems: Vec<ravel::memory_log::Memory> = corpus["NOTES_A"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| ravel::memory_log::Memory {
                    id: n["id"].as_str().unwrap().to_string(),
                    ts: n["ts"].as_str().unwrap().to_string(),
                    text: n["text"].as_str().unwrap().to_string(),
                    turns: n["turns"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|t| t.as_str().unwrap().to_string())
                        .collect(),
                    model: n["model"].as_str().unwrap().to_string(),
                })
                .collect();
            let sums: Vec<ravel::memory_log::Summary> = corpus["SUMMARIES_A"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| ravel::memory_log::Summary {
                    block: s["block"].as_str().unwrap().to_string(),
                    text: s["text"].as_str().unwrap().to_string(),
                    model: s["model"].as_str().unwrap().to_string(),
                    written: s["written"].as_str().unwrap().to_string(),
                    wording: s["wording"].as_str().map(str::to_string),
                })
                .collect();
            let mut mj = String::new();
            for m in &mems {
                mj.push_str(&serde_json::to_string(m)?);
                mj.push('\n');
            }
            std::fs::write(logdir.join("memories.jsonl"), mj)?;
            let mut sj = String::new();
            for s in &sums {
                sj.push_str(&serde_json::to_string(s)?);
                sj.push('\n');
            }
            std::fs::write(logdir.join("summaries.jsonl"), sj)?;
            ravel::memory_log::project_into(&store, &repo, &names)?;
        } else {
            let bare = Names::bare();
            graph::ingest_transcript(&store, &b1, &[], "only", &bare)?;
        }
        store.flush()?;
        let mut f = std::fs::File::create(out_path)?;
        let mut n = 0usize;
        for q in store.iter() {
            let q = q?;
            writeln!(f, "{} {} {} {} .", q.subject, q.predicate, q.object, q.graph_name)?;
            n += 1;
        }
        println!("dumped {n} quads ({mode})");
        return Ok(());
    }

    if mode == "dump" {
        let out_path = &args[4];
        let store = graph::open(store_dir)?;
        graph::ingest_transcript(&store, &a1, &an1, "sess-A", &names)?;
        graph::ingest_transcript(&store, &a2, &an2, "sess-B", &names)?;
        let bare = Names::bare();
        graph::ingest_transcript(&store, &b1, &[], "only", &bare)?;
        store.flush()?;
        let mut f = std::fs::File::create(out_path)?;
        let mut n = 0usize;
        for q in store.iter() {
            let q = q?;
            writeln!(f, "{} {} {} {} .", q.subject, q.predicate, q.object, q.graph_name)?;
            n += 1;
        }
        println!("dumped {n} quads");
        return Ok(());
    }

    if mode == "outside" {
        // Run outside (calamus-prepared) queries over an existing store and
        // print one JSON object per query: notes + normalized results.
        use oxigraph::sparql::QueryResults;
        let qpath = &args[4];
        let queries: Value = serde_json::from_str(&std::fs::read_to_string(qpath)?)?;
        let store = graph::open(store_dir)?;
        for q in queries.as_array().unwrap() {
            let id = q["id"].as_str().unwrap();
            let text = q["sparql"].as_str().unwrap();
            let base = q["base"].as_str().unwrap();
            let mut out = serde_json::Map::new();
            out.insert("id".into(), Value::String(id.to_string()));
            match ravel::outside::run(&store, text, base) {
                Ok((results, report)) => {
                    let notes: Vec<Value> = ravel::outside::notes(&report)
                        .into_iter()
                        .map(Value::String)
                        .collect();
                    out.insert("notes".into(), Value::Array(notes));
                    match results {
                        QueryResults::Solutions(rows) => {
                            let mut rs: Vec<Value> = Vec::new();
                            for row in rows {
                                let row = row?;
                                let mut m = serde_json::Map::new();
                                for var in row.variables() {
                                    if let Some(t) = row.get(var.as_str()) {
                                        m.insert(
                                            var.as_str().to_string(),
                                            Value::String(t.to_string()),
                                        );
                                    }
                                }
                                rs.push(Value::Object(m));
                            }
                            rs.sort_by_key(|r| r.to_string());
                            out.insert("kind".into(), Value::String("solutions".into()));
                            out.insert("rows".into(), Value::Array(rs));
                        }
                        QueryResults::Boolean(b) => {
                            out.insert("kind".into(), Value::String("boolean".into()));
                            out.insert("value".into(), Value::Bool(b));
                        }
                        _ => {
                            // oxigraph 0.5.6 as ravel configures it exposes no
                            // Triples variant to match on (E0599): CONSTRUCT /
                            // DESCRIBE results cannot be consumed here. The
                            // probe grades those two via their pattern SELECTs.
                            out.insert("kind".into(), Value::String("other-variant".into()));
                        }
                    }
                }
                Err(e) => {
                    out.insert("error".into(), Value::String(format!("{e:#}")));
                }
            }
            println!("{}", Value::Object(out));
        }
        return Ok(());
    }

    if mode == "crafted" {
        // Load an N-Quads file into a fresh store, then run the queries in
        // the JSON file (args[5]) through outside::run; print JSON per query.
        use oxigraph::sparql::QueryResults;
        let nq_path = &args[4];
        let qpath = &args[5];
        let _ = std::fs::remove_dir_all(store_dir);
        let store = graph::open(store_dir)?;
        let nq = std::fs::read(nq_path)?;
        store.load_from_reader(
            oxigraph::io::RdfParser::from_format(oxigraph::io::RdfFormat::NQuads),
            nq.as_slice(),
        )?;
        store.flush()?;
        let queries: Value = serde_json::from_str(&std::fs::read_to_string(qpath)?)?;
        for q in queries.as_array().unwrap() {
            let mut out = serde_json::Map::new();
            out.insert("id".into(), Value::String(q["id"].as_str().unwrap().to_string()));
            match ravel::outside::run(&store, q["sparql"].as_str().unwrap(), q["base"].as_str().unwrap()) {
                Ok((QueryResults::Solutions(rows), _report)) => {
                    let mut rs: Vec<Value> = Vec::new();
                    for row in rows {
                        let row = row?;
                        let mut m = serde_json::Map::new();
                        for var in row.variables() {
                            if let Some(t) = row.get(var.as_str()) {
                                m.insert(var.as_str().to_string(), Value::String(t.to_string()));
                            }
                        }
                        rs.push(Value::Object(m));
                    }
                    rs.sort_by_key(|r| r.to_string());
                    out.insert("rows".into(), Value::Array(rs));
                }
                Ok(_) => {
                    out.insert("error".into(), Value::String("non-solutions result".into()));
                }
                Err(e) => {
                    out.insert("error".into(), Value::String(format!("{e:#}")));
                }
            }
            println!("{}", Value::Object(out));
        }
        return Ok(());
    }

    // bench
    let _ = std::fs::remove_dir_all(store_dir);
    let store = graph::open(store_dir)?;
    let mut time = |label: &str, n: usize, mut f: &mut dyn FnMut()| {
        f(); // warm-up
        let mut xs = Vec::new();
        for _ in 0..n {
            let t = Instant::now();
            f();
            xs.push(t.elapsed().as_secs_f64());
        }
        println!("{{\"axis\":\"{label}\",\"median_s\":{},\"samples\":{:?}}}", median(xs.clone()), xs);
    };

    time("ingest_sessA", 7, &mut || {
        graph::ingest_transcript(&store, &a1, &an1, "sess-A", &names).unwrap();
    });
    time("ingest_sessB", 7, &mut || {
        graph::ingest_transcript(&store, &a2, &an2, "sess-B", &names).unwrap();
    });

    let turn = format!("{root}ravel/Turn/a0001");
    let point = format!(
        "SELECT ?ts ?role ?text WHERE {{ GRAPH ?g {{ <{turn}> <{ravel_ns}timestamp> ?ts ; <{ravel_ns}role> ?role . OPTIONAL {{ <{turn}> <{ravel_ns}text> ?text }} }} }} LIMIT 1"
    );
    time("point_lookup", 7, &mut || {
        let rows = graph::query_rows(&store, &point).unwrap();
        assert_eq!(rows.len(), 1);
    });
    let one_hop = format!("SELECT ?t ?p WHERE {{ GRAPH ?g {{ ?t <{ravel_ns}parentTurn> ?p }} }}");
    time("traversal_1hop", 7, &mut || {
        graph::query_rows(&store, &one_hop).unwrap();
    });
    time("traversal_3hop", 7, &mut || {
        let mut cur = turn.clone();
        for _ in 0..3 {
            let q = format!("SELECT ?p WHERE {{ GRAPH ?g {{ <{cur}> <{ravel_ns}parentTurn> ?p }} }}");
            let rows = graph::query_rows(&store, &q).unwrap();
            if rows.is_empty() {
                break;
            }
            cur = rows[0]["p"].clone();
        }
    });
    time("emojikeys_fixed_query", 7, &mut || {
        graph::query_emojikeys(&store, None).unwrap();
    });

    // mutation, backend-comparable: change one char, re-ingest, restore —
    // the pair timed as one sample, as measure.py's mutate() does.
    let mut changed = a1.clone();
    changed[1].text = changed[1].text.as_ref().map(|t| format!("{t}!"));
    time("mutation_reingest_pair", 5, &mut || {
        graph::ingest_transcript(&store, &changed, &an1, "sess-A", &names).unwrap();
        graph::ingest_transcript(&store, &a1, &an1, "sess-A", &names).unwrap();
    });
    // native incremental path: append one new turn to sess-A.
    let extra = vec![{
        let mut e = a1[4].clone();
        e.event_id = "u0003".to_string();
        e.parent_id = Some("x0001".to_string());
        e.timestamp = Some("2026-07-01T09:05:00Z".to_string());
        e.text = Some("one more line".to_string());
        e
    }];
    time("mutation_append_one", 7, &mut || {
        graph::append_transcript(&store, &extra, &[], "sess-A", &names).unwrap();
    });

    store.flush()?;
    drop(store);
    let mut total = 0u64;
    for e in walkdir(store_dir) {
        total += e;
    }
    println!("{{\"axis\":\"store_bytes_apparent\",\"bytes\":{total},\"dir\":\"{store_dir}\"}}");
    Ok(())
}

fn walkdir(p: &str) -> Vec<u64> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                out.extend(walkdir(path.to_str().unwrap()));
            } else if let Ok(m) = e.metadata() {
                out.push(m.len());
            }
        }
    }
    out
}
