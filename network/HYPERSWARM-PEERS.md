# HyperSwarm — multi-relay peers (decentralized Cipher)

## Layers

| Layer | Protocol | Failover |
|-------|----------|----------|
| **L1 chain** | Substrate P2P `:30333` | Any synced `tks-chain-node` |
| **Trust** | `ShardRegistry::register_validator` + `post_anchor` | On-chain endpoint list |
| **Cipher relays** | HTTP `:4002` + libp2p gossip `:4001` | `/discovery` + mesh fanout |

## Run a public relay node

```bash
tks-chain-node ... \
  --enable-cipher-relay --cipher-relay-port=4002 \
  --enable-cipher-shard-executor --cipher-shard-port=4010 \
  --enable-cipher-hyperswarm --cipher-hyperswarm-port=4001 \
  --cipher-public-relay-url=https://relay.yourdomain.com \
  --cipher-hyperswarm-bootnodes=/ip4/PRIMARY_IP/tcp/4001/p2p/PEER_ID
```

Register on L1 (validator key):

```text
ShardRegistry.register_validator(region, endpoint)
```

`endpoint` may be:

- `https://relay.yourdomain.com` (HTTP — mobile clients)
- `/ip4/x.x.x.x/tcp/4001/p2p/12D3Koo...` (libp2p — node mesh)

## Client bootstrap

1. `GET https://relay.tokenkickstarter.com/discovery` (or any peer relay)
2. Fallback: JSON-RPC `tks_listSwarmValidators` on any TKS RPC
3. Try `relays[]` in priority order (`self` → `chain` → `gossip`)

## RPC (any TKS node)

- `tks_listSwarmValidators` — active registrations from chain
- `tks_hyperswarmNetworkInfo` — shard count + epoch

## Mesh behavior (v0.1.4+)

- Each relay gossips presence every 30s on topic `tks-hyperswarm-relay-v1`
- On `/store`, best-effort replicate to up to 8 other relays from `/discovery`
- Primary VPS down → clients use next relay from discovery; messages may exist on community nodes

## nginx

- `relay.example.com` → `127.0.0.1:4002`
- Open TCP `4001` between peer nodes (not required for mobile HTTP clients)
