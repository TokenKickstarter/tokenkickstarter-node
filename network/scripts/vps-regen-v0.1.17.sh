#!/usr/bin/env bash
# Mainnet regen: new raw spec + production keys already on host under /opt/tks-docker/production-keys.
# Run ON VPS as root after v0.1.17 binary is in config/tks-chain-node.
set -euo pipefail
COMPOSE_DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"
VERSION="${TKS_VERSION:-v0.1.17}"
ASSET="tks-chain-node-${VERSION}-linux-amd64"
URL="https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/${ASSET}"
KEYS="${TKS_PRODUCTION_KEYS:-$COMPOSE_DIR/production-keys}"
SPEC_SRC="${TKS_RAW_SPEC:-$COMPOSE_DIR/config/tks-testnet-spec-raw.json}"

cd "$COMPOSE_DIR"

echo ">> Node ${VERSION}..."
curl -fsSL -o "config/tks-chain-node.new" "$URL"
chmod +x "config/tks-chain-node.new"
mv "config/tks-chain-node.new" "config/tks-chain-node"

if [[ -f "$KEYS/cipher-anchor-seed.txt" ]]; then
  install -m 600 "$KEYS/cipher-anchor-seed.txt" "config/cipher-anchor-seed.txt"
fi

mkdir -p config/session-keys
for f in validator1-aura.json validator1-grandpa.json validator2-aura.json validator2-grandpa.json; do
  install -m 600 "$KEYS/$f" "config/session-keys/$f"
done

export TKS_KEYS_DIR="$COMPOSE_DIR/config/session-keys"
export TKS_CHAIN_IN_CONTAINER="/etc/tks/tks-testnet-spec-raw.json"
RESET="$COMPOSE_DIR/scripts/vps-chain-reset-and-up.sh"
[[ -x "$RESET" ]] || RESET="/tmp/vps-chain-reset-and-up.sh"
bash "$RESET"
sleep 25
docker exec tks-fullnode tks-chain-node --version | head -1
curl -sS http://127.0.0.1:9944 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"chain_getBlockHash","params":[0]}'
echo
curl -sS http://127.0.0.1:9944 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'
echo
