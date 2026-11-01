#!/usr/bin/env bash
# Update published chainspec + bootnodes on VPS without genesis purge.
set -euo pipefail
DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"

cp -a "$DIR/config/chainspec-raw.json" "$DIR/backups/chainspec-raw.json.$(date +%Y%m%d%H%M)" 2>/dev/null || true
cp network/tks-network-spec-raw.json "$DIR/config/chainspec-raw.json"
cp network/tks-network-spec-raw.sha256 "$DIR/config/" 2>/dev/null || true
cp network/bootnodes.public.txt "$DIR/config/bootnodes.public.txt"

grep -v '^#' "$DIR/config/bootnodes.public.txt" | grep -v '^[[:space:]]*$' > "$DIR/bootnodes.txt"

echo "Restarting stack (no purge)..."
cd "$DIR"
docker compose restart

echo "Verify bootNodes in spec:"
python3 -c "import json; print(json.load(open('config/chainspec-raw.json'))['bootNodes'])"
