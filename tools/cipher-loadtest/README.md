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

## Run (phase 1 — relay)

```bash
cargo run -p cipher-loadtest -- --relay-url http://127.0.0.1:4002 --messages 5000 --concurrency 50
```

Writes `cipher_load_results.json` in the current directory.

## Embedded in `tks-chain-node` (default)

Relay `:4002` and shard executor `:4010` start together. Stored messages auto-forward to `/ingest`. DA + anchor JSON under `{base-path}/cipher/da` and `cipher/anchors`.

```bash
tks-chain-node ... --enable-cipher-relay --cipher-relay-port=4002 \
  --enable-cipher-shard-executor --cipher-shard-port=4010 --cipher-shard-id=0
```

## Standalone shard executor (phase 2)

```bash
cargo run -p cipher-shard-executor -- --shard-id 0 --port 4010 --da-dir ./data/da --anchors-dir ./data/anchors
curl -X POST http://127.0.0.1:4010/ingest -H 'content-type: application/json' -d '{
  "msg_id":"1","sender_id":"a","recipient_id":"b",
  "ciphertext_hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
  "stored_at":1
}'
```

After the anchor interval, check `./data/anchors/` for `HyperSwarmAnchor.post_anchor` JSON to submit from a **shard-assigned** validator key.

Implementation: milestone **M2** (executor scaffold) / **M3** (full benchmark + third-party replay).
