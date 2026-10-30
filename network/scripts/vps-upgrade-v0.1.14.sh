#!/usr/bin/env bash
# Upgrade /opt/tks-docker to v0.1.14 binary + audit compose flags (no chain purge).
set -euo pipefail
COMPOSE_DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"
VERSION="${TKS_VERSION:-v0.1.14}"
ASSET="tks-chain-node-${VERSION}-linux-amd64"
URL="https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/${ASSET}"

cd "$COMPOSE_DIR"
echo ">> Download ${ASSET}..."
curl -fsSL -o "config/tks-chain-node.new" "$URL"
chmod +x "config/tks-chain-node.new"
mv "config/tks-chain-node.new" "config/tks-chain-node"
cp "config/tks-chain-node" "config/tks-chain-node-${VERSION}"

python3 << 'PY'
from pathlib import Path
p = Path("docker-compose.yml")
text = p.read_text()
insert_full = [
    "      - --pool-type=fork-aware",
    "      - --pool-future-divisor=10",
    "      - --rpc-hide-node-identity",
    "      - --in-peers=32",
    "      - --out-peers=32",
]
insert_val = [
    "      - --pool-type=fork-aware",
    "      - --pool-future-divisor=10",
    "      - --in-peers=32",
    "      - --out-peers=32",
]
if "--pool-type=fork-aware" in text:
    print("compose already patched")
else:
    lines = text.splitlines()
    out = []
    i = 0
    while i < len(lines):
        out.append(lines[i])
        if lines[i].strip() == "- --pool-kbytes=65536":
            if i > 0 and "fullnode:" in text[: text.find(lines[i])]:
                out.extend(insert_full)
            else:
                out.extend(insert_val)
        i += 1
    p.write_text("\n".join(out) + "\n")
    print("patched docker-compose.yml")
PY

docker compose build
docker compose up -d --force-recreate
sleep 15
docker exec tks-fullnode tks-chain-node --version | head -1
curl -s http://127.0.0.1:9944 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'
