# Cipher relay vs shard (production layout)

## One physical host today

Production Docker on the VPS runs **relay (4002)** and **shard executor (4010)** on the **same fullnode**. Public DNS:

| Host | Port (local) | Role |
|------|----------------|------|
| `https://relay.tokenkickstarter.com` | 4002 | **POST `/store`**, `/fetch`, `/ack`, discovery |
| `https://shard.tokenkickstarter.com` | 4010 | **POST `/ingest`** (batch leaves), **GET `/version`**, `/hybrid/stats` |

Clients and auditors should **store ciphertext via relay `/store`**. The relay forwards hashed batch leaves to `http://127.0.0.1:4010/ingest`.

## On-chain `tks_listSwarmValidators`

Both validators currently register the **same public shard URL** (`https://shard.tokenkickstarter.com`) because there is **one** shard HTTP front door. That is intentional for a single fullnode deployment; it is not two independent shard VPSes.

To advertise distinct endpoints per validator, re-run `tks-chain-node shard-setup register_validator` (or multisig → sudo) with per-node URLs after you stand up separate shard hosts.

## Shard `/store` (v0.1.17+)

Older builds returned **404** on `https://shard…/store`. The shard executor now accepts **POST `/store`** with the same JSON as relay (`recipient_id`, `message_id`, `ciphertext`, …) for compatibility; prefer **relay `/store`** for production traffic so HyperSwarm mesh + relay cache stay in sync.
