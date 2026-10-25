# TKS Network — production genesis (fresh chain)

Use this when the live chain must **not** reuse Alice/Bob, dev sudo, or empty HyperSwarm registry. A new genesis changes the genesis hash; all nodes must purge data and sync from block 0.

## What you get

| Item | Value |
|------|--------|
| Chain name | **TKS Network** |
| Chain type | **Live** |
| Chain ID (EVM) | 7779 |
| Runtime | **spec_version 411** (includes `pallet-multisig`) |
| Consensus | 2 validators, unique Aura/GRANDPA keys |
| Sudo | **Multisig account** (default 2-of-3 ECDSA members) |
| HyperSwarm | **2 validators in `shardRegistry` at genesis** + epoch rotation |
| Cipher anchor | Validator1 ECDSA seed in `production-keys/cipher-anchor-seed.txt` |

## Offline steps (operator machine)

```bash
cd tks-substrate-node
cargo build --release -p tks-chain-node

cp network/production-genesis-input.example.json network/production-genesis-input.json
# Edit allocations, shard URLs, bootnodes if needed.

./network/scripts/bootstrap-production-genesis.sh
```

Outputs:

- `network/production-keys/` — **secret** mnemonics (gitignored)
- `network/production-keys/genesis-summary.json` — public addresses for runbooks
- `network/tks-network-spec-raw.json` — mount in Docker as chainspec

## VPS (Docker only)

1. **Stop** stack; **delete** volume data for fullnode + validators (new genesis).
2. Replace `config/tks-testnet-spec-raw.json` with the new **raw** spec (or bind-mount `tks-network-spec-raw.json`).
3. Inject new **Aura/GRANDPA** session keys from `production-keys/validator*-*.json` (never commit).
4. Mount `cipher-anchor-seed.txt` from production-keys for the fullnode.
5. Start validators first, then fullnode; verify `system_chain` → **TKS Network**, new genesis hash, `tks_listSwarmValidators` non-empty.

## Sudo after launch

The genesis `sudo.key` is the **multisig-derived** address, not a single mnemonic. Use `multisig.asMulti` → `sudo.sudo` for runtime upgrades and registry ops. Remove sudo when governance is ready (`sudo.sudo(sudo.setKey(...))` or `sudo.removeSudo` via multisig).

## Allocation reference

Default buckets match [GENESIS-AND-BRIDGE.md](./GENESIS-AND-BRIDGE.md) (1B TKS). Override addresses in `production-genesis-input.json` to use your own cold wallets.
