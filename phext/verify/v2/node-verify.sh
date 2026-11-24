#!/bin/bash
# v2 re-verification on rock-mountain: fixed suite, bite test, wavelets.
set -x
cd ~/ravel-v2 && tar -xzf ~/v2v.tar.gz
cp ~/ravel-v2/ravel_backend.py ~/ravel-v2/work/backend/ravel_backend.py
cp ~/ravel-v2/harness.py ~/ravel-v2/work/verify/harness.py
cp ~/ravel-v2/wavelets_test.py ~/ravel-v2/work/verify/wavelets_test.py
mkdir -p ~/ravel-v2/bite/verify
rm -rf ~/ravel-v2/bite/backend
cp -r ~/ravel-v2/bite-backend ~/ravel-v2/bite/backend
cp ~/ravel-v2/harness.py ~/ravel-v2/bite/verify/harness.py
EV=~/ravel-v2/work/bin/phext-sparql

suite() {
  T=$1
  rm -rf $T/data1 $T/data2
  mkdir -p $T/data1 $T/data2
  printf '[{"org": "7r1pl3f0rc3", "store": "spacegoat"}]' > $T/data2/souls.json
  (cd $T/backend && RAVEL_PHEXT_DATA=$T/data2 RAVEL_PORT=17881 RAVEL_SQ_PORT=18992 PHEXT_SPARQL_BIN=$EV nohup python3 server.py > $T/server.log 2>&1 &)
  sleep 4
  (cd $T/verify && HARNESS_DATA1=$T/data1 HARNESS_SQ1=18993 HARNESS_PORT=17881 HARNESS_SQ3=18997 PHEXT_SPARQL_BIN=$EV python3 harness.py > $T/harness-out.txt 2>&1)
  echo "suite $T exit=$?"
  pkill -f "python3 server.py" 2>/dev/null
  sleep 1
  pkill -f "sq host" 2>/dev/null
  sleep 1
}

echo "######## FIXED SUITE ########"
suite ~/ravel-v2/work
tail -5 ~/ravel-v2/work/harness-out.txt
grep -c "^  ok" ~/ravel-v2/work/harness-out.txt
grep "^FAIL" ~/ravel-v2/work/harness-out.txt || echo "no FAIL lines (fixed)"

echo "######## BITE TEST (v1 backend, fixed harness) ########"
suite ~/ravel-v2/bite
tail -5 ~/ravel-v2/bite/harness-out.txt
grep -c "^  ok" ~/ravel-v2/bite/harness-out.txt
grep "^FAIL" ~/ravel-v2/bite/harness-out.txt || echo "no FAIL lines (bite)"

echo "######## WAVELETS (fixed backend) ########"
cd ~/ravel-v2/work/verify && PHEXT_SPARQL_BIN=$EV python3 wavelets_test.py 2>&1 | tail -4
echo VERIFY-DONE
