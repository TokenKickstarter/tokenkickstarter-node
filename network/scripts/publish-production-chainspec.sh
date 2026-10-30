#!/usr/bin/env bash
# Copy live production raw chain spec from VPS (or local path), write SHA-256 sidecar for releases.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/network}"
RAW="$OUT_DIR/tks-network-spec-raw.json"

if [[ "${1:-}" == "--from-vps" ]]; then
  VPS="${VPS:-root@144.91.122.247}"
  SSH_PORT="${SSH_PORT:-7422}"
  SSH_KEY="${SSH_KEY:-$HOME/.ssh/tks_vps_ed25519}"
  REMOTE="${REMOTE_SPEC:-/opt/tks-docker/config/tks-testnet-spec-raw.json}"
  scp -P "$SSH_PORT" -i "$SSH_KEY" "$VPS:$REMOTE" "$RAW"
elif [[ -n "${1:-}" ]]; then
  cp "$1" "$RAW"
else
  echo "usage: $0 --from-vps | /path/to/tks-testnet-spec-raw.json" >&2
  exit 1
fi

sha256sum "$RAW" | tee "$OUT_DIR/tks-network-spec-raw.sha256"
echo "Published: $RAW ($(wc -c < "$RAW") bytes)"
