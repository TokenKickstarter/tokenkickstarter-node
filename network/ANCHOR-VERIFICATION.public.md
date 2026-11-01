# Cipher anchor verification (investor / auditor)

## What L1 stores

Each `HyperSwarmAnchor.post_anchor` extrinsic records:

- `shard_id`, `epoch`
- **`dag_root_hash`** — Merkle root over batch message leaves (32 bytes)
- **`message_count`** — number of leaves in that batch (must be > 0)

The validator’s **`message_count` is not independently proven on-chain** unless a challenger wins a dispute.

## What operators store off-chain (DA)

Under `{base-path}/cipher/da/` the shard executor writes JSON **`DataAvailabilityBlob`** files keyed by root hash, containing the message list (`msg_id`, `ciphertext_hash`, …).

## How to verify (read-only)

1. Read anchor from chain (events or storage `LatestAnchors`).
2. Load DA blob from a fullnode or published mirror for that `dag_root_hash`.
3. Recompute Merkle root (`cipher-shard-executor` / `merkle.rs` algorithm).
4. Compare to on-chain root; count messages and compare to `message_count`.

```bash
# Example: stats RPC (not a proof)
curl -s https://rpc.tokenkickstarter.com -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tks_anchorStats","params":[]}'
```

## Double-count fix (v0.1.20+)

Relay `/store` + forwarded `/ingest` previously queued the same `msg_id` twice. **Batch dedupe** in `cipher-shard-executor` prevents duplicate leaves. Historical `TotalMessageCount` is not rewritten.

## Slashing / “trustless”

`submit_challenge` and `resolve_challenge` exist; **automatic staking slashes on invalid anchors are not implemented** (sudo resolves disputes). Do not describe anchors as trustless until slashing or fraud proofs ship — use **“Merkle-commitment with off-chain DA; disputes via fisherman + sudo.”**
