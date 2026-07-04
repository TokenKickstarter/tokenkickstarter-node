#!/bin/bash
# ============================================================
#  TKS Full Node — Join the TKS Network
#  No IP address needed — connects via DNS seeds
# ============================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${SCRIPT_DIR}/../target/release/tks-chain-node"
CHAINSPEC="${SCRIPT_DIR}/tks-testnet-spec-raw.json"
DATA_DIR="${HOME}/.tks/data"
NODE_NAME="${TKS_NODE_NAME:-TKS-Node-$(hostname)}"

if [ ! -f "$BINARY" ]; then BINARY="tks-chain-node"; fi

# ── DNS-based bootnodes (no hardcoded IPs) ────────────────────
# These are resolved automatically — if any server moves, only
# the DNS record changes. The node binary never needs updating.
BOOTNODES=(
  "/dns/seed.tokenkickstarter.com/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C"
  "/dns/seed.tkstoken.com/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR"
  "/dns/seed.tksscan.com/tcp/30333/p2p/12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8"
  "/dns/seed.tokenkickstarter.ink/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C"
  "/dns/seed.tokenkickstarter.xyz/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR"
  "/dns/seed.tokenkickstarter.pw/tcp/30333/p2p/12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8"
)

echo "============================================"
echo "  TKS Network Full Node"
echo "  Name:  $NODE_NAME"
echo "  Data:  $DATA_DIR"
echo "  Seeds: 6 DNS seed nodes"
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
  --port 30333 \
  --rpc-port 9944 \
  --rpc-external \
  --rpc-cors all \
  --name "$NODE_NAME" \
  --log info
