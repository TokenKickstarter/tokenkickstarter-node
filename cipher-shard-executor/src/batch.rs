//! Canonical batch format for Cipher shard anchors (see network/CIPHER-TKS-THROUGHPUT-SPEC.md).

use serde::{Deserialize, Serialize};

/// One message included in a shard batch (DA + Merkle leaf).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CipherBatchMessage {
    pub msg_id: String,
    pub sender_id: String,
    pub recipient_id: String,
    /// Blake2-256 of ciphertext bytes (relay stores full ciphertext off-chain).
    pub ciphertext_hash: [u8; 32],
    pub stored_at: u64,
}

/// Serializable batch written to DA before `post_anchor`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataAvailabilityBlob {
    pub shard_id: u16,
    pub epoch: u32,
    pub sealed_at: u64,
    pub messages: Vec<CipherBatchMessage>,
}

impl DataAvailabilityBlob {
    pub fn message_count(&self) -> u32 {
        self.messages.len().min(u32::MAX as usize) as u32
    }
}
