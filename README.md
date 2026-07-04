# tokenkickstarter-node

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Substrate](https://img.shields.io/badge/Substrate-stable2512-purple)](https://github.com/paritytech/polkadot-sdk)
[![Chain ID](https://img.shields.io/badge/Chain%20ID-7779-green)](https://chainlist.org)
[![Build](https://github.com/TokenKickstarter/tokenkickstarter-node/actions/workflows/release.yml/badge.svg)](https://github.com/TokenKickstarter/tokenkickstarter-node/actions/workflows/release.yml)

Layer-1 Substrate blockchain with full EVM compatibility, HyperSwarm DHT anchoring, adaptive gas limits, native NFTs, and 1-second block finality.

---

## Overview

TokenKickstarter (TKS) is a Layer-1 blockchain built on [Substrate](https://substrate.io) with [Frontier](https://github.com/polkadot-evm/frontier) providing Ethereum Virtual Machine compatibility. The chain is designed for high-throughput, low-cost operation with decentralized P2P networking support built directly into the protocol layer.

| Property | Value |
|----------|-------|
| Chain ID | 7779 (`0x1E63`) |
| Native Token | TKS (18 decimals) |
| Block Time | 1 second |
| Block Finality | GRANDPA (deterministic) |
| Address Format | Ethereum-compatible (20-byte, `AccountId20`) |
| EVM Compatibility | Full — MetaMask, Hardhat, ethers.js, wagmi, Foundry |
| Consensus | Aura (block production) + GRANDPA (finality) |

---

## Features

### HyperSwarm Anchor

Publishes and resolves [HyperSwarm](https://github.com/holepunchto/hyperswarm) DHT public keys directly on-chain. Decentralized peer-to-peer applications can anchor their discovery keys to Layer-1 finality, enabling trustless and censorship-resistant peer resolution without external coordination servers.

### Adaptive Gas

Block gas limits scale automatically based on real-time network load, from 1,500 to 80,000 transactions per block. The base fee adjusts each block, keeping costs near-zero during low traffic periods and rising under sustained congestion. No manual parameter tuning required.

### Native NFT Collections (pallet-nfts)

Substrate-layer NFT collections with no deposit requirement. Collections and items are created through standard Substrate extrinsics, independent of the EVM. The chain also supports standard ERC-721 and ERC-1155 smart contracts via the EVM layer.

### Free Name Registry (pallet-name-registry)

On-chain human-readable names (e.g., `alice.tks`) with zero registration deposit. Names can be registered, transferred, and resolved on-chain, providing identity infrastructure without gas cost.

### 1-Second Block Finality

Aura-based block production with 1,000ms slot duration combined with GRANDPA deterministic finality. Transactions reach economic finality within seconds.

### Full EVM Compatibility

Standard Ethereum JSON-RPC API. Deploy any Solidity contract, use MetaMask, connect Hardhat or Foundry — no modifications required. Ethereum signatures and `secp256k1` keys are used natively at the account layer.

---

## Architecture

```
tokenkickstarter-node/
├── node/                          Node binary (networking, RPC, CLI)
│   └── src/
│       ├── chain_spec.rs          Genesis configuration
│       ├── service.rs             Full-node service
│       └── rpc.rs                 RPC handler registration
├── runtime/                       WASM runtime (all business logic)
│   └── src/
│       ├── lib.rs                 construct_runtime! and pallet configs
│       └── precompiles.rs         EVM precompile registration
└── pallets/                       Custom pallets
    ├── pallet-adaptive-gas/       Dynamic block gas limit
    ├── pallet-hyperswarm-anchor/  HyperSwarm DHT key anchoring
    ├── pallet-name-registry/      Free on-chain name system
    └── pallet-shard-registry/     Shard coordination layer
```

---

## Pallet Reference

| Pallet | Source | Purpose |
|--------|--------|---------|
| `pallet-balances` | Substrate | Native TKS token transfers |
| `pallet-evm` | Frontier | EVM execution layer |
| `pallet-ethereum` | Frontier | Ethereum RPC compatibility |
| `pallet-base-fee` | Frontier | EIP-1559 base fee tracking |
| `pallet-dynamic-fee` | Frontier | Dynamic fee adjustment |
| `pallet-adaptive-gas` | Custom | Auto-scaling block gas limit |
| `pallet-name-registry` | Custom | Free on-chain name system |
| `pallet-hyperswarm-anchor` | Custom | DHT key anchoring |
| `pallet-shard-registry` | Custom | Shard coordination |
| `pallet-nfts` | Substrate | Native NFT collections |
| `pallet-staking` | Substrate | NPoS validator staking |
| `pallet-session` | Substrate | Validator session management |
| `pallet-grandpa` | Substrate | Deterministic block finality |
| `pallet-sudo` | Substrate | Administrative governance (temporary) |

---

## Prerequisites

**Rust toolchain**

```bash
curl https://sh.rustup.rs -sSf | sh
source ~/.cargo/env
rustup target add wasm32-unknown-unknown
```

**System dependencies (macOS)**

```bash
brew install cmake protobuf llvm
```

**System dependencies (Ubuntu/Debian)**

```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential pkg-config libssl-dev \
    clang libclang-dev protobuf-compiler git
```

---

## Build from Source

```bash
git clone https://github.com/TokenKickstarter/tokenkickstarter-node
cd tokenkickstarter-node
cargo build --release
```

The compiled binary will be at `./target/release/tks-chain-node`.

Build time is approximately 20-35 minutes on first compile. Subsequent builds use Cargo's incremental compilation.

---

## Running the Node

### Development (single-node, ephemeral)

```bash
./target/release/tks-chain-node --dev
```

This starts a development node with pre-funded accounts, produces blocks immediately on transaction submission, and resets state on restart.

### Development with public RPC (for MetaMask or explorer)

```bash
./target/release/tks-chain-node --dev \
  --rpc-external \
  --rpc-cors=all \
  --rpc-port 9944
```

### Join the testnet

```bash
./target/release/tks-chain-node \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/data \
  --name "MyNode" \
  --rpc-external \
  --rpc-cors=all
```

### Run as a validator

```bash
./target/release/tks-chain-node \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/data \
  --validator \
  --name "MyValidator"
```

See `network/start-validator.sh` and `network/inject-validator-keys.sh` for the full validator setup process including key injection.

---

## Connect MetaMask

Add the TKS network to MetaMask:

| Field | Value |
|-------|-------|
| Network Name | TKS Network |
| RPC URL | `http://127.0.0.1:9944` |
| Chain ID | `7779` |
| Currency Symbol | `TKS` |
| Block Explorer | `http://localhost:3000` (if running the explorer) |

---

## Development Accounts

The following accounts are pre-funded in development mode with 1,000 TKS each. These are standard [Hardhat](https://hardhat.org/hardhat-network/docs/reference#initial-state) test accounts.

| Account | Address |
|---------|---------|
| Account 0 (deployer) | `0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266` |
| Account 1 | `0x70997970C51812dc3A010C7d01b50e0d17dc79C8` |
| Account 2 | `0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC` |
| Treasury | `0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65` |

**Account 0 private key:** `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`

> These are well-known public test keys. Do not use them on any production network.

---

## EVM and Smart Contract Deployment

### Hardhat configuration

```javascript
// hardhat.config.js
module.exports = {
  networks: {
    tks: {
      url: "http://127.0.0.1:9944",
      chainId: 7779,
      accounts: ["0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"]
    }
  }
};
```

### Deploy with ethers.js

```javascript
import { ethers } from 'ethers'

const provider = new ethers.JsonRpcProvider('http://127.0.0.1:9944')
const signer = new ethers.Wallet(PRIVATE_KEY, provider)

const factory = new ethers.ContractFactory(abi, bytecode, signer)
const contract = await factory.deploy()
await contract.waitForDeployment()
console.log('Deployed at:', await contract.getAddress())
```

### EVM Precompiles

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

Create and manage NFT collections at the Substrate layer without EVM gas costs.

Via Polkadot.js Apps (connect to `ws://127.0.0.1:9944`):

```
Developer > Extrinsics > nfts > create(admin, config)
Developer > Extrinsics > nfts > mint(collection, item, mintTo, witnessData)
Developer > Extrinsics > nfts > transfer(collection, item, dest)
```

All deposits are set to zero in the TKS configuration.

---

## Releases

Pre-built binaries are available for all major platforms on the [Releases](https://github.com/TokenKickstarter/tokenkickstarter-node/releases) page.

| Platform | Architecture | Binary |
|----------|-------------|--------|
| macOS | Apple Silicon (M1/M2/M3/M4) | `tks-chain-node-*-macos-arm64` |
| macOS | Intel (x86_64) | `tks-chain-node-*-macos-amd64` |
| Linux | x86_64 | `tks-chain-node-*-linux-amd64` |
| Linux | ARM64 | `tks-chain-node-*-linux-arm64` |
| Windows | x86_64 | `tks-chain-node-*-windows-amd64.exe` |

**One-line installer (Linux/macOS):**

```bash
curl -sSf https://raw.githubusercontent.com/TokenKickstarter/tokenkickstarter-node/main/network/install.sh | bash
```

---

## Related Projects

- [tokenkickstarter-explorer](https://github.com/TokenKickstarter/tokenkickstarter-explorer) — Block explorer built with Next.js, showing live blocks, transactions, NFTs, tokens, and accounts.

---

## License

[MIT](LICENSE)
