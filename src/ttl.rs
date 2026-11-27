//! A purpose-built Turtle reader — the Rust port of the phext backend's
//! `_ttl_tokens` / `_ttl_triples` (`phext/backend/ravel_backend.py`), which
//! is itself a faithful port of calamus's ontology Index reader.
//!
//! Scope, honestly stated: it reads the Turtle ravel actually ships and
//! loads — subject + predicate-object lists, `;` / `,` sugar, prefixed
//! names, IRIs, string literals (plain, typed, language-tagged). Blank-node
//! blocks `[ … ]` and collections `( … )` are skipped whole, exactly as
//! the Python reader skips them; a triple whose subject or object is not
//! a named node (or, for objects, a literal) is dropped. That is enough
//! for the two shipped ontologies and the ontology fixtures the test
//! suite loads; it is NOT a general Turtle parser (census gap #24, a
//! ruled divergence — the phext store itself reads N-Triples/N-Quads).

/// One term of a parsed Turtle triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TtlTerm {
    Iri(String),
    Literal {
        value: String,
        datatype: Option<String>,
        lang: Option<String>,
    },
}

pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Iri(String),
    Lit(String),
    Word(String),
    Dir(String),
    Punct(char),
    /// A literal with its suffix already attached (datatype IRI or lang).
    FullLit {
        value: String,
        datatype: Option<String>,
        lang: Option<String>,
    },
}

fn tokenize(text: &str) -> Vec<Tok> {
    let cs: Vec<char> = text.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0usize;
    let n = cs.len();
    while i < n {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '#' {
            while i < n && cs[i] != '\n' {
                i += 1;
            }
        } else if c == '<' {
            let mut j = i + 1;
            while j < n && cs[j] != '>' {
                j += 1;
            }
            toks.push(Tok::Iri(cs[i + 1..j].iter().collect()));
            i = (j + 1).min(n);
        } else if c == '"' || c == '\'' {
            let q3: String = cs[i..].iter().take(3).collect();
            if q3 == "\"\"\"" || q3 == "'''" {
                let q = q3.clone();
                let mut j = i + 3;
                let mut val = String::new();
                while j + 2 < n + 1 && cs[j..].iter().take(3).collect::<String>() != q {
                    val.push(cs[j]);
                    j += 1;
                    if j >= n {
                        break;
                    }
                }
                toks.push(Tok::Lit(val));
                i = (j + 3).min(n);
            } else {
                let mut j = i + 1;
                let mut buf = String::new();
                while j < n && cs[j] != c {
                    if cs[j] == '\\' && j + 1 < n {
                        buf.push(cs[j]);
                        buf.push(cs[j + 1]);
                        j += 2;
                    } else {
                        buf.push(cs[j]);
                        j += 1;
                    }
                }
                toks.push(Tok::Lit(buf));
                i = (j + 1).min(n);
            }
        } else if ";,.[]()".contains(c) {
            toks.push(Tok::Punct(c));
            i += 1;
        } else if c == '@' {
            let mut j = i + 1;
            while j < n && (cs[j].is_alphanumeric() || cs[j] == '-' || cs[j] == '_') {
                j += 1;
            }
            toks.push(Tok::Dir(cs[i + 1..j].iter().collect()));
            i = j;
        } else if c == '^' {
            // ^^<datatype> or ^^prefix:name follows a literal.
            toks.push(Tok::Word("^^".into()));
            i += 2;
        } else {
            let mut j = i;
            while j < n && !" \t\r\n;,.[]()<>\"'#".contains(cs[j]) {
                j += 1;
            }
            if j == i {
                j = i + 1;
            }
            toks.push(Tok::Word(cs[i..j].iter().collect()));
            i = j;
        }
    }
    toks
}

/// Parse Turtle text into triples. `prefixes` carries across calls (the
/// two shipped ontologies share one prefix table, as in the backend).
pub fn triples(
    text: &str,
    prefixes: &mut std::collections::HashMap<String, String>,
) -> Vec<(TtlTerm, TtlTerm, TtlTerm)> {
    let raw = tokenize(text);
    // Attach literal suffixes (^^dt / @lang) so the statement parser sees
    // one token per object.
    let mut toks: Vec<Tok> = Vec::new();
    let mut it = raw.into_iter().peekable();
    while let Some(t) = it.next() {
        if let Tok::Lit(v) = t {
            match it.peek() {
                Some(Tok::Dir(lang)) => {
                    let lang = lang.clone();
                    it.next();
                    toks.push(Tok::FullLit {
                        value: v,
                        datatype: None,
                        lang: Some(lang),
                    });
                }
                Some(Tok::Word(w)) if w == "^^" => {
                    it.next();
                    let dt = match it.next() {
                        Some(Tok::Iri(i)) => Some(i),
                        Some(Tok::Word(name)) => Some(expand_word(&name, prefixes)),
                        _ => None,
                    };
                    toks.push(Tok::FullLit {
                        value: v,
                        datatype: dt,
                        lang: None,
                    });
                }
                _ => toks.push(Tok::FullLit {
                    value: v,
                    datatype: None,
                    lang: None,
                }),
            }
        } else {
            toks.push(t);
        }
    }

    let expand =
        |tok: &Tok, prefixes: &std::collections::HashMap<String, String>| -> Option<TtlTerm> {
            match tok {
                Tok::Iri(i) => Some(TtlTerm::Iri(i.clone())),
                Tok::Word(w) => Some(TtlTerm::Iri(expand_word(w, prefixes))),
                Tok::FullLit {
                    value,
                    datatype,
                    lang,
                } => Some(TtlTerm::Literal {
                    value: value.clone(),
                    datatype: datatype.clone(),
                    lang: lang.clone(),
                }),
                _ => None,
            }
        };

    fn skip_block(toks: &[Tok], mut pos: usize, open: char, close: char) -> usize {
        let mut depth = 0i32;
        while pos < toks.len() {
            if toks[pos] == Tok::Punct(open) {
                depth += 1;
            } else if toks[pos] == Tok::Punct(close) {
                depth -= 1;
                if depth == 0 {
                    return pos + 1;
                }
            }
            pos += 1;
        }
        pos
    }

    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < toks.len() {
        match &toks[pos] {
            Tok::Dir(d) if d == "prefix" || d == "base" => {
                if *d == "prefix" && pos + 2 < toks.len() {
                    if let (Tok::Word(name), Tok::Iri(iri)) = (&toks[pos + 1], &toks[pos + 2]) {
                        prefixes.insert(name.trim_end_matches(':').to_string(), iri.clone());
                    }
                    pos += 3;
                } else {
                    pos += 1;
                }
                if pos < toks.len() && toks[pos] == Tok::Punct('.') {
                    pos += 1;
                }
                continue;
            }
            Tok::Dir(_) => {
                pos += 1;
                continue;
            }
            _ => {}
        }
        let subj = expand(&toks[pos], prefixes);
        pos += 1;
        while pos < toks.len() {
            if toks[pos] == Tok::Punct('.') {
                pos += 1;
                break;
            }
            if toks[pos] == Tok::Punct(';') {
                pos += 1;
                continue;
            }
            let pred = expand(&toks[pos], prefixes);
            pos += 1;
            while pos < toks.len() {
                let obj = if toks[pos] == Tok::Punct('[') {
                    pos = skip_block(&toks, pos, '[', ']');
                    None
                } else if toks[pos] == Tok::Punct('(') {
                    pos = skip_block(&toks, pos, '(', ')');
                    None
                } else {
                    let o = expand(&toks[pos], prefixes);
                    pos += 1;
                    o
                };
                if let (Some(s), Some(p), Some(o)) = (&subj, &pred, &obj) {
                    // Only named-node subjects/predicates are kept (the
                    // backend's build filter); objects may be literals.
                    if matches!(s, TtlTerm::Iri(_)) && matches!(p, TtlTerm::Iri(_)) {
                        out.push((s.clone(), p.clone(), o.clone()));
                    }
                }
                if pos < toks.len() && toks[pos] == Tok::Punct(',') {
                    pos += 1;
                    continue;
                }
                break;
            }
            if pos < toks.len() && toks[pos] == Tok::Punct(';') {
                continue;
            }
            if pos < toks.len() && toks[pos] == Tok::Punct('.') {
                pos += 1;
            }
            break;
        }
    }
    out
}

fn expand_word(w: &str, prefixes: &std::collections::HashMap<String, String>) -> String {
    if w == "a" {
        return RDF_TYPE.to_string();
    }
    if let Some((p, local)) = w.split_once(':') {
        return format!(
            "{}{}",
            prefixes.get(p).cloned().unwrap_or_else(|| format!("{p}:")),
            local
        );
    }
    w.to_string()
}
