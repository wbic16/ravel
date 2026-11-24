#!/bin/bash
# SCALE leg, native side only (runs independently of the backend pass).
set -x
BIN=~/ravel-study/target/release/examples/v2bench
for f in ~/ravel-v2/scale/*.json; do
  item=$(basename $f .json)
  echo "== native $item =="
  $BIN scale $f ~/ravel-v2/scale-native-$item
done > ~/ravel-v2/scale-native.txt 2>&1
echo SCALE-NATIVE-DONE
