# Cipher signal mesh (inside the L1 node stack)

## What exists today

| Piece | Where | Peer mesh? |
|-------|--------|------------|
| **HTTP mailbox** | `cipher-relay` `:4002` | Yes — store fanout + HyperSwarm chunk gossip |
| **HyperSwarm relay gossip** | libp2p `:4001`, topic `tks-hyperswarm-relay-v1` | Yes — live relay list for `/discovery` |
| **Instant push (WS)** | **Embedded in node** on `:4003` by default (`cipher-relay/src/signal.rs`) | No — single-host `sessions` map |
| **Push trigger** | In-process hub or `127.0.0.1:4003/internal/push` after `/store` | Local + federated (Phase 2) |

Clients learn relays + signal URLs from **`tks_listSwarmValidators`**, **`GET /discovery`**, and **gossip** (Cipher-cli / mobile pools).

## Phase 1 — Signal in relay gossip (in node)

**RelayAnnouncement** on HyperSwarm now carries optional **`signal_ws`** (from `--cipher-public-signal-ws` or derived `wss://relay-host/ws`).

Effects:

- Operators run Hyperswarm on each validator fullnode.
- Every ~30s each node gossips `{ http_base, signal_ws, federated_push, peer_id, region }`.
- Any relay’s **`GET /discovery`** merges **gossip** entries → clients get **many WSS URLs** without a central domain list.

This is **discovery mesh**, not push mesh: easier failover for clients, same as relay peers.

## Phase 2 — Federated push (implemented)

Goal: Alice stores on **relay A**; Bob is online on **signal B**; Bob still gets ~100ms push.

1. **Relay HTTP** (Rust): `POST /signal/push` — body `{ to, message_id, data }` (base64 ciphertext, same as `/internal/push`). Forwards to local `:4003/internal/push`.
2. **Auth**: loopback always allowed. Remote callers must send header **`X-TKS-Federated-Push-Key`** matching **`CIPHER_FEDERATED_PUSH_KEY`** / **`--cipher-federated-push-key`** on every validator in the mesh (use the same value on all peers).
3. **After `/store`**: local push + best-effort **HTTP POST to up to 8 gossip/chain peers’** `{relay}/signal/push` (parallel to `/store` fanout).
4. **Nginx** on each operator: proxy **`/signal/push`** → relay `:4002`, **`/ws`** → `:4003`.

No WebSocket session replication across hosts — only **route push to the right island**.

## Phase 3 — Embedded WS (implemented)

The fullnode starts an embedded signal server on **`--cipher-signal-port`** (default **4003**):

- `GET /ws?addr=…` — same protocol as `vps-signal-server`
- `POST /internal/push` — localhost only (relay / in-process hub)
- Disable with **`--cipher-embedded-signal false`** and point **`--cipher-signal-push-url`** at an external sidecar if needed.

## Operator checklist (defaults-first)

**Cipher is on by default** on validator/full nodes: relay `:4002`, HyperSwarm `:4001`, embedded signal `:4003`, shard `:4010`, hybrid throughput, federated push hooks. You do **not** need a long `--flag` list.

Typical production — only your public name + shared push secret (CLI **or** env):

```bash
export CIPHER_PUBLIC_RELAY_URL=https://relay.YOUR.domain
export CIPHER_FEDERATED_PUSH_KEY='…same on all validators…'
# optional: export CIPHER_PUBLIC_SIGNAL_WS=wss://signal.YOUR.domain/ws

tks-chain-node --chain … --validator --name … --rpc-external …
```

| Goal | One knob |
|------|-----------|
| Turn **everything** Cipher off | `--disable-cipher-stack` or `CIPHER_DISABLE_STACK=1` |
| Change ports only | `--cipher-relay-port` / `--cipher-signal-port` / env `CIPHER_*_PORT` |
| External Node signal sidecar | `--cipher-embedded-signal false` + `CIPHER_SIGNAL_PUSH_URL` |
| Skip shard executor | `--enable-cipher-shard-executor false` |

Embedded signal listens on **`4003`** by default (nginx **`/ws`** → that port). Register **`endpoint`** on chain via `register_validator`. Peers gossip **signal_ws** and **federated_push** automatically.

### Nginx (relay host)

```nginx
location /signal/push {
    proxy_pass http://127.0.0.1:4002/signal/push;
}
location /ws {
    proxy_pass http://127.0.0.1:4003/ws;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
}
```

## Honest limits

- **Signal mesh ≠ full session replication** — each WSS host still holds only its connected clients.
- **HTTP `/fetch`** remains the decentralized backup when all WS paths fail.
- **Security**: use a strong shared federated key on all validators; localhost bypass is for the sidecar only.
