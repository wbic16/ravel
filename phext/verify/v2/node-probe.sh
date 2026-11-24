#!/bin/bash
# v2 probe phase on rock-mountain: calamus Index ground truth, outside/fringe
# behavioural probe (native vs fixed backend), crafted duplicate probe.
set -x
cd ~/ravel-v2 && tar -xzf ~/v2p.tar.gz
cp ~/ravel-v2/indexdump.rs ~/ravel-v2/calamus/examples/indexdump.rs
cp ~/ravel-v2/backend-fixed.py ~/ravel-v2/work/backend/ravel_backend.py
sha256sum ~/ravel-study/ontology/subtexture/subtexture.ttl ~/ravel-study/ontology/ravel/ravel.ttl
cd ~/ravel-v2/calamus && cargo build --release --features turtle --example indexdump 2>&1 | tail -2
./target/release/examples/indexdump ~/ravel-study/ontology/subtexture/subtexture.ttl ~/ravel-study/ontology/ravel/ravel.ttl > ~/ravel-v2/indexdump.txt 2>~/ravel-v2/indexdump.err
cat ~/ravel-v2/indexdump.err
cp ~/ravel-v2/v2bench.rs ~/ravel-study/examples/v2bench.rs
cd ~/ravel-study && cargo build --release --example v2bench 2>&1 | tail -2
BIN=~/ravel-study/target/release/examples/v2bench
$BIN outside ~/ravel-v2/corpus.json ~/ravel-v2/native-store-a2 ~/ravel-v2/queries.json > ~/ravel-v2/native-results.jsonl 2>~/ravel-v2/native-results.err
cd ~/ravel-v2 && PHEXT_SPARQL_BIN=~/ravel-v2/work/bin/phext-sparql python3 probe.py
$BIN crafted ~/ravel-v2/corpus.json ~/ravel-v2/crafted-store ~/ravel-v2/crafted.nq ~/ravel-v2/crafted-queries.json > ~/ravel-v2/crafted-native.jsonl 2>~/ravel-v2/crafted-native.err
EV=~/ravel-v2/work/bin/phext-sparql
printf 'SELECT ?t WHERE { ?t a <https://repolex.ai/ontology/subtexture/Thing> }' > ~/ravel-v2/craft1.rq
printf 'SELECT ?g ?t WHERE { GRAPH ?g { ?t a <https://repolex.ai/ontology/subtexture/Thing> } }' > ~/ravel-v2/craft2.rq
$EV ~/ravel-v2/crafted.nq ~/ravel-v2/craft1.rq https://repolex.ai/7r1pl3f0rc3/spacegoat/ ~/ravel-v2/prep.txt > ~/ravel-v2/crafted-eval1.txt 2>&1
$EV ~/ravel-v2/crafted.nq ~/ravel-v2/craft2.rq https://repolex.ai/7r1pl3f0rc3/spacegoat/ ~/ravel-v2/prep.txt > ~/ravel-v2/crafted-eval2.txt 2>&1
du -sb ~/ravel-v2/native-store-a2 ~/ravel-v2/native-store-b
echo PROBE-DONE
