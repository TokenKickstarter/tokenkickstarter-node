#!/usr/bin/env bash
# Verify published sudo multisig matches node derivation (no secrets).
set -euo pipefail
NODE="${TKS_NODE:-tks-chain-node}"
RPC="${TKS_RPC:-https://rpc.tokenkickstarter.com}"
SUDO="${TKS_SUDO:-0xd0ecdb4dd41fbe6863f4ca6c68874ae61198d489}"
THRESHOLD="${TKS_SUDO_THRESHOLD:-2}"
MEMBERS="${TKS_SUDO_MEMBERS:-0xd876889fb236ebacdcf00e5007666d2094a5ada5,0x5a3173a1d18049f9b19bdd15de9b28b671e0dc29,0xaedfec3469ffe1fee0e0a54679270e183026b66e}"

derived="$("$NODE" production multisig-account --threshold "$THRESHOLD" --signatories "$MEMBERS")"
derived="${derived##*$'\n'}"
derived="${derived// /}"

echo "Derived multisig: $derived"
echo "Expected sudo:    $SUDO"
derived_lc="$(printf '%s' "$derived" | tr '[:upper:]' '[:lower:]')"
sudo_lc="$(printf '%s' "$SUDO" | tr '[:upper:]' '[:lower:]')"
[[ "$derived_lc" == "$sudo_lc" ]] || { echo "FAIL multisig derivation mismatch"; exit 1; }

bal="$(curl -s "$RPC" -H 'Content-Type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getBalance\",\"params\":[\"$SUDO\",\"latest\"]}" \
  | jq -r .result)"
echo "On-chain sudo balance (wei): $bal"
echo "PASS sudo address matches 2-of-3 derivation"
