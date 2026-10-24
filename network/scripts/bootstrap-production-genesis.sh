#!/usr/bin/env bash
# Generate production keys + chainspec (no Alice/Bob). Run from repo root.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="$ROOT/network/production-keys"
SPEC_JSON="$ROOT/network/tks-network-spec.json"
RAW_JSON="$ROOT/network/tks-network-spec-raw.json"
NODE="$ROOT/target/release/tks-chain-node"

mkdir -p "$OUT_DIR"
chmod 700 "$OUT_DIR"

if [[ ! -x "$NODE" ]]; then
  echo "Build the node first: cargo build --release -p tks-chain-node"
  exit 1
fi

echo ">> Generating validator 1 keys (Aura/GRANDPA + ECDSA anchor account)..."
V1_AURA=$("$NODE" key generate --scheme Sr25519 --output-type json)
V1_GRAN=$("$NODE" key generate --scheme Ed25519 --output-type json)
V1_ECDSA=$("$NODE" key generate --scheme Ecdsa --output-type json)

echo ">> Generating validator 2 keys..."
V2_AURA=$("$NODE" key generate --scheme Sr25519 --output-type json)
V2_GRAN=$("$NODE" key generate --scheme Ed25519 --output-type json)

echo "$V1_AURA" > "$OUT_DIR/validator1-aura.json"
echo "$V1_GRAN" > "$OUT_DIR/validator1-grandpa.json"
echo "$V1_ECDSA" > "$OUT_DIR/validator1-ecdsa.json"
echo "$V2_AURA" > "$OUT_DIR/validator2-aura.json"
echo "$V2_GRAN" > "$OUT_DIR/validator2-grandpa.json"

V1_ECDSA_SEED=$(echo "$V1_ECDSA" | python3 -c "import json,sys; print(json.load(sys.stdin)['secretSeed'])")
echo "$V1_ECDSA_SEED" > "$OUT_DIR/cipher-anchor-seed.txt"
chmod 600 "$OUT_DIR/cipher-anchor-seed.txt"

cp "$ROOT/network/tks-network-spec.template.json" "$OUT_DIR/tks-network-spec.template.json"

cat > "$OUT_DIR/README.txt" <<EOF
Production keys — NEVER commit this directory.

Next steps:
1. Edit network/production-genesis-input.json (sudo, treasury, validator stashes).
2. Run: ./network/scripts/build-tks-network-spec.sh
3. Deploy with --chain tks-network-spec-raw.json (ChainType Live, name TKS Network).
4. Mount cipher-anchor-seed.txt: --cipher-anchor-seed-file=/path/cipher-anchor-seed.txt
5. Inject Aura/GRANDPA seeds via local RPC only; set session keys on-chain.
6. Runtime upgrade to spec 410+ if migrating an existing chain.
EOF

echo ">> Keys written under $OUT_DIR"
echo ">> Complete allocation in production-genesis-input.json then run build-tks-network-spec.sh"
