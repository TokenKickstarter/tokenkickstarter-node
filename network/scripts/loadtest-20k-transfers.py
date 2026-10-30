#!/usr/bin/env python3
"""Regression: 20k EVM transfers (10 senders × 2k) — every tx must reach chain."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import threading
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed

try:
    from eth_account import Account
    from eth_utils import to_checksum_address
except ImportError:
    print("pip install eth-account eth-utils", file=sys.stderr)
    raise


def rpc(url: str, method: str, params: list) -> dict:
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        out = json.load(resp)
    if "error" in out:
        raise RuntimeError(out["error"])
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rpc", default=os.environ.get("TKS_RPC", "http://127.0.0.1:9944"))
    ap.add_argument("--senders", type=int, default=10)
    ap.add_argument("--per-sender", type=int, default=2000)
    ap.add_argument("--value-wei", type=int, default=1)
    ap.add_argument("--funder-seed", required=True, help="hex secret seed or //URI for funded account")
    ap.add_argument("--node-bin", default="tks-chain-node")
    args = ap.parse_args()

    total = args.senders * args.per_sender
    chain_id = int(rpc(args.rpc, "eth_chainId", [])["result"], 16)
    gas_price = int(rpc(args.rpc, "eth_gasPrice", [])["result"], 16)

    # Derive funder via node (Keccak H160)
    funder_h160 = subprocess.check_output(
        [args.node_bin, "production", "account-id20", args.funder_seed],
        text=True,
    ).strip().splitlines()[-1].strip()
    funder_priv = subprocess.check_output(
        [args.node_bin, "key", "inspect", "--scheme", "ecdsa", args.funder_seed],
        text=True,
        stderr=subprocess.DEVNULL,
    )
    priv = None
    for line in funder_priv.splitlines():
        if "Secret seed:" in line:
            priv = line.split()[-1].removeprefix("0x")
            break
    if not priv:
        raise SystemExit("could not read funder secret seed from node key inspect")

    funder_cs = to_checksum_address(funder_h160)
    base_nonce = int(rpc(args.rpc, "eth_getTransactionCount", [funder_cs, "latest"])["result"], 16)

    senders: list[tuple[Account, str]] = []
    for i in range(args.senders):
        acct = Account.create()
        senders.append((acct, acct.address))

    tx_hashes: list[str] = []
    lock = threading.Lock()

    def fund_sender(to_addr: str, nonce: int) -> str:
        tx = {
            "nonce": nonce,
            "to": to_checksum_address(to_addr),
            "value": 10**17,
            "gas": 21000,
            "gasPrice": gas_price,
            "chainId": chain_id,
        }
        signed = Account.sign_transaction(tx, priv)
        raw = "0x" + signed.raw_transaction.hex()
        res = rpc(args.rpc, "eth_sendRawTransaction", [raw])["result"]
        with lock:
            tx_hashes.append(res)
        return res

    print(f">> Funding {args.senders} senders from {funder_cs}...")
    with ThreadPoolExecutor(max_workers=args.senders) as ex:
        futs = [ex.submit(fund_sender, addr, base_nonce + i) for i, (_, addr) in enumerate(senders)]
        for f in as_completed(futs):
            f.result()

    time.sleep(8)
    for i, (acct, _) in enumerate(senders):
        bal = int(rpc(args.rpc, "eth_getBalance", [acct.address, "latest"])["result"], 16)
        if bal == 0:
            raise SystemExit(f"sender {i} not funded")

    def send_from_sender(idx: int) -> list[str]:
        acct, _ = senders[idx]
        recipient = Account.create().address
        out = []
        nonce = 0
        for _ in range(args.per_sender):
            tx = {
                "nonce": nonce,
                "to": to_checksum_address(recipient),
                "value": args.value_wei,
                "gas": 21000,
                "gasPrice": gas_price,
                "chainId": chain_id,
            }
            signed = Account.sign_transaction(tx, acct.key)
            raw = "0x" + signed.raw_transaction.hex()
            h = rpc(args.rpc, "eth_sendRawTransaction", [raw])["result"]
            out.append(h)
            nonce += 1
        return out

    print(f">> Burst {total} transfers...")
    t0 = time.time()
    with ThreadPoolExecutor(max_workers=args.senders) as ex:
        futs = [ex.submit(send_from_sender, i) for i in range(args.senders)]
        for f in as_completed(futs):
            with lock:
                tx_hashes.extend(f.result())
    print(f"   submitted {len(tx_hashes)} in {time.time()-t0:.1f}s")

    print(">> Waiting for inclusion...")
    pending = set(tx_hashes)
    deadline = time.time() + 600
    while pending and time.time() < deadline:
        done = []
        for h in list(pending):
            rec = rpc(args.rpc, "eth_getTransactionReceipt", [h])["result"]
            if rec is not None:
                done.append(h)
        for h in done:
            pending.discard(h)
        if pending:
            time.sleep(3)
            print(f"   pending {len(pending)}...")

    if pending:
        print(f"FAIL missing {len(pending)} receipts (first: {next(iter(pending))})", file=sys.stderr)
        sys.exit(1)

    print(f"PASS all {total} transfers included ({len(tx_hashes)} total txs with funding)")


if __name__ == "__main__":
    main()
