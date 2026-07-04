#!/bin/bash
# ============================================================
#  TKS Validator Key Injection Script
#  Run once before starting your validator node
#  Injects Aura (block production) + GRANDPA (finality) keys
# ============================================================

set -e

RPC_URL="${1:-http://127.0.0.1:9945}"

echo "============================================"
echo "  TKS Validator Key Injection"
echo "  RPC: $RPC_URL"
echo "============================================"
echo ""

# ── Step 1: Generate keys ────────────────────────────────────
echo ">> Generating Aura (Sr25519) key..."
AURA_OUT=$(./target/release/tks-chain-node key generate --scheme Sr25519 --output-type json 2>/dev/null)
AURA_SEED=$(echo "$AURA_OUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['secretSeed'])")
AURA_PUB=$(echo "$AURA_OUT"  | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['publicKey'])")
echo "   Public: $AURA_PUB"
echo "   Seed:   $AURA_SEED"
echo ""

echo ">> Generating GRANDPA (Ed25519) key..."
GRAN_OUT=$(./target/release/tks-chain-node key generate --scheme Ed25519 --output-type json 2>/dev/null)
GRAN_SEED=$(echo "$GRAN_OUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['secretSeed'])")
GRAN_PUB=$(echo "$GRAN_OUT"  | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['publicKey'])")
echo "   Public: $GRAN_PUB"
echo "   Seed:   $GRAN_SEED"
echo ""

# ── Step 2: Inject into node keystore ───────────────────────
echo ">> Injecting Aura key into node..."
curl -s -X POST "$RPC_URL" \
  -H "Content-Type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"method\":\"author_insertKey\",\"params\":[\"aura\",\"$AURA_SEED\",\"$AURA_PUB\"],\"id\":1}" \
  | python3 -m json.tool

echo ""
echo ">> Injecting GRANDPA key into node..."
curl -s -X POST "$RPC_URL" \
  -H "Content-Type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"method\":\"author_insertKey\",\"params\":[\"gran\",\"$GRAN_SEED\",\"$GRAN_PUB\"],\"id\":2}" \
  | python3 -m json.tool

echo ""
echo "============================================"
echo "  ✅ Keys injected successfully!"
echo ""
echo "  SAVE THESE KEYS SECURELY:"
echo "  Aura Seed:    $AURA_SEED"
echo "  GRANDPA Seed: $GRAN_SEED"
echo ""
echo "  Send these public keys to TKS team"
echo "  to be added as a network validator:"
echo "  Aura Public:    $AURA_PUB"
echo "  GRANDPA Public: $GRAN_PUB"
echo "============================================"
