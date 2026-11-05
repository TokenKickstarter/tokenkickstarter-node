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

echo ">> Build release binary (Docker builder image; ~30–60 min first time)"
cd "$SRC"
docker build -t tks-node-build:"$TAG" -f Dockerfile .
cp -a "$DIR/config/tks-chain-node" "$DIR/config/tks-chain-node.bak.$(date +%Y%m%d%H%M)" 2>/dev/null || true
EXTRACT="tks-bin-extract-$$"
docker create --name "$EXTRACT" tks-node-build:"$TAG" >/dev/null
docker cp "$EXTRACT:/app/tks-chain-node" "$DIR/config/tks-chain-node"
WASM_IN_IMAGE="/app/tks_runtime.compact.compressed.wasm"
if docker cp "$EXTRACT:$WASM_IN_IMAGE" "$DIR/config/tks_runtime.compact.compressed.wasm" 2>/dev/null; then
  echo ">> Extracted runtime WASM to $DIR/config/tks_runtime.compact.compressed.wasm"
else
  echo ">> WARN: could not extract $WASM_IN_IMAGE (run set_code from a host with WASM after build)"
fi
docker rm "$EXTRACT" >/dev/null

cd "$DIR"
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

if [[ -f "$DIR/config/tks_runtime.compact.compressed.wasm" ]]; then
  echo ">> To activate spec 413: TKS_WASM=$DIR/config/tks_runtime.compact.compressed.wasm bash $SRC/network/scripts/live-multisig-sudo-set-code.sh"
fi
echo "If spec_version is still 412, run runtime WASM upgrade (multisig → sudo.set_code)."
