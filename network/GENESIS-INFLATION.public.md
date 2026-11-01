# Supply, genesis holders, and inflation (public)

## Fixed genesis (regen `0x28a554…`)

| Bucket | Amount (TKS) | Public H160 | Control |
|--------|--------------|-------------|---------|
| Team | 150M | in operator `genesis-summary.json` | Multisig / company custody |
| Bridge | 300M | treasury file | Protocol bridge ops |
| Ecosystem | 150M | treasury file | Grants / partnerships |
| Treasury | 100M | treasury file | Multisig governance |
| Validators | 50M | treasury file | Staking rewards pool funding |
| Public sale | 150M | treasury file | Distribution program |
| Airdrop | 100M | treasury file | Campaigns |
| Multisig sudo | 1,000 | `0x8eeb…` | 2-of-3 offline signers |
| Validator stashes | 100k bond each | `0x02dd…`, `0x5ad4…` | Validator operators |
| Legacy Alice | **0** | `0xe04…625b` | Must remain zero on mainnet |

See [GENESIS-ALLOCATION.public.md](./GENESIS-ALLOCATION.public.md) for sudo signers.

## Inflation and minting

- **No inflation pallet** is enabled in runtime spec 412/413: new TKS appear only via **transfers**, **EVM minting contracts** (if deployed), **treasury spends**, or **failed/burned fee accounting**.
- **Day-over-day supply increases** (e.g. +259,779 TKS) should be reconciled with: block rewards (if any), treasury disbursements, test transfers, and bridge mints — publish a daily **supply audit** script output for investors.

## Validator operators (current deployment)

| Validator | Role | Operator (today) |
|-----------|------|------------------|
| V1 / V2 | Aura + GRANDPA | Same legal entity; two Docker containers on one VPS |

**Target:** ≥2 validators run by **independent** parties — track in [VALIDATORS.md](./VALIDATORS.md) when onboarded.
