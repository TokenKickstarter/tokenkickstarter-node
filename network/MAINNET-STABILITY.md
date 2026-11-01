# Mainnet stability & release policy

## What happened (Oct 31 – Nov 1, 2026 UTC)

| Event | Purpose |
|--------|---------|
| **Genesis regen (v0.1.17)** | Fix sudo multisig derivation (`0x8eeb…`), new operator keys, spec 412 Live chainspec. **Required purge** — not a runtime-only upgrade. |
| **v0.1.16 – v0.1.18** | Public RPC hardening (hide node identity), anchor/shard audit fixes, `production fund-h160`, smoke tests. **No additional genesis changes** after v0.1.17. |
| **v0.1.19** | Bootnodes in published spec, multisig timepoint fix, anchor batch dedupe, audit docs. **No genesis change** — chainspec bootNodes only affect new syncers. |

## Commitments going forward

1. **No unplanned genesis resets.** Further regen requires multisig governance, published rationale, and **≥ 7 calendar days** notice on releases + [`GENESIS-ALLOCATION.public.md`](./GENESIS-ALLOCATION.public.md).
2. **Release cadence:** at most **one** tagged mainnet Docker upgrade per **72 hours** unless security-critical; batch audit fixes into minor releases.
3. **Investor comms:** each release includes [`RELEASE_NOTES.md`](../RELEASE_NOTES.md) entry + link to [`AUDIT-OPEN-ITEMS.md`](./AUDIT-OPEN-ITEMS.md) when audit-related.
4. **Single production fullnode** hosts relay + shard + anchor submitter; validators do not duplicate anchor posts.

## Current canonical artifacts

- Genesis: `0x28a554eb58356946b3ac7ce61fa1562cbda2dd1171ee8c7719140e0e33a2e31c`
- Raw spec SHA-256: see `network/tks-network-spec-raw.sha256`
- Public RPC: `https://rpc.tokenkickstarter.com`
- Bootnodes: [`bootnodes.public.txt`](./bootnodes.public.txt)
