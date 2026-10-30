# Production sudo (multisig)

On-chain **`sudo::Key`** (spec 412 genesis `0x104c48a9…00bc`) is a **`pallet-multisig` derived account**, not a single ECDSA key.

## Published configuration (v0.1.13)

| Field | Value |
|--------|--------|
| Multisig account (sudo) | `0xd0ecdb4dd41fbe6863f4ca6c68874ae61198d489` |
| Threshold | **2-of-3** |
| Signer 1 | `0xd876889fb236ebacdcf00e5007666d2094a5ada5` |
| Signer 2 | `0x5a3173a1d18049f9b19bdd15de9b28b671e0dc29` |
| Signer 3 | `0xaedfec3469ffe1fee0e0a54679270e183026b66e` |

Example JSON: [`genesis-summary.public.example.json`](./genesis-summary.public.example.json).

## Verify on-chain (read-only RPC)

```bash
RPC=https://rpc.tokenkickstarter.com
# Sudo key must match multisig account
curl -s "$RPC" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"state_getStorage","params":["0x…sudo::Key encoding…"]}'
```

Easier: use the node helper (offline, no secrets):

```bash
tks-chain-node production multisig-account \
  --threshold 2 \
  --signatories 0xd876889fb236ebacdcf00e5007666d2094a5ada5,0x5a3173a1d18049f9b19bdd15de9b28b671e0dc29,0xaedfec3469ffe1fee0e0a54679270e183026b66e
# → 0xd0ecdb4dd41fbe6863f4ca6c68874ae61198d489
```

Compare with `sudo.key` in the published chain spec and `eth_getBalance` of that address (~1000 TKS for dispatch deposits).

**Governance path:** `multisig.asMulti` → wrapped `sudo.sudo(call)` — no single key can dispatch sudo alone.
