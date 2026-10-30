//! Identity RPC method names (stripped in patched `sc-service` when `--rpc-hide-node-identity` is set).

use jsonrpsee::RpcModule;

const HIDDEN_SYSTEM_METHODS: &[&str] = &[
    "system_localPeerId",
    "system_localListenAddresses",
    "system_peers",
    "system_unstable_networkState",
];

/// Remove methods that expose P2P identity or internal listen addresses.
pub fn strip_sensitive_system_methods(io: &mut RpcModule<()>) {
    for name in HIDDEN_SYSTEM_METHODS {
        match io.remove_method(*name) {
            Some(_) => log::info!(target: "rpc", "Public RPC: disabled `{name}`"),
            None => log::warn!(
                target: "rpc",
                "Public RPC: could not disable `{name}` (method missing from module)"
            ),
        }
    }
}
