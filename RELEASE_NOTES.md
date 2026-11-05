# TKS Node release notes

Professional changelog for [GitHub Releases](https://github.com/TokenKickstarter/tokenkickstarter-node/releases). The release workflow merges the section for the tagged version with `.github/release-footer.md`.

---

## v0.1.22

**Cipher federated messaging** — instant WebSocket push + discovery; same runtime as v0.1.21 unless you also run `set_code`.

### Features

- **Relay → signal push:** after `POST /store`, relay notifies co-located signal server (`/internal/push`) for ~100ms delivery to online peers.
- **Discovery:** `GET /discovery` advertises `signal_ws` / `self_signal_ws` per validator (no fixed hostname required).
- **CLI flags:** `--cipher-public-signal-ws`, `--cipher-signal-push-url` (default `http://127.0.0.1:4003/internal/push`).
- **`network/vps-signal-server`:** Node.js WS hub + internal push endpoint (deploy beside relay on :4003).

### Fixes

- **HTTP `/fetch`:** JSON error path no longer breaks the HTTP handler.
- **VPS Docker script:** SHA256 verify uses unpacked binary name from release `SHA256SUMS.txt`.

### Deploy

1. Tag **v0.1.22**, wait for GitHub Release assets.
2. On VPS: `VERSION=v0.1.22 bash network/scripts/vps-docker-github-release.sh`.
3. `cd network/vps-signal-server && ./deploy.sh` (or refresh container) and nginx `/ws` → :4003.
4. Start node with `--cipher-public-relay-url` and `--cipher-public-signal-ws` pointing at your domain.

---

## v0.1.21

**Governance / VPS hotfix** — same runtime **spec 413** WASM as v0.1.20; no genesis change.

### Fixes

- **Extrinsic signing:** CLI multisig / fund / shard helpers sign with **on-chain** `spec_version` + `transaction_version` from RPC (fixes `Transaction has a bad signature` when binary is ahead of live runtime during `multisig-sudo-set-code`).

### Deploy

1. Tag **v0.1.21**, wait for GitHub Release assets.
2. On VPS (`/opt/tks-docker`): `VERSION=v0.1.21 bash network/scripts/vps-docker-github-release.sh` (Docker binary only, no purge).
3. Run **`live-multisig-sudo-set-code.sh`** with existing `config/tks_runtime.compact.compressed.wasm` if spec is still 412.

---

## v0.1.20

**R6 investor audit remediation.** Runtime **spec 413** (`set_code` required on live chain).

### Fixes

- **Gas enforcement:** `AdaptiveWeightPerGas` ties block weight to live `CurrentGasLimit` (fixes blocks where `gasUsed` could exceed `gasLimit` under load). Harness: `network/scripts/assert-block-gas-limits.py`, `run-r6-20k-evidence.sh`.
- **Anchors:** batch `msg_id` dedupe (relay + ingest); [`ANCHOR-VERIFICATION.public.md`](network/ANCHOR-VERIFICATION.public.md).
- **Multisig:** timepoint-safe approve (v0.1.19+); live smoke script [`live-multisig-sudo-force-rotate.sh`](network/scripts/live-multisig-sudo-force-rotate.sh).
- **Shard HTTP:** `/stats`, `/discovery`, `POST /ack` (non-404) on shard vhost.
- **Hygiene:** `LICENSE`, CI (`ci.yml`), Frontier pinned to `0eeb4266`, release builds refuse implicit Alice/Bob chain without `TKS_ALLOW_DEV_CHAIN`.
- **Docs:** README finality / 500k / HyperSwarm scope; [`R6-AUDIT-RESPONSE.md`](network/R6-AUDIT-RESPONSE.md).

### Deploy

1. Tag **v0.1.20**, upgrade VPS Docker (no purge).
2. Submit **spec 413** WASM via multisig → sudo.
3. Run **`run-r6-20k-evidence.sh`** ×3 on VPS; attach logs to investor packet.

---

## v0.1.19

R4 audit closure — **no genesis change** (same `0x28a554…`).

### Fixes

- **Published bootnodes:** `tks-network-spec-raw.json` + [`network/bootnodes.public.txt`](network/bootnodes.public.txt) use public VPS P2P (`144.91.122.247:30333`); `build-tks-network-spec.sh` overwrites dev `127.0.0.1` from `build-spec`.
- **Multisig:** `multisig-sudo-force-rotate` waits for on-chain **timepoint** before `approve_as_multi`; **`production multisig-list-pending`** / **`multisig-cancel-pending`** for stuck calls.
- **Anchors:** shard batch engine **dedupes `msg_id`** (relay + forwarded ingest no longer double-count on L1).
- **Docs:** [`network/AUDIT-OPEN-ITEMS.md`](network/AUDIT-OPEN-ITEMS.md), [`SUDO-MULTISIG.public.md`](network/SUDO-MULTISIG.public.md) (regen), [`MAINNET-STABILITY.md`](network/MAINNET-STABILITY.md), [`R4-L1-REBASELINE.md`](network/R4-L1-REBASELINE.md).

### Deploy

1. Replace `config/chainspec-raw.json` + `bootnodes.txt` on VPS (`network/scripts/vps-patch-chainspec-bootnodes.sh`).
2. Rebuild Docker image **v0.1.19**, recreate containers (no purge).
3. Run `clear-sudo-multisig-pending.sh` if `multisig-list-pending` shows entries.
4. `./network/scripts/public-rpc-test.sh` and `./network/scripts/verify-sudo-multisig.sh`.

---

## v0.1.17

**New production genesis** (mainnet regen). **Not** a drop-in binary upgrade on old chain data — purge all validator/fullnode data and deploy the release **`tks-network-spec-raw.json`**.

### Node

- **`tks_anchorStats` RPC** — `total_anchors`, `total_messages` for audits.
- **Shard executor `POST /store`** — same JSON as relay (compat for `shard.*` vhosts); prefer relay `/store` in production.
- Wallet tooling: `generate-production-wallets.sh`, `vps-regen-v0.1.17.sh`.

### Genesis / sudo

- Sudo **`sudo.key`** matches **pallet-multisig** derivation (`production multisig-account`), not legacy `0xd0ec…`.
- New validator stashes and sudo signers — see [network/GENESIS-ALLOCATION.public.md](network/GENESIS-ALLOCATION.public.md).

### Deploy

1. Publish/release includes **`tks-network-spec-raw.json`** + SHA-256 sidecar.
2. On VPS: copy **production-keys/** (secrets, never in git), raw spec → `config/tks-testnet-spec-raw.json`, run **`network/scripts/vps-regen-v0.1.17.sh`** (purge + session keys + v0.1.17 binary).
3. Re-run **`public-rpc-test.sh`** with new `EXPECTED_GENESIS` from `chain_getBlockHash(0)`.

---

## v0.1.16

Public RPC identity hardening at the **node** (HTTP + WebSocket). **Runtime spec 412 unchanged.**

### Fixes

- **`--rpc-hide-node-identity` works end-to-end:** Patched `sc-service` `gen_rpc_module` removes `system_localPeerId`, `system_localListenAddresses`, `system_peers`, and `system_unstable_networkState` after all RPC modules are merged (no nginx Lua required for audits).
- Ops: public fullnodes should keep `--rpc-hide-node-identity`; reverse-proxy filtering is optional defense-in-depth.

### Deploy

1. Upgrade binary to **v0.1.16**, rebuild Docker, `docker compose up -d --force-recreate` on fullnode.
2. Fullnode Cmd must include `--rpc-hide-node-identity` (see `network/docker-compose-live.yml`).
3. DNS: single **A** record for `rpc` → VPS; grey or orange cloud both OK once SSL vhost serves `rpc` on 443.

---

## v0.1.15

R4 audit follow-up (node + docs). **Runtime spec 412 unchanged** unless you regen genesis for multisig sudo fix.

### Fixes

- **Anchors:** `load_ecdsa_from_seed_text` now derives the signing pair with `//mnemonic` (was `from_legacy_string` → `InvalidTransaction::BadProof`). Anchor submitter uses `TransactionSource::External`.
- **Multisig helper:** `production multisig-account` uses the same encoding as `pallet_multisig::multi_account_id` (`b"modlpy/utilisuba"` prefix). Legacy genesis `0xd0ec…` documented in [network/SUDO-MULTISIG.public.md](network/SUDO-MULTISIG.public.md).
- **Public RPC:** Documented Substrate merge order; use `network/nginx/rpc-deny-node-identity.conf` on the public reverse proxy (v0.1.14 `strip` in `create_full` did not affect `sc_rpc::system`).

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
