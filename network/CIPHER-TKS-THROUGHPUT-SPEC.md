# Cipher × TKS — Throughput & Latency Engineering Spec

**Product:** [Cipher](https://cipher.llc) messaging on TokenKickstarter (TKS) Layer 1  
**Goal:** Up to **~500k adaptive messages/sec** aggregate with **low millisecond** regional delivery  
**Status:** Engineering contract (v1) — aligns with `cipher-relay`, `pallet-shard-registry`, `pallet-hyperswarm-anchor`, `pallet-adaptive-gas`  
**Measured L1 today (v0.1.1):** ~270–420 confirmed EVM TPS; sharding **not live**

---

## 1. Problem statement

Cipher needs **high message throughput** and **ms-scale delivery**. TKS L1 (Aura ~1s blocks, GRANDPA finality) is the **trust and settlement layer**, not the chat hot path.

Putting every Cipher message on L1 as an EVM transaction would:

- Cap throughput at **hundreds–low thousands TPS** (measured),
- Add **seconds** of latency (block interval + finality),
- Make a **500k TPS** claim false.

This spec defines **two planes**, how **“adaptive 500k TPS”** is defined and measured, and the **build order** to get there without repeating off-chain hash benchmarks.

---

## 2. Architecture: data plane vs trust plane

```
┌─────────────────────────────────────────────────────────────────┐
│  Cipher clients (mobile / desktop / web)                         │
└────────────────────────────┬────────────────────────────────────┘
                             │
         ┌───────────────────┴───────────────────┐
         │  DATA PLANE (latency-critical)          │
         │  • Embedded cipher-relay (:4002)         │
         │  • HyperSwarm / libp2p shard mesh        │
         │  • Regional shard executors (future)     │
         │  Target: delivery p99 < 100ms (region)   │
         │  Target: 25k–62.5k msg/s per shard       │
         └───────────────────┬───────────────────┘
                             │ batch roots + DA blobs
         ┌───────────────────┴───────────────────┐
         │  TRUST PLANE (TKS L1)                   │
         │  • pallet-name-registry (identity)       │
         │  • pallet-shard-registry (operators)     │
         │  • pallet-hyperswarm-anchor (roots)      │
         │  • pallet-adaptive-gas (L1 headroom)     │
         │  • EVM (payments, contracts) — sparse     │
         └─────────────────────────────────────────┘
```

| Plane | Components in repo | Responsibility |
|-------|-------------------|----------------|
| **Data** | `cipher-relay/`, node `--enable-cipher-relay` | Encrypt, route, store, notify; **no block wait** |
| **Data (next)** | `cipher-shard-executor` (to build) | Ordered batches, BFT within shard, DA publish |
| **Trust** | `pallet-shard-registry`, `pallet-hyperswarm-anchor` | Who may serve shards; **32-byte roots** + counters on L1 |
| **Trust** | `pallet-adaptive-gas` | L1 block gas scales with **L1** utilization (anchors, EVM), not per chat message |

**Default message path:** Client → **relay/shard** → recipient. L1 is **not** in the delivery critical path.

---

## 3. Definitions (use in docs, investors, and benchmarks)

### 3.1 Cipher adaptive aggregate TPS

```
Cipher_TPS = Σ (confirmed deliveries on shard i over measurement window)
```

A **confirmed delivery** means:

1. Message was **signed** by sender (Cipher protocol key or EVM-linked identity),
2. **Accepted** by the assigned shard/relay and **delivered** to recipient (or stored for fetch + ack),
3. Included in a **batch** whose `dag_root_hash` is posted via `HyperSwarmAnchor::post_anchor`, and
4. Batch **payload is retrievable** from DA (see §5) for the anchor window.

**Adaptive** means measured over a window where load varies and **capacity scales** by:

- Adding shard executors / relay nodes (registry + ops), and/or
- Increasing batch frequency within SLOs,

not by multiplying off-chain hashes without (1)–(4).

### 3.2 TKS L1 TPS

Separate metric: **EVM/substrate extrinsics included in L1 blocks** (transfers, contracts, registry, anchors). Report **always separately** from Cipher_TPS.

### 3.3 Latency

| Metric | Definition | Target (production SLO) |
|--------|------------|-------------------------|
| **Delivery p50** | Send API ack → recipient `fetch`/push | **< 50 ms** same region |
| **Delivery p99** | Same | **< 100 ms** same region |
| **Anchor visibility** | Last msg in batch → `AnchorPosted` event | **≤ 15 s** (see §4) |
| **Economic finality** | Payment on L1 | GRANDPA lag (2–5 blocks acceptable) |

Chat **must not** wait for GRANDPA or 1s Aura slots.

---

## 4. Anchor cadence & L1 load

From `pallet-hyperswarm-anchor` design:

- Shard leaders call **`post_anchor(shard_id, dag_root_hash, message_count, epoch)`** (~every **10 s** per shard target).
- Anchors are **free** (`Pays::No`); footprint ~32 bytes + metadata per batch.

**Capacity check (root chain):**

| Shards | Anchors / 10 s | ~Anchors / 1 s block |
|--------|----------------|----------------------|
| 8 | 8 | ~0.8 |
| 16 | 16 | ~1.6 |
| 1024 (max config) | 1024 | ~100 (pallet comment — requires L1 headroom) |

At **8–16 shards**, anchor traffic is negligible vs L1 block space. **`pallet-adaptive-gas`** matters when anchor bursts coincide with heavy EVM load — it keeps L1 blocks from sticking at the 75M floor under combined load.

**Batch rules (shard executor MUST implement):**

- `message_count` = number of **real** messages in the batch (not zero; enforced on-chain).
- `dag_root_hash` = Merkle root over **canonical batch encoding** (specify in executor v1: sorted `(msg_id, sender, recipient, ciphertext_hash)` leaves).
- DA blob referenced by root must be **published before** `post_anchor` (order enforced by challengers).

---

## 5. Data availability (DA)

An anchor is **invalid for throughput claims** unless DA is satisfied.

**Minimum v1 DA:**

- Blob stored on **shard validators + 1 public mirror** (S3/IPFS/another relay), hash = `dag_root_hash`.
- HTTP(S) URL template registered in anchor metadata (future extrinsic) or off-chain index keyed by `(shard_id, epoch, dag_root_hash)`.
- Benchmark harness **fetches blob** and verifies Merkle root against message list.

**Challenge path:** `submit_challenge(shard_id, anchor_hash)` within `ChallengeWindow` (7 days in runtime). Until resolution exists in software, label throughput **“pre-dispute”** in public materials.

---

## 6. Scaling math → 400k–500k

| Model | Shards | Per-shard sustained | Aggregate |
|-------|--------|---------------------|-----------|
| Runtime Phase 3 (comment in `runtime/src/lib.rs`) | 8 | ~62,500 msg/s | **~500k** |
| Alternative | 16 | ~25,000 msg/s | **~400k** |

**Per-shard sustained** must be measured with **distributed clients**, not one RPC machine.

**Regional sharding (`GeoRegion` in `pallet-shard-registry`):**

- Route user → nearest shard by region (Asia, Europe, NorthAmerica, …).
- Keeps RTT low for Cipher **data plane**; cross-region = higher p99, not L1 block time.

---

## 7. Node integration (today vs target)

| Feature | Today (v0.1.1) | Target |
|---------|----------------|--------|
| `cipher-relay` on :4002 | In-process with `tks-chain-node` | Hardened + horizontal scale behind load balancer |
| Shard executor | **Not shipped** | Binary per region; reads `ShardRegistry` |
| `register_validator` | Open (no stake check) | Staked validators only |
| `post_anchor` | Open (no shard assignment check) | Caller ∈ shard validator set |
| Adaptive gas | Wired (spec ≥ 406) | Visible limit growth under L1 load tests |

**Cipher.llc in the TKS node** = ship **relay + executor + L1 pallets** together; marketing **500k** only when **§8 benchmark** passes on testnet.

---

## 8. Benchmark harness (required before public 500k claim)

Open-source harness (repo path TBD: `tools/cipher-loadtest/`):

**Phases:**

1. **Relay-only** — `POST /store`, `POST /fetch`, `POST /ack`; measure msg/s and p50/p99 (no L1).
2. **Shard + anchor** — N clients → shard executor → `post_anchor` on L1; verify DA + counters (`TotalMessageCount`, `ShardMessageCount`).
3. **Third-party replay** — Independent operator runs same config on public testnet; results within **±10%** of claimed aggregate.

**Outputs (JSON):**

- `cipher_load_results.json` — batches, included, drops, wall TPS, per-shard TPS
- `latency_histogram.json` — p50/p95/p99 delivery ms
- `anchors.json` — block numbers, `message_count` sum vs delivered
- `readonly.json` — L1 TPS separately (EVM transfers optional control)

**Invalid runs (do not publish):**

- Merkle root over random strings with no network delivery
- `message_count` without DA blob
- Single client only (note as lower bound, not peak capacity)

---

## 9. Implementation milestones

| Milestone | Deliverable | Exit criteria |
|-----------|-------------|---------------|
| **M0 Security** | Key rotation, ≥4 validators, safe RPC | No Alice/Bob sudo; see performance report §4 |
| **M1 Relay SLO** | Relay hardening + metrics | 20k+ msg/s single region, p99 < 100 ms lab |
| **M2 One shard** | `cipher-shard-executor` v0 (**in repo**) | 1 shard registered; anchors on L1; DA verify |
| **M3 Measure shard** | Public testnet + harness | **≥25k sustained** one shard, third-party replay |
| **M4 Scale out** | 8–16 shards | **≥400k aggregate** adaptive window; document shard count |
| **M5 Disputes** | Challenge resolution | Invalid anchor test causes slash/offboard |

**L1 tuning** (pool, hardware, adaptive gas toward ~3k EVM TPS) runs **in parallel** with M1–M2; it does **not** replace M3–M4.

---

## 10. External messaging (approved wording)

**OK when M3+ partially met:**

> Cipher messaging on TKS uses a **regional shard and relay data plane** for millisecond delivery, with **TKS Layer 1** providing identity, validator registry, and **batched cryptographic anchors**. Measured aggregate throughput is **X msg/s** across **Y shards** on testnet (methodology: [link to harness]).

**OK for L1 only (today):**

> TKS Layer 1 confirms **~300–400+ simple-transfer TPS** under public RPC load (v0.1.1). Cipher scale targets **shard + relay** capacity, not on-chain chat per message.

**Not OK:**

> “TKS blockchain 500k TPS” without shard metrics and DA/challenge story.

---

## 11. Related files

| Path | Role |
|------|------|
| `cipher-relay/src/lib.rs` | Relay REST API (store/fetch/ack/register) |
| `node/src/service.rs` | `--enable-cipher-relay`, `--cipher-relay-port` |
| `pallets/pallet-shard-registry/` | `register_validator`, epoch rotation, `ShardMap` |
| `pallets/pallet-hyperswarm-anchor/` | `post_anchor`, `submit_challenge`, counters |
| `pallets/pallet-adaptive-gas/` | L1 gas limit elasticity |
| `network/PERF-AND-SHARDS.md` | L1 vs shard summary |

---

## 12. Revision history

| Version | Date | Notes |
|---------|------|-------|
| 1.0 | 2026-10-24 | Initial spec post v0.1.1 performance re-test |
