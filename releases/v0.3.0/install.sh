#!/bin/bash
# ============================================================
#  TKS One-Line Installer
#  Users run: curl -sSf https://get.tks.network | bash
#  OR manually: bash install.sh
# ============================================================

set -e

VERSION="v0.3.0"
REPO="https://github.com/tokenkickstarter/tks-node/releases/download"
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Map architecture names
case "$ARCH" in
  x86_64)  ARCH="amd64" ;;
  aarch64) ARCH="arm64" ;;
  arm64)   ARCH="arm64" ;;
  *)       echo "❌ Unsupported arch: $ARCH"; exit 1 ;;
esac

# Map OS names
case "$OS" in
  linux)  OS_NAME="linux" ;;
  darwin) OS_NAME="macos" ;;
  *)      echo "❌ Unsupported OS: $OS"; exit 1 ;;
esac

BINARY_NAME="tks-chain-node-${VERSION}-${OS_NAME}-${ARCH}"
INSTALL_DIR="${HOME}/.tks/bin"
CHAINSPEC_URL="${REPO}/${VERSION}/tks-testnet-spec-raw.json"

echo ""
echo "  ████████╗██╗  ██╗███████╗"
echo "  ╚══██╔══╝██║ ██╔╝██╔════╝"
echo "     ██║   █████╔╝ ███████╗"
echo "     ██║   ██╔═██╗ ╚════██║"
echo "     ██║   ██║  ██╗███████║"
echo "     ╚═╝   ╚═╝  ╚═╝╚══════╝"
echo ""
echo "  TokenKickstarter Node Installer $VERSION"
echo ""

mkdir -p "$INSTALL_DIR"

# Download binary
BINARY_URL="${REPO}/${VERSION}/${BINARY_NAME}"
echo ">> Downloading TKS node ($OS_NAME / $ARCH)..."
if command -v curl &>/dev/null; then
  curl -L --progress-bar "$BINARY_URL" -o "$INSTALL_DIR/tks-chain-node"
elif command -v wget &>/dev/null; then
  wget --show-progress "$BINARY_URL" -O "$INSTALL_DIR/tks-chain-node"
else
  echo "❌ curl or wget required"; exit 1
fi
chmod +x "$INSTALL_DIR/tks-chain-node"

# Download chainspec
echo ">> Downloading chainspec..."
if command -v curl &>/dev/null; then
  curl -L --progress-bar "$CHAINSPEC_URL" -o "$INSTALL_DIR/tks-testnet-spec-raw.json"
else
  wget --show-progress "$CHAINSPEC_URL" -O "$INSTALL_DIR/tks-testnet-spec-raw.json"
fi

# Add to PATH
SHELL_RC="${HOME}/.bashrc"
[ -f "${HOME}/.zshrc" ] && SHELL_RC="${HOME}/.zshrc"

if ! grep -q "/.tks/bin" "$SHELL_RC" 2>/dev/null; then
  echo "" >> "$SHELL_RC"
  echo "# TKS Node" >> "$SHELL_RC"
  echo 'export PATH="$HOME/.tks/bin:$PATH"' >> "$SHELL_RC"
fi

echo ""
echo "============================================"
echo "  ✅ TKS Node installed!"
echo ""
echo "  Start your node:"
echo "  tks-chain-node \\"
echo "    --chain ~/.tks/bin/tks-testnet-spec-raw.json \\"
echo "    --bootnodes /ip4/YOUR_SERVER_IP/tcp/30333/p2p/12D3KooWNaz3nEXYRCWJUtEFXrpo2DpiuM1XuRuZ9HsKR2GDh9tT \\"
echo "    --base-path ~/.tks/data \\"
echo "    --rpc-external --rpc-cors all"
echo ""
echo "  Or use the quick-start:"
echo "  ~/.tks/bin/tks-chain-node --chain ~/.tks/bin/tks-testnet-spec-raw.json \\"
echo "    --bootnodes /ip4/YOUR_SERVER_IP/tcp/30333/p2p/12D3KooWNaz3nEXYRCWJUtEFXrpo2DpiuM1XuRuZ9HsKR2GDh9tT \\"
echo "    --base-path ~/.tks/data --rpc-external --rpc-cors all"
echo ""
echo "  Docs: https://docs.tks.network"
echo "  Explorer: https://explorer.tks.network"
echo "============================================"
