//! TKS Chain Node — main entry point.

mod account_id20;
mod chain_spec;
mod cipher_anchor;
mod cipher_epoch;
mod cli;
mod command;
mod ecdsa_seed;
mod ethereum_sign;
mod extrinsic_sign;
mod genesis_util;
mod hyperswarm_rpc;
mod multisig_pending_cmd;
mod multisig_storage;
mod multisig_sudo_cmd;
mod multisig_sudo_set_code_cmd;
mod production_fund_cmd;
mod rpc;
mod rpc_public;
mod service;
mod shard_setup_cmd;
mod tx_pool;

fn main() -> sc_cli::Result<()> {
    command::run()
}
