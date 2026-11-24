//! Ground-truth dump of calamus::Index targets over ravel's shipped
//! ontologies, for diffing against the phext backend's ontology_prep().
//! Usage: indexdump <subtexture.ttl> <ravel.ttl>
//! Prints one line per non-Same target: "prop\t<asked>\t<stored,stored,...>"
//! or "class\t<asked>\t<stored,...>".
use calamus::{Index, Targets};
use oxrdf::{NamedNode, NamedOrBlankNode, Term};
use std::collections::BTreeSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let texts: Vec<String> = args[1..]
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let index = Index::from_turtle(&refs, &["https://repolex.ai/ontology/subtexture/"])?;

    // Candidate names: every named subject/object in either ontology.
    let mut names: BTreeSet<String> = BTreeSet::new();
    for text in &texts {
        for t in oxttl::TurtleParser::new().for_slice(text.as_bytes()) {
            let t = t?;
            if let NamedOrBlankNode::NamedNode(s) = &t.subject {
                names.insert(s.as_str().to_string());
            }
            if let Term::NamedNode(o) = &t.object {
                names.insert(o.as_str().to_string());
            }
        }
    }
    for name in &names {
        let node = NamedNode::new_unchecked(name);
        match index.property(node.as_ref()) {
            Targets::Same => {}
            Targets::One(one) => println!("prop\t{name}\t{}", one.as_str()),
            Targets::Many(many) => println!(
                "prop\t{name}\t{}",
                many.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(",")
            ),
        }
        match index.class(node.as_ref()) {
            Targets::Same => {}
            Targets::One(one) => println!("class\t{name}\t{}", one.as_str()),
            Targets::Many(many) => println!(
                "class\t{name}\t{}",
                many.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(",")
            ),
        }
    }
    eprintln!("index len: {:?}", index.len());
    Ok(())
}
