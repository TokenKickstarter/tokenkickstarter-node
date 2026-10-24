//! In-memory batching and periodic seal.

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

#[derive(Default)]
pub struct BatchEngine {
    pending: Vec<CipherBatchMessage>,
}

impl BatchEngine {
    pub fn ingest(&mut self, msg: CipherBatchMessage) {
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

pub fn new_shared_engine() -> SharedEngine {
    Arc::new(Mutex::new(BatchEngine::default()))
}
