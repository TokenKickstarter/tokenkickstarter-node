#!/usr/bin/env bash
# Live mainnet: 2-of-3 multisig → sudo.sudo(force_rotate). Prints extrinsic hashes.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NODE="${TKS_NODE:-$ROOT/target/release/tks-chain-node}"
RPC="${TKS_RPC:-http://127.0.0.1:9944}"
KEYS="${TKS_KEYS_DIR:-/opt/tks-docker/production-keys}"

"$NODE" production multisig-sudo-force-rotate \
  --rpc-url "$RPC" \
  --threshold 2 \
  --member1-seed-file "$KEYS/sudo-member1.json" \
  --member2-seed-file "$KEYS/sudo-member2.json" \
  --member3-seed-file "$KEYS/sudo-member3.json"
