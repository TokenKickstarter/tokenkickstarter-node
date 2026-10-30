# Genesis allocation (spec 412 — public addresses)

Genesis hash: `0x104c48a9dd32d624c3bbba81f644d8865b36cc33c156bcbc05ffe15c327600bc`

Full bucket table: [`GENESIS-BALANCES.md`](./GENESIS-BALANCES.md).

## Named treasury H160s (Keccak-derived)

These addresses are listed in the **operator** `genesis-summary.json` (not committed). After bootstrap, publish a redacted copy matching [`genesis-summary.public.example.json`](./genesis-summary.public.example.json) with your live treasury/airdrop H160s.

| Role | H160 (spec 412) | Control |
|------|-----------------|--------|
| Bridge treasury | `0x3c0a5cf8b8966fa669ebb9ffbf87b2837a4d17b2` (300M TKS) | `treasury-bridge` ECDSA in `production-keys/` |
| Team treasury | `0x58302df59249ff0cca72c5dc092be8b5a3ae2a9e` (150M) | `treasury-team` |
| Ecosystem | `0x5060b8cbac09e5c0d4cfabfa4b72ae1ee71e012d` (150M) | `treasury-ecosystem` |
| Public sale | `0x1799db0ea733197f612dc0f58ea1a29fed7d99b1` (150M) | `treasury-public_sale` |
| On-chain treasury | `0xe8cef7292fd8545c4d988678379034e436b982e8` (100M) | `treasury-treasury` |
| Validators pool | `0xcb3e5cb25d845898265c59eb7a9f2682584279a6` (50M) | `treasury-validators` |
| Airdrop | `0x1c6c37ecc401fbb308148ce45fa4fd890e076c58` (100M) | `treasury-airdrop` (operator-held) |
| Multisig sudo (genesis) | `0xd0ecdb4dd41fbe6863f4ca6c68874ae61198d489` | 2-of-3 `sudo-member{1,2,3}.json` (offline) |
| Validator 1 stash | `0x4b36c368acf0dca5fff3fd5c2733f5c6f5156c06` | `validator1-ecdsa.json` / VPS session keys |
| Validator 2 stash | `0xd7fb644dae59383c8e4ccac994b7691fb64baa08` | `validator2-ecdsa.json` |
| Legacy Alice (must be **0**) | `0xe04cc55ebee1cbce552f250e85c57b70b2e2625b` | Dev only — not funded on mainnet |

Full amounts: [GENESIS-BALANCES.md](./GENESIS-BALANCES.md). To prove control, operators publish signed messages or test transfers from each treasury file; keys stay in `network/production-keys/` (gitignored).

Audit: `python3 network/scripts/audit-genesis-addresses.py --spec network/tks-network-spec-raw.json`

Release artifacts (v0.1.14+): `tks-network-spec.json` / `tks-network-spec-raw.json` (Live **TKS Network**, production bootnodes — not `127.0.0.1` local testnet).
