# Getting Started

This guide covers installing dependencies, building from source, and running your first TKS node.

---

## Prerequisites

### Rust Toolchain

TKS requires the Rust toolchain with the `wasm32-unknown-unknown` target for compiling the WASM runtime.

```bash
# Install rustup
curl https://sh.rustup.rs -sSf | sh
source ~/.cargo/env

# Add WASM target
rustup target add wasm32-unknown-unknown
rustup component add rust-src
```

Rust **stable** channel is required (currently 1.87+). The `rust-toolchain.toml` in the repository root pins the channel automatically — no manual version selection needed.

---

### System Dependencies

**macOS**

```bash
brew install cmake protobuf llvm
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

**Ubuntu / Debian**

```bash
sudo apt-get update
sudo apt-get install -y \
    build-essential pkg-config libssl-dev \
    clang libclang-dev protobuf-compiler git
```

**Windows**

Install [LLVM](https://github.com/llvm/llvm-project/releases) and [protoc](https://github.com/protocolbuffers/protobuf/releases), then set:

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
```

---

## Quick Install (Pre-built Binary)

Download the latest release binary for your platform from the [Releases page](https://github.com/TokenKickstarter/tokenkickstarter-node/releases/latest).

| Platform | File |
|----------|------|
| macOS Apple Silicon | `tks-chain-node-macos-arm64-*.tar.gz` |
| macOS Intel | `tks-chain-node-macos-x86_64-*.tar.gz` |
| Linux x86_64 | `tks-chain-node-linux-x86_64-*.tar.gz` |
| Linux ARM64 | `tks-chain-node-linux-arm64-*.tar.gz` |
| Windows x86_64 | `tks-chain-node-windows-x86_64-*.zip` |

**One-line install (Linux/macOS):**

```bash
curl -sSf https://raw.githubusercontent.com/TokenKickstarter/tokenkickstarter-node/main/network/install.sh | bash
```

---

## Build from Source

```bash
git clone https://github.com/TokenKickstarter/tokenkickstarter-node
cd tokenkickstarter-node
cargo build --release
```

The compiled binary will be at `./target/release/tks-chain-node`.

> **Build time:** ~20-35 minutes on first compile. Subsequent builds use Cargo incremental compilation and are much faster.

### Verify the build

```bash
./target/release/tks-chain-node --version
```

---

## Development Mode (Quick Test)

The fastest way to verify the node works is dev mode — a single-node ephemeral chain:

```bash
./target/release/tks-chain-node --dev
```

This starts a node that:
- Uses pre-funded test accounts
- Produces blocks immediately on transaction submission
- Resets all state on restart

**With external RPC access (for MetaMask or explorer):**

```bash
./target/release/tks-chain-node --dev \
  --rpc-external \
  --rpc-cors=all \
  --rpc-port 9944
```

Then open MetaMask and add a custom network:

| Field | Value |
|-------|-------|
| Network Name | TKS Dev |
| RPC URL | `http://127.0.0.1:9944` |
| Chain ID | `7779` |
| Symbol | TKS |

---

## Join the Testnet

```bash
./target/release/tks-chain-node \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/data \
  --name "MyNode" \
  --rpc-external \
  --rpc-cors=all \
  --bootnodes "/dns/seed.tokenkickstarter.com/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C" \
  --bootnodes "/dns/seed.tkstoken.com/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR" \
  --bootnodes "/dns/seed.tksscan.com/tcp/30333/p2p/12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8"
```

Or use the provided convenience script:

```bash
./network/start-node.sh
```

---

## Pre-funded Development Accounts

In `--dev` mode, these accounts are pre-funded with 1,000 TKS each (standard Hardhat test keys):

| Account | Private Key |
|---------|-------------|
| Account 0 | `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80` |
| Account 1 | `0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d` |
| Account 2 | `0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a` |
| Account 3 | `0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6` |
| Account 4 | `0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926b` |

> **Never use these keys with real funds.** They are publicly known test keys.
