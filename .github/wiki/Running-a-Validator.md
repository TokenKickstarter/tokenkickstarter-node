# Running a Validator

Validators in TKS participate in block production (Aura) and block finalization (GRANDPA). They earn staking rewards and must maintain high uptime.

---

## Requirements

- A dedicated server (VPS or bare metal) with at least:
  - 4 CPU cores
  - 8 GB RAM
  - 200 GB SSD
  - 100 Mbps network
- A bonded TKS balance for staking
- High availability (99.9%+ uptime recommended)

---

## Step 1: Start the Validator Node

```bash
./target/release/tks-chain-node \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/validator \
  --validator \
  --name "MyValidator" \
  --rpc-external \
  --rpc-cors=all \
  --bootnodes "/dns/seed.tokenkickstarter.com/tcp/30333/p2p/12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C" \
  --bootnodes "/dns/seed.tkstoken.com/tcp/30333/p2p/12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR"
```

Or use the convenience script:

```bash
./network/start-validator.sh
```

Wait for the node to fully sync before proceeding.

---

## Step 2: Generate Session Keys

Once the node is synced, generate a new set of session keys:

```bash
curl -s -H "Content-Type: application/json" \
  -d '{"id":1,"jsonrpc":"2.0","method":"author_rotateKeys","params":[]}' \
  http://localhost:9944 | python3 -m json.tool
```

The response will contain a hex-encoded key blob:

```json
{
  "result": "0xabc123...def"
}
```

Save this key — you will need it in the next step.

---

## Step 3: Inject Session Keys

Use the inject script to register the session keys with the node's keystore:

```bash
./network/inject-validator-keys.sh <SESSION_KEY_HEX>
```

Or manually using the RPC:

```bash
curl -H "Content-Type: application/json" \
  -d '{"id":1,"jsonrpc":"2.0","method":"author_insertKey","params":["aura","<SEED>","<PUBLIC_KEY>"]}' \
  http://localhost:9944

curl -H "Content-Type: application/json" \
  -d '{"id":1,"jsonrpc":"2.0","method":"author_insertKey","params":["gran","<SEED>","<PUBLIC_KEY>"]}' \
  http://localhost:9944
```

---

## Step 4: Register on Chain (Polkadot.js)

1. Go to [Polkadot.js Apps](https://polkadot.js.org/apps/?rpc=wss://rpc.tokenkickstarter.com)
2. Navigate to **Developer → Extrinsics**
3. Submit `session.setKeys(keys, proof)` with your session key blob
4. Navigate to **Network → Staking**
5. Bond TKS tokens and declare your validator intent

---

## Step 5: Monitor Your Validator

Check validator status:

```bash
# Logs
journalctl -u tks-node -f

# RPC check
curl -s -H "Content-Type: application/json" \
  -d '{"id":1,"jsonrpc":"2.0","method":"chain_getBlock","params":[]}' \
  http://localhost:9944
```

View your validator in the explorer at `https://scan.tokenkickstarter.com/validators`.

---

## Running as a systemd Service

```bash
sudo cp network/tks-chain-node /usr/local/bin/
sudo cp network/tks-node.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now tks-node

# Tail logs
journalctl -u tks-node -f
```

---

## Security Considerations

- **Never** expose the keystore directory (`~/.tks/validator/chains/tks_network/keystore/`) to untrusted processes.
- Use a **separate stash account** for bonded funds and a **controller account** for day-to-day operations.
- Enable a firewall that allows only port `30333` (P2P) publicly, and restricts `9944` (RPC) to trusted IPs only.
- Consider running behind a [Secure Validator](https://github.com/w3f/polkadot-secure-validator) setup for production.

---

## Slashing

TKS uses Substrate's standard slashing mechanism. Validators are slashed for:
- **Equivocation** (signing two different blocks at the same slot)
- **Going offline** for extended periods

Maintain uptime and do not run duplicate validator instances on the same session keys.
