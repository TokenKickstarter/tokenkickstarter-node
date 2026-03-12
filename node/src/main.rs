//! TKS Chain Node — main entry point.

mod chain_spec;
mod cli;
mod command;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
    command::run()
}
