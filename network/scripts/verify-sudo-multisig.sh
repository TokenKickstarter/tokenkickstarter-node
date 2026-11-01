#!/usr/bin/env bash
# Verify sudo multisig story vs live chain (no secrets).
set -euo pipefail
NODE="${TKS_NODE:-tks-chain-node}"
RPC="${TKS_RPC:-https://rpc.tokenkickstarter.com}"
ON_CHAIN_SUDO="${TKS_SUDO:-0x8eeb8be59ccb1c65211ab787e6ba174753a68f1a}"
THRESHOLD="${TKS_SUDO_THRESHOLD:-2}"
MEMBERS="${TKS_SUDO_MEMBERS:-0x5b5478f3929645a96b17712a3b1743eded5d4ad7,0x00373382c01459df2801f1f510926bc6acf9ddf7,0x635ed8d610027d2df9398867e3da48db54d12f49}"

out="$("$NODE" production multisig-account --threshold "$THRESHOLD" --signatories "$MEMBERS" 2>&1)" || true
pallet_derived="$(printf '%s\n' "$out" | grep -E '^0x[0-9a-fA-F]{40}$' | tail -1 | tr '[:upper:]' '[:lower:]')"

echo "Pallet-derived multisig (CLI): ${pallet_derived:-unknown}"
echo "Expected on-chain sudo.key:    $(printf '%s' "$ON_CHAIN_SUDO" | tr '[:upper:]' '[:lower:]')"

bal="$(curl -s "$RPC" -H 'Content-Type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getBalance\",\"params\":[\"$ON_CHAIN_SUDO\",\"latest\"]}" \
  | python3 -c "import json,sys; print(json.load(sys.stdin).get('result',''))")"
echo "On-chain sudo balance (wei): $bal"

if [[ "${pallet_derived:-}" != "$(printf '%s' "$ON_CHAIN_SUDO" | tr '[:upper:]' '[:lower:]')" ]]; then
  echo "FAIL CLI derivation != expected sudo"
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PENDING_RPC="${TKS_PENDING_RPC:-$RPC}"
pending_out="$(python3 "$SCRIPT_DIR/list-multisig-pending.py" --rpc "$PENDING_RPC" --multisig "$ON_CHAIN_SUDO" 2>/dev/null || true)"
if printf '%s\n' "$pending_out" | grep -qE '^0x[0-9a-fA-F]{64}$'; then
  echo "WARN pending multisig calls exist — run clear-sudo-multisig-pending.sh on VPS"
  exit 1
fi
if [[ -z "$pending_out" ]] && [[ "$PENDING_RPC" == https://* ]]; then
  echo "SKIP pending check on public RPC (use TKS_PENDING_RPC=http://127.0.0.1:9944 on VPS)"
fi

echo "PASS sudo matches pallet derivation; no pending multisig"
exit 0
