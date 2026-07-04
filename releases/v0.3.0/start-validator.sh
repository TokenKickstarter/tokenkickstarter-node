#!/bin/bash
# ============================================================
#  TKS Validator Node — Produce Blocks & Earn Rewards
#  Connects via DNS seeds — no hardcoded IPs
# ============================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${SCRIPT_DIR}/../target/release/tks-chain-node"
CHAINSPEC="${SCRIPT_DIR}/tks-testnet-spec-raw.json"
DATA_DIR="${HOME}/.tks/validator-data"
NODE_NAME="${TKS_NODE_NAME:-TKS-Validator-$(hostname)}"

if [ ! -f "$BINARY" ]; then BINARY="tks-chain-node"; fi

BOOTNODES=(
  "/dns/seed.tokenkickstarter.com/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C"
  "/dns/seed.tkstoken.com/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR"
  "/dns/seed.tksscan.com/tcp/30333/p2p/12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8"
  "/dns/seed.tokenkickstarter.ink/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C"
  "/dns/seed.tokenkickstarter.xyz/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR"
  "/dns/seed.tokenkickstarter.pw/tcp/30333/p2p/12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8"
)

echo "============================================"
echo "  TKS Validator Node"
echo "  Name:  $NODE_NAME"
echo "  Data:  $DATA_DIR"
echo ""
echo "  ⚠️  Run inject-validator-keys.sh first!"
echo "============================================"

mkdir -p "$DATA_DIR"

BOOTNODE_ARGS=""
for bn in "${BOOTNODES[@]}"; do
  BOOTNODE_ARGS="$BOOTNODE_ARGS --bootnodes $bn"
done

exec "$BINARY" \
  --chain "$CHAINSPEC" \
  --base-path "$DATA_DIR" \
  $BOOTNODE_ARGS \
  --port 30334 \
  --rpc-port 9945 \
  --rpc-external \
  --rpc-cors all \
  --name "$NODE_NAME" \
  --validator \
  --log info
