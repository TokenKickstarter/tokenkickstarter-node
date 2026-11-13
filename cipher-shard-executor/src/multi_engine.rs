//! Multi-shard in-memory batching with per-shard locking and hybrid-driven seal policy.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use cipher_hybrid::HybridController;
use tokio::sync::Mutex;

use crate::anchor::AnchorSubmitPayload;
use crate::batch::CipherBatchMessage;
use crate::engine::BatchEngine;

struct ShardSlot {
    engine: BatchEngine,
    last_seal: Instant,
}

pub struct MultiShardEngine {
    shards: Vec<Arc<Mutex<ShardSlot>>>,
    total_pending: AtomicUsize,
}

impl MultiShardEngine {
    pub fn new(shard_count: u16) -> Self {
        let n = shard_count.max(1) as usize;
        let now = Instant::now();
        Self {
            shards: (0..n)
                .map(|_| {
                    Arc::new(Mutex::new(ShardSlot {
                        engine: BatchEngine::default(),
                        last_seal: now,
                    }))
                })
                .collect(),
            total_pending: AtomicUsize::new(0),
        }
    }

    pub fn shard_count(&self) -> u16 {
        self.shards.len() as u16
    }

    pub async fn ingest(&self, local_shard: u16, msg: CipherBatchMessage) -> usize {
        let idx = (local_shard as usize).min(self.shards.len().saturating_sub(1));
        let mut slot = self.shards[idx].lock().await;
        if slot.engine.ingest(msg) {
            self.total_pending.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.total_pending.load(Ordering::Relaxed)
        }
    }

    /// Group items by shard and acquire each shard's mutex once per batch.
    pub async fn ingest_batch(&self, items: Vec<(u16, CipherBatchMessage)>) -> usize {
        if items.is_empty() {
            return self.total_pending.load(Ordering::Relaxed);
        }
        let max_idx = self.shards.len().saturating_sub(1);
        let mut by_shard: HashMap<usize, Vec<CipherBatchMessage>> = HashMap::new();
        for (local_shard, msg) in items {
            let idx = (local_shard as usize).min(max_idx);
            by_shard.entry(idx).or_default().push(msg);
        }

        let mut total_after = self.total_pending.load(Ordering::Relaxed);
        for (idx, msgs) in by_shard {
            let mut slot = self.shards[idx].lock().await;
            let mut added = 0usize;
            for msg in msgs {
                if slot.engine.ingest(msg) {
                    added += 1;
                }
            }
            if added > 0 {
                total_after = self.total_pending.fetch_add(added, Ordering::Relaxed) + added;
            }
        }
        total_after
    }

    pub fn pending_total(&self) -> usize {
        self.total_pending.load(Ordering::Relaxed)
    }

    /// Seal shards that hit hybrid batch size or seal interval.
    /// Drains each ready shard under its individual lock and performs Merkle hashing + DA I/O lock-free.
    pub async fn seal_ready(
        &self,
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

        for (i, shard_lock) in self.shards.iter().enumerate() {
            let drained = {
                let mut slot = shard_lock.lock().await;
                let pending = slot.engine.pending_len();
                if pending == 0 {
                    continue;
                }
                let due_time = slot.last_seal.elapsed() >= interval;
                let due_size = pending >= max_batch;
                if !due_time && !due_size {
                    continue;
                }
                let msgs = slot.engine.take_seal_batch();
                if !msgs.is_empty() {
                    self.total_pending.fetch_sub(msgs.len(), Ordering::Relaxed);
                }
                slot.last_seal = Instant::now();
                msgs
            };

            if drained.is_empty() {
                continue;
            }

            // Logical hybrid indices parallelize off-chain batching; L1 `post_anchor`
            // uses the validator's assigned on-chain shard (`ShardRegistry`).
            let _ = i;
            let global_shard_id = base_shard_id;
            if let Some(payload) = BatchEngine::finalize_sealed_batch(
                drained,
                global_shard_id,
                epoch,
                da_dir,
                anchors_dir,
            )? {
                hybrid.record_sealed(payload.message_count);
                out.push(payload);
            }
        }
        Ok(out)
    }
}

pub type SharedMultiEngine = Arc<MultiShardEngine>;

pub fn new_shared_multi_engine(shard_count: u16) -> SharedMultiEngine {
    Arc::new(MultiShardEngine::new(shard_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cipher_hybrid::{HybridConfig, HybridController};

    fn sample_msg(id: &str, recipient: &str) -> CipherBatchMessage {
        CipherBatchMessage {
            msg_id: id.into(),
            sender_id: "sender".into(),
            recipient_id: recipient.into(),
            ciphertext_hash: [7u8; 32],
            stored_at: 1,
        }
    }

    #[tokio::test]
    async fn parallel_shard_ingest_and_batch_sealing() {
        let engine = new_shared_multi_engine(8);
        let mut batch = Vec::new();
        for i in 0..32u16 {
            batch.push((
                i % 8,
                sample_msg(&format!("msg-{i}"), &format!("r-{}", i % 8)),
            ));
        }
        // Add duplicate to ensure dedup still counts accurately.
        batch.push((0, sample_msg("msg-0", "r-0")));

        let queued = engine.ingest_batch(batch).await;
        assert_eq!(queued, 32);
        assert_eq!(engine.pending_total(), 32);

        let dir = std::env::temp_dir().join(format!("tks-multi-seal-{}", std::process::id()));
        let da = dir.join("da");
        let anchors = dir.join("anchors");
        let mut cfg = HybridConfig::default();
        cfg.logical_shard_count = 8;
        let hybrid = HybridController::new(cfg);

        // Below max_batch and before seal_interval_ms, nothing seals yet.
        let early = engine
            .seal_ready(0, 1, &da, &anchors, &hybrid)
            .await
            .unwrap();
        assert!(early.is_empty());

        let max_batch = hybrid.settings().max_batch_size as usize;
        // Fill all 8 shards concurrently up to max_batch so size-based sealing triggers across shards.
        let mut handles = Vec::new();
        for i in 0..8u16 {
            let eng = engine.clone();
            handles.push(tokio::spawn(async move {
                let items = (0..max_batch)
                    .map(|j| (i, sample_msg(&format!("fill-{i}-{j}"), &format!("r-{i}"))))
                    .collect();
                eng.ingest_batch(items).await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let total_ingested = 32 + 8 * max_batch;
        assert_eq!(engine.pending_total(), total_ingested);

        // One seal pass drains one bounded fraud-proof batch (up to MAX_BATCH_MESSAGES=512) per ready shard.
        let sealed = engine
            .seal_ready(0, 1, &da, &anchors, &hybrid)
            .await
            .unwrap();
        assert_eq!(sealed.len(), 8);
        let sealed_total: usize = sealed.iter().map(|p| p.message_count as usize).sum();
        assert!(sealed_total > 0 && sealed_total <= total_ingested);
        assert_eq!(engine.pending_total(), total_ingested - sealed_total);
        for p in &sealed {
            assert!(p.message_count > 0 && p.message_count <= 512);
            assert_eq!(p.shard_id, 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
