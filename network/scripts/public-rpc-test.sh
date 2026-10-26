#!/usr/bin/env bash
# Public RPC smoke test — no secrets. Usage: TKS_RPC=https://rpc.tokenkickstarter.com ./public-rpc-test.sh
set -euo pipefail

RPC="${TKS_RPC:-https://rpc.tokenkickstarter.com}"
RELAY="${TKS_RELAY:-https://relay.tokenkickstarter.com}"
SHARD="${TKS_SHARD:-https://shard.tokenkickstarter.com}"
EXPECTED_GENESIS="${EXPECTED_GENESIS:-0xd6c706533a997bfb0b97de3f4d5eb83c1ab6f6eab7983e23ff273fcabe7ac43b}"
EXPECTED_CHAIN="${EXPECTED_CHAIN:-TKS Network}"
EXPECTED_SPEC="${EXPECTED_SPEC:-412}"
MULTISIG="${MULTISIG:-0x1af24cf357ddbb9434522984dcd540b40bd25ebd}"
ALICE_LEGACY="${ALICE_LEGACY:-0xe04cc55ebee1cbce552f250e85c57b70b2e2625b}"
V1="${V1:-0x02a7c7d31b22423ccdb1c0405b7fa63a1c6d8f19}"

export EXPECTED_GENESIS EXPECTED_CHAIN EXPECTED_SPEC MULTISIG ALICE_LEGACY V1

PASS=0
FAIL=0
SKIP=0

rpc() {
  local method="$1" params="${2:-[]}"
  curl -sS --connect-timeout 15 -m 30 -X POST "$RPC" \
    -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}"
}

check_ok() {
  local name="$1" json="$2" py="$3"
  if result=$(echo "$json" | python3 -c "$py" 2>/dev/null); then
    echo "PASS  $name — $result"
    PASS=$((PASS + 1))
  else
    echo "FAIL  $name"
    echo "      $(echo "$json" | head -c 200)"
    FAIL=$((FAIL + 1))
  fi
}

check_http() {
  local name="$1" url="$2" py="$3"
  local body code
  body=$(curl -sS --connect-timeout 12 -m 25 -w '\n%{http_code}' "$url" 2>/dev/null || echo -e '\n000')
  code=$(echo "$body" | tail -1)
  body=$(echo "$body" | sed '$d')
  if [[ "$code" != "200" ]]; then
    echo "FAIL  $name — HTTP $code"
    FAIL=$((FAIL + 1))
    return
  fi
  if echo "$body" | python3 -c "$py" 2>/dev/null; then
    echo "PASS  $name — $(echo "$body" | head -c 120)"
    PASS=$((PASS + 1))
  else
    echo "FAIL  $name — bad body"
    FAIL=$((FAIL + 1))
  fi
}

echo "=== TKS public RPC test ==="
echo "RPC:   $RPC"
echo "Relay: $RELAY"
echo "Shard: $SHARD"
echo ""

# ── System ──
r=$(rpc system_chain)
check_ok "system_chain" "$r" "
import json,sys,os
d=json.load(sys.stdin)
assert d.get('result')==os.environ['EXPECTED_CHAIN']
print(d['result'])
"

r=$(rpc system_version)
check_ok "system_version" "$r" "
import json,sys
d=json.load(sys.stdin)
v=d['result']
assert v.startswith('0.1.13')
print(v)
"

r=$(rpc system_name)
check_ok "system_name" "$r" "import json,sys; d=json.load(sys.stdin); print(d['result'])"

r=$(rpc system_health)
check_ok "system_health" "$r" "
import json,sys
d=json.load(sys.stdin)
h=d['result']
assert 'peers' in h and h['isSyncing'] is False
print(f\"peers={h['peers']} syncing={h['isSyncing']}\")
"

r=$(rpc system_properties)
check_ok "system_properties" "$r" "
import json,sys
d=json.load(sys.stdin)
p=d['result']
assert p.get('tokenSymbol') in ('TKS', ['TKS'])
print(p)
"

r=$(rpc system_localPeerId)
if echo "$r" | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'result' in d or 'error' in d" 2>/dev/null; then
  if echo "$r" | grep -q '"error"'; then
    echo "PASS  system_localPeerId — blocked on public Safe RPC (expected)"
    PASS=$((PASS + 1))
  else
    check_ok "system_localPeerId" "$r" "import json,sys; print(json.load(sys.stdin)['result'][:20])"
  fi
fi

# ── Chain ──
r=$(rpc chain_getBlockHash '[0]')
check_ok "chain_getBlockHash(0)" "$r" "
import json,sys,os
d=json.load(sys.stdin)
assert d['result'].lower()==os.environ['EXPECTED_GENESIS'].lower()
print(d['result'][:18]+'…')
"

r=$(rpc chain_getHeader)
check_ok "chain_getHeader" "$r" "
import json,sys
d=json.load(sys.stdin)
h=d['result']
n=int(h['number'],16)
assert n>=1
print(f'head #{n}')
"

r=$(rpc chain_getFinalizedHead)
check_ok "chain_getFinalizedHead" "$r" "
import json,sys,os
d=json.load(sys.stdin)
fh=d['result'].lower()
gh=os.environ['EXPECTED_GENESIS'].lower()
assert fh!=gh, 'finalized head still genesis — reinstall validator keys with install-production-validator-keys.sh'
print(d['result'][:18]+'… (not genesis)')
"

r=$(rpc chain_getBlock)
if echo "$r" | python3 -c "import json,sys; d=json.load(sys.stdin); assert d.get('result')" 2>/dev/null; then
  echo "PASS  chain_getBlock(latest) — ok"
  PASS=$((PASS + 1))
else
  echo "FAIL  chain_getBlock"
  FAIL=$((FAIL + 1))
fi

# ── State ──
r=$(rpc state_getRuntimeVersion)
check_ok "state_getRuntimeVersion" "$r" "
import json,sys,os
d=json.load(sys.stdin)
rv=d['result']
assert int(rv['specVersion'])==int(os.environ['EXPECTED_SPEC'])
print(f\"spec={rv['specVersion']} impl={rv['implVersion']}\")
"

# ── TKS HyperSwarm custom ──
r=$(rpc tks_listSwarmValidators)
check_ok "tks_listSwarmValidators" "$r" "
import json,sys,os
d=json.load(sys.stdin)
v=d['result']
assert len(v)==2
assert v[0]['account_id'].lower()==os.environ['V1'].lower()
print(f'{len(v)} validators')
"

r=$(rpc tks_hyperswarmNetworkInfo)
check_ok "tks_hyperswarmNetworkInfo" "$r" "
import json,sys
d=json.load(sys.stdin)
i=d['result']
assert i['active_shard_count']>=1
print(i)
"

# ── EVM ──
r=$(rpc eth_chainId)
check_ok "eth_chainId" "$r" "
import json,sys
d=json.load(sys.stdin)
cid=int(d['result'],16)
assert cid==7779
print(f'7779 (0x{cid:x})')
"

r=$(rpc net_version)
check_ok "net_version" "$r" "import json,sys; print(json.load(sys.stdin)['result'])"

r=$(rpc eth_blockNumber)
check_ok "eth_blockNumber" "$r" "
import json,sys
d=json.load(sys.stdin)
n=int(d['result'],16)
assert n>=1
print(f'#{n}')
"

r=$(rpc eth_syncing)
check_ok "eth_syncing" "$r" "
import json,sys
d=json.load(sys.stdin)
assert d['result'] is False or d['result']=='false'
print('false')
"

r=$(rpc eth_gasPrice)
check_ok "eth_gasPrice" "$r" "import json,sys; print(json.load(sys.stdin)['result'])"

r=$(rpc net_peerCount)
check_ok "net_peerCount" "$r" "import json,sys; print(json.load(sys.stdin)['result'])"

r=$(rpc eth_getBalance "[\"$ALICE_LEGACY\",\"latest\"]")
check_ok "eth_getBalance(legacy Alice)" "$r" "
import json,sys
d=json.load(sys.stdin)
assert d['result'] in ('0x0','0x0000000000000000000000000000000000000000000000000000000000000000')
print('0')
"

r=$(rpc eth_getBalance "[\"$MULTISIG\",\"latest\"]")
check_ok "eth_getBalance(multisig sudo)" "$r" "
import json,sys
d=json.load(sys.stdin)
v=int(d['result'],16)
assert v>0
print(f'{v/10**18:.4f} TKS')
"

r=$(rpc eth_getBlockByNumber '["latest", false]')
check_ok "eth_getBlockByNumber(latest)" "$r" "
import json,sys
d=json.load(sys.stdin)
b=d['result']
assert b and 'hash' in b
print(b['hash'][:18]+'…')
"

r=$(rpc eth_feeHistory '[4,"latest",[25,75]]')
if echo "$r" | grep -q '"result"'; then
  echo "PASS  eth_feeHistory"
  PASS=$((PASS + 1))
else
  echo "SKIP  eth_feeHistory — $(echo "$r" | head -c 80)"
  SKIP=$((SKIP + 1))
fi

r=$(rpc txpool_status)
if echo "$r" | grep -q '"result"'; then
  echo "PASS  txpool_status — $(echo "$r" | python3 -c "import json,sys; print(json.load(sys.stdin)['result'])" 2>/dev/null)"
  PASS=$((PASS + 1))
else
  echo "SKIP  txpool_status"
  SKIP=$((SKIP + 1))
fi

# Unsafe must not work on public RPC
r=$(rpc author_insertKey '["aura","0x00","0x00"]')
if echo "$r" | grep -q '"error"'; then
  echo "PASS  author_insertKey blocked — Safe RPC"
  PASS=$((PASS + 1))
else
  echo "FAIL  author_insertKey should be denied on public RPC"
  FAIL=$((FAIL + 1))
fi

# Health HTTP (nginx)
code=$(curl -sS -o /dev/null -w '%{http_code}' --connect-timeout 10 "$RPC/health" 2>/dev/null || echo 000)
if [[ "$code" == "200" || "$code" == "404" ]]; then
  echo "PASS  RPC /health HTTP $code"
  PASS=$((PASS + 1))
else
  echo "SKIP  RPC /health HTTP $code"
  SKIP=$((SKIP + 1))
fi

# ── Cipher HTTP (via public reverse proxy if configured) ──
check_http "relay GET /version" "$RELAY/version" "
import json,sys,os
d=json.load(sys.stdin)
assert d.get('chain_name')==os.environ['EXPECTED_CHAIN']
assert d.get('spec_version')==int(os.environ['EXPECTED_SPEC'])
print(d.get('node_version'))
"

check_http "shard GET /version" "$SHARD/version" "
import json,sys,os
d=json.load(sys.stdin)
assert d.get('genesis_hash','').lower()==os.environ['EXPECTED_GENESIS'].lower()
print('genesis ok')
"

for path in /stats /hybrid/stats /discovery; do
  code=$(curl -sS -o /dev/null -w '%{http_code}' --connect-timeout 10 "$RELAY$path" 2>/dev/null || echo 000)
  if [[ "$code" == "200" ]]; then
    echo "PASS  relay GET $path — HTTP 200"
    PASS=$((PASS + 1))
  else
    echo "SKIP  relay GET $path — HTTP $code"
    SKIP=$((SKIP + 1))
  fi
done

echo ""
echo "=== Summary: PASS=$PASS FAIL=$FAIL SKIP=$SKIP ==="
[[ "$FAIL" -eq 0 ]]
