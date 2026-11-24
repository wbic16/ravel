#!/bin/bash
# v2 compare phase on rock-mountain: rebuild example, dumpa + edge, three-way compare.
set -x
cd ~/ravel-v2 && tar -xzf ~/v2c.tar.gz
cp ~/ravel-v2/v2bench.rs ~/ravel-study/examples/v2bench.rs
cd ~/ravel-study && cargo build --release --example v2bench 2>&1 | tail -2
BIN=~/ravel-study/target/release/examples/v2bench
$BIN dumpa ~/ravel-v2/corpus.json ~/ravel-v2/native-store-a
$BIN edge ~/ravel-v2/corpus.json ~/ravel-v2/native-store-edge | grep -E "Turn/e0001" | grep -c "ontology/ravel/text" || true
$BIN edge ~/ravel-v2/corpus.json ~/ravel-v2/native-store-edge2 | grep "Turn/e0001" | head -20
cd ~/ravel-v2 && MEASURE_SQ=18995 python3 compare.py
