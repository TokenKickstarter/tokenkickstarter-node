//! TKS Chain Node — main entry point.

mod account_id20;
mod chain_spec;
mod cipher_anchor;
mod cipher_epoch;
mod cli;
mod command;
mod ethereum_sign;
mod extrinsic_sign;
mod genesis_util;
mod multisig_sudo_cmd;
mod shard_setup_cmd;
mod hyperswarm_rpc;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
    command::run()
}
