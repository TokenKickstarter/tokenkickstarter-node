#!/usr/bin/env bash
# Install Aura + GRANDPA session keys into a validator keystore BEFORE starting the node.
#
# GRANDPA requires keystore files written by `tks-chain-node key insert` (URI/mnemonic).
# Do NOT use author_insertKey with JSON `secretSeed` — that stores 0x… hex and finality stays at genesis.
#
# Usage (from repo root):
#   ./network/scripts/install-production-validator-keys.sh /data 1
#   ./network/scripts/install-production-validator-keys.sh /data 2
#
# Docker example:
#   docker compose -f network/docker-compose-live.yml run --rm --no-deps \
#     -v tks-docker_tks-data-validator1:/data \
#     -v "$PWD/network/production-keys:/keys:ro" \
#     tks-validator1 /app/install-validator-keys.sh /data 1 /keys
set -euo pipefail

BASE_PATH="${1:?base-path (e.g. /data)}"
VALIDATOR_NUM="${2:?validator number 1 or 2}"
KEYS_DIR="${3:-$(cd "$(dirname "$0")/../production-keys" && pwd)}"
CHAIN="${4:-/chainspec.json}"

NODE="${TKS_NODE_BIN:-./target/release/tks-chain-node}"
if [[ ! -x "$NODE" ]]; then
  NODE="/app/tks-chain-node"
fi
if [[ ! -x "$NODE" ]]; then
  echo "tks-chain-node binary not found (set TKS_NODE_BIN or build release)" >&2
  exit 1
fi

AURA_JSON="$KEYS_DIR/validator${VALIDATOR_NUM}-aura.json"
GRAN_JSON="$KEYS_DIR/validator${VALIDATOR_NUM}-grandpa.json"
for f in "$AURA_JSON" "$GRAN_JSON"; do
  if [[ ! -f "$f" ]]; then
    echo "missing key file: $f" >&2
    exit 1
  fi
done

aura_phrase=$(python3 -c "import json; print(json.load(open('$AURA_JSON'))['secretPhrase'])")
gran_phrase=$(python3 -c "import json; print(json.load(open('$GRAN_JSON'))['secretPhrase'])")

echo ">> Installing session keys for validator ${VALIDATOR_NUM} into ${BASE_PATH}"
"$NODE" key insert --base-path "$BASE_PATH" --chain "$CHAIN" \
  --key-type aura --scheme Sr25519 --suri "$aura_phrase"
"$NODE" key insert --base-path "$BASE_PATH" --chain "$CHAIN" \
  --key-type gran --scheme Ed25519 --suri "$gran_phrase"

echo "OK: aura + gran installed (use mnemonic form). Restart the validator if it is already running."
