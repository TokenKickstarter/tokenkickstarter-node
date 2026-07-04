#!/bin/bash
# ============================================================
#  TKS Server Setup Script
#  Installs TKS node as a systemd service on Linux
#  Run as root: sudo bash server-setup.sh
# ============================================================

set -e

BOOTNODE_IP="${1:-}"
NODE_TYPE="${2:-fullnode}"   # bootnode | fullnode | validator
VERSION="v0.3.0"
PEER_ID="12D3KooWNaz3nEXYRCWJUtEFXrpo2DpiuM1XuRuZ9HsKR2GDh9tT"

# ── Check root ───────────────────────────────────────────────
if [ "$EUID" -ne 0 ]; then
  echo "❌ Please run as root: sudo bash server-setup.sh <bootnode-ip> [bootnode|fullnode|validator]"
  exit 1
fi

if [ -z "$BOOTNODE_IP" ] && [ "$NODE_TYPE" != "bootnode" ]; then
  echo "❌ Usage: sudo bash server-setup.sh <bootnode-ip> [fullnode|validator]"
  echo "   Example: sudo bash server-setup.sh 1.2.3.4 fullnode"
  exit 1
fi

echo ""
echo "  ╔═══════════════════════════════╗"
echo "  ║   TKS Network Server Setup    ║"
echo "  ║   Type: $NODE_TYPE"
echo "  ╚═══════════════════════════════╝"
echo ""

# ── 1. Create tks user ───────────────────────────────────────
echo ">> Creating tks system user..."
if ! id "tks" &>/dev/null; then
  useradd --system --no-create-home --shell /bin/false tks
fi

# ── 2. Create directories ────────────────────────────────────
echo ">> Setting up directories..."
mkdir -p /opt/tks
mkdir -p /var/lib/tks
mkdir -p /var/lib/tks-bootnode

# ── 3. Install binary ────────────────────────────────────────
echo ">> Installing TKS node binary..."
ARCH=$(uname -m)
case "$ARCH" in
  x86_64)  ARCH_SUFFIX="amd64" ;;
  aarch64) ARCH_SUFFIX="arm64" ;;
  *)       echo "❌ Unsupported arch: $ARCH"; exit 1 ;;
esac

BINARY_URL="https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/tks-chain-node-${VERSION}-linux-${ARCH_SUFFIX}"
echo "   Downloading from: $BINARY_URL"
curl -L --progress-bar "$BINARY_URL" -o /opt/tks/tks-chain-node
chmod +x /opt/tks/tks-chain-node

# Verify binary
/opt/tks/tks-chain-node --version || { echo "❌ Binary verification failed"; exit 1; }

# ── 4. Install chainspec ─────────────────────────────────────
echo ">> Installing chainspec..."
CHAINSPEC_URL="https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/tks-testnet-spec-raw.json"
curl -L --progress-bar "$CHAINSPEC_URL" -o /opt/tks/tks-testnet-spec-raw.json

# ── 5. Bootnode key (bootnode only) ──────────────────────────
if [ "$NODE_TYPE" = "bootnode" ]; then
  if [ -f "bootnode.key" ]; then
    echo ">> Installing bootnode key..."
    cp bootnode.key /opt/tks/bootnode.key
    chmod 600 /opt/tks/bootnode.key
  else
    echo "❌ bootnode.key not found. Copy it to this directory first."
    exit 1
  fi
fi

# ── 6. Install systemd service ───────────────────────────────
echo ">> Installing systemd service..."

if [ "$NODE_TYPE" = "bootnode" ]; then
  cp tks-bootnode.service /etc/systemd/system/tks-node.service
else
  # Update bootnode IP in service file
  sed "s/YOUR_SERVER_IP/$BOOTNODE_IP/g" tks-node.service > /etc/systemd/system/tks-node.service
fi

# ── 7. Set permissions ───────────────────────────────────────
chown -R tks:tks /opt/tks
chown -R tks:tks /var/lib/tks
chown -R tks:tks /var/lib/tks-bootnode

# ── 8. Open firewall ─────────────────────────────────────────
echo ">> Opening firewall ports..."
if command -v ufw &>/dev/null; then
  ufw allow 30333/tcp comment "TKS P2P"
  ufw allow 9944/tcp comment "TKS RPC"
  echo "   ufw: ports 30333, 9944 opened"
elif command -v firewall-cmd &>/dev/null; then
  firewall-cmd --permanent --add-port=30333/tcp
  firewall-cmd --permanent --add-port=9944/tcp
  firewall-cmd --reload
  echo "   firewalld: ports 30333, 9944 opened"
fi

# ── 9. Enable and start service ──────────────────────────────
echo ">> Starting TKS node service..."
systemctl daemon-reload
systemctl enable tks-node
systemctl start tks-node

echo ""
echo "============================================"
echo "  ✅ TKS Node installed and running!"
echo ""
echo "  Check status:  systemctl status tks-node"
echo "  View logs:     journalctl -u tks-node -f"
echo "  Stop node:     systemctl stop tks-node"
echo "  RPC endpoint:  http://$(hostname -I | awk '{print $1}'):9944"

if [ "$NODE_TYPE" = "bootnode" ]; then
  echo ""
  echo "  🟢 Bootnode running!"
  echo "  Peer multiaddr:"
  echo "  /ip4/$(hostname -I | awk '{print $1}')/tcp/30333/p2p/$PEER_ID"
  echo ""
  echo "  Update start-node.sh and start-validator.sh with your IP:"
  echo "  sed -i 's/YOUR_SERVER_IP/$(hostname -I | awk '{print $1}')/g' start-node.sh"
fi
echo "============================================"
