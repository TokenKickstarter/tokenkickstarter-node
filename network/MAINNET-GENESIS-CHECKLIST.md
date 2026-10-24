# Mainnet genesis checklist

Do **not** expose `local_testnet_config()` (Alice/Bob, `TKS Local Testnet`, `ChainType::Local`) on public infrastructure.

## 1. Naming and chain type

| Field | Dev (current local) | Production target |
|-------|----------------------|-------------------|
| `system_chain` | TKS Local Testnet | TKS Network (or approved product name) |
| `chainType` | Local | Live |
| `chain id` (properties / EVM) | 7779 | 7779 (unchanged unless fork) |
| Spec file id | `tks_local` | `tks_mainnet` (distinct from dev) |

Build raw spec:

```bash
./target/release/tks-chain-node build-spec --chain <production-spec.json> --raw > tks-mainnet-spec-raw.json
```

Publish genesis hash and spec version in release notes.

## 2. Authorities and session keys

- [ ] 4–7 independent validator operators (minimum 2 for Aura, recommend more for decentralization claims).
- [ ] Unique stash account per validator; bond >= `minValidatorBond`.
- [ ] Aura (sr25519) and GRANDPA (ed25519) keys generated offline per validator.
- [ ] Genesis `session.keys` and `staking.stakers` list **only** production keys.
- [ ] No `//Alice`, `//Bob`, or Hardhat keys in production genesis.

## 3. Sudo and treasury

- [ ] No single leaked or dev key as sudo.
- [ ] Treasury allocation moved to multisig cold storage.
- [ ] Sudo removed or held by multisig after launch checklist complete.
- [ ] Document vesting / bridge reserve wallets in [GENESIS-AND-BRIDGE.md](./GENESIS-AND-BRIDGE.md).

## 4. EVM genesis accounts

- [ ] Remove Hardhat test accounts from production `evm.accounts`.
- [ ] Fund only audited contract deployers and bridge contracts.

## 5. Runtime

- [ ] `spec_version` set for launch features (408 name batch, 409 adaptive gas, etc.).
- [ ] WASM embedded in node binary matches on-chain `spec_version` after launch.

## 6. Bootnodes and DNS

- [ ] DNS seeds point at production bootnodes ([DNS-SETUP.md](./DNS-SETUP.md)).
- [ ] Bootnode keys not reused from dev.

## 7. Launch verification

```bash
curl -s ... system_chain      # production name
curl -s ... system_chainType  # Live
curl -s ... state_getRuntimeVersion
```

Compare genesis hash across all validators before accepting traffic.

## Related

- [AUDIT-REMEDIATION-v0.1.8.md](./AUDIT-REMEDIATION-v0.1.8.md)
- [GENESIS-AND-BRIDGE.md](./GENESIS-AND-BRIDGE.md)
- [PRODUCTION-DEPLOYMENT-CHECKLIST.md](./PRODUCTION-DEPLOYMENT-CHECKLIST.md)
