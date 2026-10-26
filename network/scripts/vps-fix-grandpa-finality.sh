#!/usr/bin/env bash
# Fix stuck GRANDPA finality on production VPS (validators injected via author_insertKey + secretSeed).
# Run on the host in /opt/tks-docker (or set TKS_DOCKER_DIR).
set -euo pipefail

COMPOSE_DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"
KEYS_DIR="${TKS_KEYS_DIR:-$COMPOSE_DIR/config/session-keys}"
COMPOSE_FILE="${TKS_COMPOSE_FILE:-$COMPOSE_DIR/docker-compose.yml}"
CHAIN="${TKS_CHAIN_IN_CONTAINER:-/etc/tks/tks-testnet-spec-raw.json}"

if [[ ! -f "$COMPOSE_FILE" ]]; then
  echo "Compose file not found: $COMPOSE_FILE" >&2
  exit 1
fi
if [[ ! -d "$KEYS_DIR" ]]; then
  echo "Session keys dir not found: $KEYS_DIR" >&2
  exit 1
fi

cd "$COMPOSE_DIR"
IMAGE=$(docker inspect tks-validator1 --format '{{.Config.Image}}')

echo ">> Stopping validators..."
docker compose -f "$COMPOSE_FILE" stop validator1 validator2

rekey() {
  local n=$1
  local data="${COMPOSE_DIR}/data-validator${n}"
  local ks="${data}/chains/tks_network/keystore"
  echo ">> Re-keying validator ${n} (image ${IMAGE})..."
  rm -f "${ks}"/617572* "${ks}"/677261* 2>/dev/null || true
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

echo ">> Starting validators..."
docker compose -f "$COMPOSE_FILE" up -d validator1 validator2

echo ">> Wait ~30s, then verify:"
echo "curl -s -H 'Content-Type: application/json' -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"chain_getFinalizedHead\",\"params\":[]}' http://127.0.0.1:9944"
