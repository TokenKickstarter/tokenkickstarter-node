# TKS `@username` registration throughput (L1)

## What you are measuring

| Metric | Layer | Hybrid DAG helps? |
|--------|--------|-------------------|
| **Global `@username` uniqueness** | L1 `pallet-name-registry` | **No** — one world-wide map (`Names`, `ReverseLookup`) |
| **Cipher message / ciphertext TPS** | Relay + shard executor + DA | **Yes** — parallel shards (~62.5k/shard target in spec) |
| **Trust / anchors** | L1 `pallet-hyperswarm-anchor` | Batches many shard roots; not per-message |

Registering `@alice` on-chain is an **L1 consensus write**. The Hybrid DAG stack increases **messaging** throughput; it does **not** split global name uniqueness across shards without a new design (e.g. deferred Merkle commit, L2 name auction, or shard-local aliases + L1 settlement).

## Theoretical L1 ceiling (weight math)

Runtime (spec **408**):

- Block time: **1 s**
- Normal dispatch budget: **~75%** of block weight (~**5.25×10¹²** ref-time units on the 700 ms compute budget)
- Single claim: `register_unsigned` ≈ **50×10⁶** weight (+ DB)
- Batch claim: `register_batch_unsigned` ≈ **20×10⁶ + 45×10⁶ × N** (+ DB per entry), **N ≤ 128**

Rough **weight-only** upper bound (name extrinsics filling the normal ratio):

| Mode | Names per block (max) | Sustained names/s (1 s blocks) |
|------|------------------------|--------------------------------|
| Single (`register_unsigned`) | ~100k+ | Same order (weight is conservative vs CPU) |
| Batch N=128 | ~1.3k batches → ~**166k** names | ~**166k** |

Real nodes are limited by **ECDSA recover per claim**, **tx pool**, **block packing**, **P2P**, and **validator CPU**. Expect **lower** numbers until load-tested on your VPS fleet.

## What we shipped for max practical TPS (spec 408)

1. **`register_batch_unsigned`** — up to **128** signed claims per extrinsic; one block can include many batches.
2. **`ValidateUnsigned`** for batches — per-entry `provides` tags; priority **120**; longevity **64** blocks.
3. **Same per-name signature** as single register: `keccak256(name || "register")`, secp256k1 recover → H160 must match `owner`.

Clients should **prefer batching** (e.g. 64–128 names per extrinsic) for registration storms (app launch, airdrop identity linking).

## Hybrid DAG + names (recommended product split)

```
User registers @name  ──►  L1 NameRegistry (global, lower TPS, authoritative)
User sends messages  ──►  Cipher relay → shard executor → DA → periodic L1 anchor
```

Do **not** quote **500k adaptive TPS** as **@username TPS**; quote **messaging TPS** for Cipher and **names/s** for L1 (measure separately).

## Load test checklist

1. Runtime upgrade to **spec_version ≥ 408** on `rpc.tokenkickstarter.com`.
2. Build batches off-chain; submit via `author_submitExtrinsic` (unsigned batch).
3. Tune node: `--pool-limit` / `--pool-kbytes` if the pool drops under burst.
4. Report: **included names/s**, **p50/p95 inclusion latency**, **% dropped (Stale)**.

## Future (not in 408)

- Off-chain queue + relayer service that packs batches (no consensus change).
- Merkle root commit extrinsic (one L1 tx for millions of pre-reserved names) — requires economic + fraud-proof design.
- Shorter blocks or dedicated “name registration” block space via runtime config.
