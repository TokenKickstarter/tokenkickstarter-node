#!/usr/bin/env bash
# Run on the VPS as root (aaPanel terminal or SSH). No secrets in this script.
set -euo pipefail

COMPOSE_DIR="${TKS_DOCKER_DIR:-/opt/tks-docker}"
RPC="${TKS_RPC:-http://127.0.0.1:9944}"

echo "== Docker =="
docker ps --format 'table {{.Names}}\t{{.Image}}\t{{.Status}}' | grep -E 'tks-|NAMES' || true

echo
echo "== Fullnode Cipher / anchor flags =="
docker inspect tks-fullnode --format '{{json .Config.Cmd}}' 2>/dev/null | tr ',' '\n' | grep -E 'cipher|anchor' || echo "(no cipher flags in Cmd)"

echo
echo "== Anchor seed mount =="
docker inspect tks-fullnode --format '{{range .Mounts}}{{println .Source "->" .Destination}}{{end}}' 2>/dev/null | grep -i secret || echo "(no /secrets mount)"

echo
echo "== Anchor JSON on disk (fullnode) =="
docker exec tks-fullnode sh -c 'ls -la /data/cipher/anchors 2>/dev/null | tail -5; ls /data/cipher/anchors/.posted 2>/dev/null | wc -l | xargs echo posted_markers' 2>/dev/null || echo "(cannot read /data/cipher/anchors)"

echo
echo "== Recent cipher-anchor logs =="
docker logs tks-fullnode 2>&1 | tail -200 | grep -E 'cipher-anchor|post_anchor|Invalid.*seed' | tail -15 || echo "(no cipher-anchor log lines)"

echo
echo "== Node version (RPC) =="
curl -sS -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"system_version","params":[]}' "$RPC" || true
echo

echo
echo "== Chain identity =="
curl -sS -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"system_chain","params":[]}' "$RPC" || true
echo

echo "Done. Fix checklist:"
echo "  1) Image/binary must be v0.1.9+ with --cipher-auto-post-anchor and --cipher-anchor-seed-file"
echo "  2) Seed file must exist on host before bind-mount (see network/docker-compose-live.yml)"
echo "  3) On-chain: ShardRegistry.register_validator + sudo force_rotate (or wait epoch)"
echo "  4) post_anchor signer must be in ShardValidators for shard_id 0"
