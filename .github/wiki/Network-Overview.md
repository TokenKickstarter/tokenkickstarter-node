# Network Overview

## Chain Properties

| Property | Value |
|----------|-------|
| Chain Name | TKS Network |
| Chain ID (EVM) | `7779` (`0x1E63`) |
| Chain ID (Substrate) | `tks_mainnet` |
| Native Token | TKS |
| Token Decimals | 18 |
| SS58 Prefix | 42 |
| Block Time | 1,000ms (1 second) |
| Block Finality | Deterministic (GRANDPA) |
| Consensus | Aura + GRANDPA |
| Address Format | Ethereum-compatible `AccountId20` (20 bytes) |
| Signature Scheme | `secp256k1` (Ethereum-style) |

---

## Network Endpoints

### Public RPC

| Endpoint | URL |
|----------|-----|
| HTTPS RPC | `https://rpc.tokenkickstarter.com` |
| WebSocket RPC | `wss://rpc.tokenkickstarter.com` |

### Block Explorer

`https://scan.tokenkickstarter.com`

---

## DNS Bootnodes

The network uses DNS-based bootnode resolution. No IP addresses need to be hardcoded — DNS updates automatically on server changes.

| Domain | Region | Peer ID |
|--------|--------|---------|
| `seed.tokenkickstarter.com` | US | `12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C` |
| `seed.tokenkickstarter.ink` | US | `12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C` |
| `seed.tkstoken.com` | EU | `12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR` |
| `seed.tokenkickstarter.xyz` | EU | `12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR` |
| `seed.tksscan.com` | Asia | `12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8` |
| `seed.tokenkickstarter.pw` | Asia | `12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8` |

---

## Port Reference

| Port | Protocol | Purpose |
|------|----------|---------|
| `30333` | TCP | P2P networking (libp2p) |
| `9944` | TCP | RPC (HTTP + WebSocket) |
| `9615` | TCP | Prometheus metrics (optional) |

---

## Network Architecture

```
                   DNS Layer (no single point of failure)
   ┌──────────────────────────────────────────────────────────┐
   │  seed.tokenkickstarter.com  →  Server 1 (US)            │
   │  seed.tkstoken.com          →  Server 2 (EU)            │
   │  seed.tksscan.com           →  Server 3 (Asia)          │
   └──────────────────────────────────────────────────────────┘
                             │
                   P2P / libp2p (port 30333)
             ┌───────────────┼───────────────┐
        ┌────▼────┐    ┌─────▼────┐    ┌─────▼────┐
        │Bootnode │    │Bootnode  │    │Bootnode  │
        │  (US)   │    │  (EU)    │    │ (Asia)   │
        └────┬────┘    └────┬─────┘    └────┬─────┘
             │              │               │
      ┌──────┘       ┌──────┘       ┌───────┘
      ▼              ▼              ▼
   Full Nodes      Validators     Full Nodes
```

All nodes share the SAME genesis — the SAME chain state. DNS changes propagate transparently with zero downtime.

---

## Genesis Configuration

The genesis is defined in `node/src/chain_spec.rs`.

Key genesis allocations:
- Pre-funded development accounts (dev mode only)
- Initial validator set (Aura/GRANDPA authority set)
- Initial sudo key (temporary governance)

Chain spec files:
- `network/tks-testnet-spec.json` — Human-readable spec
- `network/tks-testnet-spec-raw.json` — Raw encoded spec (used by nodes)

---

## Token Economics

| Parameter | Value |
|-----------|-------|
| Symbol | TKS |
| Decimals | 18 |
| Smallest unit | 1 Planck = 0.000000000000000001 TKS |
| Base fee mechanism | EIP-1559 (adaptive, via `pallet-base-fee`) |
| Gas price (low traffic) | Near-zero |
| Gas price (congestion) | Auto-scales upward |
| Staking | NPoS (Nominated Proof of Stake) |
