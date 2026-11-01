#!/usr/bin/env bash
# VPS: bootnodes + chainspec patch + Docker rebuild v0.1.19 (no genesis purge).
set -euo pipefail
DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"
TAG="${TKS_IMAGE_TAG:-v0.1.19-audit-fix}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

echo ">> Sync chainspec + bootnodes"
install -d "$DIR/config" "$DIR/backups"
cp -a "$DIR/config/chainspec-raw.json" "$DIR/backups/chainspec-raw.json.$(date +%Y%m%d%H%M)" 2>/dev/null || true
cp "$ROOT/network/tks-network-spec-raw.json" "$DIR/config/chainspec-raw.json"
cp "$ROOT/network/tks-network-spec-raw.sha256" "$DIR/config/" 2>/dev/null || true
cp "$ROOT/network/bootnodes.public.txt" "$DIR/config/bootnodes.public.txt"
grep -v '^#' "$DIR/config/bootnodes.public.txt" | grep -v '^[[:space:]]*$' > "$DIR/bootnodes.txt"

echo ">> Docker build ($TAG)"
cd "$DIR"
export TKS_VERSION="$TAG"
docker compose build --no-cache
docker compose up -d --force-recreate

sleep 8
curl -sS http://127.0.0.1:9944 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_version","params":[]}'
echo ""
python3 -c "import json; print('bootNodes', json.load(open('config/chainspec-raw.json'))['bootNodes'])"
