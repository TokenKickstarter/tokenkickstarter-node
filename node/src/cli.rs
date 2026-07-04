use sc_cli::RunCmd;

#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<Subcommand>,

    #[clap(flatten)]
    pub run: RunCmd,

    /// Whether to enable the embedded Cipher Storage Relay
    #[arg(long, default_value_t = true)]
    pub enable_cipher_relay: bool,

    /// Port for the embedded Cipher Storage Relay HTTP API
    #[arg(long, default_value_t = 4002)]
    pub cipher_relay_port: u16,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Subcommand {
    /// Build a chain specification.
    BuildSpec(sc_cli::BuildSpecCmd),

    /// Validate blocks.
    CheckBlock(sc_cli::CheckBlockCmd),

    /// Export blocks.
    ExportBlocks(sc_cli::ExportBlocksCmd),

    /// Export the state of a given block into a chain spec.
    ExportState(sc_cli::ExportStateCmd),

    /// Import blocks.
    ImportBlocks(sc_cli::ImportBlocksCmd),

    /// Remove the whole chain.
    PurgeChain(sc_cli::PurgeChainCmd),

    /// Revert the chain to a previous state.
    Revert(sc_cli::RevertCmd),

    /// Key management CLI utilities.
    #[command(subcommand)]
    Key(sc_cli::KeySubcommand),

    /// Db meta columns information.
    ChainInfo(sc_cli::ChainInfoCmd),

    /// List all available RPC methods exposed by this node.
    ///
    /// Shows every JSON-RPC method your node accepts, grouped by category.
    /// For live discovery from a running node, use:
    ///   curl -X POST http://127.0.0.1:9944 \
    ///     -H "Content-Type: application/json" \
    ///     -d '{"jsonrpc":"2.0","method":"rpc_methods","params":[],"id":1}'
    RpcHelp(RpcHelpCmd),
}

/// Arguments for the `rpc-help` subcommand.
#[derive(Debug, clap::Parser)]
pub struct RpcHelpCmd {
    /// Show only methods matching this filter string.
    #[arg(long, short = 'f')]
    pub filter: Option<String>,

    /// Show only Ethereum (eth_*) methods.
    #[arg(long)]
    pub eth: bool,

    /// Show only Substrate (system_*, chain_*, state_*, author_*) methods.
    #[arg(long)]
    pub substrate: bool,

    /// Show only staking-related methods.
    #[arg(long)]
    pub staking: bool,
}

impl RpcHelpCmd {
    pub fn run(&self) {
        let methods = rpc_method_list();

        println!();
        println!("  ████████╗██╗  ██╗███████╗  RPC Methods Reference");
        println!("  ╚══██╔══╝██║ ██╔╝██╔════╝  ─────────────────────");
        println!("     ██║   █████╔╝ ███████╗  RPC endpoint: http://127.0.0.1:9944");
        println!("     ██║   ██╔═██╗ ╚════██║  WebSocket:   ws://127.0.0.1:9944");
        println!("     ██║   ██║  ██╗███████║  Chain ID:    7779 (MetaMask/Trust Wallet)");
        println!("     ╚═╝   ╚═╝  ╚═╝╚══════╝");
        println!();

        let filter = self.filter.as_deref().unwrap_or("").to_lowercase();

        for (category, items) in &methods {
            // Apply category filter flags
            if self.eth && !category.contains("Ethereum") { continue; }
            if self.substrate && category.contains("Ethereum") { continue; }
            if self.staking && !category.contains("Staking") { continue; }

            let filtered: Vec<_> = items.iter()
                .filter(|(method, _)| filter.is_empty() || method.to_lowercase().contains(&filter))
                .collect();

            if filtered.is_empty() { continue; }

            println!("  ┌─ {} ", category);
            for (method, desc) in &filtered {
                println!("  │  {:<45} {}", method, desc);
            }
            println!("  └──────────────────────────────────────────────────────────");
            println!();
        }

        println!("  💡 Live discovery (node must be running):");
        println!("     curl -s -X POST http://127.0.0.1:9944 \\");
        println!("       -H 'Content-Type: application/json' \\");
        println!("       -d '{{\"jsonrpc\":\"2.0\",\"method\":\"rpc_methods\",\"params\":[],\"id\":1}}'");
        println!();
        println!("  📖 Full docs: network/RPC-CLI-REFERENCE.md");
        println!("  🌐 GUI:       https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9944");
        println!();
    }
}

/// Complete list of RPC methods available on the TKS node.
fn rpc_method_list() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        ("Ethereum JSON-RPC — Network", vec![
            ("eth_chainId",                   "Returns the chain ID (7779)"),
            ("eth_blockNumber",               "Latest block number"),
            ("eth_syncing",                   "Sync status (false = fully synced)"),
            ("net_version",                   "Network ID"),
            ("net_peerCount",                 "Number of connected peers"),
            ("net_listening",                 "Whether node is listening"),
            ("web3_clientVersion",            "Node client version string"),
        ]),
        ("Ethereum JSON-RPC — Accounts & Balances", vec![
            ("eth_getBalance",                "TKS balance of an address (in wei)"),
            ("eth_getTransactionCount",       "Nonce of an address"),
            ("eth_getCode",                   "Bytecode at a contract address"),
            ("eth_getStorageAt",              "Storage slot value at a contract"),
            ("eth_accounts",                  "List of node-managed accounts"),
        ]),
        ("Ethereum JSON-RPC — Blocks", vec![
            ("eth_getBlockByNumber",          "Block data by number"),
            ("eth_getBlockByHash",            "Block data by hash"),
            ("eth_getBlockTransactionCountByNumber", "Tx count in a block"),
            ("eth_getBlockTransactionCountByHash",   "Tx count in a block (by hash)"),
            ("eth_getUncleCountByBlockNumber", "Always 0 (no uncles in TKS)"),
        ]),
        ("Ethereum JSON-RPC — Transactions", vec![
            ("eth_sendRawTransaction",        "Submit a signed transaction"),
            ("eth_call",                      "Call contract (read-only, no gas cost)"),
            ("eth_estimateGas",               "Estimate gas for a transaction"),
            ("eth_getTransactionByHash",      "Transaction details by hash"),
            ("eth_getTransactionReceipt",     "Receipt (status, logs, gas used)"),
            ("eth_getTransactionByBlockNumberAndIndex", "Tx by block + position"),
            ("eth_getTransactionByBlockHashAndIndex",   "Tx by block hash + position"),
        ]),
        ("Ethereum JSON-RPC — Gas & Fees (EIP-1559)", vec![
            ("eth_gasPrice",                  "Current gas price in wei"),
            ("eth_feeHistory",                "Historical fee data (EIP-1559)"),
            ("eth_maxPriorityFeePerGas",      "Suggested max priority fee"),
        ]),
        ("Ethereum JSON-RPC — Logs & Filters", vec![
            ("eth_getLogs",                   "Query event logs with filter"),
            ("eth_newFilter",                 "Create a log filter"),
            ("eth_newBlockFilter",            "Create a new-block filter"),
            ("eth_getFilterChanges",          "Poll filter for new results"),
            ("eth_uninstallFilter",           "Remove a filter"),
        ]),
        ("Ethereum JSON-RPC — Mempool", vec![
            ("txpool_status",                 "Count of pending/queued transactions"),
            ("txpool_content",                "Full content of the transaction pool"),
            ("txpool_inspect",                "Summary of transaction pool"),
        ]),
        ("Substrate — System", vec![
            ("system_name",                   "Node name (TKS Chain Node)"),
            ("system_version",                "Node binary version"),
            ("system_chain",                  "Chain name (TKS Network)"),
            ("system_chainType",              "Chain type (Live/Development)"),
            ("system_properties",             "Token symbol, decimals, SS58 prefix"),
            ("system_health",                 "Peers count, sync status"),
            ("system_peers",                  "List of connected peers + latency"),
            ("system_networkState",           "Full P2P network state"),
            ("system_localPeerId",            "This node's libp2p Peer ID"),
            ("system_localListenAddresses",   "Addresses this node listens on"),
            ("system_nodeRoles",              "FULL / AUTHORITY / LIGHT"),
            ("system_syncState",             "Block sync progress"),
            ("system_addReservedPeer",        "Add a reserved peer (unsafe)"),
            ("system_removeReservedPeer",     "Remove a reserved peer (unsafe)"),
        ]),
        ("Substrate — Chain", vec![
            ("chain_getHeader",               "Block header (latest or by hash)"),
            ("chain_getBlock",                "Full block by hash"),
            ("chain_getBlockHash",            "Block hash by number"),
            ("chain_getFinalizedHead",         "Latest finalized block hash"),
            ("chain_subscribeNewHeads",       "WS: stream of new block headers"),
            ("chain_subscribeFinalizedHeads", "WS: stream of finalized headers"),
        ]),
        ("Substrate — Author (Validators)", vec![
            ("author_submitExtrinsic",        "Submit a signed extrinsic"),
            ("author_pendingExtrinsics",      "Pending extrinsics in mempool"),
            ("author_rotateKeys",             "Generate new session keys (unsafe)"),
            ("author_insertKey",              "Insert key into keystore (unsafe)"),
            ("author_hasKey",                 "Check if a key exists in keystore"),
            ("author_hasSessionKeys",         "Check if session key set exists"),
            ("author_removeExtrinsic",        "Remove extrinsic from pool (unsafe)"),
        ]),
        ("Substrate — State", vec![
            ("state_getStorage",              "Raw storage value by key"),
            ("state_getStorageHash",          "Hash of a storage value"),
            ("state_getStorageSize",          "Size of a storage value"),
            ("state_call",                    "Call a runtime API method"),
            ("state_getMetadata",             "Full runtime metadata (pallet ABI)"),
            ("state_getRuntimeVersion",       "Runtime spec_version and APIs"),
            ("state_queryStorage",            "Historical storage queries"),
            ("state_subscribeStorage",        "WS: subscribe to storage changes"),
            ("state_getKeys",                 "All storage keys with prefix"),
            ("state_getPairs",                "Storage key-value pairs"),
        ]),
        ("Substrate — Staking & Mining", vec![
            ("state_getStorage [staking.currentEra]",   "Current era number"),
            ("state_getStorage [staking.validators]",   "All current validators"),
            ("state_getStorage [staking.minValidatorBond]", "Min bond (100,000 TKS)"),
            ("state_getStorage [staking.minNominatorBond]", "Min nominator bond"),
            ("state_getStorage [staking.erasStakers]",  "Staker info for an era"),
            ("author_submitExtrinsic [staking.bond]",   "Lock TKS to begin staking"),
            ("author_submitExtrinsic [staking.validate]","Register as validator"),
            ("author_submitExtrinsic [staking.nominate]","Nominate validators"),
            ("author_submitExtrinsic [staking.setPayee]","Set reward destination"),
            ("author_submitExtrinsic [staking.payoutStakers]", "Collect era rewards"),
            ("author_submitExtrinsic [staking.unbond]", "Unlock bonded TKS"),
        ]),
        ("Substrate — GRANDPA Finality", vec![
            ("grandpa_roundState",            "Current GRANDPA voting round"),
            ("grandpa_subscribeJustifications","WS: stream finality justifications"),
        ]),
    ]
}
