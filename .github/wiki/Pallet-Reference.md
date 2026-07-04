# Pallet Reference

All pallets active in the TKS runtime. Custom pallets are marked accordingly.

---

## Substrate / Polkadot SDK Pallets

| Pallet | Version Source | Purpose |
|--------|---------------|---------|
| `frame-system` | SDK | Core runtime system — accounts, blocks, events |
| `pallet-timestamp` | SDK | On-chain time (Unix timestamp per block) |
| `pallet-balances` | SDK | Native TKS token balances and transfers |
| `pallet-transaction-payment` | SDK | Fee calculation and payment |
| `pallet-sudo` | SDK | Administrative extrinsics (temporary governance) |
| `pallet-aura` | SDK | Aura block production consensus |
| `pallet-grandpa` | SDK | GRANDPA deterministic finality |
| `pallet-session` | SDK | Validator session key management |
| `pallet-staking` | SDK | Nominated proof-of-stake validator elections |
| `pallet-election-provider-multi-phase` | SDK | NPoS election computation |
| `pallet-bags-list` | SDK | Sorted nomination pools for elections |
| `pallet-offences` | SDK | Misbehavior recording (for slashing) |
| `pallet-im-online` | SDK | Validator liveness heartbeats |
| `pallet-authorship` | SDK | Track block author for rewards |
| `pallet-nfts` | SDK | Native NFT collections and items |
| `pallet-utility` | SDK | Batch / dispatch utility extrinsics |
| `pallet-multisig` | SDK | Multi-signature approval |
| `pallet-scheduler` | SDK | Delayed and recurring on-chain calls |
| `pallet-preimage` | SDK | On-chain preimage storage |
| `pallet-identity` | SDK | On-chain identity registration |
| `pallet-proxy` | SDK | Account delegation and proxying |

---

## Frontier (EVM) Pallets

| Pallet | Purpose |
|--------|---------|
| `pallet-evm` | EVM bytecode execution (SputnikVM) |
| `pallet-ethereum` | Ethereum block/transaction/receipt storage |
| `pallet-base-fee` | EIP-1559 base fee tracking |
| `pallet-dynamic-fee` | Dynamic gas fee adjustment |
| `pallet-hotfix-sufficients` | EVM account existence correction |

---

## Custom Pallets

| Pallet | Location | Purpose |
|--------|----------|---------|
| `pallet-adaptive-gas` | `pallets/pallet-adaptive-gas/` | Auto-scaling block gas limit |
| `pallet-hyperswarm-anchor` | `pallets/pallet-hyperswarm-anchor/` | On-chain HyperSwarm DHT key anchoring |
| `pallet-name-registry` | `pallets/pallet-name-registry/` | Free human-readable on-chain names |
| `pallet-shard-registry` | `pallets/pallet-shard-registry/` | Shard coordination and registration |

See [Custom Pallets](Custom-Pallets) for detailed documentation on each.

---

## Storage Items by Pallet

### pallet-balances

| Storage | Type | Description |
|---------|------|-------------|
| `Account` | Map `AccountId20 → AccountData` | Account balance information |
| `TotalIssuance` | `Balance` | Total circulating supply |
| `Locks` | Map `AccountId20 → Vec<BalanceLock>` | Locked balance entries |
| `Reserves` | Map `AccountId20 → Vec<ReserveData>` | Reserved balance entries |

### pallet-staking

| Storage | Type | Description |
|---------|------|-------------|
| `Validators` | Map `AccountId20 → ValidatorPrefs` | Active validator set preferences |
| `Nominators` | Map `AccountId20 → Nominations` | Nominator targets |
| `ErasStakers` | Map `(EraIndex, AccountId20) → Exposure` | Staking exposure per era |
| `CurrentEra` | `Option<EraIndex>` | Current era number |
| `BondedEras` | `Vec<(EraIndex, SessionIndex)>` | Era-session mapping |

---

## Extrinsics Quick Reference

### Transfer TKS

```
pallet-balances → transfer_keep_alive(dest, value)
```

### Submit EVM Transaction

```
pallet-ethereum → transact(transaction)
```
(Typically handled automatically by MetaMask / ethers.js)

### Bond TKS for Staking

```
pallet-staking → bond(controller, value, payee)
```

### Nominate Validators

```
pallet-staking → nominate(targets)
```

### Register On-chain Name

```
pallet-name-registry → register(name)
```

### Anchor HyperSwarm Key

```
pallet-hyperswarm-anchor → anchor(key)
```

### Create NFT Collection

```
pallet-nfts → create(admin, config)
```

### Mint NFT

```
pallet-nfts → mint(collection, item, mint_to, witness_data)
```
