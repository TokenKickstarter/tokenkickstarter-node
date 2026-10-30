# Genesis allocation (spec 412 — public addresses)

Genesis hash: `0x104c48a9dd32d624c3bbba81f644d8865b36cc33c156bcbc05ffe15c327600bc`

Full bucket table: [`GENESIS-BALANCES.md`](./GENESIS-BALANCES.md).

## Named treasury H160s (Keccak-derived)

These addresses are listed in the **operator** `genesis-summary.json` (not committed). After bootstrap, publish a redacted copy matching [`genesis-summary.public.example.json`](./genesis-summary.public.example.json) with your live treasury/airdrop H160s.

| Role | Example / note |
|------|----------------|
| Multisig sudo | `0xd0ecdb4dd41fbe6863f4ca6c68874ae61198d489` |
| Validator 1 stash | `0x4b36c368acf0dca5fff3fd5c2733f5c6f5156c06` |
| Validator 2 stash | `0xd7fb644dae59383c8e4ccac994b7691fb64baa08` |
| Treasury airdrop | `0x1c6c37ecc401fbb308148ce45fa4fd890e076c58` |
| Legacy Alice (must be **0**) | `0xe04cc55ebee1cbce552f250e85c57b70b2e2625b` |

Audit: `python3 network/scripts/audit-genesis-addresses.py --spec network/tks-network-spec-raw.json`

Release artifacts (v0.1.14+): `tks-network-spec.json` / `tks-network-spec-raw.json` (Live **TKS Network**, production bootnodes — not `127.0.0.1` local testnet).
