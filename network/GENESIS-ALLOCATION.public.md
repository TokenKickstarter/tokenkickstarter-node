# Genesis allocation (v0.1.17 regen — public addresses)

**New mainnet genesis** shipped in release **v0.1.17** (`tks-network-spec-raw.json` + `.sha256`).

Genesis hash: **`0x28a554eb58356946b3ac7ce61fa1562cbda2dd1171ee8c7719140e0e33a2e31c`**

SHA-256 (raw spec): **`d83bc9d26c46561586b59821633559f92cadfcefc489eb64d7832a00b252a49e`**

Full bucket table: [`GENESIS-BALANCES.md`](./GENESIS-BALANCES.md).

## Core roles (v0.1.17 regen)

| Role | H160 | Control |
|------|------|--------|
| Multisig sudo (pallet-derived) | `0x8eeb8be59ccb1c65211ab787e6ba174753a68f1a` | 2-of-3 `sudo-member{1,2,3}.json` (offline) |
| Sudo signer 1 | `0x5b5478f3929645a96b17712a3b1743eded5d4ad7` | `sudo-member1.json` |
| Sudo signer 2 | `0x00373382c01459df2801f1f510926bc6acf9ddf7` | `sudo-member2.json` |
| Sudo signer 3 | `0x635ed8d610027d2df9398867e3da48db54d12f49` | `sudo-member3.json` |
| Validator 1 stash / cipher | `0x02dd89492248adf97484f3e55b1498e969e87632` | `validator1-ecdsa.json` + session keys |
| Validator 2 stash / cipher | `0x5ad443fa2255e47f737cbd0c93d2166638dc8b6e` | `validator2-ecdsa.json` + session keys |
| Legacy Alice (must be **0**) | `0xe04cc55ebee1cbce552f250e85c57b70b2e2625b` | Not funded |

Treasury bucket H160s: in operator `production-keys/genesis-summary.json` after `assemble-tks-network-spec.sh` (not committed).

Audit: `python3 network/scripts/audit-genesis-addresses.py --spec network/tks-network-spec-raw.json`

**Previous spec 412** (hash `0x104c48a9…`) is superseded by this regen — all nodes must purge and sync from block 0.
