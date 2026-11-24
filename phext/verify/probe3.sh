#!/bin/bash
D=$HOME/workspace/ravel-upgrade/sq-probe3
rm -rf $D; mkdir -p $D; cd $D || exit 1
printf '\x01' > alpha.phext
echo "--- host only, no share"
nohup $HOME/.cargo/bin/sq host 18996 > host.log 2>&1 &
sleep 2
echo "toc alpha:"; curl -s "http://127.0.0.1:18996/api/v2/toc?p=alpha.phext"; echo "[end]"
echo "update:"; curl -s -X POST "http://127.0.0.1:18996/api/v2/update?c=1.1.1/1.1.1/1.1.1&p=alpha.phext" -d "hello"; echo
echo "select:"; curl -s "http://127.0.0.1:18996/api/v2/select?c=1.1.1/1.1.1/1.1.1&p=alpha.phext"; echo "[end]"
echo "toc alpha after:"; curl -s "http://127.0.0.1:18996/api/v2/toc?p=alpha.phext"; echo "[end]"
ls -la
kill %1 2>/dev/null; sleep 1
echo "--- now share as daemon"
nohup $HOME/.cargo/bin/sq share alpha.phext > share.log 2>&1 &
sleep 2
cat share.log
ss -ltnp 2>/dev/null | grep -i sq || ss -ltn | head -20
pkill -f "sq share" 2>/dev/null
exit 0
