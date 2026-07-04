# RPC API Reference

TKS exposes both the Ethereum JSON-RPC API and the Substrate RPC API on the same port (default: `9944`).

**Endpoint:** `http://localhost:9944` (HTTP) or `ws://localhost:9944` (WebSocket)

---

## Ethereum JSON-RPC

All standard Ethereum methods are supported.

### `eth_blockNumber`

Returns the current block number.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://localhost:9944
```

Response:
```json
{"jsonrpc":"2.0","result":"0x1a4","id":1}
```

---

### `eth_getBalance`

Returns the native TKS balance of an address (in wei).

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getBalance","params":["0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac","latest"],"id":1}' \
  http://localhost:9944
```

---

### `eth_sendRawTransaction`

Submit a signed Ethereum transaction.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_sendRawTransaction","params":["0x02f8..."],"id":1}' \
  http://localhost:9944
```

---

### `eth_call`

Call a contract function (read-only, no state change).

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_call","params":[{"to":"0x...","data":"0x70a08231..."},"latest"],"id":1}' \
  http://localhost:9944
```

---

### `eth_estimateGas`

Estimate the gas required for a transaction.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_estimateGas","params":[{"from":"0x...","to":"0x...","value":"0x1"}],"id":1}' \
  http://localhost:9944
```

---

### `eth_getLogs`

Query event logs.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getLogs","params":[{"fromBlock":"0x0","toBlock":"latest","address":"0x..."}],"id":1}' \
  http://localhost:9944
```

---

### Ethereum Methods Summary

| Method | Description |
|--------|-------------|
| `eth_blockNumber` | Current block number |
| `eth_getBalance` | Account ETH balance |
| `eth_getTransactionCount` | Account nonce |
| `eth_getBlockByNumber` | Block by number |
| `eth_getBlockByHash` | Block by hash |
| `eth_getTransactionByHash` | Transaction details |
| `eth_getTransactionReceipt` | Transaction receipt |
| `eth_sendRawTransaction` | Submit signed tx |
| `eth_call` | Read-only contract call |
| `eth_estimateGas` | Gas estimation |
| `eth_gasPrice` | Current gas price |
| `eth_maxPriorityFeePerGas` | EIP-1559 priority fee |
| `eth_feeHistory` | Historical fee data |
| `eth_getLogs` | Event log query |
| `eth_chainId` | Chain ID (7779) |
| `net_version` | Network version |
| `net_listening` | P2P listening status |
| `net_peerCount` | Connected peer count |
| `web3_clientVersion` | Client version string |
| `txpool_status` | Transaction pool status |
| `txpool_content` | Transaction pool contents |

---

## Substrate RPC

### `author_rotateKeys`

Generate a new set of session keys and store them in the keystore. Returns the encoded public keys for use with `session.setKeys`.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"author_rotateKeys","params":[],"id":1}' \
  http://localhost:9944
```

---

### `chain_getBlock`

Get the latest block.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"chain_getBlock","params":[],"id":1}' \
  http://localhost:9944
```

---

### `chain_getFinalizedHead`

Get the hash of the latest finalized block.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"chain_getFinalizedHead","params":[],"id":1}' \
  http://localhost:9944
```

---

### `system_health`

Check node health.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"system_health","params":[],"id":1}' \
  http://localhost:9944
```

Response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "peers": 5,
    "isSyncing": false,
    "shouldHavePeers": true
  },
  "id": 1
}
```

---

### `system_peers`

List connected peers.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"system_peers","params":[],"id":1}' \
  http://localhost:9944
```

---

### `state_getStorage`

Read raw storage value at a key.

```bash
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"state_getStorage","params":["0x...storage_key..."],"id":1}' \
  http://localhost:9944
```

---

### Substrate Methods Summary

| Method | Description |
|--------|-------------|
| `author_rotateKeys` | Generate new session keys |
| `author_insertKey` | Insert a key into the keystore |
| `author_submitExtrinsic` | Submit a signed extrinsic |
| `chain_getBlock` | Get block by hash or latest |
| `chain_getBlockHash` | Get block hash by number |
| `chain_getFinalizedHead` | Get finalized block hash |
| `chain_getHeader` | Get block header |
| `state_getStorage` | Read storage value |
| `state_getRuntimeVersion` | Runtime spec version |
| `state_subscribeStorage` | Subscribe to storage changes (WS) |
| `system_chain` | Chain name |
| `system_health` | Node health status |
| `system_localPeerId` | Local peer ID |
| `system_peers` | Connected peers |
| `system_version` | Node version |

---

## WebSocket Subscriptions

Connect via WebSocket for real-time data:

```javascript
const ws = new WebSocket("ws://localhost:9944");

ws.onopen = () => {
  // Subscribe to new blocks
  ws.send(JSON.stringify({
    jsonrpc: "2.0",
    method: "chain_subscribeNewHeads",
    params: [],
    id: 1,
  }));
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log("New block:", data.params?.result?.number);
};
```

| Subscription | Description |
|-------------|-------------|
| `chain_subscribeNewHeads` | New block headers |
| `chain_subscribeFinalizedHeads` | New finalized block headers |
| `state_subscribeStorage` | Storage key changes |
| `author_submitAndWatchExtrinsic` | Extrinsic lifecycle events |
| `eth_subscribe("newHeads")` | New Ethereum-formatted block headers |
| `eth_subscribe("logs")` | New EVM event logs |
