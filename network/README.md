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

  All nodes share the SAME genesis → SAME chain ✅
  Server IP changes → Update DNS only. Zero downtime. ✅
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

## Running Bootnodes (For Team)

See [DNS-SETUP.md](./DNS-SETUP.md) for full DNS configuration guide.

Quick summary:
1. Get 3 VPS servers (~$5/mo each — Hetzner, Vultr, DigitalOcean)
2. Point DNS A records → server IPs (Cloudflare, DNS-only mode)
3. Upload binary + chainspec + bootnode key to each server
4. Run `./start-bootnode-1.sh` (Server 1), `2`, `3`
5. Open port 30333 in firewall

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
| `bootnodes/bootnode-1.key` | Server 1 private key — **keep secret** |
| `bootnodes/bootnode-2.key` | Server 2 private key — **keep secret** |
| `bootnodes/bootnode-3.key` | Server 3 private key — **keep secret** |
| `start-node.sh` | Mac/Linux full node |
| `start-node.bat` | Windows full node |
| `start-validator.sh` | Mac/Linux validator |
| `start-bootnode-1.sh` | Server 1 bootnode |
| `start-bootnode-2.sh` | Server 2 bootnode |
| `start-bootnode-3.sh` | Server 3 bootnode |
| `inject-validator-keys.sh` | Generate session keys |
| `install.sh` | One-liner installer |
| `server-setup.sh` | Linux systemd install |
| `purge-chain.sh` | Reset chain data |
| `docker-compose.yml` | Docker deployment |
| `DNS-SETUP.md` | DNS configuration guide |

---

## How Sync Works

1. Node starts → reads `tks-testnet-spec-raw.json` → gets genesis hash
2. Queries DNS seeds → gets server IPs → connects via P2P (port 30333)
3. Downloads all blocks from peers
4. Saves discovered peers locally → next restart doesn't even need DNS seeds
5. Produces/validates new blocks in real-time

**If all bootnodes go offline:**
- Nodes that already synced → keep running, reconnect to saved peers ✅
- Brand new nodes → can't join until at least 1 bootnode is back

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

*TokenKickstarter — Build DApps, Tokens & Apps on TKS*  
*Docs: https://docs.tokenkickstarter.com*  
*Explorer: https://tksscan.com*
