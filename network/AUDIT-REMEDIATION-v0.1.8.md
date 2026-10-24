# Audit remediation tracker (v0.1.8 internal)

**Source:** TKS L1 + Cipher investor audit, 2026-10-24 (Dhaka 02:26–02:36).  
**Classification:** Internal until section H items are closed.  
**Full report (local):** `TKS_Investor_Audit_v0.1.8.md` / PDF (not committed to this repo).

This document maps audit verdicts to owners, repo changes, and re-test criteria.

## Verdict summary

| ID | Topic | Verdict | Re-test when |
|----|--------|---------|--------------|
| A | Liveness / naming | PASS liveness; FAIL naming | Production chainspec live; `system_chain` not "TKS Local Testnet" |
| B | RPC security | PASS | After any RPC flag change |
| C | L1 throughput | PASS (measured) | After adaptive gas or block pipeline changes |
| D | Cipher relay demo | PASS; 500k NOT VERIFIED | After hybrid fix + distributed load test |
| E | Shards / anchors | NOT VERIFIED | `ShardRegistry` validators > 0 and L1 anchor count > 0 |
| F | `@username` batch | PASS (feature) | Optional throughput benchmark |
| G | Release / Docker | G2 PASS; Docker NOT VERIFIED | Checklist log attached (see deployment doc) |
| H | Security | OPEN (3/3) | New genesis + keys + multisig |

## Must fix before investors (section H)

### H1 — Validator keys (Alice / Bob)

**Finding:** On-chain validators still use well-known `//Alice` and `//Bob` Aura/GRANDPA keys. Anyone can derive them and attempt block production.

**Remediation:**

1. Generate fresh sr25519 (Aura) and ed25519 (GRANDPA) keys per validator offline.
2. Inject via node keystore (`author_insertKey`) on a **local-only** RPC or during genesis.
3. Update session keys on-chain (`session.setKeys`) and authority set to match new stash accounts.
4. Never expose `author_insertKey` on public RPC (already blocked; keep it that way).

**Repo:** [MAINNET-GENESIS-CHECKLIST.md](./MAINNET-GENESIS-CHECKLIST.md), [inject-validator-keys.sh](./inject-validator-keys.sh).

### H2 — Sudo and treasury (leaked key)

**Finding:** Sudo account matches leaked dev material; holds ~400M TKS (~44% supply).

**Remediation:**

1. Treat the key as public; do not use it for new transactions except migration.
2. Create multisig (for example 3-of-5 on hardware wallets).
3. Transfer treasury and sudo capabilities to multisig or governance pallet.
4. Remove `pallet-sudo` or set sudo to multisig after migration.

### H3 — Chain naming and resets

**Finding:** Live RPC reports `TKS Local Testnet` and `chainType: Local`. Chain was reset again during audit (low block height).

**Remediation:**

1. Build production chainspec: name `TKS Network` (or product name), `ChainType::Live`, id `tks_mainnet` (or distinct testnet id).
2. Keep **dev** spec (`local_testnet_config`) for local Docker only; do not point public DNS at dev genesis.
3. Document genesis hash and spec version in release notes when launching production.

## Should fix soon (claims and metrics)

### E — Shards and L1 anchors

**Finding:** 0 shard validators, 0 L1 anchors after 1,001 sealed batches.

**Cause:** Embedded executor writes DA and anchor **JSON artifacts** under `{base-path}/cipher/anchors/`; it does not submit `HyperSwarmAnchor.post_anchor` extrinsics automatically. Validators must register in `ShardRegistry` and submit anchors (or automation must be added).

**Remediation:**

1. Validator calls `ShardRegistry.register_validator(region, endpoint)`.
2. Operator or bot reads anchor JSON and submits extrinsic with assigned validator key.
3. Re-audit: `tks_listSwarmValidators` non-empty; on-chain anchor count increases after seal interval.

### D — Relay throughput honesty

**Finding:** Hybrid `lifetime_ingress` increased by 2 per message (fixed in repo: count only at relay `/store`).

**Finding:** `mode` stayed `idle` at ~256 msg/s (below 1k floor); 500k not verified.

**Remediation:** Publish only `measured_tps` and audit scripts; label `settings.target_*` as configured ceilings (see `metrics_note` in hybrid snapshot).

### C — Gas and block time

**Finding:** One block slightly over 50M gas limit; sustained wall-clock TPS ~190–255 vs higher inclusion peak; block interval stretched to ~2s under load.

**Remediation:** Review `pallet-adaptive-gas` enforcement vs EVM block builder; profile validator CPU; document honest sustained TPS separately from per-block peak.

### Multi-node Cipher 500k

**Finding:** Not verified.

**Remediation:** `tools/cipher-loadtest` phase 2/3; 8+ shard executors; independent operator replay within agreed tolerance.

## Deploy note corrections (audit)

| Deploy note | Measured reality |
|-------------|------------------|
| Session keys injected | On-chain authorities still Alice/Bob until rotated |
| 500k / 8 shards in discovery | Configuration targets, not measured capacity |
| Shard ingest implies L1 anchors | Ingest queues batches; L1 anchors need extrinsic submission |
| Transfer load tests at 0.0001 TKS | Below minimum balance; txs included but failed (use >= min balance, e.g. 0.01 TKS) |

## Re-audit process

After each remediation batch:

1. Run the same public RPC / relay / shard black-box suite.
2. Attach raw JSON under dated folder (for example `audit-YYYYMMDD/`).
3. Update this tracker and [SECURITY-CHANGELOG.md](../SECURITY-CHANGELOG.md).
4. Only then mark the PDF/markdown report suitable for external investors.
