# Investor audit r6 (v0.1.18) — remediation tracker

**Live chain (evidence date):** genesis `0x28a554eb58356946b3ac7ce61fa1562cbda2dd1171ee8c7719140e0e33a2e31c`  
**Target release:** **v0.1.20** (runtime **spec 413**) — deploy + WASM upgrade required for gas/weight fix.

## MUST FIX

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | Gas limit under load | **Code fixed (wip→main); not proven under 20k on spec 413** | `AdaptiveWeightPerGas`, **`SLOT_GAS_CAP`**, runtime block-builder gas cap, Aura **2/3 slot** proposal limit. Harness: [`assert-block-gas-limits.py`](scripts/assert-block-gas-limits.py). **Needs:** `set_code` + `run-r6-20k-evidence.sh`. |
| 2 | Throughput / stalls | **Code mitigations; not proven** | [`tx_pool.rs`](../node/src/tx_pool.rs) burst scaling (future queue floor 24k). Re-run 20k ×3 on VPS after **v0.1.20** deploy. |
| 3 | Anchor 2× / trustless | **Partial (stronger on wip)** | Dedupe + shared [`cipher-merkle`](../cipher-merkle/). On-chain **`prove_false_anchor`** + poster slash burn; docs updated. Not trustless without running proofs. |
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
| 12 | Archive RPC | **Documented** — [`ARCHIVE-RPC.public.md`](ARCHIVE-RPC.public.md); archive host not deployed yet. |

## Deploy checklist (v0.1.20)

1. Merge **wip/r6-audit-fixes** → **main**, push (no tag required).
2. VPS: `VERSION=v0.1.20 bash network/scripts/vps-docker-github-release.sh` — **no purge**.
3. Runtime **set_code** to spec **413** WASM (multisig → sudo); confirm `state_getRuntimeVersion.specVersion == 413`.
4. Run `R6_FUNDER_SEED=… ./network/scripts/run-r6-20k-evidence.sh` on VPS; attach `network/r6-evidence-*` to investor packet.
