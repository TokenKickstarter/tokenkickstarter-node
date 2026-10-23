#!/bin/bash
# ================================================================
#  TKS Network — Deploy 5 Nodes Across 5 VPS Servers
#  Run this script LOCALLY. It SSHes into each VPS and sets up
#  the TKS node automatically.
#
#  Usage:
#    chmod +x deploy-5-nodes.sh
#    ./deploy-5-nodes.sh
#
#  Requirements:
#    - SSH access to all 5 VPS (key-based, no password)
#    - .tks_vps_key SSH private key
# ================================================================

set -euo pipefail

# ── Colors ───────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

log()  { echo -e "${CYAN}[$(date '+%H:%M:%S')]${RESET} $*"; }
ok()   { echo -e "${GREEN}✓${RESET} $*"; }
warn() { echo -e "${YELLOW}⚠${RESET}  $*"; }
die()  { echo -e "${RED}✗${RESET} $*"; exit 1; }

# ── Configuration — EDIT THESE ───────────────────────────────────
VERSION="v0.1.1"
SSH_KEY="${SSH_KEY:-$HOME/Downloads/mobile-app/ninjaswap-mobile-design/.tks_vps_key}"
SSH_USER="${SSH_USER:-root}"

# VPS IP addresses — set these to your actual server IPs
VPS1_IP="${VPS1_IP:-}"   # Bootnode + Validator 1
VPS2_IP="${VPS2_IP:-}"   # Bootnode + Validator 2
VPS3_IP="${VPS3_IP:-}"   # Validator 3
VPS4_IP="${VPS4_IP:-}"   # Validator 4
VPS5_IP="${VPS5_IP:-}"   # RPC Full Node

# Network config
CHAIN_SPEC_URL="https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/tks-testnet-spec-raw.json"
BINARY_BASE_URL="https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}"

# Bootnode peer IDs (generated from bootnode.key — stable across restarts)
BOOTNODE1_PEER_ID="12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C"
BOOTNODE2_PEER_ID="12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR"

# ── Helper functions ─────────────────────────────────────────────
ssh_cmd() {
  local ip="$1"; shift
  ssh -i "$SSH_KEY" \
      -o StrictHostKeyChecking=no \
      -o ConnectTimeout=10 \
      "${SSH_USER}@${ip}" "$@"
}

scp_file() {
  local src="$1" ip="$2" dst="$3"
  scp -i "$SSH_KEY" \
      -o StrictHostKeyChecking=no \
      "$src" "${SSH_USER}@${ip}:${dst}"
}

check_ssh() {
  local ip="$1" name="$2"
  log "Testing SSH connection to $name ($ip)..."
  ssh_cmd "$ip" "echo ok" &>/dev/null && ok "$name reachable" || die "Cannot reach $name at $ip"
}

# ── Prompt for IPs if not set ────────────────────────────────────
prompt_ips() {
  echo ""
  echo -e "${BOLD}══════════════════════════════════════════════════${RESET}"
  echo -e "${BOLD}    TKS Network — 5 VPS Deployment                ${RESET}"
  echo -e "${BOLD}══════════════════════════════════════════════════${RESET}"
  echo ""
  echo "  Node roles:"
  echo "  VPS1  Bootnode + Validator 1  (Alice)"
  echo "  VPS2  Bootnode + Validator 2  (Bob)"
  echo "  VPS3  Validator 3             (Charlie)"
  echo "  VPS4  Validator 4             (Dave)"
  echo "  VPS5  RPC Full Node           (Public endpoint)"
  echo ""

  [ -z "$VPS1_IP" ] && read -rp "  VPS1 IP (Bootnode + Validator 1): " VPS1_IP
  [ -z "$VPS2_IP" ] && read -rp "  VPS2 IP (Bootnode + Validator 2): " VPS2_IP
  [ -z "$VPS3_IP" ] && read -rp "  VPS3 IP (Validator 3):            " VPS3_IP
  [ -z "$VPS4_IP" ] && read -rp "  VPS4 IP (Validator 4):            " VPS4_IP
  [ -z "$VPS5_IP" ] && read -rp "  VPS5 IP (RPC Full Node):          " VPS5_IP
  echo ""
}

# ── Install base system on a VPS ────────────────────────────────
install_base() {
  local ip="$1" name="$2"
  log "[$name] Installing system dependencies..."
  ssh_cmd "$ip" bash <<'REMOTE'
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl wget ufw jq htop
# Create tks user if not exists
id tks &>/dev/null || useradd --system --no-create-home --shell /bin/false tks
# Directories
mkdir -p /opt/tks /var/lib/tks
chown -R tks:tks /var/lib/tks
REMOTE
  ok "[$name] Base system ready"
}

# ── Install TKS binary ───────────────────────────────────────────
install_binary() {
  local ip="$1" name="$2" version="$3"
  log "[$name] Downloading TKS binary $version..."
  ssh_cmd "$ip" bash -s -- "$version" "$BINARY_BASE_URL" <<'REMOTE'
VERSION="$1"; BASE_URL="$2"
ARCH=$(uname -m)
case "$ARCH" in
  x86_64)  SUFFIX="amd64" ;;
  aarch64) SUFFIX="arm64" ;;
  *)       echo "Unsupported arch: $ARCH"; exit 1 ;;
esac
URL="${BASE_URL}/tks-chain-node-${VERSION}-linux-${SUFFIX}"
echo "  Downloading: $URL"
curl -fsSL "$URL" -o /opt/tks/tks-chain-node
chmod +x /opt/tks/tks-chain-node
chown tks:tks /opt/tks/tks-chain-node
VER=$(/opt/tks/tks-chain-node --version 2>&1 | head -1)
echo "  Binary: $VER"
REMOTE
  ok "[$name] Binary installed"
}

# ── Install chainspec ────────────────────────────────────────────
install_chainspec() {
  local ip="$1" name="$2"
  log "[$name] Installing chainspec..."
  ssh_cmd "$ip" bash -s -- "$CHAIN_SPEC_URL" <<'REMOTE'
URL="$1"
curl -fsSL "$URL" -o /opt/tks/tks-testnet-spec-raw.json
echo "  Chainspec: $(wc -c < /opt/tks/tks-testnet-spec-raw.json) bytes"
REMOTE
  ok "[$name] Chainspec installed"
}

# ── Configure firewall ───────────────────────────────────────────
configure_firewall() {
  local ip="$1" name="$2" open_rpc="${3:-false}"
  log "[$name] Configuring firewall..."
  ssh_cmd "$ip" bash -s -- "$open_rpc" <<'REMOTE'
OPEN_RPC="$1"
ufw --force reset
ufw default deny incoming
ufw default allow outgoing
ufw allow ssh
ufw allow 30333/tcp comment "TKS P2P"
[ "$OPEN_RPC" = "true" ] && ufw allow 9944/tcp comment "TKS RPC (public)"
ufw --force enable
ufw status
REMOTE
  ok "[$name] Firewall configured"
}

# ── Install bootnode systemd service ─────────────────────────────
install_bootnode_service() {
  local ip="$1" name="$2" node_name="$3"
  log "[$name] Installing bootnode service..."
  ssh_cmd "$ip" bash -s -- "$node_name" <<'REMOTE'
NODE_NAME="$1"
cat > /etc/systemd/system/tks-bootnode.service <<EOF
[Unit]
Description=TKS Bootnode
After=network.target
Wants=network-online.target

[Service]
User=tks
Group=tks
ExecStart=/opt/tks/tks-chain-node \
  --chain /opt/tks/tks-testnet-spec-raw.json \
  --base-path /var/lib/tks \
  --name "${NODE_NAME}" \
  --node-key-file /opt/tks/bootnode.key \
  --rpc-external \
  --rpc-cors=all \
  --rpc-port 9944 \
  --port 30333 \
  --validator \
  --prometheus-external \
  --prometheus-port 9615
Restart=always
RestartSec=5
LimitNOFILE=65536
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable tks-bootnode
REMOTE
  ok "[$name] Bootnode service installed"
}

# ── Install validator systemd service ───────────────────────────
install_validator_service() {
  local ip="$1" name="$2" node_name="$3" boot1_ip="$4" boot2_ip="$5" \
        boot1_peer="$6" boot2_peer="$7"
  log "[$name] Installing validator service..."
  ssh_cmd "$ip" bash -s -- "$node_name" "$boot1_ip" "$boot2_ip" "$boot1_peer" "$boot2_peer" <<'REMOTE'
NODE_NAME="$1"; BOOT1_IP="$2"; BOOT2_IP="$3"; BOOT1_PEER="$4"; BOOT2_PEER="$5"
cat > /etc/systemd/system/tks-node.service <<EOF
[Unit]
Description=TKS Validator Node
After=network.target
Wants=network-online.target

[Service]
User=tks
Group=tks
ExecStart=/opt/tks/tks-chain-node \
  --chain /opt/tks/tks-testnet-spec-raw.json \
  --base-path /var/lib/tks \
  --name "${NODE_NAME}" \
  --validator \
  --rpc-external \
  --rpc-cors=all \
  --rpc-port 9944 \
  --port 30333 \
  --bootnodes /ip4/${BOOT1_IP}/tcp/30333/p2p/${BOOT1_PEER} \
  --bootnodes /ip4/${BOOT2_IP}/tcp/30333/p2p/${BOOT2_PEER} \
  --prometheus-external \
  --prometheus-port 9615
Restart=always
RestartSec=5
LimitNOFILE=65536
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable tks-node
REMOTE
  ok "[$name] Validator service installed"
}

# ── Install RPC full node service ────────────────────────────────
install_rpc_service() {
  local ip="$1" name="$2" node_name="$3" boot1_ip="$4" boot2_ip="$5" \
        boot1_peer="$6" boot2_peer="$7"
  log "[$name] Installing RPC full node service..."
  ssh_cmd "$ip" bash -s -- "$node_name" "$boot1_ip" "$boot2_ip" "$boot1_peer" "$boot2_peer" <<'REMOTE'
NODE_NAME="$1"; BOOT1_IP="$2"; BOOT2_IP="$3"; BOOT1_PEER="$4"; BOOT2_PEER="$5"
cat > /etc/systemd/system/tks-node.service <<EOF
[Unit]
Description=TKS RPC Full Node
After=network.target
Wants=network-online.target

[Service]
User=tks
Group=tks
ExecStart=/opt/tks/tks-chain-node \
  --chain /opt/tks/tks-testnet-spec-raw.json \
  --base-path /var/lib/tks \
  --name "${NODE_NAME}" \
  --rpc-external \
  --rpc-cors=all \
  --rpc-port 9944 \
  --port 30333 \
  --bootnodes /ip4/${BOOT1_IP}/tcp/30333/p2p/${BOOT1_PEER} \
  --bootnodes /ip4/${BOOT2_IP}/tcp/30333/p2p/${BOOT2_PEER} \
  --prometheus-external \
  --prometheus-port 9615
Restart=always
RestartSec=5
LimitNOFILE=65536
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable tks-node
REMOTE
  ok "[$name] RPC full node service installed"
}

# ── Upload bootnode key ──────────────────────────────────────────
upload_bootnode_key() {
  local ip="$1" name="$2" key_num="$3"
  local keyfile="bootnodes/bootnode-${key_num}.key"
  [ -f "$keyfile" ] || keyfile="bootnode.key"
  log "[$name] Uploading bootnode key..."
  scp_file "$keyfile" "$ip" "/opt/tks/bootnode.key"
  ssh_cmd "$ip" "chmod 600 /opt/tks/bootnode.key && chown tks:tks /opt/tks/bootnode.key"
  ok "[$name] Bootnode key uploaded"
}

# ── Start service ────────────────────────────────────────────────
start_service() {
  local ip="$1" name="$2" svc="$3"
  log "[$name] Starting $svc..."
  ssh_cmd "$ip" "systemctl start $svc && sleep 3 && systemctl is-active $svc"
  ok "[$name] $svc started"
}

# ── Generate session keys ────────────────────────────────────────
generate_session_keys() {
  local ip="$1" name="$2"
  log "[$name] Generating session keys..."
  local keys
  keys=$(ssh_cmd "$ip" "curl -s -H 'Content-Type: application/json' \
    -d '{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"author_rotateKeys\",\"params\":[]}' \
    http://localhost:9944 | jq -r .result")
  echo "$keys"
}

# ── Check node status ────────────────────────────────────────────
check_node_status() {
  local ip="$1" name="$2"
  log "[$name] Checking node status..."
  local peers
  peers=$(ssh_cmd "$ip" "curl -s -H 'Content-Type: application/json' \
    -d '{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"system_health\",\"params\":[]}' \
    http://localhost:9944 | jq -r '.result | \"peers: \" + (.peers|tostring) + \", syncing: \" + (.isSyncing|tostring)'")
  echo -e "  ${GREEN}$name${RESET}: $peers"
}

# ════════════════════════════════════════════════════════════════
#  MAIN DEPLOYMENT
# ════════════════════════════════════════════════════════════════
main() {
  # Check SSH key exists
  [ -f "$SSH_KEY" ] || die "SSH key not found: $SSH_KEY\nSet SSH_KEY env var or place key at default path."

  prompt_ips

  # Validate all IPs provided
  for var in VPS1_IP VPS2_IP VPS3_IP VPS4_IP VPS5_IP; do
    [ -n "${!var}" ] || die "$var is empty. All 5 VPS IPs are required."
  done

  echo ""
  echo -e "${BOLD}Network topology:${RESET}"
  echo "  VPS1 $VPS1_IP  →  Bootnode 1 + Validator 1 (Alice)"
  echo "  VPS2 $VPS2_IP  →  Bootnode 2 + Validator 2 (Bob)"
  echo "  VPS3 $VPS3_IP  →  Validator 3 (Charlie)"
  echo "  VPS4 $VPS4_IP  →  Validator 4 (Dave)"
  echo "  VPS5 $VPS5_IP  →  RPC Full Node (public)"
  echo ""
  read -rp "Proceed with deployment? [y/N] " confirm
  [[ "$confirm" =~ ^[Yy]$ ]] || { echo "Aborted."; exit 0; }
  echo ""

  # ── Phase 1: Test SSH connectivity ────────────────────────────
  echo -e "${BOLD}Phase 1: Testing SSH connectivity${RESET}"
  check_ssh "$VPS1_IP" "VPS1"
  check_ssh "$VPS2_IP" "VPS2"
  check_ssh "$VPS3_IP" "VPS3"
  check_ssh "$VPS4_IP" "VPS4"
  check_ssh "$VPS5_IP" "VPS5"
  echo ""

  # ── Phase 2: Install base + binary on all nodes ───────────────
  echo -e "${BOLD}Phase 2: Installing TKS binary on all nodes${RESET}"
  for pair in "VPS1:$VPS1_IP" "VPS2:$VPS2_IP" "VPS3:$VPS3_IP" "VPS4:$VPS4_IP" "VPS5:$VPS5_IP"; do
    NAME="${pair%%:*}"; IP="${pair##*:}"
    install_base "$IP" "$NAME"
    install_binary "$IP" "$NAME" "$VERSION"
    install_chainspec "$IP" "$NAME"
  done
  echo ""

  # ── Phase 3: Configure firewalls ──────────────────────────────
  echo -e "${BOLD}Phase 3: Configuring firewalls${RESET}"
  configure_firewall "$VPS1_IP" "VPS1" "false"
  configure_firewall "$VPS2_IP" "VPS2" "false"
  configure_firewall "$VPS3_IP" "VPS3" "false"
  configure_firewall "$VPS4_IP" "VPS4" "false"
  configure_firewall "$VPS5_IP" "VPS5" "true"   # RPC open to public
  echo ""

  # ── Phase 4: Upload bootnode keys ─────────────────────────────
  echo -e "${BOLD}Phase 4: Uploading bootnode keys${RESET}"
  upload_bootnode_key "$VPS1_IP" "VPS1" "1"
  upload_bootnode_key "$VPS2_IP" "VPS2" "2"
  echo ""

  # ── Phase 5: Install systemd services ─────────────────────────
  echo -e "${BOLD}Phase 5: Installing systemd services${RESET}"
  install_bootnode_service "$VPS1_IP" "VPS1" "TKS-Bootnode-1"
  install_bootnode_service "$VPS2_IP" "VPS2" "TKS-Bootnode-2"
  install_validator_service "$VPS3_IP" "VPS3" "TKS-Validator-3" \
    "$VPS1_IP" "$VPS2_IP" "$BOOTNODE1_PEER_ID" "$BOOTNODE2_PEER_ID"
  install_validator_service "$VPS4_IP" "VPS4" "TKS-Validator-4" \
    "$VPS1_IP" "$VPS2_IP" "$BOOTNODE1_PEER_ID" "$BOOTNODE2_PEER_ID"
  install_rpc_service "$VPS5_IP" "VPS5" "TKS-RPC-Node" \
    "$VPS1_IP" "$VPS2_IP" "$BOOTNODE1_PEER_ID" "$BOOTNODE2_PEER_ID"
  echo ""

  # ── Phase 6: Start bootnodes first, then validators ───────────
  echo -e "${BOLD}Phase 6: Starting nodes (bootnodes first)${RESET}"
  start_service "$VPS1_IP" "VPS1" "tks-bootnode"
  start_service "$VPS2_IP" "VPS2" "tks-bootnode"
  log "Waiting 15 seconds for bootnodes to stabilize..."
  sleep 15
  start_service "$VPS3_IP" "VPS3" "tks-node"
  start_service "$VPS4_IP" "VPS4" "tks-node"
  start_service "$VPS5_IP" "VPS5" "tks-node"
  echo ""

  # ── Phase 7: Generate session keys for validators ─────────────
  echo -e "${BOLD}Phase 7: Generating validator session keys${RESET}"
  log "Waiting 20 seconds for nodes to fully start..."
  sleep 20

  echo ""
  echo -e "${BOLD}══════════════════════════════════════════════════${RESET}"
  echo -e "${BOLD}    SESSION KEYS — SAVE THESE!                    ${RESET}"
  echo -e "${BOLD}══════════════════════════════════════════════════${RESET}"
  echo ""
  echo -e "${YELLOW}VPS1 (Validator 1) session key:${RESET}"
  VPS1_KEYS=$(generate_session_keys "$VPS1_IP" "VPS1")
  echo "  $VPS1_KEYS"
  echo ""
  echo -e "${YELLOW}VPS2 (Validator 2) session key:${RESET}"
  VPS2_KEYS=$(generate_session_keys "$VPS2_IP" "VPS2")
  echo "  $VPS2_KEYS"
  echo ""
  echo -e "${YELLOW}VPS3 (Validator 3) session key:${RESET}"
  VPS3_KEYS=$(generate_session_keys "$VPS3_IP" "VPS3")
  echo "  $VPS3_KEYS"
  echo ""
  echo -e "${YELLOW}VPS4 (Validator 4) session key:${RESET}"
  VPS4_KEYS=$(generate_session_keys "$VPS4_IP" "VPS4")
  echo "  $VPS4_KEYS"
  echo ""

  # Save session keys to file
  cat > session-keys.txt <<EOF
TKS Network Session Keys — $(date)
Generated for network deployment

VPS1 ($VPS1_IP) — Validator 1:
$VPS1_KEYS

VPS2 ($VPS2_IP) — Validator 2:
$VPS2_KEYS

VPS3 ($VPS3_IP) — Validator 3:
$VPS3_KEYS

VPS4 ($VPS4_IP) — Validator 4:
$VPS4_KEYS
EOF
  ok "Session keys saved to session-keys.txt"
  echo ""

  # ── Phase 8: Health check ──────────────────────────────────────
  echo -e "${BOLD}Phase 8: Node health check${RESET}"
  log "Waiting 30 seconds for peers to connect..."
  sleep 30
  for pair in "VPS1:$VPS1_IP" "VPS2:$VPS2_IP" "VPS3:$VPS3_IP" "VPS4:$VPS4_IP" "VPS5:$VPS5_IP"; do
    NAME="${pair%%:*}"; IP="${pair##*:}"
    check_node_status "$IP" "$NAME"
  done
  echo ""

  # ── Summary ───────────────────────────────────────────────────
  echo -e "${BOLD}══════════════════════════════════════════════════${RESET}"
  echo -e "${BOLD}    ✅ DEPLOYMENT COMPLETE                        ${RESET}"
  echo -e "${BOLD}══════════════════════════════════════════════════${RESET}"
  echo ""
  echo "  Network:     TKS Testnet ($VERSION)"
  echo "  Bootnodes:   $VPS1_IP, $VPS2_IP"
  echo "  Validators:  $VPS1_IP, $VPS2_IP, $VPS3_IP, $VPS4_IP"
  echo "  RPC:         http://$VPS5_IP:9944"
  echo ""
  echo -e "${BOLD}Next steps:${RESET}"
  echo "  1. Register each validator on-chain via Polkadot.js:"
  echo "     https://polkadot.js.org/apps/?rpc=ws://$VPS5_IP:9944"
  echo "     → Developer → Extrinsics → session.setKeys(key, 0x)"
  echo ""
  echo "  2. Bond TKS and validate:"
  echo "     → Network → Staking → Bond → Validate"
  echo ""
  echo "  3. Monitor logs:"
  echo "     ssh ${SSH_USER}@VPS_IP 'journalctl -u tks-node -f'"
  echo ""
  echo "  Session keys saved to: ./session-keys.txt"
  echo -e "${BOLD}══════════════════════════════════════════════════${RESET}"
}

main "$@"
