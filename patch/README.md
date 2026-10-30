# Patched upstream crates

## `sc-service` (polkadot-sdk `stable2512`)

Vendored copy with a small change in `gen_rpc_module`: when `TKS_RPC_HIDE_NODE_IDENTITY=1` (set by `--rpc-hide-node-identity`), remove public identity RPC methods **after** Substrate merges `sc_rpc::system`.

Re-sync from upstream when bumping polkadot-sdk; re-apply the block marked `TKS:` in `sc-service/src/builder.rs`.
