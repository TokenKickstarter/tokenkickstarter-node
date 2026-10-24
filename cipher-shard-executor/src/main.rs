use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use cipher_shard_executor::engine::new_shared_engine;
use cipher_shard_executor::server;

#[derive(Parser, Debug)]
#[command(name = "cipher-shard-executor")]
#[command(about = "Cipher HyperSwarm shard executor (batch → DA → L1 anchor payload)")]
struct Args {
    /// Shard id this process serves (must match on-chain assignment).
    #[arg(long, default_value_t = 0)]
    shard_id: u16,

    /// Epoch label stored in DA / anchor args (query chain for current epoch).
    #[arg(long, default_value_t = 0)]
    epoch: u32,

    /// HTTP ingest port.
    #[arg(long, default_value_t = 4010)]
    port: u16,

    /// Seal batch at this interval even if batch is small.
    #[arg(long, default_value_t = 10)]
    anchor_interval_secs: u64,

    /// Directory for data-availability JSON blobs.
    #[arg(long, default_value = "./data/da")]
    da_dir: PathBuf,

    /// Directory for anchor submit JSON payloads.
    #[arg(long, default_value = "./data/anchors")]
    anchors_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::init();
    let args = Args::parse();

    std::fs::create_dir_all(&args.da_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&args.anchors_dir).map_err(|e| e.to_string())?;

    let engine = new_shared_engine();
    let engine_tick = engine.clone();
    let shard_id = args.shard_id;
    let epoch = args.epoch;
    let da_dir = args.da_dir.clone();
    let anchors_dir = args.anchors_dir.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(args.anchor_interval_secs));
        loop {
            interval.tick().await;
            let mut guard = engine_tick.lock().await;
            if guard.pending_len() == 0 {
                continue;
            }
            if let Err(e) = guard.seal(shard_id, epoch, &da_dir, &anchors_dir) {
                log::error!("seal failed: {e}");
            }
        }
    });

    server::run_http_ingest(args.port, engine).await
}
