# Public RPC history (archive)

## Current limitation

The production **fullnode** behind `https://rpc.tokenkickstarter.com` is a **pruned** node. EVM `eth_getBlockByNumber` for blocks older than roughly **5 minutes** may return **null** — auditors cannot walk long history from the public URL alone.

## Target architecture

| Role | Retention | Public? |
|------|-----------|---------|
| **Validator / pruned fullnode** | Recent blocks + state | Yes (current RPC) |
| **Archive fullnode** | Full chain + historical state | Optional read-only vhost |

## Operator steps (when funded)

1. Run a second container with `--pruning=archive` (or Substrate archive mode) on larger disk.
2. Nginx route `https://archive-rpc.tokenkickstarter.com` → archive `:9944` (Safe methods only).
3. Document genesis hash and earliest block available in this file.

Until then, auditors should use **on-VPS localhost RPC** or a **self-synced** node with [`bootnodes.public.txt`](./bootnodes.public.txt).
