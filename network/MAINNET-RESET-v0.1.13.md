# Mainnet reset — v0.1.13 (spec 412)

v0.1.12 production genesis allocated ~1B TKS to **un-signable** H160s (first 20 bytes of compressed ECDSA pubkeys). v0.1.13 fixes derivation and ships a **new** `tks-network-spec-raw.json`.

## Operator checklist

1. Build release binary: `cargo build --release -p tks-chain-node`
2. Confirm keys under `network/production-keys/` (unchanged mnemonics; **do not** commit).
3. Regenerate spec (if needed): `network/scripts/assemble-tks-network-spec.sh` && `network/scripts/build-tks-network-spec.sh`
4. Record new **genesis hash** from raw spec / node log on first start.
5. On each validator: stop containers → replace chainspec → **purge chain** (`tks-chain-node purge-chain …`) → start with `--chain /path/to/tks-network-spec-raw.json`
6. Re-insert Aura/GRANDPA session keys from `production-keys/` (same as initial launch).
7. Verify: `network/scripts/public-rpc-test.sh` with `EXPECTED_SPEC=412`
8. Smoke: fund test wallet from a treasury key via MetaMask or `eth_sendRawTransaction` (balances now at Keccak-derived H160s).

Public address map: `production-keys/genesis-summary.json` (local only). Example shape: `genesis-summary.public.example.json`.
