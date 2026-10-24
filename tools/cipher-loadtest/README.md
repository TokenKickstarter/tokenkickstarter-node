# Cipher load test harness (planned)

Implements the benchmark contract in [`network/CIPHER-TKS-THROUGHPUT-SPEC.md`](../../network/CIPHER-TKS-THROUGHPUT-SPEC.md) §8.

## Outputs

| File | Purpose |
|------|---------|
| `cipher_load_results.json` | Throughput, drops, per-shard totals |
| `latency_histogram.json` | Delivery p50/p95/p99 (ms) |
| `anchors.json` | L1 `AnchorPosted` vs messages delivered |
| `readonly.json` | L1 baseline (chain id, gas limit, registry state) |

## Phases

1. **Relay-only** — stress `cipher-relay` `:4002` (`/store`, `/fetch`, `/ack`).
2. **Shard + anchor** — distributed senders → shard executor → `HyperSwarmAnchor::post_anchor` + DA verify.
3. **Third-party replay** — same config, independent operator, ±10% of claimed aggregate.

Implementation tracked under milestone **M3** in the spec.
