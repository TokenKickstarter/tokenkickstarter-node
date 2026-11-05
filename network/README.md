# TKS Network — Node Operator Guide

## Architecture

```
                     DNS Layer (no single IP)
    ┌──────────────────────────────────────────────────────┐
    │  seed.tokenkickstarter.com  →  Server 1 (US)        │
    │  seed.tokenkickstarter.ink  →  Server 1 (US)        │
    │  seed.tkstoken.com          →  Server 2 (EU)        │
    │  seed.tokenkickstarter.xyz  →  Server 2 (EU)        │
    │  seed.tksscan.com           →  Server 3 (Asia)      │
    │  seed.tokenkickstarter.pw   →  Server 3 (Asia)      │
    └──────────────────────────────────────────────────────┘
                              │
                   P2P (port 30333)
              ┌───────────────┼───────────────┐
         ┌────▼────┐    ┌─────▼────┐    ┌─────▼────┐
         │ Node A  │    │  Node B  │    │  Node C  │
         │(syncing)│    │(syncing) │    │(syncing) │
         └─────────┘    └──────────┘    └──────────┘

  All nodes share the same genesis and the same chain.
  Server IP changes: update DNS only; no binary change required.
```

---

## Network Info

| Property | Value |
|----------|-------|
| Chain Name | `TKS Network` |
| Chain ID | `tks_mainnet` |
| Token | `TKS` (18 decimals) |
| Consensus | Aura + GRANDPA |
| P2P Port | `30333` |
| RPC Port | `9944` |
| SS58 Prefix | `42` |

### DNS Seeds

| Domain | Server | Peer ID |
|--------|--------|---------|
| `seed.tokenkickstarter.com` | US | `12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C` |
| `seed.tokenkickstarter.ink` | US | `12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C` |
| `seed.tkstoken.com` | EU | `12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR` |
| `seed.tokenkickstarter.xyz` | EU | `12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR` |
| `seed.tksscan.com` | Asia | `12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8` |
| `seed.tokenkickstarter.pw` | Asia | `12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8` |

---

## Quick Start (Join as Full Node)

### Install & Run (Mac/Linux)
```bash
curl -sSf https://get.tks.network | bash
# Then:
./start-node.sh
```

### Windows
Double-click `start-node.bat`

### Manual
```bash
./tks-chain-node \
  --chain tks-testnet-spec-raw.json \
  --bootnodes "/dns/seed.tokenkickstarter.com/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C" \
  --bootnodes "/dns/seed.tkstoken.com/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR" \
  --bootnodes "/dns/seed.tksscan.com/tcp/30333/p2p/12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8" \
  --base-path ~/.tks/data \
  --rpc-external --rpc-cors all
```

> No IP addresses needed — DNS resolves automatically.

---

## Running Bootnodes (operators)

1. Generate a stable libp2p node key per host: `./target/release/tks-chain-node key generate-node-key` → save as `bootnode.key` (never commit).
2. Point DNS **A** records at the host (Cloudflare **DNS only** for P2P seeds is recommended).
3. Ship binary + [`tks-network-spec-raw.json`](./tks-network-spec-raw.json) + `bootnode.key`.
4. Run [`start-bootnode.sh`](./start-bootnode.sh) or `server-setup.sh bootnode`.
5. Open TCP **30333**; publish the multiaddr in [`bootnodes.public.txt`](./bootnodes.public.txt) via release/chainspec update.

---

## Running a Validator

```bash
# Step 1: Start validator node
./start-validator.sh

# Step 2: Generate + inject session keys (ONCE only)
./inject-validator-keys.sh http://127.0.0.1:9945

# Step 3: Send public keys to TKS team to be added on-chain
```

---

## Install as System Service (Linux)

```bash
sudo bash server-setup.sh fullnode
# or for bootnode:
sudo bash server-setup.sh bootnode
```

Manages node as a systemd service with auto-restart.

---

## File Reference

| File | Purpose |
|------|---------|
| `tks-testnet-spec-raw.json` | **Ship this** — raw genesis chainspec |
| `tks-testnet-spec.json` | Human-readable chainspec |
| `bootnode.key` | Bootnode libp2p key — **generate per server, never commit** |
| `bootnodes.public.txt` | Published P2P multiaddrs for syncers |
| `start-node.sh` | Mac/Linux full node |
| `start-node.bat` | Windows full node |
| `start-validator.sh` | Mac/Linux validator |
| `start-bootnode.sh` | Single-host bootnode launcher |
| `inject-validator-keys.sh` | Generate session keys |
| `install.sh` | One-liner installer |
| `server-setup.sh` | Linux systemd install |
| `purge-chain.sh` | Reset chain data |
| `docker-compose.yml` | Docker deployment |
| `docker-compose-live.yml` | Production full node + validators (Cipher flags) |
| `CIPHER-TKS-THROUGHPUT-SPEC.md` | Cipher hybrid throughput targets |
| `nginx/` | Reference snippets for public RPC + relay hosts |
| `vps-signal-server/` | Optional WebSocket push sidecar for relays |
| `HYPERSWARM-PEERS.md` | Relay mesh and discovery |
| `CIPHER-CLIENT-CHUNKING-SPEC.md` | Client chunking for large media |

---

## Production RPC node (Cipher + HyperSwarm)

For a public RPC host (for example `rpc.tokenkickstarter.com`), run the full node with embedded Cipher services. Prefer `docker-compose-live.yml` or equivalent flags:

```bash
docker compose -f docker-compose-live.yml up -d
```

Key ports:

| Port | Exposure | Purpose |
|------|----------|---------|
| 9944 | Public (via nginx or bind) | JSON-RPC / WebSocket |
| 30333 | Public | Substrate P2P |
| 4002 | Usually localhost + reverse proxy | Cipher relay HTTP |
| 4010 | Usually localhost | Shard executor |
| 4001 | Between peer nodes | HyperSwarm libp2p |

Upgrade to the latest [release binary](https://github.com/TokenKickstarter/tokenkickstarter-node/releases) (for example `tks-chain-node-v0.1.8-linux-amd64`), restart containers or systemd, then verify:

```bash
curl -s -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"system_health","params":[],"id":1}' \
  http://127.0.0.1:9944
curl -s http://127.0.0.1:4002/stats
```

---

## Upgrade procedure

1. Stop the stack: `docker compose -f docker-compose-live.yml down` or `systemctl stop tks-node`.
2. Download the matching release artifact for your OS and architecture.
3. Replace the binary or rebuild the Docker image from tag `v0.1.8` (or newer).
4. Start again with the same `--base-path` and chainspec (no resync required).
5. Apply a runtime WASM upgrade separately if you need new on-chain logic (spec 408/409).

---

## How Sync Works

1. Node starts → reads `tks-testnet-spec-raw.json` → gets genesis hash
2. Queries DNS seeds → gets server IPs → connects via P2P (port 30333)
3. Downloads all blocks from peers
4. Saves discovered peers locally → next restart doesn't even need DNS seeds
5. Produces/validates new blocks in real-time

**If all bootnodes go offline:**

- Nodes that already synced continue running and reconnect via saved peers.
- New nodes cannot join until at least one bootnode is reachable again.

---

## Troubleshooting

```bash
# Node not finding peers
nc -zv seed.tokenkickstarter.com 30333   # test P2P port
dig seed.tokenkickstarter.com A +short   # test DNS

# Wrong genesis — must use exact same chainspec
sha256sum tks-testnet-spec-raw.json   # compare with official

# Reset and resync
./purge-chain.sh

# View logs (if running as service)
journalctl -u tks-node -f
```

---

Documentation: https://docs.tokenkickstarter.com  
Explorer: https://tksscan.com
