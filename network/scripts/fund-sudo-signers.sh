#!/usr/bin/env bash
# Fund the three sudo multisig signers with native TKS for extrinsic fees (operator-only).
#
# Requires a funded H160 seed file (e.g. treasury or validator stash JSON with secretPhrase).
# Does NOT commit secrets. Run on a trusted host:
#
#   FUND_SEED_FILE=/path/to/funder.json AMOUNT_TKS=1 ./fund-sudo-signers.sh
#
set -euo pipefail
RPC="${TKS_RPC:-https://rpc.tokenkickstarter.com}"
FUND_SEED_FILE="${FUND_SEED_FILE:?set FUND_SEED_FILE to a JSON key file}"
AMOUNT_TKS="${AMOUNT_TKS:-1}"
SIGNERS=(
  0xd876889fb236ebacdcf00e5007666d2094a5ada5
  0x5a3173a1d18049f9b19bdd15de9b28b671e0dc29
  0xaedfec3469ffe1fee0e0a54679270e183026b66e
)

if ! python3 -c "import eth_account" 2>/dev/null; then
  echo "Install: python3 -m pip install eth-account web3" >&2
  exit 1
fi

export RPC AMOUNT_TKS FUND_SEED_FILE
python3 <<PY
import json, os, subprocess, urllib.request
from eth_account import Account

rpc = os.environ["RPC"]
seed_path = os.environ["FUND_SEED_FILE"]
amount_wei = int(float(os.environ["AMOUNT_TKS"]) * 10**18)
raw = open(seed_path).read().strip()
phrase = json.loads(raw)["secretPhrase"] if raw.startswith("{") else raw.splitlines()[0].strip()
acct = Account.from_mnemonic(phrase)

def rpc_call(method, params):
    body = json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params}).encode()
    req = urllib.request.Request(rpc, data=body, headers={"Content-Type":"application/json"})
    return json.load(urllib.request.urlopen(req, timeout=60))

chain_id = int(rpc_call("eth_chainId", [])["result"], 16)
nonce = int(rpc_call("eth_getTransactionCount", [acct.address, "latest"])["result"], 16)
gas_price = int(rpc_call("eth_gasPrice", [])["result"], 16)

signers = [
  "0xd876889fb236ebacdcf00e5007666d2094a5ada5",
  "0x5a3173a1d18049f9b19bdd15de9b28b671e0dc29",
  "0xaedfec3469ffe1fee0e0a54679270e183026b66e",
]
for i, to in enumerate(signers):
    tx = {
        "chainId": chain_id,
        "nonce": nonce + i,
        "to": to,
        "value": amount_wei,
        "gas": 21000,
        "gasPrice": gas_price,
    }
    signed = acct.sign_transaction(tx)
    hex_tx = "0x" + signed.raw_transaction.hex()
    h = rpc_call("eth_sendRawTransaction", [hex_tx])["result"]
    print(f"Funded {to} with {os.environ['AMOUNT_TKS']} TKS — tx {h}")
PY
