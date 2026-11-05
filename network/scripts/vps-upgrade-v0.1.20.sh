#!/usr/bin/env bash
# VPS: pull latest main, rebuild Docker (v0.1.20 / spec 413 node), no purge.
set -euo pipefail
DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"
REPO="${TKS_REPO_DIR:-/opt/tks-src}"
TAG="${TKS_IMAGE_TAG:-v0.1.20-r6}"

if [[ -d "$REPO/.git" ]]; then
  echo ">> git pull in $REPO"
  git -C "$REPO" fetch origin main
  git -C "$REPO" checkout main
  git -C "$REPO" pull --ff-only origin main
  SRC="$REPO"
else
  SRC="$(cd "$(dirname "$0")/../.." && pwd)"
  echo ">> Using local tree $SRC (set TKS_REPO_DIR for git pull on VPS)"
fi

cp "$SRC/network/tks-network-spec-raw.json" "$DIR/config/chainspec-raw.json"
cp "$SRC/network/tks-network-spec-raw.sha256" "$DIR/config/" 2>/dev/null || true
cp "$SRC/network/bootnodes.public.txt" "$DIR/bootnodes.public.txt"
grep -v '^#' "$DIR/bootnodes.public.txt" | grep -v '^[[:space:]]*$' > "$DIR/bootnodes.txt"

echo ">> Docker build context: $SRC"
export TKS_BUILD_CONTEXT="$SRC"
cd "$DIR"
docker compose build
docker compose up -d --force-recreate

sleep 12
curl -sS http://127.0.0.1:9944 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_version","params":[]}'
echo ""
curl -sS http://127.0.0.1:9944 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"state_getRuntimeVersion","params":[]}' | python3 -c "
import json,sys
r=json.load(sys.stdin).get('result') or {}
print('spec_version', r.get('specVersion'), 'impl', r.get('implVersion'))
"

echo "If spec_version is still 412, run runtime WASM upgrade (multisig → sudo.set_code)."
