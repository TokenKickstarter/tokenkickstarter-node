#!/usr/bin/env bash
# Purge VPS chain data (same chainspec), reinstall validator session keys, restart stack.
# Run ON the VPS host in /opt/tks-docker as root.
set -euo pipefail

COMPOSE_DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"
cd "$COMPOSE_DIR"
COMPOSE_FILE="${TKS_COMPOSE_FILE:-docker-compose.yml}"
KEYS_DIR="${TKS_KEYS_DIR:-$COMPOSE_DIR/config/session-keys}"
CHAIN="${TKS_CHAIN_IN_CONTAINER:-/etc/tks/tks-testnet-spec-raw.json}"
TS="$(date +%Y%m%d-%H%M%S)"

echo ">> Backup and purge chain data ($TS)..."
mkdir -p backups
docker compose -f "$COMPOSE_FILE" down || true
tar czf "backups/chain-data-${TS}.tgz" data-fullnode data-validator1 data-validator2
find data-fullnode data-validator1 data-validator2 -mindepth 1 -delete 2>/dev/null || true

echo ">> Rebuild images..."
docker compose -f "$COMPOSE_FILE" build

IMAGE="${TKS_DOCKER_IMAGE:-tks-docker-fullnode:latest}"

rekey() {
  local n=$1
  local data="${COMPOSE_DIR}/data-validator${n}"
  mkdir -p "$data"
  echo ">> key insert validator $n..."
  local aura gran
  aura=$(python3 -c "import json; print(json.load(open('${KEYS_DIR}/validator${n}-aura.json'))['secretPhrase'])")
  gran=$(python3 -c "import json; print(json.load(open('${KEYS_DIR}/validator${n}-grandpa.json'))['secretPhrase'])")
  docker run --rm --user tks -v "${data}:/data" "$IMAGE" \
    key insert --base-path /data --chain "$CHAIN" --key-type aura --scheme Sr25519 --suri "$aura"
  docker run --rm --user tks -v "${data}:/data" "$IMAGE" \
    key insert --base-path /data --chain "$CHAIN" --key-type gran --scheme Ed25519 --suri "$gran"
}

rekey 1
rekey 2

echo ">> Starting stack..."
docker compose -f "$COMPOSE_FILE" up -d

echo ">> Done. Verify in ~20s:"
echo "curl -s http://127.0.0.1:9944 -H 'Content-Type: application/json' -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_blockNumber\",\"params\":[]}'"
echo "curl -s http://127.0.0.1:9944 -H 'Content-Type: application/json' -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"chain_getBlockHash\",\"params\":[0]}'"
