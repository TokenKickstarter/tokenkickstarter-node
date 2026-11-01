# Investor audit r6 (v0.1.18) — remediation tracker

**Live chain (evidence date):** genesis `0x28a554eb58356946b3ac7ce61fa1562cbda2dd1171ee8c7719140e0e33a2e31c`  
**Target release:** **v0.1.20** (runtime **spec 413**) — deploy + WASM upgrade required for gas/weight fix.

## MUST FIX

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | Gas limit under load | **Code fixed; not proven under 20k on spec 413** | Root cause: `WeightPerGas` used **MAX** gas while `BlockGasLimit` was adaptive → [`AdaptiveWeightPerGas`](runtime/src/lib.rs). Harness: [`assert-block-gas-limits.py`](scripts/assert-block-gas-limits.py). Idle scan VPS blocks 108390–108470: **0 violations**. **Needs:** `run-r6-20k-evidence.sh` after spec **413** live. |
| 2 | Throughput / stalls | **Open** | Re-run [`loadtest-20k-transfers.py`](scripts/loadtest-20k-transfers.py) ×3 via [`run-r6-20k-evidence.sh`](scripts/run-r6-20k-evidence.sh) with `R6_FUNDER_SEED` on VPS. |
| 3 | Anchor 2× / trustless | **Partial** | Dedupe: [`engine.rs`](../cipher-shard-executor/src/engine.rs) test `ingest_dedupes_same_msg_id` **PASS**. Docs: [`ANCHOR-VERIFICATION.public.md`](ANCHOR-VERIFICATION.public.md). Slashing still **sudo-only** (`resolve_challenge` TODO). |
| 4 | Multisig sudo executed | **Submitted live; verify inclusion** | VPS **2026-11-01** `production multisig-sudo-force-rotate`: pool hashes **`0xbd974d…372db9`** (member1 `as_multi`), **`0x7ebd17…9ad7dc`** (member2 `approve_as_multi`). Pending for `0x8eeb…`: **0** (stale `0x1cb3…` cleared). **Note:** these are Substrate extrinsics — confirm in block via `chain_getBlock` / explorer, not `eth_getTransactionReceipt`. |
| 5 | Docs overclaim | **In progress** | [`README.md`](../README.md) updated (finality, 500k target, HyperSwarm scope). |
| 6 | Stability / resets | **Policy** | [`MAINNET-STABILITY.md`](MAINNET-STABILITY.md) — freeze **v0.1.20** after deploy; no further genesis resets without 7-day notice. |

## SHOULD FIX

| # | Item | Status |
|---|------|--------|
| 7 | Bootnodes | **Live VPS spec** uses `144.91.122.247:30333` — see [`bootnodes.public.txt`](bootnodes.public.txt). Republish GitHub release raw spec with same. |
| 8 | Shard /ack, /stats, /discovery | **Fixed in v0.1.20 node** — aliases on shard HTTP (not 404). Deploy binary to see on `shard.tokenkickstarter.com`. |
| 9 | Independent validators | **Open** — [`VALIDATORS.md`](VALIDATORS.md) / [`GENESIS-INFLATION.public.md`](GENESIS-INFLATION.public.md). |
| 10 | Allocation / inflation | **Partial** — [`GENESIS-INFLATION.public.md`](GENESIS-INFLATION.public.md). |
| 11 | Repo hygiene | **Partial** — [`LICENSE`](../LICENSE), [`.github/workflows/ci.yml`](../.github/workflows/ci.yml), Frontier **rev pin**, dev chain refused without `--chain` in release builds. |
| 12 | Archive RPC | **Open** — ops: dedicated archive fullnode + nginx route (document when live). |

## Deploy checklist (v0.1.20)

1. Tag `v0.1.20`, CI build + GitHub release (raw spec + bootnodes).
2. VPS `docker compose build && up` — **no purge**.
3. Runtime **set_code** to spec **413** (multisig → sudo or governance process).
4. Run `R6_FUNDER_SEED=… ./network/scripts/run-r6-20k-evidence.sh` on VPS; attach `network/r6-evidence-*` to investor packet.
