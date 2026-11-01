#!/usr/bin/env bash
# Run 20k transfer load test N times + assert per-block gas limits. Requires funded --funder-seed.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
RPC="${TKS_RPC:-http://127.0.0.1:9944}"
RUNS="${R6_RUNS:-3}"
FUNDER="${R6_FUNDER_SEED:?set R6_FUNDER_SEED (offline treasury URI)}"
NODE="${TKS_NODE:-$ROOT/target/release/tks-chain-node}"
OUT="${R6_EVIDENCE_DIR:-$ROOT/network/r6-evidence-$(date +%Y%m%d%H%M)}"
mkdir -p "$OUT"

start_hex="$(curl -sS "$RPC" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"])')"
start=$((16#${start_hex#0x}))

for i in $(seq 1 "$RUNS"); do
  echo "== Run $i / $RUNS =="
  log="$OUT/run${i}.log"
  python3 "$ROOT/network/scripts/loadtest-20k-transfers.py" \
    --rpc "$RPC" --node-bin "$NODE" --funder-seed "$FUNDER" \
    --senders 10 --per-sender 2000 2>&1 | tee "$log"
  sleep 30
done

end_hex="$(curl -sS "$RPC" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["result"])')"
end=$((16#${end_hex#0x}))

python3 "$ROOT/network/scripts/assert-block-gas-limits.py" \
  --rpc "$RPC" --from-block "$start" --to-block "$end" | tee "$OUT/gas-assert.txt"

echo "Evidence written to $OUT"
