# Cipher shard HTTP endpoints

## Current production (v0.1.13 audit)

| Validator | On-chain region | Registered endpoint |
|-------------|-----------------|---------------------|
| Validator 1 stash | NorthAmerica | `https://shard.tokenkickstarter.com` |
| Validator 2 stash | Europe | `https://shard.tokenkickstarter.com` |

**There is only one public shard HTTP host today.** Both validators register the same URL; traffic is load-balanced at the reverse proxy to the **fullnode** shard executor (`127.0.0.1:4010` on VPS). Independent per-validator shard hosts are planned; genesis supports distinct URLs via `shard_endpoints` in `production-genesis-input.json`:

```json
"shard_endpoints": [
  "https://shard1.tokenkickstarter.com",
  "https://shard2.tokenkickstarter.com"
]
```

Until DNS and TLS exist for `shard1` / `shard2`, audits should treat the table above as **single-host, dual-validator registration**.

Relay: `https://relay.tokenkickstarter.com` (fullnode embedded relay).
