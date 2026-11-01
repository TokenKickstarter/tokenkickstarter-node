# Production sudo (multisig)

On-chain **`sudo::Key`** on **current mainnet** (regen genesis **`0x28a554eb…`**, spec **412**, release **v0.1.17+**) is the runtime **`pallet-multisig`** derived account — not a single ECDSA key.

## Published configuration (regen mainnet)

| Field | Value |
|--------|--------|
| Genesis hash | `0x28a554eb58356946b3ac7ce61fa1562cbda2dd1171ee8c7719140e0e33a2e31c` |
| Multisig account (`sudo.key`) | **`0x8eeb8be59ccb1c65211ab787e6ba174753a68f1a`** |
| Threshold | **2-of-3** |
| Signer 1 | `0x5b5478f3929645a96b17712a3b1743eded5d4ad7` |
| Signer 2 | `0x00373382c01459df2801f1f510926bc6acf9ddf7` |
| Signer 3 | `0x635ed8d610027d2df9398867e3da48db54d12f49` |

Example JSON: [`genesis-summary.public.example.json`](./genesis-summary.public.example.json).  
Allocation table: [`GENESIS-ALLOCATION.public.md`](./GENESIS-ALLOCATION.public.md).

## Verify offline (no secrets)

```bash
tks-chain-node production multisig-account \
  --threshold 2 \
  --signatories 0x5b5478f3929645a96b17712a3b1743eded5d4ad7,0x00373382c01459df2801f1f510926bc6acf9ddf7,0x635ed8d610027d2df9398867e3da48db54d12f49
# Must print: 0x8eeb8be59ccb1c65211ab787e6ba174753a68f1a
```

Compare with `sudo.key` in [`tks-network-spec-raw.json`](./tks-network-spec-raw.json) and `eth_getBalance` of that address (~1000 TKS for dispatch deposits).

## Verify on-chain (read-only RPC)

```bash
RPC=https://rpc.tokenkickstarter.com
curl -s "$RPC" -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_getBalance","params":["0x8eeb8be59ccb1c65211ab787e6ba174753a68f1a","latest"]}'
```

Pending multisig ops (should be **empty** after governance smoke tests):

```bash
tks-chain-node production multisig-list-pending \
  --rpc-url "$RPC" \
  --multisig 0x8eeb8be59ccb1c65211ab787e6ba174753a68f1a
```

## Governance path

`multisig.asMulti` → wrapped `sudo.sudo(call)`. No single signer can dispatch sudo alone.

**v0.1.19+** smoke tooling waits for the on-chain **timepoint** before `approve_as_multi` (fixes stuck pending from `maybe_timepoint: None`).

Cancel a stuck pending call (any one member):

```bash
tks-chain-node production multisig-cancel-pending \
  --rpc-url "$RPC" \
  --multisig 0x8eeb8be59ccb1c65211ab787e6ba174753a68f1a \
  --member-seed-file sudo-member1.json \
  --threshold 2 \
  --signatories 0x5b5478f3929645a96b17712a3b1743eded5d4ad7,0x00373382c01459df2801f1f510926bc6acf9ddf7,0x635ed8d610027d2df9398867e3da48db54d12f49
```

## Superseded chain (historical only)

**Spec 412 / genesis `0x104c48a9…`** (pre–v0.1.17 regen) used broken offline helpers (`0xd0ec…` in `sudo.key` vs pallet-derived `0x921e…`). That chain was **purged and replaced**; do not use old signer addresses or load numbers from that era for current audits.
