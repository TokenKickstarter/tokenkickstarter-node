# Production deployment checklist

Use this for VPS / Docker installs (for example `/opt/tks-docker/`). Attach completed logs to audit evidence (audit section G).

## Pre-flight

- [ ] Production chainspec JSON (raw) built from [MAINNET-GENESIS-CHECKLIST.md](./MAINNET-GENESIS-CHECKLIST.md), not `--chain local`.
- [ ] Validator and sudo keys generated offline; Alice/Bob seeds not used.
- [ ] Release binary version matches tag (for example `v0.1.8`); `sha256sum` matches GitHub `SHA256SUMS.txt`.
- [ ] Firewall: 30333 public; 9944 as policy allows; 4001 between HyperSwarm peers; 4002/4010 bound to localhost unless proxied.

## Docker (full node + Cipher)

Reference: [docker-compose-live.yml](./docker-compose-live.yml).

```bash
cd /opt/tks-docker
docker compose -f docker-compose-live.yml pull   # if using registry
docker compose -f docker-compose-live.yml build    # if building from source tag
docker compose -f docker-compose-live.yml up -d
docker compose -f docker-compose-live.yml ps
```

Verification (save output for audit):

```bash
docker compose -f docker-compose-live.yml logs --tail=50 tks-fullnode
curl -s -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"system_version","params":[],"id":1}' \
  http://127.0.0.1:9944
curl -s -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"system_chain","params":[],"id":1}' \
  http://127.0.0.1:9944
curl -s http://127.0.0.1:4002/stats
curl -s http://127.0.0.1:4002/hybrid/stats | jq .
```

Expected for production (when genesis is fixed):

- `system_chain` is **not** `TKS Local Testnet`.
- `system_chainType` is **not** `Local`.
- Hybrid `metrics_note` explains configured vs measured TPS.

## Session keys (validators)

**Important:** Injecting keys into a node keystore does **not** by itself change the on-chain authority set. You must also set session keys on-chain for the validator stash accounts defined in genesis.

1. Start validator with **local** RPC only for key ops (or use `inject-validator-keys.sh` against `127.0.0.1`).
2. Record Aura + GRANDPA public keys from script output.
3. Submit on-chain session key update for each validator (Polkadot.js or sudo batch).
4. Confirm `session.nextKeys` / author mapping matches new keys, not Alice/Bob.

Do **not** enable `--rpc-methods Unsafe` on public endpoints.

## Cipher / shard / anchors

- [ ] Relay public URL set: `--cipher-public-relay-url=https://relay.example.com`
- [ ] nginx (or equivalent) proxies `relay.*` to `127.0.0.1:4002`
- [ ] Each shard operator: `ShardRegistry.register_validator`
- [ ] Process to submit `HyperSwarmAnchor.post_anchor` from `{base-path}/cipher/anchors/` JSON (manual or automation)
- [ ] Confirm on-chain anchor count increases after seal interval

## Upgrade binary (same chain data)

```bash
VERSION=v0.1.8
curl -L -o tks-chain-node-new \
  "https://github.com/TokenKickstarter/tokenkickstarter-node/releases/download/${VERSION}/tks-chain-node-${VERSION}-linux-amd64"
chmod +x tks-chain-node-new
./tks-chain-node-new --version
docker compose -f docker-compose-live.yml down
# replace binary or rebuild image from tag
docker compose -f docker-compose-live.yml up -d
```

## Post-deploy audit hooks

- [ ] Archive `docker compose ps` and health RPC outputs
- [ ] Run public black-box audit script (no SSH) or request re-audit
- [ ] Update [SECURITY-CHANGELOG.md](../SECURITY-CHANGELOG.md) if security-relevant config changed
