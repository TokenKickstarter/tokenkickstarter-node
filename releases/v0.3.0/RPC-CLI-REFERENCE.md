# TKS Node — Complete RPC & CLI Reference

## Overview

TKS exposes two RPC interfaces on port **9944**:

| Interface | Protocol | Base Path | Description |
|-----------|----------|-----------|-------------|
| Ethereum JSON-RPC | HTTP/WS | `/` | EVM-compatible — works with MetaMask, ethers.js, web3.js |
| Substrate JSON-RPC | HTTP/WS | `/` | Substrate-native — staking, session, governance |

---

## 1. CLI Commands

### Node Lifecycle

```bash
# Start a full node (persistent)
./tks-chain-node \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/data \
  --rpc-external --rpc-cors all

# Start as validator (block production)
./tks-chain-node ... --validator

# Start with all RPC methods enabled (needed for sudo, staking ops)
./tks-chain-node ... --rpc-methods unsafe

# Purge chain data (full reset)
./tks-chain-node purge-chain --chain network/tks-testnet-spec-raw.json --base-path ~/.tks/data

# Revert last N blocks
./tks-chain-node revert --chain ... --base-path ... --num 10

# Export blocks to file
./tks-chain-node export-blocks --chain ... --base-path ... --to /tmp/blocks.bin

# Import blocks from file
./tks-chain-node import-blocks --chain ... --base-path ... --from /tmp/blocks.bin

# Validate a block
./tks-chain-node check-block --chain ... --base-path ... <BLOCK_HASH>
```

### Key Management CLI

```bash
# Generate a new account (Sr25519 — default for EVM/signing)
./tks-chain-node key generate --scheme Sr25519
# Output: mnemonic, secret seed, public key, SS58 address

# Generate with Ed25519 (used for GRANDPA finality)
./tks-chain-node key generate --scheme Ed25519

# Generate and output as JSON
./tks-chain-node key generate --scheme Sr25519 --output-type json

# Inspect an existing key (from mnemonic or seed)
./tks-chain-node key inspect "word1 word2 ... word12"
./tks-chain-node key inspect "0xYOUR_SECRET_SEED"

# Inspect with derivation path
./tks-chain-node key inspect "word1 ... word12//validator//0"

# Insert key into running node's keystore (for validators)
./tks-chain-node key insert \
  --chain network/tks-testnet-spec-raw.json \
  --base-path ~/.tks/validator-data \
  --scheme Sr25519 \
  --key-type aura \
  --suri "word1 word2 ... word12"

# Generate a P2P node key
./tks-chain-node key generate-node-key --file ~/.tks/node.key

# Get Peer ID from existing key file
./tks-chain-node key inspect-node-key --file ~/.tks/node.key
```

---

## 2. Ethereum RPC (EVM Layer)

All standard Ethereum JSON-RPC — compatible with MetaMask, ethers.js, Hardhat, Foundry.

### Connection

```bash
RPC URL:  http://localhost:9944
WS URL:   ws://localhost:9944
Chain ID: (set in your chainspec EVM config)
```

### Network & Chain

```bash
# Get chain ID
curl -s -X POST http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}'

# Get network ID
curl ... -d '{"jsonrpc":"2.0","method":"net_version","params":[],"id":1}'

# Check if node is syncing
curl ... -d '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}'

# Get peer count
curl ... -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}'
```

### Block Queries

```bash
# Latest block number
curl ... -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'

# Get block by number (with full transactions)
curl ... -d '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["latest",true],"id":1}'

# Get block by hash
curl ... -d '{"jsonrpc":"2.0","method":"eth_getBlockByHash","params":["0xBLOCK_HASH",true],"id":1}'
```

### Account & Balance

```bash
# Get TKS balance (in wei)
curl ... -d '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0xYOUR_ADDRESS","latest"],"id":1}'

# Get transaction count (nonce)
curl ... -d '{"jsonrpc":"2.0","method":"eth_getTransactionCount","params":["0xYOUR_ADDRESS","latest"],"id":1}'

# Get contract bytecode
curl ... -d '{"jsonrpc":"2.0","method":"eth_getCode","params":["0xCONTRACT","latest"],"id":1}'

# Get storage slot
curl ... -d '{"jsonrpc":"2.0","method":"eth_getStorageAt","params":["0xCONTRACT","0x0","latest"],"id":1}'
```

### Gas

```bash
# Current gas price
curl ... -d '{"jsonrpc":"2.0","method":"eth_gasPrice","params":[],"id":1}'

# EIP-1559 fee history
curl ... -d '{"jsonrpc":"2.0","method":"eth_feeHistory","params":[4,"latest",[25,75]],"id":1}'

# Estimate gas for a transaction
curl ... -d '{
  "jsonrpc":"2.0","method":"eth_estimateGas",
  "params":[{"from":"0xSENDER","to":"0xRECEIVER","value":"0xDE0B6B3A7640000"}],
  "id":1
}'
```

### Sending Transactions

```bash
# Send raw signed transaction
curl ... -d '{
  "jsonrpc":"2.0","method":"eth_sendRawTransaction",
  "params":["0xSIGNED_TX_HEX"],
  "id":1
}'

# Call contract (read-only, no gas cost)
curl ... -d '{
  "jsonrpc":"2.0","method":"eth_call",
  "params":[{"to":"0xCONTRACT","data":"0xCALLDATA"},"latest"],
  "id":1
}'
```

### Transaction Queries

```bash
# Get transaction by hash
curl ... -d '{"jsonrpc":"2.0","method":"eth_getTransactionByHash","params":["0xTX_HASH"],"id":1}'

# Get transaction receipt (includes status, gas used, logs)
curl ... -d '{"jsonrpc":"2.0","method":"eth_getTransactionReceipt","params":["0xTX_HASH"],"id":1}'

# Get logs / events
curl ... -d '{
  "jsonrpc":"2.0","method":"eth_getLogs",
  "params":[{"fromBlock":"0x0","toBlock":"latest","address":"0xCONTRACT"}],
  "id":1
}'
```

### Mempool (Frontier)

```bash
# Pending transaction count
curl ... -d '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}'

# Inspect pending/queued transactions
curl ... -d '{"jsonrpc":"2.0","method":"txpool_content","params":[],"id":1}'
```

---

## 3. Substrate RPC

### System Info

```bash
# Node name, version, chain
curl ... -d '{"jsonrpc":"2.0","method":"system_name","params":[],"id":1}'
curl ... -d '{"jsonrpc":"2.0","method":"system_version","params":[],"id":1}'
curl ... -d '{"jsonrpc":"2.0","method":"system_chain","params":[],"id":1}'

# Node health (peers, syncing status)
curl ... -d '{"jsonrpc":"2.0","method":"system_health","params":[],"id":1}'

# Connected peers
curl ... -d '{"jsonrpc":"2.0","method":"system_peers","params":[],"id":1}'

# Network state
curl ... -d '{"jsonrpc":"2.0","method":"system_networkState","params":[],"id":1}'

# Local peer ID
curl ... -d '{"jsonrpc":"2.0","method":"system_localPeerId","params":[],"id":1}'

# Local listen addresses
curl ... -d '{"jsonrpc":"2.0","method":"system_localListenAddresses","params":[],"id":1}'

# Properties (token symbol, decimals, SS58 prefix)
curl ... -d '{"jsonrpc":"2.0","method":"system_properties","params":[],"id":1}'
```

### Chain Queries

```bash
# Get latest block header
curl ... -d '{"jsonrpc":"2.0","method":"chain_getHeader","params":[],"id":1}'

# Get block by hash
curl ... -d '{"jsonrpc":"2.0","method":"chain_getBlock","params":["0xBLOCK_HASH"],"id":1}'

# Get latest block hash
curl ... -d '{"jsonrpc":"2.0","method":"chain_getBlockHash","params":[],"id":1}'

# Get finalized head
curl ... -d '{"jsonrpc":"2.0","method":"chain_getFinalizedHead","params":[],"id":1}'
```

### Author (Validator Operations)

```bash
# Rotate session keys (generates new keys in keystore, returns public keys)
curl ... -d '{"jsonrpc":"2.0","method":"author_rotateKeys","params":[],"id":1}'

# Insert a key into keystore
curl ... -d '{
  "jsonrpc":"2.0","method":"author_insertKey",
  "params":["aura","0xSECRET_SEED","0xPUBLIC_KEY"],
  "id":1
}'

# Check if key exists
curl ... -d '{
  "jsonrpc":"2.0","method":"author_hasKey",
  "params":["0xPUBLIC_KEY","aura"],
  "id":1
}'

# Check if session key set exists
curl ... -d '{
  "jsonrpc":"2.0","method":"author_hasSessionKeys",
  "params":["0xSESSION_KEY_HEX"],
  "id":1
}'

# Submit and watch an extrinsic
curl ... -d '{
  "jsonrpc":"2.0","method":"author_submitExtrinsic",
  "params":["0xSIGNED_EXTRINSIC_HEX"],
  "id":1
}'

# Get pending extrinsics in mempool
curl ... -d '{"jsonrpc":"2.0","method":"author_pendingExtrinsics","params":[],"id":1}'
```

### State Queries

```bash
# Get raw storage value
curl ... -d '{
  "jsonrpc":"2.0","method":"state_getStorage",
  "params":["0xSTORAGE_KEY"],
  "id":1
}'

# Get storage hash
curl ... -d '{
  "jsonrpc":"2.0","method":"state_getStorageHash",
  "params":["0xSTORAGE_KEY"],
  "id":1
}'

# Call runtime API
curl ... -d '{
  "jsonrpc":"2.0","method":"state_call",
  "params":["AccountNonceApi_account_nonce","0xENCODED_PARAMS"],
  "id":1
}'

# Get runtime metadata (full ABI of all pallets)
curl ... -d '{"jsonrpc":"2.0","method":"state_getMetadata","params":[],"id":1}'

# Get runtime version
curl ... -d '{"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[],"id":1}'
```

---

## 4. Staking — Mining & Rewards

TKS uses **Nominated Proof-of-Stake (NPoS)** via `pallet-staking`.
"Mining" = being an active validator and producing blocks.

### Key Concepts

| Term | Meaning |
|------|---------|
| **Validator** | Node that produces blocks, earns rewards |
| **Nominator** | TKS holder that backs a validator, shares rewards |
| **Era** | ~24 hours — rewards distributed per era |
| **Reward destination** | Where your earned TKS goes |
| **Bond** | TKS locked as stake |

### Set Your Reward Address

Reward destinations available:
- `Staked` — auto-compound back into bond
- `Stash` — to your stash account
- `Account(0xADDRESS)` — to any custom address

```javascript
// Using @polkadot/api
const api = await ApiPromise.create({ provider: new WsProvider('ws://localhost:9944') });

// Set reward destination to a custom address
await api.tx.staking
  .setPayee({ Account: '0xYOUR_REWARD_ADDRESS' })
  .signAndSend(yourKeyPair);

// Auto-compound (reinvest rewards into your bond)
await api.tx.staking
  .setPayee('Staked')
  .signAndSend(yourKeyPair);
```

### Bond TKS (Begin Staking)

```javascript
// Bond 1000 TKS as a validator
await api.tx.staking.bond(
  1000_000_000_000_000_000_000n,  // 1000 TKS in wei
  'Staked'                         // reward destination
).signAndSend(keyPair);
```

### Collect / Payout Rewards

Rewards must be manually claimed per era (or automated):

```javascript
// Collect rewards for era 42, validator 0xVALIDATOR_ADDRESS
await api.tx.staking
  .payoutStakers('0xVALIDATOR_ADDRESS', 42)
  .signAndSend(anyKeyPair);  // Anyone can trigger this

// Collect last 84 eras at once (script)
const currentEra = await api.query.staking.currentEra();
for (let era = currentEra - 84; era < currentEra; era++) {
  await api.tx.staking
    .payoutStakers('0xVALIDATOR_ADDRESS', era)
    .signAndSend(keyPair);
}
```

### Validate (Start Mining)

```javascript
// Set validator preferences (5% commission)
await api.tx.staking
  .validate({ commission: 50_000_000, blocked: false })
  .signAndSend(keyPair);
```

### Nominate a Validator

```javascript
// Nominate validators with your stake
await api.tx.staking
  .nominate(['0xVALIDATOR_1', '0xVALIDATOR_2'])
  .signAndSend(keyPair);
```

### Query Staking State

```bash
# Current era
curl ... -d '{"jsonrpc":"2.0","method":"state_getStorage","params":["0x5f3e4907f716ac89b6347d15ececedca422adb579f1dbf4f3886c5cfa3bb8cc4"],"id":1}'

# Via polkadot/api (easier)
const era = await api.query.staking.currentEra();
const validators = await api.query.session.validators();
const myRewards = await api.query.staking.erasStakers(era, '0xVALIDATOR');
```

---

## 5. Treasury & Governance

```javascript
// Propose treasury spend
await api.tx.treasury
  .proposeSpend(amount, '0xBENEFICIARY')
  .signAndSend(keyPair);

// Approve a treasury proposal (sudo/council)
await api.tx.sudo
  .sudo(api.tx.treasury.approveProposal(proposalId))
  .signAndSend(sudoKeyPair);
```

---

## 6. Token Transfers

### Native TKS (Substrate)

```javascript
// Transfer 10 TKS to another address
await api.tx.balances
  .transferKeepAlive('0xRECIPIENT', 10_000_000_000_000_000_000n)
  .signAndSend(keyPair);

// Force transfer (sudo only)
await api.tx.sudo
  .sudo(api.tx.balances.forceTransfer('0xFROM', '0xTO', amount))
  .signAndSend(sudoKeyPair);
```

### ERC-20 Tokens (EVM layer)

```bash
# Transfer ERC-20 via eth_sendRawTransaction
# Or use: ethers.js / web3.js / MetaMask — works identically to Ethereum
```

---

## 7. Custom TKS Pallets

Your node includes custom pallets — `pallet-name-registry`, `pallet-shard-registry`, `pallet-hyperswarm-anchor`:

```javascript
// Name Registry — register a human-readable name
await api.tx.nameRegistry
  .register('myname.tks')
  .signAndSend(keyPair);

// Shard Registry — register a shard
await api.tx.shardRegistry
  .registerShard(shardId, metadata)
  .signAndSend(keyPair);

// Hyperswarm Anchor — anchor data to chain
await api.tx.hyperswarmAnchor
  .anchor(topicHash, data)
  .signAndSend(keyPair);
```

---

## 8. Sudo (Admin Operations)

The `pallet-sudo` root key can:

```javascript
// Execute any privileged call
await api.tx.sudo
  .sudo(anyPrivilegedCall)
  .signAndSend(sudoKeyPair);

// Transfer sudo to another account
await api.tx.sudo
  .setKey('0xNEW_SUDO_ADDRESS')
  .signAndSend(sudoKeyPair);

// Force-set account balance (faucet / genesis allocation)
await api.tx.sudo
  .sudo(api.tx.balances.setBalance('0xACCOUNT', amount, 0))
  .signAndSend(sudoKeyPair);
```

---

## 9. Quick Reference Table

| Operation | Method / Tool |
|-----------|--------------|
| Check balance | `eth_getBalance` or `balances.account` |
| Send TKS (EVM) | `eth_sendRawTransaction` |
| Send TKS (native) | `balances.transferKeepAlive` |
| Deploy contract | `eth_sendRawTransaction` (to: null) |
| Call contract | `eth_call` |
| Current block | `eth_blockNumber` |
| Gas price | `eth_gasPrice` |
| Start validating | `staking.validate` |
| Set reward address | `staking.setPayee` |
| Collect rewards | `staking.payoutStakers(validator, era)` |
| Bond TKS | `staking.bond` |
| Nominate | `staking.nominate` |
| Rotate keys | `author_rotateKeys` |
| Insert key | `author_insertKey` |
| Pending mempool | `txpool_status` / `author_pendingExtrinsics` |
| Node peers | `system_peers` |
| Runtime version | `state_getRuntimeVersion` |
| Sudo any call | `sudo.sudo(call)` |
| Treasury spend | `treasury.proposeSpend` |

---

## 10. Useful Libraries

| Language | Library | Install |
|----------|---------|---------|
| JavaScript | `@polkadot/api` | `npm i @polkadot/api` |
| JavaScript | `ethers.js` (EVM) | `npm i ethers` |
| JavaScript | `web3.js` (EVM) | `npm i web3` |
| Python | `substrate-interface` | `pip install substrate-interface` |
| Python | `web3.py` (EVM) | `pip install web3` |
| Rust | `subxt` | `cargo add subxt` |
| Go | `go-substrate-rpc-client` | `go get github.com/centrifuge/go-substrate-rpc-client` |

---

## 11. Example: Full Validator Setup via CLI+RPC

```bash
# 1. Start validator node
./start-validator.sh

# 2. Generate session keys
KEYS=$(curl -s -X POST http://localhost:9945 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"author_rotateKeys","params":[],"id":1}' \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['result'])")
echo "Session keys: $KEYS"

# 3. Set session keys on-chain (via polkadot/api or Polkadot.js Apps)
#    → Go to polkadot.js.org/apps → Network → Staking → Account Actions

# 4. Bond TKS and start validating (via Polkadot.js Apps)
#    → Staking → Bond → Validate

# 5. Wait for next era, collect rewards
#    → Staking → Payouts → PayAll
```

---

*TKS Explorer: https://tksscan.com*
*Polkadot.js UI: https://polkadot.js.org/apps/?rpc=ws://localhost:9944*
