//! Run shard executor inside `tks-chain-node` (ingest HTTP + periodic seal).

use std::path::PathBuf;
use std::time::Duration;

use crate::engine::new_shared_engine;
use crate::server;

/// Configuration for embedded shard executor.
#[derive(Clone, Debug)]
pub struct EmbeddedExecutorConfig {
    pub shard_id: u16,
    pub epoch: u32,
    pub port: u16,
    pub anchor_interval_secs: u64,
    pub da_dir: PathBuf,
    pub anchors_dir: PathBuf,
}

/// Starts ingest server and background batch sealing. Runs until the HTTP server exits.
pub async fn run_embedded(cfg: EmbeddedExecutorConfig) -> Result<(), String> {
    std::fs::create_dir_all(&cfg.da_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cfg.anchors_dir).map_err(|e| e.to_string())?;

    let engine = new_shared_engine();
    let engine_tick = engine.clone();
    let shard_id = cfg.shard_id;
    let epoch = cfg.epoch;
    let da_dir = cfg.da_dir.clone();
    let anchors_dir = cfg.anchors_dir.clone();
    let interval_secs = cfg.anchor_interval_secs;

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            let mut guard = engine_tick.lock().await;
            if guard.pending_len() == 0 {
                continue;
            }
            if let Err(e) = guard.seal(shard_id, epoch, &da_dir, &anchors_dir) {
                log::error!("Cipher shard seal failed: {e}");
            }
        }
    });

    log::info!(
        "Cipher shard executor: shard_id={} ingest=http://127.0.0.1:{}/ingest da={} anchors={}",
        cfg.shard_id,
        cfg.port,
        cfg.da_dir.display(),
        cfg.anchors_dir.display()
    );

    server::run_http_ingest(cfg.port, engine).await
}
