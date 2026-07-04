# Running a Node

## Node Types

| Type | Description | Use Case |
|------|-------------|----------|
| **Development** | Single-node ephemeral chain | Local testing |
| **Full Node** | Syncs full chain state | RPC access, explorers |
| **Validator** | Produces and finalizes blocks | Earn rewards |
| **Bootnode** | Provides peer discovery | Network infrastructure |
| **Archive Node** | Stores all historical state | Block explorers, analytics |

---

## Development Node

Starts a single-node chain with pre-funded accounts. State resets on restart.

```bash
./target/release/tks-chain-node --dev
```

**With public RPC (for wallet / explorer access):**

```bash
./target/release/tks-chain-node --dev \
  --rpc-external \
  --rpc-cors=all \
  --rpc-port 9944
```

---

## Full Node (Testnet)

A full node syncs the entire chain and optionally exposes RPC. Requires the chain spec and connectivity to bootnodes.

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

Or use the convenience script:

```bash
./network/start-node.sh
```

---

## Archive Node

An archive node retains the full historical state at every block, required for querying past storage values.

```bash
./target/release/tks-chain-node \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/archive \
  --name "MyArchive" \
  --pruning archive \
  --rpc-external \
  --rpc-cors=all
```

---

## Bootnode

Bootnodes provide peer discovery. They do not produce blocks and do not participate in consensus.

```bash
./target/release/tks-chain-node \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/bootnode \
  --node-key-file network/bootnode.key \
  --listen-addr /ip4/0.0.0.0/tcp/30333 \
  --no-grandpa \
  --no-telemetry
```

See `network/start-bootnode.sh` for a ready-to-use script.

---

## Running as a systemd Service (Linux)

The `network/` directory includes pre-configured systemd unit files.

**Install and enable the full node service:**

```bash
sudo cp network/tks-chain-node /usr/local/bin/
sudo cp network/tks-node.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now tks-node
```

**Check status:**

```bash
sudo systemctl status tks-node
journalctl -u tks-node -f
```

---

## Data Directory Layout

By default, node data is stored in `~/.tks/data/` (or `$HOME/.local/share/tks-chain-node` depending on the OS).

```
~/.tks/data/
├── chains/
│   └── tks_network/
│       ├── db/                 RocksDB state database
│       ├── keystore/           Session keys (validators only)
│       └── network/            libp2p peer keys
└── ...
```

**Purge chain state (reset to genesis):**

```bash
./target/release/tks-chain-node purge-chain \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/data
```

Or use the convenience script:

```bash
./network/purge-chain.sh
```

---

## Monitoring

The node exposes a Prometheus metrics endpoint on port `9615` by default.

```bash
# Enable metrics
./target/release/tks-chain-node ... --prometheus-external
```

Scrape target: `http://<node-ip>:9615/metrics`

---

## Common Startup Flags

| Flag | Description |
|------|-------------|
| `--chain <spec>` | Chain spec JSON file |
| `--base-path <dir>` | Data directory |
| `--name <name>` | Node display name (visible in telemetry) |
| `--rpc-external` | Listen on all interfaces for RPC |
| `--rpc-cors=all` | Allow all CORS origins |
| `--rpc-port <port>` | RPC port (default: 9944) |
| `--ws-port <port>` | WebSocket port (default: 9944) |
| `--port <port>` | P2P port (default: 30333) |
| `--pruning archive` | Keep full historical state |
| `--validator` | Run as validator |
| `--bootnodes <addr>` | Bootnode multiaddr |
| `--no-telemetry` | Disable telemetry |
| `--prometheus-external` | Expose Prometheus metrics |

See [CLI Reference](CLI-Reference) for the full list.
