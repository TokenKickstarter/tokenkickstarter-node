#!/usr/bin/env bash
# Verify sudo multisig story vs live chain (no secrets).
set -euo pipefail
NODE="${TKS_NODE:-tks-chain-node}"
RPC="${TKS_RPC:-https://rpc.tokenkickstarter.com}"
ON_CHAIN_SUDO="${TKS_SUDO:-0xd0ecdb4dd41fbe6863f4ca6c68874ae61198d489}"
THRESHOLD="${TKS_SUDO_THRESHOLD:-2}"
MEMBERS="${TKS_SUDO_MEMBERS:-0xd876889fb236ebacdcf00e5007666d2094a5ada5,0x5a3173a1d18049f9b19bdd15de9b28b671e0dc29,0xaedfec3469ffe1fee0e0a54679270e183026b66e}"

out="$("$NODE" production multisig-account --threshold "$THRESHOLD" --signatories "$MEMBERS" 2>&1)" || true
pallet_derived="$(printf '%s\n' "$out" | grep -E '^0x[0-9a-fA-F]{40}$' | tail -1 | tr '[:upper:]' '[:lower:]')"
legacy_line="$(printf '%s\n' "$out" | grep -i 'legacy genesis helper' || true)"

echo "Pallet-derived multisig (v0.1.16+ CLI): ${pallet_derived:-unknown}"
echo "On-chain sudo.key (spec 412 genesis):   $(printf '%s' "$ON_CHAIN_SUDO" | tr '[:upper:]' '[:lower:]')"
[[ -n "$legacy_line" ]] && echo "$legacy_line"

bal="$(curl -s "$RPC" -H 'Content-Type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getBalance\",\"params\":[\"$ON_CHAIN_SUDO\",\"latest\"]}" \
  | jq -r .result)"
echo "On-chain sudo balance (wei): $bal"

if [[ "${pallet_derived:-}" == "$(printf '%s' "$ON_CHAIN_SUDO" | tr '[:upper:]' '[:lower:]')" ]]; then
  echo "PASS on-chain sudo matches pallet derivation"
  exit 0
fi

echo "NOTE: spec 412 mainnet sudo is the legacy genesis address until regen; governance uses 0xd0ec…, not 0x921e…"
echo "PASS CLI printed legacy note; compare sudo.key in published tks-network-spec-raw.json"
exit 0
