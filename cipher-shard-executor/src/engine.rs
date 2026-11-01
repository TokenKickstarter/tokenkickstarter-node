//! In-memory batching and periodic seal.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;

use crate::anchor::AnchorSubmitPayload;
use crate::batch::{CipherBatchMessage, DataAvailabilityBlob};
use crate::da::write_blob;
use crate::merkle::compute_dag_root;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub struct BatchEngine {
    pending: Vec<CipherBatchMessage>,
    /// Dedupe relay `/store` + forwarded `/ingest` (and shard `/store` retries).
    pending_ids: HashSet<String>,
}

impl Default for BatchEngine {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            pending_ids: HashSet::new(),
        }
    }
}

impl BatchEngine {
    pub fn ingest(&mut self, msg: CipherBatchMessage) {
        if !self.pending_ids.insert(msg.msg_id.clone()) {
            return;
        }
        self.pending.push(msg);
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn seal(
        &mut self,
        shard_id: u16,
        epoch: u32,
        da_dir: &Path,
        anchors_dir: &Path,
    ) -> Result<Option<AnchorSubmitPayload>, String> {
        if self.pending.is_empty() {
            return Ok(None);
        }

        let messages = std::mem::take(&mut self.pending);
        for m in &messages {
            self.pending_ids.remove(&m.msg_id);
        }
        let root = compute_dag_root(&messages);
        let blob = DataAvailabilityBlob {
            shard_id,
            epoch,
            sealed_at: now_secs(),
            messages,
        };
        let count = blob.message_count();
        let da_path = write_blob(da_dir, &root, &blob)?;

        std::fs::create_dir_all(anchors_dir).map_err(|e| e.to_string())?;
        let payload = AnchorSubmitPayload::new(shard_id, root, count, epoch, da_path.display().to_string());
        let anchor_file = anchors_dir.join(format!(
            "anchor_{}_{}.json",
            shard_id,
            payload.dag_root_hash.trim_start_matches("0x")
        ));
        let json = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
        std::fs::write(&anchor_file, json).map_err(|e| e.to_string())?;

        log::info!(
            "⚓ Sealed batch: shard={} msgs={} root={} da={}",
            shard_id,
            count,
            payload.dag_root_hash,
            da_path.display()
        );

        Ok(Some(payload))
    }
}

pub type SharedEngine = Arc<Mutex<BatchEngine>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::CipherBatchMessage;

    fn sample_msg(id: &str) -> CipherBatchMessage {
        CipherBatchMessage {
            msg_id: id.into(),
            sender_id: "t".into(),
            recipient_id: "r".into(),
            ciphertext_hash: [0u8; 32],
            stored_at: 1,
        }
    }

    #[test]
    fn ingest_dedupes_same_msg_id() {
        let mut e = BatchEngine::default();
        e.ingest(sample_msg("m1"));
        e.ingest(sample_msg("m1"));
        assert_eq!(e.pending_len(), 1);
    }
}

pub fn new_shared_engine() -> SharedEngine {
    Arc::new(Mutex::new(BatchEngine::default()))
}
