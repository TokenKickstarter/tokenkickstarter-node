# Production sudo (multisig)

On-chain **`sudo::Key`** (spec 412 genesis `0x104c48a9…00bc`) is a **`pallet-multisig` derived account**, not a single ECDSA key.

## Published configuration (v0.1.13)

| Field | Value |
|--------|--------|
| Multisig account (sudo, **spec 412 genesis**) | `0xd0ecdb4dd41fbe6863f4ca6c68874ae61198d489` (legacy helper — see below) |
| **`pallet-multisig` id (runtime, v0.1.15+ helper)** | **`0x921e298d9b7725892eb5c655aa5d4e55369613cd`** for the signers below (see `tks-chain-node production multisig-account`) |
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
# v0.1.14 and earlier (bug): 0xd0ecdb4dd41fbe6863f4ca6c68874ae61198d489
# v0.1.15+ (matches pallet_multisig::multi_account_id for AccountId20)
```

### Why auditors saw `0xd0ec…` vs `0x622b…`

Genesis **spec 412** was assembled with a broken offline helper that SCALE-encoded the prefix as a **compact string** (`"modlpy/utilisuba"`) instead of the pallet’s **`b"modlpy/utilisuba"`** byte literal. That yields **`0xd0ec…`** in `sudo.key`.

Tools that follow [`pallet_multisig::Pallet::multi_account_id`](https://github.com/paritytech/polkadot-sdk) with **`b"modlpy/utilisuba"`** and sorted **`AccountId20`** signatories produce **`0x921e298d…613cd`** (node **v0.1.16+** CLI). Auditors who saw **`0x622b…77a2`** typically used a different signatory encoding (e.g. SS58 / 32-byte account) or an older Substrate helper — always compare against the node CLI, not a third-party calculator.

**Governance:** `multisig.asMulti` → `sudo.sudo(call)` only works when **`sudo.key` equals the runtime-derived multisig account**. Spec 412 mainnet still has the legacy `0xd0ec…` key until genesis is regenerated with the fixed helper and the chain is reset.

Compare `sudo.key` in the published raw chain spec and `eth_getBalance` of that address (~1000 TKS for dispatch deposits).

**Governance path:** `multisig.asMulti` → wrapped `sudo.sudo(call)` — no single key can dispatch sudo alone.
