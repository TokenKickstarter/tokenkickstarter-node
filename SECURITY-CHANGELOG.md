# Security changelog

Track security-relevant release and operational changes. Pair with [RELEASE_NOTES.md](RELEASE_NOTES.md) for feature history.

## v0.1.9

| Area | Change |
|------|--------|
| Cipher metrics | Fixed double increment of `lifetime_ingress` (relay `/store` + shard `/ingest` on the same `HybridController`). |
| Hybrid API | `metrics_note` on hybrid snapshots (configured ceilings vs measured TPS). |
| L1 anchors | Optional node auto-submitter for `post_anchor` from anchor JSON (`cipher-anchor` module). |
| Shard registry | Production genesis can require staked validators before HyperSwarm registration. |
| Adaptive gas | `record_gas_used` capped at current block gas limit. |
| Runtime | **spec_version 410** (upgrade required on live chains). |
| Tooling | Production genesis scripts; `tks-network` chainspec loader; `network/production-keys/` gitignored. |

### Still open before investor-facing release (ops / genesis)

- Replace Alice/Bob validator session keys; rotate on-chain authority set.
- Retire leaked sudo key; migrate ~400M TKS treasury to multisig; remove or transfer sudo.
- Deploy filled `tks-network-spec.json` (do not point public DNS at `tks_local` dev genesis).
- Register shard validators on-chain; confirm `post_anchor` extrinsics succeed with staked ECDSA account.
- Independent validators (target 4–7 operators).

## v0.1.8

| Area | Status |
|------|--------|
| Public RPC | Safe methods only; key insert and admin calls rejected (verified in audit). |
| Binary integrity | Tag matches commit; SHA256SUMS verified. |
| Known risks | Public dev validator keys, leaked sudo account, local testnet naming on live RPC. |

## v0.1.3

| Area | Status |
|------|--------|
| Hybrid / spec 409 | Runtime features shipped; on-chain activation requires `set_code` on existing chains. |
| Security notes | No dedicated security section in release notes (gap addressed from v0.1.9 onward). |

## v0.1.1 / v0.1.0

Initial public binaries. No security changelog entry was published at release time.
