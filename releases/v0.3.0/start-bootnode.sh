#!/bin/bash
# ============================================================
#  TKS Bootnode Launcher
#  Run this on your always-on server (VPS/cloud)
#  This is the entry point all new nodes connect to
# ============================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${SCRIPT_DIR}/../target/release/tks-chain-node"
CHAINSPEC="${SCRIPT_DIR}/tks-testnet-spec-raw.json"
NODE_KEY="${SCRIPT_DIR}/bootnode.key"
DATA_DIR="${SCRIPT_DIR}/bootnode-data"

# Allow custom binary path for deployed servers
if [ ! -f "$BINARY" ]; then
  BINARY="tks-chain-node"
fi

echo "============================================"
echo "  TKS Network Bootnode"
echo "  Chain:    TKS Testnet"
echo "  Data:     $DATA_DIR"
echo "  Peer ID:  12D3KooWNaz3nEXYRCWJUtEFXrpo2DpiuM1XuRuZ9HsKR2GDh9tT"
echo "============================================"
echo ""

mkdir -p "$DATA_DIR"

exec "$BINARY" \
  --chain "$CHAINSPEC" \
  --base-path "$DATA_DIR" \
  --node-key-file "$NODE_KEY" \
  --port 30333 \
  --rpc-port 9944 \
  --rpc-external \
  --rpc-cors all \
  --name "TKS-Bootnode-1" \
  --validator \
  --log info
