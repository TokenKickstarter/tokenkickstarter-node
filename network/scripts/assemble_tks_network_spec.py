#!/usr/bin/env python3
"""Assemble Live `TKS Network` chain spec JSON from offline keys + allocation input."""

from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
from pathlib import Path

TKS = 10**18
STAKING_BOND = 100_000 * TKS


def run_node(node: Path, *args: str) -> str:
    out = subprocess.check_output([str(node), *args], text=True).strip()
    return out.splitlines()[-1].strip()


def h160_from_phrase(node: Path, phrase: str) -> str:
    return run_node(node, "production", "account-id20", phrase)


def load_json_key(path: Path) -> dict:
    return json.loads(path.read_text())


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--node", type=Path, required=True)
    ap.add_argument("--keys-dir", type=Path, required=True)
    ap.add_argument("--input", type=Path, required=True)
    ap.add_argument("--template", type=Path, required=True)
    ap.add_argument("--output", type=Path, required=True)
    args = ap.parse_args()

    cfg = json.loads(args.input.read_text())
    keys = args.keys_dir

    v1_aura = load_json_key(keys / "validator1-aura.json")
    v1_gran = load_json_key(keys / "validator1-grandpa.json")
    v1_ecdsa = load_json_key(keys / "validator1-ecdsa.json")
    v2_aura = load_json_key(keys / "validator2-aura.json")
    v2_gran = load_json_key(keys / "validator2-grandpa.json")
    v2_ecdsa = load_json_key(keys / "validator2-ecdsa.json")

    v1_stash = h160_from_phrase(args.node, v1_ecdsa["secretPhrase"])
    v2_stash = h160_from_phrase(args.node, v2_ecdsa["secretPhrase"])
    v1_cipher = v1_stash
    v2_cipher = v2_stash

    sudo_members = []
    for i in range(1, int(cfg.get("sudo_multisig_members", 3)) + 1):
        p = keys / f"sudo-member{i}.json"
        if not p.exists():
            raise SystemExit(f"missing {p} — re-run bootstrap-production-genesis.sh")
        m = load_json_key(p)
        sudo_members.append(h160_from_phrase(args.node, m["secretPhrase"]))

    threshold = int(cfg["sudo_multisig_threshold"])
    sudo_addr = run_node(
        args.node,
        "production",
        "multisig-account",
        "--threshold",
        str(threshold),
        "--signatories",
        ",".join(sudo_members),
    )

    endpoints = cfg.get("shard_endpoints") or [
        "https://shard.tokenkickstarter.com",
        "https://shard.tokenkickstarter.com",
    ]

    balances: dict[str, int] = {}
    for alloc in cfg["allocations"]:
        amt = int(alloc["amount_tks"]) * TKS
        addr = alloc.get("address")
        if addr:
            balances[addr.lower()] = balances.get(addr.lower(), 0) + amt
        else:
            label = alloc["label"]
            p = keys / f"treasury-{label}.json"
            if not p.exists():
                raise SystemExit(f"missing treasury key {p}")
            t = load_json_key(p)
            addr = h160_from_phrase(args.node, t["secretPhrase"])
            balances[addr] = balances.get(addr, 0) + amt

    for stash in (v1_stash, v2_stash):
        balances[stash] = balances.get(stash, 0) + STAKING_BOND + 50_000 * TKS

    balances[sudo_addr.lower()] = balances.get(sudo_addr.lower(), 0) + 1_000 * TKS

    balance_list = [[addr, amount] for addr, amount in sorted(balances.items())]

    session_keys = [
        [v1_stash, v1_stash, {"aura": v1_aura["ss58Address"], "grandpa": v1_gran["ss58Address"]}],
        [v2_stash, v2_stash, {"aura": v2_aura["ss58Address"], "grandpa": v2_gran["ss58Address"]}],
    ]

    stakers = [
        [v1_stash, v1_stash, STAKING_BOND, "Validator"],
        [v2_stash, v2_stash, STAKING_BOND, "Validator"],
    ]

    shard_validators = [
        [v1_cipher, "NorthAmerica", list(endpoints[0].encode("utf-8"))],
        [v2_cipher, "Europe", list(endpoints[1 if len(endpoints) > 1 else 0].encode("utf-8"))],
    ]

    patch = {
        "balances": {"balances": balance_list},
        "session": {"keys": session_keys},
        "staking": {
            "validatorCount": 2,
            "minimumValidatorCount": 1,
            "stakers": stakers,
            "invulnerables": [v1_stash, v2_stash],
            "forceEra": "NotForcing",
            "minValidatorBond": STAKING_BOND,
            "minNominatorBond": 1_000 * TKS,
            "slashRewardFraction": 100_000_000,
        },
        "sudo": {"key": sudo_addr},
        "shardRegistry": {
            "initialShardCount": int(cfg.get("initial_shard_count", 8)),
            "validatorsPerShard": int(cfg.get("validators_per_shard", 7)),
            "validators": shard_validators,
            "requireStakedValidators": bool(cfg.get("require_staked_validators", True)),
        },
        "evmChainId": {"chainId": 7779},
        "baseFee": {"baseFeePerGas": "0x3B9ACA00"},
        "evm": {"accounts": {}},
        "nameRegistry": {"names": []},
    }

    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tmp:
        base_path = Path(tmp.name)
    with open(base_path, "w") as base_out:
        subprocess.check_call(
            [str(args.node), "build-spec", "--chain", "local"],
            stdout=base_out,
            stderr=subprocess.DEVNULL,
        )
    spec = json.loads(base_path.read_text())
    base_path.unlink(missing_ok=True)

    spec["name"] = "TKS Network"
    spec["id"] = "tks_network"
    spec["chainType"] = "Live"
    spec["protocolId"] = "tks-network"
    spec["bootNodes"] = cfg.get("boot_nodes") or []

    runtime_genesis = spec.setdefault("genesis", {}).setdefault("runtimeGenesis", {})
    base_patch = runtime_genesis.get("patch", {})
    for key, val in patch.items():
        base_patch[key] = val
    runtime_genesis["patch"] = base_patch

    args.output.write_text(json.dumps(spec, indent=2) + "\n")

    summary = {
        "genesis_sudo_multisig": sudo_addr,
        "sudo_threshold": threshold,
        "sudo_members": sudo_members,
        "validator_stash": [v1_stash, v2_stash],
        "cipher_accounts": [v1_cipher, v2_cipher],
        "shard_validators_at_genesis": len(shard_validators),
    }
    (keys / "genesis-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
