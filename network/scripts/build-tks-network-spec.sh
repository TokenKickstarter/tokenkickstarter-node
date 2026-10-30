#!/usr/bin/env bash
# Build raw production chainspec from network/tks-network-spec.json
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NODE="${TKS_NODE:-$ROOT/target/release/tks-chain-node}"
SPEC="$ROOT/network/tks-network-spec.json"
RAW="$ROOT/network/tks-network-spec-raw.json"

if [[ ! -f "$SPEC" ]]; then
  echo "Missing $SPEC — copy network/tks-network-spec.template.json and fill validator accounts."
  exit 1
fi

"$NODE" build-spec --chain "$SPEC" --raw > "$RAW"
echo "Wrote $RAW"
sha256sum "$RAW" || shasum -a 256 "$RAW"
