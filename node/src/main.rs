//! TKS Chain Node — main entry point.

mod chain_spec;
mod cipher_anchor;
mod cipher_epoch;
mod cli;
mod command;
mod hyperswarm_rpc;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
    command::run()
}
