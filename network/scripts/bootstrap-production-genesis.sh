#!/usr/bin/env bash
# Generate production keys + chainspec (no Alice/Bob). Run from repo root.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="$ROOT/network/production-keys"
NODE="$ROOT/target/release/tks-chain-node"
INPUT_EXAMPLE="$ROOT/network/production-genesis-input.example.json"
INPUT="$ROOT/network/production-genesis-input.json"

mkdir -p "$OUT_DIR"
chmod 700 "$OUT_DIR"

if [[ ! -x "$NODE" ]]; then
  echo "Build the node first: cargo build --release -p tks-chain-node"
  exit 1
fi

if [[ ! -f "$INPUT" ]]; then
  cp "$INPUT_EXAMPLE" "$INPUT"
  echo "Created $INPUT — edit allocations/endpoints, then re-run this script."
fi

MEMBERS="$(python3 -c "import json; print(json.load(open('$INPUT'))['sudo_multisig_members'])")"

echo ">> Validator 1 (Aura + GRANDPA + ECDSA stash/cipher)..."
echo "$("$NODE" key generate --scheme Sr25519 --output-type json)" > "$OUT_DIR/validator1-aura.json"
echo "$("$NODE" key generate --scheme Ed25519 --output-type json)" > "$OUT_DIR/validator1-grandpa.json"
echo "$("$NODE" key generate --scheme Ecdsa --output-type json)" > "$OUT_DIR/validator1-ecdsa.json"

echo ">> Validator 2..."
echo "$("$NODE" key generate --scheme Sr25519 --output-type json)" > "$OUT_DIR/validator2-aura.json"
echo "$("$NODE" key generate --scheme Ed25519 --output-type json)" > "$OUT_DIR/validator2-grandpa.json"
echo "$("$NODE" key generate --scheme Ecdsa --output-type json)" > "$OUT_DIR/validator2-ecdsa.json"

V1_SEED="$(python3 -c "import json; print(json.load(open('$OUT_DIR/validator1-ecdsa.json'))['secretPhrase'])")"
printf '%s\n' "$V1_SEED" > "$OUT_DIR/cipher-anchor-seed.txt"
chmod 600 "$OUT_DIR/cipher-anchor-seed.txt"

echo ">> Sudo multisig members ($MEMBERS)..."
for i in $(seq 1 "$MEMBERS"); do
  echo "$("$NODE" key generate --scheme Ecdsa --output-type json)" > "$OUT_DIR/sudo-member${i}.json"
done

echo ">> Treasury / allocation keys..."
for label in team bridge ecosystem treasury validators public_sale airdrop; do
  echo "$("$NODE" key generate --scheme Ecdsa --output-type json)" > "$OUT_DIR/treasury-${label}.json"
done

echo ">> Assembling tks-network-spec.json..."
chmod +x "$ROOT/network/scripts/assemble-tks-network-spec.sh"
"$ROOT/network/scripts/assemble-tks-network-spec.sh"

echo ">> Building raw chainspec..."
chmod +x "$ROOT/network/scripts/build-tks-network-spec.sh"
"$ROOT/network/scripts/build-tks-network-spec.sh"

cat > "$OUT_DIR/README.txt" <<'EOF'
Production keys — NEVER commit this directory.

Artifacts:
  cipher-anchor-seed.txt     — validator1 HyperSwarm / post_anchor signer
  genesis-summary.json       — public addresses (safe to copy for ops runbooks)
  ../tks-network-spec.json   — human-readable Live spec (TKS Network)
  ../tks-network-spec-raw.json — deploy with docker --chain /path/to/raw

Sudo: genesis uses pallet-multisig account (see genesis-summary.json).
Dispatch sudo via Multisig.as_multi wrapping Sudo.sudo(call).

Wipe old chain data on every validator/fullnode before starting new genesis.
EOF

echo ">> Done. Review network/production-keys/genesis-summary.json and raw spec hash before VPS deploy."
