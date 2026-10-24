use std::path::PathBuf;

use clap::Parser;
use cipher_shard_executor::{run_embedded, EmbeddedExecutorConfig};

#[derive(Parser, Debug)]
#[command(name = "cipher-shard-executor")]
#[command(about = "Standalone Cipher shard executor (also embedded in tks-chain-node)")]
struct Args {
    #[arg(long, default_value_t = 0)]
    shard_id: u16,
    #[arg(long, default_value_t = 0)]
    epoch: u32,
    #[arg(long, default_value_t = 4010)]
    port: u16,
    #[arg(long, default_value_t = 10)]
    anchor_interval_secs: u64,
    #[arg(long, default_value = "./data/da")]
    da_dir: PathBuf,
    #[arg(long, default_value = "./data/anchors")]
    anchors_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::init();
    let args = Args::parse();
    run_embedded(EmbeddedExecutorConfig {
        shard_id: args.shard_id,
        epoch: args.epoch,
        port: args.port,
        anchor_interval_secs: args.anchor_interval_secs,
        da_dir: args.da_dir,
        anchors_dir: args.anchors_dir,
    })
    .await
}
