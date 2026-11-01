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

# Substrate `build-spec` may inject dev bootnodes (127.0.0.1); production uses human spec only.
HUMAN_BOOT=$(python3 -c "import json; print(json.dumps(json.load(open('$SPEC')).get('bootNodes', [])))")
python3 -c "
import json, sys
raw = json.load(open('$RAW'))
human = json.loads(sys.argv[1])
raw['bootNodes'] = human
json.dump(raw, open('$RAW', 'w'), indent=2)
" "$HUMAN_BOOT"

echo "Wrote $RAW (bootNodes=$(echo "$HUMAN_BOOT" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))') entries)"
sha256sum "$RAW" 2>/dev/null | tee "$RAW.sha256" || shasum -a 256 "$RAW" | tee "$RAW.sha256"
