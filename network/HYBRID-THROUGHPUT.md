# Hybrid adaptive throughput (1k ↔ 500k)

Two coupled controllers:

| Layer | Low load | Peak load |
|-------|----------|-----------|
| **Cipher data plane** (`cipher-hybrid`) | ~**1k** msg/s regime — 1s seals, small batches | **500k** aggregate — 10ms seals, 8 logical shards × ~62.5k |
| **L1 EVM** (`pallet-adaptive-gas`, spec **409**) | **50M** gas/block (~**1k** TPS @ 50k gas/tx) | up to **4B** gas (~**80k** L1 TPS) |

Cipher messaging TPS and L1 TPS are **separate metrics** (see `CIPHER-TKS-THROUGHPUT-SPEC.md`).

## Node flags (default on)

```bash
tks-chain-node ... \
  --cipher-hybrid-throughput \
  --cipher-hybrid-floor-tps=1000 \
  --cipher-hybrid-ceiling-tps=500000 \
  --cipher-hybrid-shard-count=8
```

## Observability

```bash
curl -s http://127.0.0.1:4002/hybrid/stats | jq .
curl -s http://127.0.0.1:4010/hybrid/stats | jq .
```

Fields: `mode` (`idle` | `ramp` | `peak`), `measured_tps`, `settings.target_aggregate_tps`, `settings.seal_interval_ms`.

## Honest capacity

- **500k** requires **distributed** senders and enough CPU/network; one VPS runs the **algorithm** and **8 logical shard lanes**, not a proven 500k single-machine benchmark.
- Run `tools/cipher-loadtest` + spec §8 before public claims.
