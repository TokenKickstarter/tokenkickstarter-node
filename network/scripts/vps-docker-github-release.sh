#!/usr/bin/env bash
# VPS /opt/tks-docker: install tks-chain-node from GitHub Release tarball + rebuild 3 containers.
# No git pull, no source build. Optional: extract WASM from same tag if present in release bundle.
set -euo pipefail

VERSION="${VERSION:?Set VERSION e.g. v0.1.21}"
DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"
REPO="${TKS_GITHUB_REPO:-TokenKickstarter/tokenkickstarter-node}"
ARCH="${TKS_LINUX_ARCH:-x86_64}"

TAR="tks-chain-node-linux-${ARCH}-${VERSION}.tar.gz"
BASE="https://github.com/${REPO}/releases/download/${VERSION}"
WORKDIR="$(mktemp -d /tmp/tks-release-XXXXXX)"
trap 'rm -rf "$WORKDIR"' EXIT

echo ">> Download ${BASE}/${TAR}"
curl -fsSL -o "${WORKDIR}/${TAR}" "${BASE}/${TAR}"

echo ">> Install binary"
tar -xzf "${WORKDIR}/${TAR}" -C "$WORKDIR"
BIN="$WORKDIR/tks-chain-node-linux-${ARCH}"

# Release SHA256SUMS lists unpacked binary names (e.g. tks-chain-node-v0.1.21-linux-amd64).
if curl -fsSL -o "${WORKDIR}/SHA256SUMS.txt" "${BASE}/SHA256SUMS.txt" 2>/dev/null; then
  case "$ARCH" in
    x86_64) SUMS_NAME="tks-chain-node-${VERSION}-linux-amd64" ;;
    arm64|aarch64) SUMS_NAME="tks-chain-node-${VERSION}-linux-arm64" ;;
    *) SUMS_NAME="" ;;
  esac
  EXPECTED=""
  for candidate in \
    "tks-chain-node-${VERSION}-linux-amd64" \
    "tks-chain-node-linux-${ARCH}" \
    "tks-chain-node-${VERSION}-linux-arm64"; do
    line="$(grep -E " ${candidate}$" "${WORKDIR}/SHA256SUMS.txt" 2>/dev/null | head -1 || true)"
    if [[ -n "$line" ]]; then
      EXPECTED="${line%% *}"
      break
    fi
  done
  if [[ -n "$EXPECTED" ]]; then
    ACTUAL="$(sha256sum "$BIN" | awk '{print $1}')"
    if [[ "$ACTUAL" != "$EXPECTED" ]]; then
      echo ">> SHA256 mismatch for $BIN (expected $EXPECTED, got $ACTUAL)" >&2
      exit 1
    fi
    echo ">> SHA256 OK ($BIN)"
  else
    echo ">> WARN: no SHA256SUMS entry for ${SUMS_NAME:-$ARCH}; skipping checksum"
  fi
else
  echo ">> WARN: no SHA256SUMS.txt on release; skipping checksum"
fi
install -d "$DIR/config"
cp -a "$DIR/config/tks-chain-node" "$DIR/config/tks-chain-node.bak.$(date +%Y%m%d%H%M)" 2>/dev/null || true
install -m 755 "$BIN" "$DIR/config/tks-chain-node"
"$DIR/config/tks-chain-node" --version | head -1

# Optional runtime WASM (when release ships it)
for wasm_name in tks_runtime.compact.compressed.wasm "tks-runtime-${VERSION}.wasm"; do
  if curl -fsSL -o "${WORKDIR}/${wasm_name}" "${BASE}/${wasm_name}" 2>/dev/null; then
    cp -a "$DIR/config/tks_runtime.compact.compressed.wasm" \
      "$DIR/config/tks_runtime.compact.compressed.wasm.bak.$(date +%Y%m%d%H%M)" 2>/dev/null || true
    install -m 644 "${WORKDIR}/${wasm_name}" "$DIR/config/tks_runtime.compact.compressed.wasm"
    echo ">> Updated $DIR/config/tks_runtime.compact.compressed.wasm"
    break
  fi
done

echo ">> Rebuild Docker images + recreate fullnode + 2 validators"
cd "$DIR"
docker compose build
docker compose up -d --force-recreate

sleep 12
echo ">> RPC checks"
curl -sS http://127.0.0.1:9944 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_version","params":[]}'
echo
curl -sS http://127.0.0.1:9944 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"state_getRuntimeVersion","params":[]}' \
  | python3 -c "import json,sys; r=json.load(sys.stdin).get('result') or {}; print('spec_version', r.get('specVersion'))"

echo ">> Done. If spec_version is still 412: TKS_WASM=$DIR/config/tks_runtime.compact.compressed.wasm bash live-multisig-sudo-set-code.sh (from repo scripts on host)."
