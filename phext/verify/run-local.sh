#!/bin/bash
# Local harness run (this VM's sq is v0.6.0, the version phextio was
# probed against; the ranch node carries 0.5.6 with daemon-mode share).
B=$HOME/workspace/ravel-upgrade
pkill -f "python3 server.py" 2>/dev/null; sleep 1
rm -rf $B/verify/data1 $B/verify/data2
mkdir -p $B/verify/data1 $B/verify/data2
printf '[{"org": "7r1pl3f0rc3", "store": "spacegoat"}]' > $B/verify/data2/souls.json
export PHEXT_SPARQL_BIN=$B/bin/phext-sparql
cd $B/backend || exit 1
RAVEL_PHEXT_DATA=$B/verify/data2 RAVEL_PORT=17881 RAVEL_SQ_PORT=18992 nohup python3 server.py > $B/verify/server.log 2>&1 &
SRV=$!
sleep 4
cd $B/verify || exit 1
HARNESS_DATA1=$B/verify/data1 HARNESS_SQ1=18993 HARNESS_PORT=17881 python3 harness.py
EXIT=$?
kill -9 $SRV 2>/dev/null
pkill -f "python3 server.py" 2>/dev/null
sleep 1
pkill -f "sq host 18992" 2>/dev/null
pkill -f "sq host 18993" 2>/dev/null
exit $EXIT
