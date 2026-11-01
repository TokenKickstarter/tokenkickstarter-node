# R4 follow-up — “still open” items (status)

Mainnet as of **regen genesis `0x28a554…`**, node **v0.1.19+** unless noted.

| Auditor item | Resolution |
|--------------|------------|
| **L1 load numbers (r4 on old chain)** | **Invalid for current chain.** See [`R4-L1-REBASELINE.md`](./R4-L1-REBASELINE.md). Re-run `network/scripts/loadtest-20k-transfers.py` against regen before citing throughput. |
| **Stuck pending multisig** | **Fixed in tooling:** `approve_as_multi` now uses on-chain **timepoint**; **`production multisig-list-pending`** / **`multisig-cancel-pending`** clear stragglers. Run list on public RPC after deploy. |
| **Doubled anchor message count** | **Fixed v0.1.19:** shard batch engine **dedupes `msg_id`** so relay `/store` + forwarded `/ingest` (or shard `/store` retries) do not inflate L1 `TotalMessageCount`. Historical totals before upgrade are not retroactively corrected. |
| **Shard 404s** | **Expected routing:** use **relay `POST /store`**; shard exposes **`/ingest`**, **`/store`**, **`/health`**, **`/version`**, **`/hybrid/stats`**. Other paths 404. See [`SHARD-AND-RELAY.public.md`](./SHARD-AND-RELAY.public.md). |
| **Bootnode `127.0.0.1`** | **Fixed in published spec:** [`tks-network-spec-raw.json`](./tks-network-spec-raw.json) + [`bootnodes.public.txt`](./bootnodes.public.txt) use **public VPS** multiaddr. `build-tks-network-spec.sh` strips dev bootnodes from `build-spec` output. |
| **Release / reset pace** | **Process:** see [`MAINNET-STABILITY.md`](./MAINNET-STABILITY.md) — regen rationale, freeze policy, no further genesis resets without 7-day notice. |
| **Multisig doc old genesis** | **Fixed:** [`SUDO-MULTISIG.public.md`](./SUDO-MULTISIG.public.md) documents **regen** sudo `0x8eeb…` only; legacy spec 412 in appendix. |

## Quick verification (public)

```bash
./network/scripts/public-rpc-test.sh
./network/scripts/verify-sudo-multisig.sh
curl -s https://rpc.tokenkickstarter.com -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tks_anchorStats","params":[]}'
python3 -c "import json; print(json.load(open('network/tks-network-spec-raw.json'))['bootNodes'])"
```
