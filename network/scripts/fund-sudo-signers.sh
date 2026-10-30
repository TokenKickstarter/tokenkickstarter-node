#!/usr/bin/env bash
# Fund sudo multisig signers with native TKS (Substrate extrinsic via node CLI).
set -euo pipefail
NODE="${TKS_NODE:-tks-chain-node}"
RPC="${TKS_RPC:-http://127.0.0.1:9944}"
FUND_SEED_FILE="${FUND_SEED_FILE:?set FUND_SEED_FILE}"
AMOUNT_TKS="${AMOUNT_TKS:-2}"
SUMMARY="${TKS_GENESIS_SUMMARY:-}"

if [[ -n "$SUMMARY" && -f "$SUMMARY" ]]; then
  mapfile -t SIGNERS < <(python3 -c "import json; print('\n'.join(json.load(open('$SUMMARY'))['sudo_members']))")
else
  SIGNERS=(
    0x5b5478f3929645a96b17712a3b1743eded5d4ad7
    0x00373382c01459df2801f1f510926bc6acf9ddf7
    0x635ed8d610027d2df9398867e3da48db54d12f49
  )
fi

for to in "${SIGNERS[@]}"; do
  "$NODE" production fund-h160 \
    --rpc-url "$RPC" \
    --seed-file "$FUND_SEED_FILE" \
    --to "$to" \
    --amount-tks "$AMOUNT_TKS"
done
