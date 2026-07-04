#!/bin/bash
# ============================================================
#  TKS Chain Reset / Purge Script
#  Wipes your local chain data for a clean resync
# ============================================================

CHAINSPEC="${1:-network/tks-testnet-spec-raw.json}"
DATA_DIR="${2:-${HOME}/.tks/data}"
BINARY="./target/release/tks-chain-node"

if [ ! -f "$BINARY" ]; then BINARY="tks-chain-node"; fi

echo ""
echo "⚠️  TKS Chain Purge"
echo "   Chainspec: $CHAINSPEC"
echo "   Data dir:  $DATA_DIR"
echo ""
echo "   This will DELETE all synced blocks and start fresh."
echo "   Your keys and accounts are NOT deleted."
echo ""
read -p "   Are you sure? (yes/no): " CONFIRM

if [ "$CONFIRM" != "yes" ]; then
  echo "   Aborted."
  exit 0
fi

echo ""
echo ">> Purging chain data..."
"$BINARY" purge-chain \
  --chain "$CHAINSPEC" \
  --base-path "$DATA_DIR" \
  -y

echo ""
echo "✅ Chain data purged. Run start-node.sh to resync from scratch."
