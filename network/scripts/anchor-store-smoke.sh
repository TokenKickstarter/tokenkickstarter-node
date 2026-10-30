#!/usr/bin/env bash
# Relay store → wait for auto post_anchor (run on VPS or with TKS_RELAY=http://127.0.0.1:4002).
set -euo pipefail
RELAY="${TKS_RELAY:-http://127.0.0.1:4002}"
RPC="${TKS_RPC:-http://127.0.0.1:9944}"
CONTAINER="${TKS_FULLNODE_CONTAINER:-tks-fullnode}"
WAIT_SECS="${ANCHOR_WAIT_SECS:-45}"

now=$(date +%s)
msg_id="anchor-smoke-$now"
before=$(docker exec "$CONTAINER" sh -c 'ls /data/cipher/anchors/.posted 2>/dev/null | wc -l' 2>/dev/null | tr -d ' ' || echo 0)

echo ">> POST $RELAY/store"
curl -sS -X POST "$RELAY/store" -H 'Content-Type: application/json' \
  -d "{\"recipient_id\":\"0x023349888b9cc8c07e1e667cc156626120be4e6f\",\"message_id\":\"$msg_id\",\"ciphertext\":[1,2,3,4],\"stored_at\":$now,\"expires_at\":$((now+3600))}"
echo

echo ">> Waiting up to ${WAIT_SECS}s for cipher-anchor submitter..."
deadline=$((SECONDS + WAIT_SECS))
tx=""
while (( SECONDS < deadline )); do
  line=$(docker logs "$CONTAINER" 2>&1 | grep 'Submitted post_anchor' | tail -1 || true)
  if [[ -n "$line" ]]; then
    tx=$(echo "$line" | sed -n 's/.*(tx \([^)]*\)).*/\1/p')
    break
  fi
  sleep 3
done

after=$(docker exec "$CONTAINER" sh -c 'ls /data/cipher/anchors/.posted 2>/dev/null | wc -l' 2>/dev/null | tr -d ' ' || echo 0)
echo "posted markers: before=$before after=$after"
[[ -n "$tx" ]] && echo "anchor extrinsic: $tx"

if command -v curl >/dev/null; then
  r=$(curl -sS "$RPC" -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"tks_anchorStats","params":[]}' 2>/dev/null || true)
  if echo "$r" | grep -q total_anchors; then
    echo "tks_anchorStats: $r"
  else
    echo "(tks_anchorStats RPC available after node upgrade with anchor stats RPC)"
  fi
fi

[[ -n "$tx" ]] || { echo "FAIL no Submitted post_anchor in logs"; exit 1; }
echo "PASS anchor pipeline"
