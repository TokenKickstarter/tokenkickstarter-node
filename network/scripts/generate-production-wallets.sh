#!/usr/bin/env bash
# Generate ECDSA sudo members, validator stash/session keys, treasury buckets, and a public summary.
# Does NOT commit secrets. Default output: network/production-keys/ (override with PRODUCTION_KEYS_DIR).
#
# Live mainnet (spec 412): sudo signers are baked into genesis — new keys require a new raw spec + chain reset.
# To only fund / use EXISTING genesis signers, do NOT re-run this; use fund-sudo-signers.sh + multisig-sudo-force-rotate.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="${PRODUCTION_KEYS_DIR:-$ROOT/network/production-keys}"
NODE="${TKS_NODE:-$ROOT/target/release/tks-chain-node}"
INPUT="${PRODUCTION_GENESIS_INPUT:-$ROOT/network/production-genesis-input.json}"
INPUT_EXAMPLE="$ROOT/network/production-genesis-input.example.json"

if [[ ! -x "$NODE" ]]; then
  echo "Set TKS_NODE to a built or release tks-chain-node binary (missing: $NODE)" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
chmod 700 "$OUT_DIR"

if [[ ! -f "$INPUT" ]]; then
  cp "$INPUT_EXAMPLE" "$INPUT"
  echo "Created $INPUT — edit allocations/endpoints if needed."
fi

MEMBERS="$(python3 -c "import json; print(json.load(open('$INPUT'))['sudo_multisig_members'])")"
THRESHOLD="$(python3 -c "import json; print(json.load(open('$INPUT'))['sudo_multisig_threshold'])")"

h160() {
  "$NODE" production account-id20 "$1" 2>/dev/null | tail -1
}

echo ">> Validator 1 (Aura + GRANDPA + ECDSA stash)..."
"$NODE" key generate --scheme Sr25519 --output-type json > "$OUT_DIR/validator1-aura.json"
"$NODE" key generate --scheme Ed25519 --output-type json > "$OUT_DIR/validator1-grandpa.json"
"$NODE" key generate --scheme Ecdsa --output-type json > "$OUT_DIR/validator1-ecdsa.json"

echo ">> Validator 2..."
"$NODE" key generate --scheme Sr25519 --output-type json > "$OUT_DIR/validator2-aura.json"
"$NODE" key generate --scheme Ed25519 --output-type json > "$OUT_DIR/validator2-grandpa.json"
"$NODE" key generate --scheme Ecdsa --output-type json > "$OUT_DIR/validator2-ecdsa.json"

V1_SEED="$(python3 -c "import json; print(json.load(open('$OUT_DIR/validator1-ecdsa.json'))['secretPhrase'])")"
printf '%s\n' "$V1_SEED" > "$OUT_DIR/cipher-anchor-seed.txt"
chmod 600 "$OUT_DIR/cipher-anchor-seed.txt"

echo ">> Sudo multisig members ($MEMBERS, threshold $THRESHOLD)..."
for i in $(seq 1 "$MEMBERS"); do
  "$NODE" key generate --scheme Ecdsa --output-type json > "$OUT_DIR/sudo-member${i}.json"
done

echo ">> Treasury / allocation keys..."
for label in team bridge ecosystem treasury validators public_sale airdrop; do
  "$NODE" key generate --scheme Ecdsa --output-type json > "$OUT_DIR/treasury-${label}.json"
done

V1_STASH="$(h160 "$(python3 -c "import json; print(json.load(open('$OUT_DIR/validator1-ecdsa.json'))['secretPhrase'])")")"
V2_STASH="$(h160 "$(python3 -c "import json; print(json.load(open('$OUT_DIR/validator2-ecdsa.json'))['secretPhrase'])")")"

SUDO_MEMBERS=()
for i in $(seq 1 "$MEMBERS"); do
  phrase="$(python3 -c "import json; print(json.load(open('$OUT_DIR/sudo-member$i.json'))['secretPhrase'])")"
  SUDO_MEMBERS+=("$(h160 "$phrase")")
done
SIGNERS_CSV="$(IFS=,; echo "${SUDO_MEMBERS[*]}")"
export SUDO_MEMBERS_JSON="$(python3 -c "import json,sys; print(json.dumps(sys.argv[1:]))" "${SUDO_MEMBERS[@]}")"

MULTISIG_OUT="$("$NODE" production multisig-account --threshold "$THRESHOLD" --signatories "$SIGNERS_CSV" 2>&1)"
PALLET_SUDO="$(printf '%s\n' "$MULTISIG_OUT" | grep -E '^0x[0-9a-fA-F]{40}$' | tail -1)"
LEGACY_LINE="$(printf '%s\n' "$MULTISIG_OUT" | grep -i 'legacy genesis helper' || true)"

SUMMARY="$OUT_DIR/genesis-summary.json"
export SUMMARY THRESHOLD PALLET_SUDO V1_STASH V2_STASH SUDO_MEMBERS_JSON
python3 <<'PY'
import json, os
members = os.environ["SUDO_MEMBERS_JSON"]
summary = {
  "spec_version_note": "Regenerate tks-network-spec-raw.json with assemble + build-tks-network-spec after editing production-genesis-input.json",
  "sudo_threshold": int(os.environ["THRESHOLD"]),
  "sudo_members": json.loads(members),
  "genesis_sudo_multisig_pallet": os.environ["PALLET_SUDO"],
  "validator_stash": [os.environ["V1_STASH"], os.environ["V2_STASH"]],
  "cipher_accounts": [os.environ["V1_STASH"], os.environ["V2_STASH"]],
  "shard_validators_at_genesis": 2,
}
path = os.environ["SUMMARY"]
json.dump(summary, open(path, "w"), indent=2)
print("Wrote", path)
PY

echo ""
echo "=== Public addresses (secrets stay in $OUT_DIR) ==="
echo "Validator 1 stash / cipher: $V1_STASH"
echo "Validator 2 stash / cipher: $V2_STASH"
echo "Sudo signers (${THRESHOLD}-of-${MEMBERS}):"
for i in "${!SUDO_MEMBERS[@]}"; do echo "  member$((i+1)): ${SUDO_MEMBERS[$i]}"; done
echo "Multisig sudo (pallet, new genesis): $PALLET_SUDO"
[[ -n "$LEGACY_LINE" ]] && echo "$LEGACY_LINE"
echo ""
echo "Next: ./network/scripts/assemble-tks-network-spec.sh && ./network/scripts/build-tks-network-spec.sh"
echo "      (uses TKS_NODE=$NODE and keys in $OUT_DIR)"
echo "Deploying new sudo/validators on LIVE chain without regen is NOT possible — genesis must match these files."
