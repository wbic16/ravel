#!/bin/bash
# SCALE leg on rock-mountain: backend + native measurements per item.
set -x
cd ~/ravel-v2 && tar -xzf ~/v2s.tar.gz
cp ~/ravel-v2/v2bench.rs ~/ravel-study/examples/v2bench.rs
cd ~/ravel-study && cargo build --release --example v2bench 2>&1 | tail -2
BIN=~/ravel-study/target/release/examples/v2bench
cd ~/ravel-v2 && PHEXT_SPARQL_BIN=~/ravel-v2/work/bin/phext-sparql python3 scale.py > ~/ravel-v2/scale-backend.jsonl 2>~/ravel-v2/scale-backend.err
for f in ~/ravel-v2/scale/*.json; do
  item=$(basename $f .json)
  echo "== native $item =="
  $BIN scale $f ~/ravel-v2/scale-native-$item
done > ~/ravel-v2/scale-native.txt 2>&1
echo SCALE-DONE
