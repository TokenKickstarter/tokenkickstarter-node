#!/usr/bin/env python3
"""Flag production keys whose on-chain genesis H160 used the truncated-pubkey bug."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NODE = ROOT / "target" / "release" / "tks-chain-node"
KEYS = ROOT / "network" / "production-keys"


def run_node(*args: str) -> str:
    out = subprocess.check_output([str(NODE), *args], text=True).strip()
    return out.splitlines()[-1].strip()


def eth_h160_from_phrase(phrase: str) -> str:
    from eth_utils import keccak  # pip install eth-utils

    inspect = subprocess.check_output(
        [str(NODE), "key", "inspect", "--scheme", "ecdsa", f"//{phrase}"],
        text=True,
    )
    seed_line = next(l for l in inspect.splitlines() if "Secret seed:" in l)
    seed = bytes.fromhex(seed_line.split()[-1].removeprefix("0x"))
    # secp256k1: derive uncompressed pubkey from secret (same as Frontier)
    import coincurve  # pip install coincurve

    pk = coincurve.PrivateKey(seed)
    uncompressed = pk.public_key.format(compressed=False)  # 65 bytes, 0x04 prefix
    return "0x" + keccak(uncompressed)[-20:].hex()


def load_phrase(path: Path) -> str:
    return json.loads(path.read_text())["secretPhrase"]


def main() -> None:
    if not NODE.is_file():
        sys.exit(f"Build node first: cargo build --release -p tks-chain-node ({NODE})")

    rows: list[tuple[str, str, str, str]] = []
    for path in sorted(KEYS.glob("*.json")):
        if path.name == "genesis-summary.json":
            continue
        try:
            phrase = load_phrase(path)
        except (KeyError, json.JSONDecodeError):
            continue
        on_chain = run_node("production", "account-id20", phrase)
        correct = eth_h160_from_phrase(phrase)
        status = "OK" if on_chain.lower() == correct.lower() else "MISMATCH"
        rows.append((path.name, on_chain, correct, status))

    print(f"{'file':<28} {'node account-id20':<44} {'ethereum H160':<44} status")
    print("-" * 120)
    for row in rows:
        print(f"{row[0]:<28} {row[1]:<44} {row[2]:<44} {row[3]}")

    mism = sum(1 for r in rows if r[3] == "MISMATCH")
    print(f"\n{len(rows)} key files, {mism} mismatched (0 expected after chain_spec fix + regen).")


if __name__ == "__main__":
    main()
