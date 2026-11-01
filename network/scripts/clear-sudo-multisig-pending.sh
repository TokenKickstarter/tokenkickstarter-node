#!/usr/bin/env bash
# Cancel all pending pallet-multisig calls for regen sudo (operator; needs one member seed on VPS).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NODE="${TKS_NODE:-$ROOT/target/release/tks-chain-node}"
RPC="${TKS_RPC:-http://127.0.0.1:9944}"
KEYS="${TKS_KEYS_DIR:-/opt/tks-docker/production-keys}"
MULTISIG="${TKS_SUDO:-0x8eeb8be59ccb1c65211ab787e6ba174753a68f1a}"
MEMBERS="0x5b5478f3929645a96b17712a3b1743eded5d4ad7,0x00373382c01459df2801f1f510926bc6acf9ddf7,0x635ed8d610027d2df9398867e3da48db54d12f49"

echo ">> Pending before:"
"$NODE" production multisig-list-pending --rpc-url "$RPC" --multisig "$MULTISIG" || true

"$NODE" production multisig-cancel-pending \
  --rpc-url "$RPC" \
  --multisig "$MULTISIG" \
  --member-seed-file "$KEYS/sudo-member1.json" \
  --threshold 2 \
  --signatories "$MEMBERS"

echo ">> Pending after:"
"$NODE" production multisig-list-pending --rpc-url "$RPC" --multisig "$MULTISIG" || true
