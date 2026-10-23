# TKS performance: what we can claim vs what to build

## Your summary is mostly right — one correction

| Claim | Verdict |
|--------|---------|
| L1 mempool tuning → ~**3,000 TPS** on one chain | **Plausible** after adaptive gas fix + pool/proposer tuning + hardware (gas ceiling ~3,571 at 75M / 21k gas / 1s) |
| **16 × 25,000 = 400,000 TPS** | **Only if** each shard executes **25k confirmed signed ops/sec** with DA + anchors + dispute path — not local Merkle hashing |
| Cipher relay **20k–50k msg/s** | Off-chain messaging; **not** L1 TPS |
| Adaptive gas **75M → 4B** | Requires **spec 404+** runtime (`AdaptiveGasOnCharge` records EVM gas) and sustained block fill |

## Phase A — L1 (this repo, deploy to VPS)

1. Build node with **spec_version 404** WASM and schedule `set_code` (or chain restart on testnet).
2. Redeploy Docker with `network/docker-compose-live.yml`:
   - `--rpc-methods Safe` on the public-facing fullnode
   - `--pool-limit 50000 --pool-kbytes 65536` on validators + fullnode
3. Nginx: proxy only `9944`; block `author_*`, `system_add*`, `offchain_*`, `eth_sendTransaction` if you use hot keys on the node.
4. Validators: **8+ CPU**, NVMe, 1 Gbps+; reserved peers between the three containers.
5. Re-run the Oct-24 load harness; expect gas limit to **rise** under load and `GasLimitAdjusted` events on-chain.

## Phase B — 400k–500k (not in one node binary)

Per `runtime/src/lib.rs` Phase 3: **~8 shards × ~62.5k TPS** (or ~16 × 25k with the same math).

Each shard needs:

- HyperSwarm (or dedicated) **executor** process with real state
- Validators in `ShardRegistry`
- Periodic `HyperSwarmAnchor::post_anchor` + retrievable batch data
- Fraud/validity checks before investor-facing numbers

Aggregate TPS = **sum of independently measured shard confirmed TPS**, reproduced by a third party.

## Security (before mainnet demo)

Same as performance report §8: rotate sudo/dev key, replace Alice/Bob session keys, safe RPC, rename chain spec from "TKS Local Testnet".
