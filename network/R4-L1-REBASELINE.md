# R4 L1 load — re-baseline on regen mainnet

## Why old numbers do not apply

R4 throughput, loss, and pool metrics were measured on **pre–v0.1.17 genesis** (`0x104c48a9…`). Mainnet was **reset** to regen genesis **`0x28a554eb…`** with new validators, sudo multisig, and Docker layout. **Do not cite r4 L1 load figures for the current chain.**

## How to reproduce (operators)

1. Sync a node against [`tks-network-spec-raw.json`](./tks-network-spec-raw.json) (genesis hash above).
2. Fund a test account from an offline treasury key (never commit seeds).
3. Run the regression harness:

```bash
export TKS_RPC=https://rpc.tokenkickstarter.com   # or local 9944
python3 network/scripts/loadtest-20k-transfers.py \
  --rpc "$TKS_RPC" \
  --senders 10 \
  --per-sender 2000 \
  --funder-seed "<offline treasury URI>"
```

4. Record in your audit packet: block range, `eth_blockNumber` delta, failed vs submitted count, `txpool_status` if available, and **`system_version`** (must be ≥ **0.1.19** for anchor dedupe + multisig tooling).

## Smaller public smoke (no secrets)

Public RPC cannot run full 20k without a funded key. Use:

```bash
./network/scripts/public-rpc-test.sh
```

For investor-facing summary after a full run, append results to this file or link a dated gist; include **date**, **genesis hash**, and **node version**.
