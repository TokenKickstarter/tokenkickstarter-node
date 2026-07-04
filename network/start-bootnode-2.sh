#!/bin/bash
# ============================================================
#  TKS Bootnode 2 — seed.tkstoken.com + seed.tokenkickstarter.xyz
#  Run this on Server 2 (EU region recommended)
#
#  DNS Setup for this server:
#    seed.tkstoken.com           A  <THIS_SERVER_IP>
#    seed.tokenkickstarter.xyz   A  <THIS_SERVER_IP>
# ============================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${SCRIPT_DIR}/../target/release/tks-chain-node"
CHAINSPEC="${SCRIPT_DIR}/tks-testnet-spec-raw.json"
NODE_KEY="${SCRIPT_DIR}/bootnodes/bootnode-2.key"
DATA_DIR="${SCRIPT_DIR}/bootnode-2-data"

if [ ! -f "$BINARY" ]; then BINARY="tks-chain-node"; fi

echo "============================================"
echo "  TKS Bootnode 2 (Server 2)"
echo "  DNS:     seed.tkstoken.com"
echo "           seed.tokenkickstarter.xyz"
echo "  Peer ID: 12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR"
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
  --name "TKS-Seed-2" \
  --validator \
  --log info
