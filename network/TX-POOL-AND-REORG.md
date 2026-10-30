# Transaction pool under burst load and reorgs

## What happened (audit v0.1.13)

- 20,000 EVM transfers accepted by RPC; ~1,966 never mined (~9.8%).
- Losses concentrated on **one sender** (~846 included) while others completed.
- Under load, **peers dropped 2 → 0** and blocks **1048 / 1069** were **reorganized**.

## Root cause (node)

1. **Future queue too small.** With `--pool-limit 50000`, Substrate still sets `future.count = pool_limit / 10` (**5,000**). A burst of **10 × 2,000** nonce-sequenced transfers needs ~**20,000** future slots. Extra submissions are **`ImmediatelyDropped`** even though RPC returns a hash from a shallow accept path on the gateway.
2. **Reorg + peer loss.** With only two validators, GRANDPA can briefly disagree on best head when the mempool and block import fall behind. Evicted or fork-orphaned transactions are **re-queued only if still in pool**; dropped futures are **not** recovered automatically.

## v0.1.14 mitigations

| Change | Effect |
|--------|--------|
| `--pool-future-divisor 10` (default) | Scales ready/future caps so **future ≈ nominal `--pool-limit`** (50k → 500k internal ready, 50k future). |
| `--pool-type fork-aware` (explicit in compose) | Fork-aware pool **re-submits** transactions from retracted blocks when possible ([Substrate pool tests](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/client/transaction-pool/tests)). |
| Validator **reserved peers** + higher `--in-peers` / `--out-peers` | Reduces peer count collapse under load. |
| `network/scripts/loadtest-20k-transfers.py` | Regression: assert **100% inclusion** against a dev or staging RPC. |

## Operator checklist

- Fullnode + validators: `--pool-limit 50000 --pool-kbytes 65536 --pool-future-divisor 10 --pool-type fork-aware`
- Validators: `--reserved-nodes` for each other + fullnode (see `network/docker-compose-live.yml`)
- Load clients: retry on `eth_getTransactionReceipt` timeout; do not fire unbounded parallel nonces without confirmation on production until post-audit pass
- Hardware: ≥8 vCPU / 16 GiB RAM per validator container; no Docker memory cap below 12 GiB

## Per-account limits

There is **no** separate Substrate CLI flag for per-account pool count. Limits are global ready/future buckets. One saturated sender can evict **lowest-priority future** txs globally during pressure — another reason to size the future queue for peak `(senders × in-flight nonces)`.
