# MetaMask and EVM

TKS provides full Ethereum Virtual Machine compatibility via the [Frontier](https://github.com/polkadot-evm/frontier) pallet suite. Any EVM-compatible tool works with TKS without modification.

---

## Connect MetaMask

### Network Settings

Open MetaMask → Settings → Networks → Add Network → Add a network manually:

| Field | Value |
|-------|-------|
| Network Name | TKS Network |
| New RPC URL | `https://rpc.tokenkickstarter.com` |
| Chain ID | `7779` |
| Currency Symbol | `TKS` |
| Block Explorer URL | `https://scan.tokenkickstarter.com` |

### Local Development

| Field | Value |
|-------|-------|
| New RPC URL | `http://127.0.0.1:9944` |
| Chain ID | `7779` |

---

## EVM Compatibility

TKS is compatible with:

| Tool | Status |
|------|--------|
| MetaMask | Full |
| WalletConnect | Full |
| ethers.js | Full |
| viem / wagmi | Full |
| Hardhat | Full |
| Foundry | Full |
| Truffle | Full |
| Remix IDE | Full |
| OpenZeppelin | Full |
| TheGraph | Full (with local graph-node) |

---

## Deploy with Hardhat

**`hardhat.config.js`:**

```javascript
module.exports = {
  networks: {
    tks: {
      url: "https://rpc.tokenkickstarter.com",
      chainId: 7779,
      accounts: ["0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"],
    },
    tksLocal: {
      url: "http://127.0.0.1:9944",
      chainId: 7779,
      accounts: ["0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"],
    },
  },
  solidity: "0.8.24",
};
```

**Deploy:**

```bash
npx hardhat run scripts/deploy.js --network tks
```

---

## Deploy with Foundry

**`foundry.toml`:**

```toml
[profile.default]
src = "src"
out = "out"
libs = ["lib"]

[rpc_endpoints]
tks = "https://rpc.tokenkickstarter.com"
tks_local = "http://127.0.0.1:9944"
```

**Deploy:**

```bash
forge script script/Deploy.s.sol:DeployScript \
  --rpc-url tks \
  --private-key $PRIVATE_KEY \
  --broadcast
```

---

## Connect with ethers.js

```javascript
const { ethers } = require("ethers");

const provider = new ethers.JsonRpcProvider("https://rpc.tokenkickstarter.com");

// Get block number
const blockNumber = await provider.getBlockNumber();
console.log("Block:", blockNumber);

// Send transaction
const wallet = new ethers.Wallet(PRIVATE_KEY, provider);
const tx = await wallet.sendTransaction({
  to: "0x...",
  value: ethers.parseEther("1.0"),
});
await tx.wait();
```

---

## Connect with viem

```typescript
import { createPublicClient, createWalletClient, http, defineChain } from "viem";
import { privateKeyToAccount } from "viem/accounts";

const tks = defineChain({
  id: 7779,
  name: "TKS Network",
  nativeCurrency: { name: "TKS", symbol: "TKS", decimals: 18 },
  rpcUrls: {
    default: { http: ["https://rpc.tokenkickstarter.com"] },
  },
  blockExplorers: {
    default: { name: "TKS Explorer", url: "https://scan.tokenkickstarter.com" },
  },
});

const publicClient = createPublicClient({ chain: tks, transport: http() });
const blockNumber = await publicClient.getBlockNumber();
```

---

## EIP Support

| EIP | Description | Status |
|-----|-------------|--------|
| EIP-155 | Replay protection | Supported |
| EIP-1559 | Fee market | Supported |
| EIP-2718 | Transaction envelope | Supported |
| EIP-2930 | Access lists | Supported |
| EIP-3541 | Reject 0xEF contracts | Supported |
| EIP-3855 | PUSH0 opcode | Supported |
| EIP-4399 | PREVRANDAO | Supported |
| ERC-20 | Fungible tokens | Deployable |
| ERC-721 | NFTs | Deployable |
| ERC-1155 | Multi-token | Deployable |

---

## Gas and Fees

TKS uses EIP-1559 base fee mechanics:

- **Base fee** adjusts automatically each block based on utilization
- **Priority fee** (tip) goes to the block producer
- **Base fee** is burned
- Gas limit per block scales from 1,500 to 80,000 transactions equivalent (adaptive)

Estimate gas programmatically:

```javascript
const gasEstimate = await provider.estimateGas({
  to: contractAddress,
  data: calldata,
});
const feeData = await provider.getFeeData();
```

---

## JSON-RPC Endpoints

The node exposes the standard Ethereum JSON-RPC namespace plus Substrate namespaces:

| Namespace | Description |
|-----------|-------------|
| `eth_*` | Ethereum API |
| `net_*` | Network info |
| `web3_*` | Web3 utilities |
| `debug_*` | Debug tracing |
| `txpool_*` | Transaction pool |
| `author_*` | Substrate author API |
| `chain_*` | Substrate chain API |
| `state_*` | Substrate state API |
| `system_*` | Substrate system API |

See [RPC API Reference](RPC-API-Reference) for detailed endpoint documentation.
