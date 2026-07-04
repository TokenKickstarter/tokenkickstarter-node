# TKS Node — Wiki Home

**TokenKickstarter (TKS)** is a Layer-1 blockchain built on [Substrate](https://substrate.io) with full Ethereum Virtual Machine compatibility, 1-second block finality, on-chain DHT anchoring, adaptive gas, and native NFTs.

---

## Quick Navigation

| Section | Description |
|---------|-------------|
| [Network Overview](Network-Overview) | Chain properties, endpoints, genesis |
| [Getting Started](Getting-Started) | Install, build, run |
| [Running a Node](Running-a-Node) | Full node, dev, testnet |
| [Running a Validator](Running-a-Validator) | Validator setup and key injection |
| [MetaMask & EVM](MetaMask-and-EVM) | Connect wallets and deploy contracts |
| [Architecture](Architecture) | Codebase layout and component overview |
| [Pallet Reference](Pallet-Reference) | All runtime pallets and their purpose |
| [Custom Pallets](Custom-Pallets) | Adaptive gas, HyperSwarm, name registry |
| [RPC API Reference](RPC-API-Reference) | JSON-RPC endpoints and examples |
| [CLI Reference](CLI-Reference) | All command-line flags |
| [Build & Release](Build-and-Release) | CI/CD pipeline and release process |
| [Contributing](Contributing) | Development workflow and standards |
| [Security](Security) | Responsible disclosure policy |

---

## At a Glance

| Property | Value |
|----------|-------|
| Chain ID | **7779** (`0x1E63`) |
| Native Token | **TKS** (18 decimals) |
| Block Time | **1 second** |
| Block Finality | **GRANDPA** (deterministic) |
| Consensus | **Aura** (production) + **GRANDPA** (finality) |
| Address Format | Ethereum-compatible (20-byte `AccountId20`) |
| EVM | Full — MetaMask, Hardhat, Foundry, wagmi, ethers.js |
| Substrate SDK | [polkadot-sdk stable2512](https://github.com/paritytech/polkadot-sdk) |
| License | MIT |

---

## Key Features

- **Full EVM Compatibility** — Deploy any Solidity contract without modification
- **1-Second Finality** — Aura block production + GRANDPA deterministic finality
- **Adaptive Gas** — Block gas limits scale automatically from 1,500 to 80,000 tx/block
- **HyperSwarm Anchor** — Decentralized DHT key anchoring at Layer-1 finality
- **Free Name Registry** — Human-readable on-chain names with no deposit
- **Native NFTs** — Substrate-layer NFT collections independent of EVM
- **NPoS Staking** — Full nominated proof-of-stake validator election

---

## Network Endpoints

| Service | URL |
|---------|-----|
| RPC (HTTP) | `https://rpc.tokenkickstarter.com` |
| RPC (WebSocket) | `wss://rpc.tokenkickstarter.com` |
| Block Explorer | `https://scan.tokenkickstarter.com` |
| Chain ID | `7779` |

### DNS Bootnodes

| Domain | Region |
|--------|--------|
| `seed.tokenkickstarter.com` | US |
| `seed.tkstoken.com` | EU |
| `seed.tksscan.com` | Asia |

---

## Repository Structure

```
tokenkickstarter-node/
├── node/           Node binary (networking, RPC, CLI)
├── runtime/        WASM runtime (all business logic)
├── pallets/        Custom pallets
│   ├── pallet-adaptive-gas/
│   ├── pallet-hyperswarm-anchor/
│   ├── pallet-name-registry/
│   └── pallet-shard-registry/
├── network/        Operator scripts, chain specs, systemd units
└── cipher-relay/   Encrypted relay helper library
```

---

*This wiki is maintained by the [TokenKickstarter](https://github.com/TokenKickstarter) team.*
