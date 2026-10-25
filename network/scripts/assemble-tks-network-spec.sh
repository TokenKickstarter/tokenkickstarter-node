#!/usr/bin/env bash
# Build network/tks-network-spec.json from network/production-keys/ + input JSON.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NODE="$ROOT/target/release/tks-chain-node"
KEYS="$ROOT/network/production-keys"
INPUT="${TKS_GENESIS_INPUT:-$ROOT/network/production-genesis-input.json}"
OUT="$ROOT/network/tks-network-spec.json"
TEMPLATE="$ROOT/network/tks-network-spec.template.json"

if [[ ! -x "$NODE" ]]; then
  echo "Build release node first: cargo build --release -p tks-chain-node"
  exit 1
fi
if [[ ! -d "$KEYS" ]]; then
  echo "Missing $KEYS — run bootstrap-production-genesis.sh first"
  exit 1
fi
if [[ ! -f "$INPUT" ]]; then
  echo "Missing $INPUT — copy production-genesis-input.example.json"
  exit 1
fi

python3 "$ROOT/network/scripts/assemble_tks_network_spec.py" \
  --node "$NODE" \
  --keys-dir "$KEYS" \
  --input "$INPUT" \
  --template "$TEMPLATE" \
  --output "$OUT"

echo "Wrote $OUT (review before publishing; never commit production-keys/)"
