# Security changelog

Track security-relevant release and operational changes. Pair with [RELEASE_NOTES.md](RELEASE_NOTES.md) for feature history.

## Unreleased (post v0.1.8 audit)

| Area | Change |
|------|--------|
| Cipher metrics | Fixed double increment of `lifetime_ingress` (relay `/store` + shard `/ingest` on the same `HybridController`). |
| Hybrid API | `GET /hybrid/stats` and discovery hybrid block include `metrics_note` clarifying configured ceilings vs measured TPS. |
| Documentation | [AUDIT-REMEDIATION-v0.1.8.md](network/AUDIT-REMEDIATION-v0.1.8.md), [PRODUCTION-DEPLOYMENT-CHECKLIST.md](network/PRODUCTION-DEPLOYMENT-CHECKLIST.md), [MAINNET-GENESIS-CHECKLIST.md](network/MAINNET-GENESIS-CHECKLIST.md). |

### Open before any investor-facing release (from internal audit 2026-10-25)

- Replace Alice/Bob validator session keys; rotate on-chain authority set.
- Retire leaked sudo key; migrate ~400M TKS treasury to multisig; remove or transfer sudo.
- New production genesis (do not reuse reset testnet chain).
- Rename chain from `TKS Local Testnet` / `Local` to production naming in chainspec.
- Register shard validators on-chain; verify L1 `post_anchor` extrinsics (not only JSON artifacts).
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
