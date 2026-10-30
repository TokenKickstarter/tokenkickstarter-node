//! Multi-shard in-memory batching with hybrid-driven seal policy.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use cipher_hybrid::HybridController;
use tokio::sync::Mutex;

use crate::anchor::AnchorSubmitPayload;
use crate::batch::CipherBatchMessage;
use crate::engine::BatchEngine;

pub struct MultiShardEngine {
    shards: Vec<BatchEngine>,
    last_seal: Vec<Instant>,
}

impl MultiShardEngine {
    pub fn new(shard_count: u16) -> Self {
        let n = shard_count.max(1) as usize;
        let now = Instant::now();
        Self {
            shards: (0..n).map(|_| BatchEngine::default()).collect(),
            last_seal: vec![now; n],
        }
    }

    pub fn shard_count(&self) -> u16 {
        self.shards.len() as u16
    }

    pub fn ingest(&mut self, local_shard: u16, msg: CipherBatchMessage) {
        let idx = (local_shard as usize).min(self.shards.len().saturating_sub(1));
        self.shards[idx].ingest(msg);
    }

    pub fn pending_total(&self) -> usize {
        self.shards.iter().map(|s| s.pending_len()).sum()
    }

    /// Seal shards that hit hybrid batch size or seal interval.
    pub fn seal_ready(
        &mut self,
        base_shard_id: u16,
        epoch: u32,
        da_dir: &Path,
        anchors_dir: &Path,
        hybrid: &HybridController,
    ) -> Result<Vec<AnchorSubmitPayload>, String> {
        let settings = hybrid.settings();
        let interval = std::time::Duration::from_millis(settings.seal_interval_ms);
        let max_batch = settings.max_batch_size as usize;
        let mut out = Vec::new();

        for (i, engine) in self.shards.iter_mut().enumerate() {
            let pending = engine.pending_len();
            if pending == 0 {
                continue;
            }
            let due_time = self.last_seal[i].elapsed() >= interval;
            let due_size = pending >= max_batch;
            if !due_time && !due_size {
                continue;
            }
            // Logical hybrid indices are off-chain only; L1 `post_anchor` uses the node's
            // assigned shard until independent shard hosts exist per validator.
            let _ = i;
            let global_shard_id = base_shard_id;
            if let Some(payload) = engine.seal(global_shard_id, epoch, da_dir, anchors_dir)? {
                hybrid.record_sealed(payload.message_count);
                self.last_seal[i] = Instant::now();
                out.push(payload);
            }
        }
        Ok(out)
    }
}

pub type SharedMultiEngine = Arc<Mutex<MultiShardEngine>>;

pub fn new_shared_multi_engine(shard_count: u16) -> SharedMultiEngine {
    Arc::new(Mutex::new(MultiShardEngine::new(shard_count)))
}
