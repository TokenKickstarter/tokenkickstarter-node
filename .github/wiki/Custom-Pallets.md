# Custom Pallets

TKS includes four custom pallets that extend the Substrate SDK functionality.

---

## pallet-adaptive-gas

**Location:** `pallets/pallet-adaptive-gas/`

### Purpose

Automatically scales the block gas limit based on real-time network utilization. This eliminates the need for manual parameter tuning and allows the chain to handle traffic bursts while staying lean during quiet periods.

### How It Works

Each block, the pallet measures the gas consumed relative to the current gas limit. If utilization is consistently high, the gas limit increases. If consistently low, it decreases. The algorithm is similar to EIP-1559's base fee adjustment but applied to the gas limit rather than price.

```
New Gas Limit = Current Gas Limit × (1 + adjustment_factor × utilization_delta)
```

### Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `MinGasLimit` | Minimum allowed gas limit | ~1,500 tx equivalent |
| `MaxGasLimit` | Maximum allowed gas limit | ~80,000 tx equivalent |
| `AdjustmentFactor` | Speed of adjustment (per block) | 1/8 |
| `TargetUtilization` | Target fill ratio | 50% |

### Storage

| Item | Type | Description |
|------|------|-------------|
| `CurrentGasLimit` | `u64` | Current block gas limit |

### Extrinsics

None — the gas limit adjusts automatically via the `on_finalize` hook.

---

## pallet-hyperswarm-anchor

**Location:** `pallets/pallet-hyperswarm-anchor/`

### Purpose

Allows decentralized applications to anchor their [HyperSwarm](https://github.com/holepunchto/hyperswarm) DHT discovery keys to Layer-1 blockchain finality.

### What is HyperSwarm?

HyperSwarm is a P2P networking protocol used by [Holepunch](https://holepunch.to) (the Hypercore Protocol ecosystem). Applications announce themselves on the DHT using a 32-byte public key. Other peers discover them by looking up this key. Anchoring this key on-chain provides:

1. **Tamper-proof registration** — the key is stored at Layer-1 finality
2. **Censorship resistance** — no central registrar can remove it
3. **Cross-chain verifiability** — any chain that trusts TKS can verify the key

### Extrinsics

#### `anchor(key: [u8; 32])`

Register or update a HyperSwarm key for the calling account.

```
pallet-hyperswarm-anchor → anchor(key: 0xdeadbeef...)
```

#### `revoke()`

Remove the calling account's anchored key.

### Storage

| Item | Type | Description |
|------|------|-------------|
| `Anchors` | Map `AccountId20 → [u8; 32]` | Account → HyperSwarm key |

### Events

| Event | Description |
|-------|-------------|
| `KeyAnchored(who, key)` | Key registered or updated |
| `KeyRevoked(who)` | Key removed |

---

## pallet-name-registry

**Location:** `pallets/pallet-name-registry/`

### Purpose

Provides free (zero-deposit) human-readable on-chain names such as `alice.tks`. Names are registered, transferred, and resolved entirely on-chain.

### Design Decisions

- **No deposit** — Unlike ENS or similar registries, TKS names require no deposit bond. This maximizes accessibility.
- **First-come, first-served** — Names are allocated to the first account that registers them.
- **Transferable** — Names can be transferred to another account.
- **Substrate-native** — Resolved on the Substrate side, not via EVM smart contracts (though EVM contracts can query names via precompiles).

### Name Format

Names are UTF-8 strings up to 64 bytes. The TLD `.tks` is implicit in the context of this pallet.

### Extrinsics

#### `register(name: Vec<u8>)`

Register a name for the calling account. Fails if the name is already taken.

#### `transfer(name: Vec<u8>, dest: AccountId20)`

Transfer ownership of a registered name.

#### `deregister(name: Vec<u8>)`

Release a registered name. Makes it available for re-registration.

### Storage

| Item | Type | Description |
|------|------|-------------|
| `Names` | Map `Vec<u8> → AccountId20` | Name → owner |
| `OwnedNames` | Map `AccountId20 → Vec<Vec<u8>>` | Owner → names |

### Events

| Event | Description |
|-------|-------------|
| `NameRegistered(name, owner)` | New name registered |
| `NameTransferred(name, from, to)` | Name ownership transferred |
| `NameDeregistered(name)` | Name released |

---

## pallet-shard-registry

**Location:** `pallets/pallet-shard-registry/`

### Purpose

Provides coordination infrastructure for TKS shards — logical sub-chains or processing units that can be registered, tracked, and verified at Layer-1.

### Use Cases

- Register a new shard with its genesis configuration hash
- Track which validators are assigned to which shard
- Coordinate cross-shard state roots for bridge verification

### Extrinsics

#### `register_shard(shard_id: u32, config_hash: [u8; 32])`

Register a new shard with its configuration hash.

#### `deregister_shard(shard_id: u32)`

Remove a shard registration (sudo-gated).

### Storage

| Item | Type | Description |
|------|------|-------------|
| `Shards` | Map `u32 → ShardInfo` | Shard ID → registration data |
| `ShardCount` | `u32` | Total registered shards |

---

## Writing Custom Pallets

If you want to extend TKS with a new pallet:

1. Create `pallets/pallet-your-pallet/`
2. Add `pallet-your-pallet = { path = "../pallets/pallet-your-pallet", default-features = false }` to `runtime/Cargo.toml`
3. Implement `frame_support::pallet!` macro with your storage, events, errors, and calls
4. Add `YourPallet: pallet_your_pallet` to `construct_runtime!` in `runtime/src/lib.rs`
5. Implement the required `Config` trait

See the [Substrate documentation](https://docs.substrate.io/reference/frame-pallets/) for full guidance.
