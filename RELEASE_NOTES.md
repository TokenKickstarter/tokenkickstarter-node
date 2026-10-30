# TKS Node release notes

Professional changelog for [GitHub Releases](https://github.com/TokenKickstarter/tokenkickstarter-node/releases). The release workflow merges the section for the tagged version with `.github/release-footer.md`.

---

## v0.1.15

R4 audit follow-up (node + docs). **Runtime spec 412 unchanged** unless you regen genesis for multisig sudo fix.

### Fixes

- **Anchors:** `load_ecdsa_from_seed_text` now derives the signing pair with `//mnemonic` (was `from_legacy_string` → `InvalidTransaction::BadProof`). Anchor submitter uses `TransactionSource::External`.
- **Multisig helper:** `production multisig-account` uses the same encoding as `pallet_multisig::multi_account_id` (`b"modlpy/utilisuba"` prefix). Legacy genesis `0xd0ec…` documented in [network/SUDO-MULTISIG.public.md](network/SUDO-MULTISIG.public.md).
- **Public RPC:** `--rpc-hide-node-identity` strips `sc_rpc::system` methods **after** the full RPC module is built (v0.1.14 only removed frame methods, which never exposed `localPeerId`).

### Ops

- `network/scripts/publish-production-chainspec.sh` — pull live raw spec + SHA-256 for releases.

---

## v0.1.14

Audit remediation (node + ops). **No runtime spec bump** — deploy binary + Docker flags on existing spec **412** genesis.

### Fixes

- **Mempool bursts:** `--pool-future-divisor` (default `10`) scales pool caps so the *future* queue matches nominal `--pool-limit` (fixes ~10×2k transfer loss). See [network/TX-POOL-AND-REORG.md](network/TX-POOL-AND-REORG.md).
- **Anchors:** Hybrid shard executor always posts L1 anchors for the node’s `--cipher-shard-id` (was using logical shard indices 1…N → `NotShardValidator`).
- **Anchor signer:** `--cipher-anchor-seed-file` accepts JSON `secretPhrase` / `//URI` (same as production keys).
- **Public RPC:** `--rpc-hide-node-identity` removes `system_localPeerId`, listen addresses, and raw peer lists.
- **Dev keys:** `EthDevSigner` off by default; use `--ethereum-dev-signer` only on dev chains.

### Ops / audit docs

- [network/SUDO-MULTISIG.public.md](network/SUDO-MULTISIG.public.md), [network/GENESIS-ALLOCATION.public.md](network/GENESIS-ALLOCATION.public.md), [network/SHARD-ENDPOINTS.md](network/SHARD-ENDPOINTS.md), [network/AUDIT-READ-ACCESS.md](network/AUDIT-READ-ACCESS.md)
- Regression: `network/scripts/loadtest-20k-transfers.py`
- Release bundle includes `tks-network-spec.template.json` (not local 127.0.0.1 testnet as production spec)

### Deploy (VPS `/opt/tks-docker`)

1. Upgrade binary to **v0.1.14**, rebuild images, `docker compose up -d --force-recreate`
2. Fullnode Cmd must include: `--pool-type fork-aware --pool-future-divisor 10 --rpc-hide-node-identity`
3. Sweep any **//Alice** balance on live chain to treasury (never fund dev H160s on mainnet)

---

## v0.1.13

### Runtime **spec 412** — **new genesis required**

Production mainnet launched on v0.1.12 must **wipe and redeploy** with this release. Balances on v0.1.12 at `0x02…` / `0x03…` “addresses” (truncated compressed pubkeys) **cannot be signed for**; they are not standard Ethereum H160s.

### Fixes

- **`get_account_id_from_seed`:** derive H160 via `fp_account::AccountId20` (Keccak-256 of uncompressed secp256k1 pubkey), matching `EthereumSignature` verification and MetaMask.
- Regenerated **`network/tks-network-spec.json`** and **`tks-network-spec-raw.json`** from existing `production-keys/` (same mnemonics, **correct** recipient addresses).

### Ops

- Re-assemble after key changes: `network/scripts/assemble-tks-network-spec.sh` then `build-tks-network-spec.sh`.
- Audit: `network/scripts/audit-genesis-addresses.py`.
- Coordinate validator **chain-spec replace + purge** on VPS; update `genesis-summary.json` / session keys as needed.
- **Do not** upgrade v0.1.12 in place — genesis hash and all funded H160s change.

---

## v0.1.12

### Runtime **spec 411**

- `pallet-multisig` for production sudo (2-of-N derived account).
- Production genesis tooling: `bootstrap-production-genesis.sh`, Live **TKS Network** spec, HyperSwarm validators at genesis.

### Fixes

- **AccountId20 extrinsics:** Keccak-256 signing for `EthereumSignature` (fixes `bad signature` on `cipher-register-shard` and anchor auto-post).
- **AccountId20 identity:** Cipher/signer accounts match genesis `get_account_id_from_seed` (not raw `pair.public()`).

### Ops

- `cipher-register-shard` requires `--seed-file` (no default `//Alice`).
- `--disable-cipher-stack` for validator-only containers.
- Cipher relay + shard: **GET `/version`** (node version, spec version, chain name, genesis hash).
- `production multisig-sudo-force-rotate` — smoke test for multisig → sudo dispatch.
- Release CI rejects `production-keys` / secret material in artifacts.

---

## v0.1.11

### Ops

- CLI: `tks-chain-node cipher-register-shard` — `ShardRegistry.register_validator` + sudo `force_rotate` via validator RPC (Frontier ECDSA signing).

---

## v0.1.10

### Fixes

- **Cipher auto-anchor:** avoid `SignedPayload::new` on the async executor (fixes node panic when `--cipher-auto-post-anchor` is enabled). Use `SignedPayload::from_raw` with explicit version/genesis data.

### Upgrade (Linux amd64 example)

```bash
VERSION=v0.1.10
curl -L -o tks-chain-node \
  "https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/tks-chain-node-${VERSION}-linux-amd64"
chmod +x tks-chain-node
```

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
