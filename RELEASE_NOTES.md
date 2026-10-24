# TKS Node release notes

Professional changelog for [GitHub Releases](https://github.com/TokenKickstarter/tokenkickstarter-node/releases). The release workflow merges the section for the tagged version with `.github/release-footer.md`.

---

## v0.1.9

Audit remediation release (code + operator tooling). **Runtime spec 410** — existing chains need `set_code` before pallet fixes apply.

### Fixes

- Hybrid `lifetime_ingress` no longer double-counts when relay forwards to shard ingest on the same node.
- Hybrid snapshot includes `metrics_note` (configured ceilings vs measured TPS).
- `pallet-adaptive-gas`: block gas accumulator capped at `CurrentGasLimit` (no overshoot in metrics).
- `pallet-shard-registry`: optional `require_staked_validators` genesis flag; `register_validator` checks staking ledger when enabled.

### Cipher / L1 anchors

- Node watches `{base-path}/cipher/anchors/` and submits signed `HyperSwarmAnchor::post_anchor` extrinsics (`--cipher-auto-post-anchor`, `--cipher-anchor-seed-file`).
- Embedded shard executor follows on-chain `ShardRegistry::CurrentEpoch` (5s poll).
- Docker live compose: anchor seed mount and auto-post flags documented in [network/docker-compose-live.yml](network/docker-compose-live.yml).

### Production chainspec

- `--chain tks-network` loads [network/tks-network-spec.json](network/tks-network-spec.json) (Live / `TKS Network` template).
- Scripts: [network/scripts/bootstrap-production-genesis.sh](network/scripts/bootstrap-production-genesis.sh), [network/scripts/build-tks-network-spec.sh](network/scripts/build-tks-network-spec.sh).

### Upgrade (Linux amd64 example)

```bash
VERSION=v0.1.9
curl -L -o tks-chain-node \
  "https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/tks-chain-node-${VERSION}-linux-amd64"
chmod +x tks-chain-node
# Restart node; mount cipher anchor seed if using auto-post; run set_code for spec 410.
```

---

## v0.1.8

**Recommended production binary** (HyperSwarm RPC and Cipher stack compile cleanly on CI).

### Fixes

- Register HyperSwarm JSON-RPC before `Web3` consumes the shared client `Arc` (`tks_listSwarmValidators`, `tks_hyperswarmNetworkInfo`).

### Includes (from v0.1.4 through v0.1.7)

- Embedded Cipher relay (HTTP `:4002`), shard executor (`:4010`), HyperSwarm libp2p mesh (`:4001`).
- libp2p torrent-style ciphertext chunk pull and mesh HTTP fanout.
- Hybrid throughput controller (`cipher-hybrid`, approximately 1k to 500k aggregate Cipher messaging under load).
- Runtime support for name batch registration and adaptive gas (spec 408/409) in WASM; live chains require a `set_code` upgrade to activate on-chain rules.

### Documentation

- README and `network/` operator guides updated for production flags and upgrades.
- [CIPHER-CLIENT-CHUNKING-SPEC.md](network/CIPHER-CLIENT-CHUNKING-SPEC.md) (512 KiB client pieces).

### Upgrade (Linux amd64 example)

```bash
VERSION=v0.1.8
curl -L -o tks-chain-node \
  "https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/tks-chain-node-${VERSION}-linux-amd64"
chmod +x tks-chain-node
# Replace binary and restart systemd or docker compose (keep same --base-path and chainspec)
```

---

## v0.1.7

### Fixes

- Node build: add `serde` and `sp-storage` dependencies for HyperSwarm RPC types.
- Align `StorageProvider::storage_keys` with polkadot-sdk stable2512 (prefix + start key).
- Thread `BE` through `HyperswarmRpc` generics; import `HyperswarmRpc` in `rpc.rs`.

---

## v0.1.6

Release tag only (version bump). Same artifacts intent as v0.1.5 plus client chunking specification document on `main`.

---

## v0.1.5

### Features

- libp2p request-response protocol for ciphertext chunk pull (`/tks-hyperswarm/chunk/1.0.0`).
- Gossip notifications when chunks are available on peers.

---

## v0.1.4

### Features

- HyperSwarm libp2p relay mesh and HTTP discovery endpoint.
- JSON-RPC: `tks_listSwarmValidators`, `tks_hyperswarmNetworkInfo`.
- Mesh fanout for relay redundancy.

---

## v0.1.3

### Features

- `cipher-hybrid`: adaptive Cipher throughput (floor approximately 1k, ceiling approximately 500k aggregate msg/s).
- `pallet-name-registry`: batch unsigned registration path (runtime spec 408).
- `pallet-adaptive-gas`: initial/min block gas targeting approximately 1k L1 TPS at idle (spec 409).
- CI fix: capture cipher base path before config move in node service startup.

### Notes

- Cipher aggregate TPS and L1 EVM TPS are separate metrics; see [CIPHER-TKS-THROUGHPUT-SPEC.md](network/CIPHER-TKS-THROUGHPUT-SPEC.md).

---

## v0.1.1

EVM + Frontier node baseline, testnet chainspec, multi-platform release binaries.

---

## v0.1.0

Initial public node release.
