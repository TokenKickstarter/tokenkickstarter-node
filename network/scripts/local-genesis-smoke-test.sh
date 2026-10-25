#!/usr/bin/env bash
# Throwaway 2-validator + fullnode smoke test for production genesis (no Docker required).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NODE="$ROOT/target/release/tks-chain-node"
RAW="$ROOT/network/tks-network-spec-raw.json"
KEYS="$ROOT/network/production-keys"
BASE="/tmp/tks-genesis-smoke-$$"
RPC_V1="http://127.0.0.1:9945"
RPC_V2="http://127.0.0.1:9946"
RPC_FN="http://127.0.0.1:9947"

cleanup() {
  jobs -p | xargs -r kill 2>/dev/null || true
  sleep 2
}
trap cleanup EXIT

if [[ ! -x "$NODE" || ! -f "$RAW" || ! -d "$KEYS" ]]; then
  echo "Run: cargo build --release -p tks-chain-node && ./network/scripts/bootstrap-production-genesis.sh"
  exit 1
fi

inject_keys() {
  local rpc="$1" aura="$2" gran="$3"
  local aura_seed aura_pub gran_seed gran_pub
  aura_seed=$(python3 -c "import json; print(json.load(open('$aura'))['secretSeed'])")
  aura_pub=$(python3 -c "import json; print(json.load(open('$aura'))['publicKey'])")
  gran_seed=$(python3 -c "import json; print(json.load(open('$gran'))['secretSeed'])")
  gran_pub=$(python3 -c "import json; print(json.load(open('$gran'))['publicKey'])")
  curl -s "$rpc" -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"author_insertKey\",\"params\":[\"aura\",\"$aura_seed\",\"$aura_pub\"]}" >/dev/null
  curl -s "$rpc" -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"author_insertKey\",\"params\":[\"gran\",\"$gran_seed\",\"$gran_pub\"]}" >/dev/null
}

rpc() {
  curl -s "$1" -H 'Content-Type: application/json' -d "$2"
}

mkdir -p "$BASE"/{v1,v2,fn}

mkdir -p "$BASE/v1" "$BASE/v2" "$BASE/fn"
PEER1=$("$NODE" key generate-node-key --file "$BASE/v1/node-key" 2>&1 | grep '^12D3' | head -1)
"$NODE" key generate-node-key --file "$BASE/v2/node-key" >/dev/null 2>&1
"$NODE" key generate-node-key --file "$BASE/fn/node-key" >/dev/null 2>&1
BOOT="/ip4/127.0.0.1/tcp/30334/p2p/${PEER1}"

CIPHER_OFF=(--disable-cipher-stack)

echo ">> Starting validators + fullnode..."
"$NODE" --chain "$RAW" --base-path "$BASE/v1" --validator --node-key-file "$BASE/v1/node-key" --rpc-port 9945 \
  --rpc-external --rpc-cors all --rpc-methods Unsafe --port 30334 --prometheus-port 9615 --name smoke-v1 \
  "${CIPHER_OFF[@]}" &
sleep 3
inject_keys "$RPC_V1" "$KEYS/validator1-aura.json" "$KEYS/validator1-grandpa.json"

"$NODE" --chain "$RAW" --base-path "$BASE/v2" --validator --node-key-file "$BASE/v2/node-key" --rpc-port 9946 \
  --rpc-external --rpc-cors all --rpc-methods Unsafe --port 30335 --prometheus-port 9616 --name smoke-v2 \
  --bootnodes "$BOOT" "${CIPHER_OFF[@]}" &
sleep 3
inject_keys "$RPC_V2" "$KEYS/validator2-aura.json" "$KEYS/validator2-grandpa.json"

"$NODE" --chain "$RAW" --base-path "$BASE/fn" --node-key-file "$BASE/fn/node-key" --rpc-port 9947 --rpc-external --rpc-cors all \
  --rpc-methods Safe --port 30336 --prometheus-port 9617 --name smoke-fullnode --bootnodes "$BOOT" \
  --enable-cipher-relay --cipher-relay-port=4002 \
  --enable-cipher-shard-executor --cipher-shard-port=4010 --cipher-shard-id=0 \
  --cipher-auto-post-anchor --cipher-anchor-seed-file="$KEYS/cipher-anchor-seed.txt" &

echo ">> Waiting for blocks..."
for _ in $(seq 1 60); do
  h=$(rpc "$RPC_FN" '{"jsonrpc":"2.0","id":1,"method":"chain_getHeader","params":[]}' | python3 -c "import json,sys; print(json.load(sys.stdin).get('result',{}).get('number','0x0'))")
  if [[ "$h" != "0x0" && "$h" != "0x1" ]]; then break; fi
  sleep 2
done

chain=$(rpc "$RPC_FN" '{"jsonrpc":"2.0","id":1,"method":"system_chain","params":[]}')
spec=$(rpc "$RPC_FN" '{"jsonrpc":"2.0","id":1,"method":"state_getRuntimeVersion","params":[]}')
swarm=$(rpc "$RPC_FN" '{"jsonrpc":"2.0","id":1,"method":"tks_listSwarmValidators","params":[]}')
final=$(rpc "$RPC_FN" '{"jsonrpc":"2.0","id":1,"method":"chain_getFinalizedHead","params":[]}')
genesis=$(rpc "$RPC_FN" '{"jsonrpc":"2.0","id":1,"method":"chain_getBlockHash","params":[0]}')
alice_bal=$(rpc "$RPC_FN" '{"jsonrpc":"2.0","id":1,"method":"eth_getBalance","params":["0xe04cc55ebee1cbce552f250e85c57b70b2e2625b","latest"]}')
bob_bal=$(rpc "$RPC_FN" '{"jsonrpc":"2.0","id":1,"method":"eth_getBalance","params":["0x25451a4de12dccc2d166922fa938e900fcc4ed24","latest"]}')

echo "system_chain: $chain"
echo "runtime: $spec"
echo "swarm: $swarm"
echo "finalized: $final"
echo "genesis: $genesis"
echo "alice balance: $alice_bal"
echo "bob balance: $bob_bal"
python3 - <<'PY' "$alice_bal" "$bob_bal"
import json, sys
for raw in sys.argv[1:]:
    v = json.loads(raw)["result"]
    assert v in ("0x0", "0x0000000000000000000000000000000000000000000000000000000000000000"), raw
print("OK: legacy Alice/Bob balances empty")
PY

python3 - <<'PY' "$chain" "$spec" "$swarm"
import json, sys
chain, spec, swarm = map(json.loads, sys.argv[1:4])
assert "TKS Network" in chain.get("result",""), chain
assert spec["result"]["specVersion"] == 411, spec
assert len(swarm.get("result",[])) == 2, swarm
print("OK: chain name, spec 411, 2 swarm validators")
PY

echo ">> Multisig sudo force_rotate..."
"$NODE" production multisig-sudo-force-rotate \
  --rpc-url "$RPC_V1" \
  --member1-seed-file "$KEYS/sudo-member1.json" \
  --member2-seed-file "$KEYS/sudo-member2.json" \
  --member3-seed-file "$KEYS/sudo-member3.json"

echo ">> Relay /version + store..."
curl -sSf "http://127.0.0.1:4002/version" | python3 -m json.tool
curl -sSf "http://127.0.0.1:4010/version" | python3 -m json.tool

now=$(date +%s)
curl -sS -X POST "http://127.0.0.1:4002/store" -H 'Content-Type: application/json' \
  -d "{\"recipient_id\":\"0x023349888b9cc8c07e1e667cc156626120be4e6f\",\"message_id\":\"smoke-$now\",\"ciphertext\":[1,2,3,4],\"stored_at\":$now,\"expires_at\":$((now+3600))}"
echo

echo ">> Waiting for anchor .posted markers..."
for _ in $(seq 1 30); do
  n=$(find "$BASE/fn/cipher/anchors" -name '*.posted' 2>/dev/null | wc -l | tr -d ' ')
  if [[ "$n" -gt 0 ]]; then echo "anchors posted: $n"; exit 0; fi
  sleep 2
done
echo "WARN: no .posted anchors yet (may need relay traffic)"
exit 0
