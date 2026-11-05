#!/usr/bin/env bash
# Live mainnet: 2-of-3 multisig → sudo.sudo(system.set_code). Requires built WASM on disk.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NODE="${TKS_NODE:-$ROOT/target/release/tks-chain-node}"
RPC="${TKS_RPC:-http://127.0.0.1:9944}"
KEYS="${TKS_KEYS_DIR:-/opt/tks-docker/production-keys}"
WASM="${TKS_WASM:-$ROOT/target/release/wbuild/tks-runtime/tks_runtime.compact.compressed.wasm}"

if [[ ! -f "$WASM" ]]; then
  echo "Missing WASM: $WASM (set TKS_WASM or build node first)" >&2
  exit 1
fi

"$NODE" production multisig-sudo-set-code \
  --rpc-url "$RPC" \
  --wasm-file "$WASM" \
  --threshold 2 \
  --member1-seed-file "$KEYS/sudo-member1.json" \
  --member2-seed-file "$KEYS/sudo-member2.json" \
  --member3-seed-file "$KEYS/sudo-member3.json"
