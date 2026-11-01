#!/usr/bin/env python3
"""List pending pallet-multisig call hashes for an H160 (public RPC, no secrets)."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request

from hashlib import blake2b

try:
    import xxhash
except ImportError:
    print("Install: pip install xxhash", file=sys.stderr)
    raise


def twox128(data: bytes) -> bytes:
    return xxhash.xxh64(data, seed=0).digest() + xxhash.xxh64(data, seed=1).digest()


def blake128_concat(data: bytes) -> bytes:
    h = blake2b(data, digest_size=16).digest()
    return h + data


def rpc(url: str, method: str, params: list) -> object:
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        out = json.load(resp)
    if "error" in out:
        raise RuntimeError(out["error"])
    return out["result"]


def account20_bytes(hex_addr: str) -> bytes:
    h = hex_addr.lower().removeprefix("0x")
    return bytes.fromhex(h)


def multisigs_prefix(multisig: bytes) -> bytes:
    key = twox128(b"Multisig") + twox128(b"Multisigs") + blake128_concat(multisig)
    return key


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rpc", default="https://rpc.tokenkickstarter.com")
    ap.add_argument("--multisig", required=True)
    args = ap.parse_args()

    acc = account20_bytes(args.multisig)
    # SCALE AccountId20: 20 bytes raw in storage keys for this runtime
    prefix = multisigs_prefix(acc)
    hex_prefix = "0x" + prefix.hex()
    keys = rpc(args.rpc, "state_getKeys", [hex_prefix])
    if not keys:
        print("No pending multisig calls.", file=sys.stderr)
        return
    for k in keys:
        kh = k.removeprefix("0x")
        call_hash = kh[-64:]
        print(f"0x{call_hash}")


if __name__ == "__main__":
    main()
