//! In-memory batching and periodic seal.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;

use cipher_merkle::{MAX_ANCHOR_LEAVES, MAX_LEAF_FIELD_LEN};

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
    /// Dedupe relay `/store` + forwarded `/ingest` (and shard `/store` retries).
    pending_ids: HashSet<String>,
}

impl BatchEngine {
    /// Insert a message into the batch buffer; returns `true` if queued, `false` if dropped/duplicate.
    pub fn ingest(&mut self, msg: CipherBatchMessage) -> bool {
        if msg.msg_id.len() > MAX_LEAF_FIELD_LEN
            || msg.sender_id.len() > MAX_LEAF_FIELD_LEN
            || msg.recipient_id.len() > MAX_LEAF_FIELD_LEN
        {
            log::warn!(
                "dropping cipher message {}: id exceeds {MAX_LEAF_FIELD_LEN} bytes",
                msg.msg_id
            );
            return false;
        }
        // Relay `/store` forwards the same msg_id to `/ingest`. Without this,
        // one client upload became two leaves (r6: 1,020 messages, 2,030 anchored).
        if !self.pending_ids.insert(msg.msg_id.clone()) {
            return false;
        }
        self.pending.push(msg);
        true
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Drain up to `MAX_ANCHOR_LEAVES` under the shard lock so Merkle/DA I/O can run lock-free.
    pub fn take_seal_batch(&mut self) -> Vec<CipherBatchMessage> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let take_n = self.pending.len().min(MAX_ANCHOR_LEAVES);
        let messages: Vec<CipherBatchMessage> = self.pending.drain(..take_n).collect();
        for m in &messages {
            self.pending_ids.remove(&m.msg_id);
        }
        messages
    }

    /// Compute Merkle DAG root, write DA blob, and atomically publish anchor JSON.
    pub fn finalize_sealed_batch(
        messages: Vec<CipherBatchMessage>,
        shard_id: u16,
        epoch: u32,
        da_dir: &Path,
        anchors_dir: &Path,
    ) -> Result<Option<AnchorSubmitPayload>, String> {
        if messages.is_empty() {
            return Ok(None);
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
        let payload =
            AnchorSubmitPayload::new(shard_id, root, count, epoch, da_path.display().to_string());
        let root_hex = payload.dag_root_hash.trim_start_matches("0x");
        let anchor_file = anchors_dir.join(format!("anchor_{}_{}.json", shard_id, root_hex));
        let tmp_file = anchors_dir.join(format!(".anchor_{}_{}.tmp", shard_id, root_hex));
        let json = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
        std::fs::write(&tmp_file, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp_file, &anchor_file).map_err(|e| e.to_string())?;

        log::info!(
            "⚓ Sealed batch: shard={} msgs={} root={} da={}",
            shard_id,
            count,
            payload.dag_root_hash,
            da_path.display()
        );

        Ok(Some(payload))
    }

    pub fn seal(
        &mut self,
        shard_id: u16,
        epoch: u32,
        da_dir: &Path,
        anchors_dir: &Path,
    ) -> Result<Option<AnchorSubmitPayload>, String> {
        let messages = self.take_seal_batch();
        Self::finalize_sealed_batch(messages, shard_id, epoch, da_dir, anchors_dir)
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

    #[test]
    fn seal_counts_each_message_once_and_root_matches_leaves() {
        let mut e = BatchEngine::default();
        e.ingest(sample_msg("m1"));
        e.ingest(sample_msg("m1"));
        e.ingest(sample_msg("m2"));
        let dir = std::env::temp_dir().join(format!("tks-seal-{}", std::process::id()));
        let da = dir.join("da");
        let anchors = dir.join("anchors");
        std::fs::create_dir_all(&da).unwrap();
        std::fs::create_dir_all(&anchors).unwrap();
        let payload = e.seal(0, 1, &da, &anchors).unwrap().expect("sealed");
        assert_eq!(payload.message_count, 2);
        let root = compute_dag_root(&[sample_msg("m2"), sample_msg("m1")]);
        assert_eq!(payload.dag_root_hash, format!("0x{}", hex::encode(root)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn seal_chunks_above_fraud_proof_bound() {
        let mut e = BatchEngine::default();
        for i in 0..(MAX_ANCHOR_LEAVES + 3) {
            e.ingest(sample_msg(&format!("id-{i}")));
        }
        let dir = std::env::temp_dir().join(format!("tks-seal-chunk-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("da")).unwrap();
        std::fs::create_dir_all(dir.join("anchors")).unwrap();
        let first = e
            .seal(0, 1, &dir.join("da"), &dir.join("anchors"))
            .unwrap()
            .unwrap();
        assert_eq!(first.message_count as usize, MAX_ANCHOR_LEAVES);
        assert_eq!(e.pending_len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub fn new_shared_engine() -> SharedEngine {
    Arc::new(Mutex::new(BatchEngine::default()))
}
