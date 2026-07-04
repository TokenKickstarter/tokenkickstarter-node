# Architecture

## Overview

TKS is a Substrate-based Layer-1 blockchain with a clear separation between the node binary, the WASM runtime, and custom pallets.

```
tokenkickstarter-node/
├── node/                   Binary — networking, RPC, consensus client
│   └── src/
│       ├── main.rs         Entry point
│       ├── chain_spec.rs   Genesis configuration
│       ├── service.rs      Full-node service (P2P, sync, consensus)
│       └── rpc.rs          RPC handler registration
│
├── runtime/                WASM runtime — all on-chain business logic
│   └── src/
│       ├── lib.rs          construct_runtime! + pallet configurations
│       └── precompiles.rs  EVM precompile address registration
│
├── pallets/                Custom Substrate pallets
│   ├── pallet-adaptive-gas/        Dynamic block gas limit
│   ├── pallet-hyperswarm-anchor/   On-chain DHT key storage
│   ├── pallet-name-registry/       Free human-readable names
│   └── pallet-shard-registry/      Shard coordination layer
│
├── cipher-relay/           Encrypted relay helper library (workspace member)
│
└── network/                Operator tooling
    ├── tks-testnet-spec.json       Human-readable chain spec
    ├── tks-testnet-spec-raw.json   Raw (encoded) chain spec — used by nodes
    ├── start-node.sh               Quick-start full node
    ├── start-validator.sh          Validator node startup
    ├── start-bootnode.sh           Bootnode startup
    ├── inject-validator-keys.sh    Session key injection helper
    ├── purge-chain.sh              Wipe local state
    ├── install.sh                  Binary installer
    ├── server-setup.sh             VPS provisioning script
    ├── tks-node.service            systemd unit (full node)
    ├── tks-bootnode.service        systemd unit (bootnode)
    └── docker-compose.yml          Docker multi-node local testnet
```

---

## Node Binary (`node/`)

The node binary handles everything external to consensus logic:

- **P2P networking** — libp2p (TCP, QUIC, mDNS, Kademlia DHT)
- **Block sync** — Substrate sync protocol (fast sync, warp sync)
- **Consensus client** — Aura slot worker, GRANDPA voter
- **RPC server** — HTTP and WebSocket JSON-RPC
- **CLI** — Command-line interface via Substrate's SC CLI framework
- **Chain spec** — Genesis block definition and initial authority set

The node binary does **not** contain business logic. All state transition logic lives in the WASM runtime.

---

## WASM Runtime (`runtime/`)

The runtime is compiled to both **native** (for fast execution) and **WASM** (for on-chain upgrades). All state machine logic is defined here.

Key responsibilities:
- Defining all pallets and their configurations (`construct_runtime!`)
- Specifying extrinsic weight/fee calculations
- Managing pallet storage migrations
- Registering EVM precompile addresses
- Implementing the Substrate runtime API

The runtime is embedded in the chain spec's genesis state and can be upgraded via governance without touching the node binary.

### Runtime API Implementation

The runtime implements all required Substrate runtime APIs:

| API | Purpose |
|-----|---------|
| `BlockBuilder` | Build and validate blocks |
| `TaggedTransactionQueue` | Transaction validation |
| `OffchainWorkerApi` | Offchain worker support |
| `AuraApi` | Aura consensus integration |
| `GrandpaApi` | GRANDPA finality integration |
| `AccountNonceApi` | Account nonce for RPC |
| `TransactionPaymentApi` | Fee estimation |
| `EthereumRuntimeRPCApi` | Ethereum RPC API (Frontier) |
| `ConvertTransactionRuntimeApi` | Ethereum tx conversion |

---

## Consensus Model

TKS uses a hybrid consensus:

```
┌─────────────────────────────────────────────────┐
│  BLOCK PRODUCTION: Aura (Authority Round)        │
│                                                   │
│  - Fixed set of authorities, round-robin slots   │
│  - Slot duration: 1,000ms                        │
│  - Each authority produces exactly 1 block       │
│    per slot when online                           │
└─────────────────────────┬───────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────┐
│  BLOCK FINALITY: GRANDPA                         │
│                                                   │
│  - Byzantine fault tolerant (BFT)               │
│  - Deterministic finality (not probabilistic)    │
│  - Tolerates up to 1/3 of validators offline     │
│  - Finalizes blocks in batches                   │
└─────────────────────────────────────────────────┘
```

**Result:** Blocks are produced every 1 second and reach deterministic finality within 2–4 seconds under normal network conditions.

---

## Account Model

TKS uses **Ethereum-style accounts** (`AccountId20`):

- 20-byte addresses (same as Ethereum)
- `secp256k1` key pairs
- ECDSA signatures (Ethereum-style, with `v` value for chain ID replay protection)
- Compatible with all Ethereum wallets (MetaMask, Ledger, Trezor, etc.)

This differs from standard Substrate which uses `AccountId32` (32-byte SS58 addresses).

---

## EVM Layer (Frontier)

The Frontier pallet suite bridges Substrate and Ethereum:

```
Ethereum dApp / MetaMask
         │
         ▼
    eth_* JSON-RPC
         │
         ▼
   pallet-ethereum    ← maps Ethereum tx types, stores ETH block/receipts
         │
         ▼
   pallet-evm         ← executes EVM bytecode (SputnikVM)
         │
         ▼
   pallet-base-fee    ← EIP-1559 base fee tracking
   pallet-dynamic-fee ← dynamic fee adjustment
```

---

## Storage Backend

TKS uses [RocksDB](https://rocksdb.org) as the on-disk storage backend (via `kvdb-rocksdb`). RocksDB provides:
- Write-optimized LSM tree storage
- Parallel compaction
- Bloom filters for fast key lookups
- Compression (zstd)

State is stored as a [Merkle Patricia Trie](https://docs.substrate.io/learn/state-transitions-and-storage/) (same as Ethereum), enabling:
- State root proofs
- Light client support
- Storage proofs for bridge/cross-chain verification

---

## Upgrade Path

Runtime upgrades are done via:

1. Compile new WASM runtime blob
2. Submit `system.setCode(blob)` extrinsic via sudo (or governance)
3. Node automatically detects the new runtime and switches execution

**No node restart required.** Full nodes continue operating through the upgrade.
