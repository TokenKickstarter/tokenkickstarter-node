use crate::chain_spec;
use crate::cli::{Cli, Subcommand};
use crate::service;
use sc_cli::SubstrateCli;
use sc_service::PartialComponents;
use tks_runtime::{MIN_SUPPORTED_SPEC_VERSION, SPEC_VERSION_TOLERANCE, VERSION};


impl SubstrateCli for Cli {
    fn impl_name() -> String {
        "TKS Chain Node".into()
    }

    fn impl_version() -> String {
        env!("SUBSTRATE_CLI_IMPL_VERSION").into()
    }

    fn description() -> String {
        "TokenKickstarter Substrate Node — powering Cipher decentralized messenger".into()
    }

    fn author() -> String {
        env!("CARGO_PKG_AUTHORS").into()
    }

    fn support_url() -> String {
        "https://github.com/TokenKickstarter/tks-substrate-node/issues".into()
    }

    fn copyright_start_year() -> i32 {
        2026
    }

    fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
        Ok(match id {
            "dev" => Box::new(chain_spec::development_config()?),
            "" | "local" => Box::new(chain_spec::local_testnet_config()?),
            "tks-network" | "network" => Box::new(chain_spec::tks_network_config()?),
            path => Box::new(chain_spec::ChainSpec::from_json_file(
                std::path::PathBuf::from(path),
            )?),
        })
    }
}

// ─── Version Tolerance Check ────────────────────────────────────────
//
// Called at node startup. Compares this binary's spec_version against
// the current on-chain runtime. Rules:
//
//   on_chain_version ≤ binary + TOLERANCE  → ✅ OK
//   binary < on_chain_version - TOLERANCE  → ❌ too old, refuse to start
//   binary < on_chain_version              → ⚠️  warn, but still start
//
// This function checks the binary's OWN spec_version. When the node
// connects to peers and downloads a newer on-chain WASM, Substrate
// will execute the newer WASM natively — so older binaries can still
// follow the chain as long as the host functions are compatible.
// Beyond TOLERANCE we hard-stop because host function changes may
// have been introduced that the old binary cannot support.
fn check_node_version(on_chain_spec_version: Option<u32>) {
    let binary_version = VERSION.spec_version;

    // If we know the on-chain version (e.g. from a cached state), compare.
    if let Some(on_chain) = on_chain_spec_version {
        if binary_version < on_chain.saturating_sub(SPEC_VERSION_TOLERANCE) {
            // Hard failure — binary is too old.
            log::error!(
                "╔══════════════════════════════════════════════════════════════╗\n\
                 ║  TKS NODE VERSION TOO OLD — REFUSING TO START               ║\n\
                 ║                                                              ║\n\
                 ║  Binary spec_version : {}                                  \n\
                 ║  On-chain spec_version: {}                                  \n\
                 ║  Minimum supported:    {} (current - {})                   \n\
                 ║                                                              ║\n\
                 ║  Please update your TKS node binary:                        ║\n\
                 ║  https://github.com/TokenKickstarter/tks-node/releases      ║\n\
                 ╚══════════════════════════════════════════════════════════════╝",
                binary_version, on_chain,
                MIN_SUPPORTED_SPEC_VERSION, SPEC_VERSION_TOLERANCE
            );
            std::process::exit(1);
        } else if binary_version < on_chain {
            // Soft warning — within tolerance, still starts.
            log::warn!(
                "⚠️  TKS node is {} version(s) behind the on-chain runtime \
                 (binary: {}, on-chain: {}). Update soon — nodes more than {} \
                 versions behind will be rejected. Get the latest binary: \
                 https://github.com/TokenKickstarter/tks-node/releases",
                on_chain - binary_version, binary_version, on_chain,
                SPEC_VERSION_TOLERANCE
            );
        }
    }

    // Always log the current binary version on startup.
    log::info!(
        "TKS Node binary spec_version: {} | tolerance window: ±{} | \
         min supported: {}",
        binary_version, SPEC_VERSION_TOLERANCE, MIN_SUPPORTED_SPEC_VERSION
    );
}

/// Parse and run command line arguments.
pub fn run() -> sc_cli::Result<()> {
    // Run version check at startup (pass None — check is informational at this point;
    // once connected to peers the runtime will enforce compatibility directly).
    check_node_version(None);

    let cli = Cli::from_args();


    match &cli.subcommand {
        Some(Subcommand::Key(cmd)) => cmd.run(&cli),
        Some(Subcommand::RpcHelp(cmd)) => {
            cmd.run();
            Ok(())
        }
        Some(Subcommand::CipherRegisterShard(cmd)) => cmd.run(),
        Some(Subcommand::Production(cmd)) => match cmd {
            crate::genesis_util::ProductionCmd::AccountId20(c) => c.run(),
            crate::genesis_util::ProductionCmd::MultisigAccount(c) => c.run(),
            crate::genesis_util::ProductionCmd::MultisigSudoForceRotate(c) => c.run(),
            crate::genesis_util::ProductionCmd::FundH160(c) => c.run(),
        },

        Some(Subcommand::BuildSpec(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
        }
        Some(Subcommand::CheckBlock(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    import_queue,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(Subcommand::ExportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, config.database), task_manager))
            })
        }
        Some(Subcommand::ExportState(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, config.chain_spec), task_manager))
            })
        }
        Some(Subcommand::ImportBlocks(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    import_queue,
                    ..
                } = service::new_partial(&config)?;
                Ok((cmd.run(client, import_queue), task_manager))
            })
        }
        Some(Subcommand::PurgeChain(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run(config.database))
        }
        Some(Subcommand::Revert(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.async_run(|config| {
                let PartialComponents {
                    client,
                    task_manager,
                    backend,
                    ..
                } = service::new_partial(&config)?;
                let aux_revert = Box::new(|client, _, blocks| {
                    sc_consensus_grandpa::revert(client, blocks)?;
                    Ok(())
                });
                Ok((cmd.run(client, backend, Some(aux_revert)), task_manager))
            })
        }
        Some(Subcommand::ChainInfo(cmd)) => {
            let runner = cli.create_runner(cmd)?;
            runner.sync_run(|config| cmd.run::<tks_runtime::Block>(&config))
        }
        None => {
            let cipher = service::CipherStackConfig {
                disable_cipher_stack: cli.disable_cipher_stack,
                enable_relay: cli.enable_cipher_relay,
                relay_port: cli.cipher_relay_port,
                enable_shard_executor: cli.enable_cipher_shard_executor,
                shard_port: cli.cipher_shard_port,
                shard_id: cli.cipher_shard_id,
                shard_epoch: cli.cipher_shard_epoch,
                anchor_interval_secs: cli.cipher_anchor_interval_secs,
                hybrid_throughput: cli.cipher_hybrid_throughput,
                hybrid_floor_tps: cli.cipher_hybrid_floor_tps,
                hybrid_ceiling_tps: cli.cipher_hybrid_ceiling_tps,
                hybrid_shard_count: cli.cipher_hybrid_shard_count,
                enable_hyperswarm: cli.enable_cipher_hyperswarm,
                hyperswarm_port: cli.cipher_hyperswarm_port,
                public_relay_url: cli.cipher_public_relay_url.clone(),
                hyperswarm_bootnodes: cli.cipher_hyperswarm_bootnodes.clone(),
                hyperswarm_region: cli.cipher_hyperswarm_region.clone(),
                auto_post_anchor: cli.cipher_auto_post_anchor,
                anchor_seed_file: cli.cipher_anchor_seed_file.clone(),
                enable_ethereum_dev_signer: cli.ethereum_dev_signer,
                rpc_hide_node_identity: cli.rpc_hide_node_identity,
            };
            let runner = cli.create_runner(&cli.run)?;
            let pool_future_divisor = cli.pool_future_divisor;
            let pool_params = cli.run.pool_config.clone();
            runner.run_node_until_exit(move |mut config| async move {
                crate::tx_pool::apply_burst_pool_scaling(
                    &mut config,
                    &pool_params,
                    pool_future_divisor,
                );
                service::new_full(config, cipher).map_err(sc_cli::Error::Service)
            })
        }
    }
}
