### Downloads

| Platform | Architecture | File |
|----------|-------------|------|
| macOS | Apple Silicon (M1/M2/M3/M4) | `tks-chain-node-*-macos-arm64` |
| macOS | Intel (x86_64) | `tks-chain-node-*-macos-amd64` |
| Linux | x86_64 (servers, VPS) | `tks-chain-node-*-linux-amd64` |
| Linux | ARM64 (Raspberry Pi, VPS) | `tks-chain-node-*-linux-arm64` |
| Windows | x86_64 | `tks-chain-node-*-windows-amd64.exe` |

---

### Quick install (Linux/macOS)

```bash
curl -sSf https://raw.githubusercontent.com/TokenKickstarter/tokenkickstarter-node/RELEASE_VERSION/network/install.sh | bash
```

Replace `RELEASE_VERSION` with this tag (for example `v0.1.8`).

---

### Join the network (full node)

```bash
./tks-chain-node \
  --chain tks-testnet-spec-raw.json \
  --base-path ~/.tks/data \
  --rpc-external --rpc-cors all \
  --name "MyNode"
```

### Production RPC with Cipher and HyperSwarm

See [README.md](https://github.com/TokenKickstarter/tokenkickstarter-node/blob/main/README.md#production-full-node-cipher--hyperswarm) and `network/docker-compose-live.yml`.

```bash
./tks-chain-node \
  --chain /path/to/chainspec-raw.json \
  --base-path /data \
  --rpc-port 9944 --rpc-external --rpc-cors all --ws-external \
  --port 30333 \
  --enable-cipher-relay --cipher-relay-port=4002 \
  --enable-cipher-shard-executor --cipher-shard-port=4010 --cipher-shard-id=0 \
  --enable-cipher-hyperswarm --cipher-hyperswarm-port=4001 \
  --cipher-public-relay-url=https://relay.example.com
```

### Run as validator

```bash
./tks-chain-node \
  --chain tks-testnet-spec-raw.json \
  --validator \
  --name "MyValidator"
```

Full steps: `network/start-validator.sh`, `network/inject-validator-keys.sh`.

---

### Network

| | |
|-|-|
| Chain ID | 7779 (`0x1E63`) |
| Token | TKS (18 decimals) |
| Block time | ~1 second |
| Consensus | Aura + GRANDPA |
| P2P | 30333 |
| RPC | 9944 |

---

Full documentation: [README.md](https://github.com/TokenKickstarter/tokenkickstarter-node#readme) · [RELEASE_NOTES.md](https://github.com/TokenKickstarter/tokenkickstarter-node/blob/main/RELEASE_NOTES.md)
