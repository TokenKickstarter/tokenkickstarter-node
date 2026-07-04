# tokenkickstarter-node

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Substrate](https://img.shields.io/badge/Substrate-stable2512-purple)](https://github.com/paritytech/polkadot-sdk)
[![Chain ID](https://img.shields.io/badge/Chain%20ID-7779-green)](https://chainlist.org)
[![Block Time](https://img.shields.io/badge/Block%20Time-1s-orange)](https://github.com/TokenKickstarter/tokenkickstarter-node)

> Substrate-based EVM-compatible blockchain with adaptive gas, native NFTs, free name registry, and 1-second block finality.

---

## Overview

**TKS (TokenKickstarter)** is a Layer-1 blockchain built with [Substrate](https://substrate.io) and [Frontier](https://github.com/polkadot-evm/frontier), providing full Ethereum Virtual Machine (EVM) compatibility alongside Substrate-native capabilities.

| Property | Value |
|----------|-------|
| **Chain ID** | 7779 (`0x1E63`) |
| **Token** | TKS |
| **Block Time** | ~1 second |
| **Consensus** | Aura (block production) + GRANDPA (finality) |
| **Address Format** | Ethereum-compatible (`AccountId20`) |
| **EVM** | Full compatibility — MetaMask, Hardhat, ethers.js, wagmi |

---

## Key Features

### ⛽ Adaptive Gas
Block gas limit auto-scales from **1.5k to 80k transactions per block** based on network load. The base fee adjusts dynamically — cheap in low traffic, rises under congestion.

### 🎨 Native NFTs (`pallet-nfts`)
Substrate-layer NFT collections — no EVM gas required. Also supports standard **ERC-721** and **ERC-1155** contracts via EVM.

### 📛 Free Name Registry (`pallet-name-registry`)
On-chain human-readable names (e.g. `alice.tks`) with zero deposit. Register, transfer, and look up names for free.

### 🌐 HyperSwarm Anchor (`pallet-hyperswarm-anchor`)
On-chain anchoring for HyperSwarm DHT (decentralised peer-to-peer networking). Enables off-chain data discovery via the Substrate layer.

### ⚡ 1-Second Blocks
1,000ms slot duration for near-instant confirmation on all transactions — native and EVM.

---

## Architecture

```
tokenkickstarter-node/
├── node/                     # Node binary — networking, RPC, service
│   └── src/
│       ├── chain_spec.rs     # Genesis configuration
│       ├── service.rs        # Full-node service setup
│       └── rpc.rs            # Custom RPC endpoints
├── runtime/                  # WASM runtime — all business logic
│   └── src/
│       ├── lib.rs            # construct_runtime! + pallet configs
│       └── precompiles.rs    # EVM precompiles
└── pallets/                  # Custom pallets
    ├── pallet-name-registry/ # Free on-chain name system
    ├── pallet-shard-registry/# Shard coordination layer
    ├── pallet-hyperswarm-anchor/ # DHT anchoring
    └── pallet-adaptive-gas/  # Dynamic block gas limit
```

---

## Getting Started

### Prerequisites

```bash
# Install Rust
curl https://sh.rustup.rs -sSf | sh

# Add WASM target
rustup target add wasm32-unknown-unknown

# Install system deps (macOS)
brew install cmake protobuf
```

### Build

```bash
git clone https://github.com/TokenKickstarter/tokenkickstarter-node
cd tokenkickstarter-node
cargo build --release
```

### Run (Development)

```bash
# Start a dev node (purges chain on restart, pre-funded accounts)
./target/release/tks-chain-node --dev

# With public RPC (for MetaMask / explorer)
./target/release/tks-chain-node --dev \
  --rpc-external \
  --rpc-cors=all \
  --rpc-port 9944
```

### Connect MetaMask

| Field | Value |
|-------|-------|
| Network Name | TKS Network |
| RPC URL | `http://127.0.0.1:9944` |
| Chain ID | `7779` |
| Currency Symbol | `TKS` |

### Pre-funded Dev Accounts

| Account | Address | Balance |
|---------|---------|---------|
| Hardhat #0 | `0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266` | 1,000 TKS |
| Hardhat #1 | `0x70997970C51812dc3A010C7d01b50e0d17dc79C8` | 1,000 TKS |
| Hardhat #2 | `0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC` | 1,000 TKS |
| Treasury | `0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65` | 100,000 TKS |

> **Private key for Hardhat #0:** `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`  
> ⚠️ Devnet only — never use these keys on mainnet.

---

## EVM / Smart Contracts

TKS is fully EVM-compatible via [Frontier](https://github.com/polkadot-evm/frontier). Deploy any Solidity contract using standard tools:

```bash
# Hardhat config
networks: {
  tks: {
    url: "http://127.0.0.1:9944",
    chainId: 7779,
    accounts: ["0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"]
  }
}
```

### Deploying ERC-721 NFTs

```js
import { ethers } from 'ethers'

const provider = new ethers.JsonRpcProvider('http://127.0.0.1:9944')
const signer = new ethers.Wallet(PRIVATE_KEY, provider)

// Deploy any standard ERC-721 contract — costs ~0.00034 TKS
const factory = new ethers.ContractFactory(abi, bytecode, signer)
const nft = await factory.deploy('My Collection', 'MCOL')
await nft.waitForDeployment()
```

### Supported Precompiles

| Address | Precompile |
|---------|-----------|
| `0x01` | ECRecover |
| `0x02` | SHA-256 |
| `0x03` | RIPEMD-160 |
| `0x04` | Identity |
| `0x05` | Modexp |
| `0x400` | SHA3FIPS-256 |
| `0x401` | ECRecoverPublicKey |

---

## Native NFTs (pallet-nfts)

Create collections and mint NFTs at the Substrate layer — no gas, no EVM needed:

```bash
# Via Polkadot.js
# Extrinsics → Nfts → create(admin, config)
# Extrinsics → Nfts → mint(collection, item, mintTo, witnessData)
```

All deposits are set to **0** for the TKS free-to-use model.

---

## Pallets

| Pallet | Source | Purpose |
|--------|--------|---------|
| `pallet-balances` | Substrate | Native TKS token |
| `pallet-evm` | Frontier | EVM execution |
| `pallet-ethereum` | Frontier | Ethereum RPC compat |
| `pallet-adaptive-gas` | Custom | Dynamic block gas |
| `pallet-name-registry` | Custom | Free on-chain names |
| `pallet-shard-registry` | Custom | Shard coordination |
| `pallet-hyperswarm-anchor` | Custom | DHT anchoring |
| `pallet-nfts` | Substrate | Native NFT collections |
| `pallet-staking` | Substrate | NPoS validator staking |
| `pallet-sudo` | Substrate | Governance (temporary) |

---

## Explorer

The TKS block explorer is available at:  
👉 **[tokenkickstarter-explorer](https://github.com/TokenKickstarter/tokenkickstarter-explorer)**

Features live block/tx tracking, NFT collections, token transfers, accounts, and gas analytics — all connected directly to the node RPC.

---

## License

[MIT](LICENSE) © TokenKickstarter
