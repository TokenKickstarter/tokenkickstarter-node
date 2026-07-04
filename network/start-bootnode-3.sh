#!/bin/bash
# ============================================================
#  TKS Bootnode 3 — seed.tksscan.com + seed.tokenkickstarter.pw
#  Run this on Server 3 (Asia region recommended)
#
#  DNS Setup for this server:
#    seed.tksscan.com         A  <THIS_SERVER_IP>
#    seed.tokenkickstarter.pw A  <THIS_SERVER_IP>
# ============================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${SCRIPT_DIR}/../target/release/tks-chain-node"
CHAINSPEC="${SCRIPT_DIR}/tks-testnet-spec-raw.json"
NODE_KEY="${SCRIPT_DIR}/bootnodes/bootnode-3.key"
DATA_DIR="${SCRIPT_DIR}/bootnode-3-data"

if [ ! -f "$BINARY" ]; then BINARY="tks-chain-node"; fi

echo "============================================"
echo "  TKS Bootnode 3 (Server 3)"
echo "  DNS:     seed.tksscan.com"
echo "           seed.tokenkickstarter.pw"
echo "  Peer ID: 12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8"
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
  --name "TKS-Seed-3" \
  --validator \
  --log info
