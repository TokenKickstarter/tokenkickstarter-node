#!/bin/bash
# ============================================================
#  TKS Bootnode 1 — seed.tokenkickstarter.com + seed.tokenkickstarter.ink
#  Run this on Server 1 (US region recommended)
#
#  DNS Setup for this server:
#    seed.tokenkickstarter.com  A  <THIS_SERVER_IP>
#    seed.tokenkickstarter.ink  A  <THIS_SERVER_IP>
# ============================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${SCRIPT_DIR}/../target/release/tks-chain-node"
CHAINSPEC="${SCRIPT_DIR}/tks-testnet-spec-raw.json"
NODE_KEY="${SCRIPT_DIR}/bootnodes/bootnode-1.key"
DATA_DIR="${SCRIPT_DIR}/bootnode-1-data"

if [ ! -f "$BINARY" ]; then BINARY="tks-chain-node"; fi

echo "============================================"
echo "  TKS Bootnode 1 (Server 1)"
echo "  DNS:     seed.tokenkickstarter.com"
echo "           seed.tokenkickstarter.ink"
echo "  Peer ID: 12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C"
echo "  Data:    $DATA_DIR"
echo "============================================"

mkdir -p "$DATA_DIR"

exec "$BINARY" \
  --chain "$CHAINSPEC" \
  --base-path "$DATA_DIR" \
  --node-key-file "$NODE_KEY" \
  --port 30333 \
  --rpc-port 9944 \
  --rpc-external \
  --rpc-cors all \
  --name "TKS-Seed-1" \
  --validator \
  --log info
