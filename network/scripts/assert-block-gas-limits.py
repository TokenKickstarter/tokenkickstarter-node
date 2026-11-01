#!/usr/bin/env python3
"""Assert eth_getBlockByNumber gasUsed <= gasLimit for a block range (r6 audit harness)."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request


def rpc(url: str, method: str, params: list) -> object:
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        out = json.load(resp)
    if "error" in out:
        raise RuntimeError(out["error"])
    return out["result"]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rpc", default="http://127.0.0.1:9944")
    ap.add_argument("--from-block", type=int, required=True)
    ap.add_argument("--to-block", type=int, required=True)
    args = ap.parse_args()

    violations = []
    for n in range(args.from_block, args.to_block + 1):
        block = rpc(args.rpc, "eth_getBlockByNumber", [hex(n), False])
        if not block:
            continue
        used = int(block.get("gasUsed", "0x0"), 16)
        limit = int(block.get("gasLimit", "0x0"), 16)
        if used > limit:
            violations.append((n, used, limit, block.get("hash")))

    if violations:
        print(f"FAIL {len(violations)} block(s) with gasUsed > gasLimit:", file=sys.stderr)
        for n, used, limit, h in violations[:20]:
            print(f"  block {n}: used={used} limit={limit} hash={h}", file=sys.stderr)
        sys.exit(1)

    print(
        f"PASS gasUsed <= gasLimit for blocks {args.from_block}..{args.to_block} "
        f"({args.to_block - args.from_block + 1} checked)"
    )


if __name__ == "__main__":
    main()
