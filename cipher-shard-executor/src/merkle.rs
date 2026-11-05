//! Batch Merkle root. Same bytes as `pallet-hyperswarm-anchor` fraud proofs
//! (`cipher-merkle`, leaf domain `tks-leaf-v1`). The on-chain field is still
//! named `dag_root_hash`. The structure is a binary Merkle tree, not a general DAG.

use cipher_merkle::{merkle_root, MerkleLeaf};

use crate::batch::CipherBatchMessage;

/// Computes the batch Merkle root used in `HyperSwarmAnchor::post_anchor`.
pub fn compute_dag_root(messages: &[CipherBatchMessage]) -> [u8; 32] {
    let leaves: Vec<MerkleLeaf> = messages
        .iter()
        .map(|msg| MerkleLeaf {
            msg_id: msg.msg_id.as_bytes().to_vec(),
            sender_id: msg.sender_id.as_bytes().to_vec(),
            recipient_id: msg.recipient_id.as_bytes().to_vec(),
            ciphertext_hash: msg.ciphertext_hash,
            stored_at: msg.stored_at,
        })
        .collect();
    merkle_root(&leaves)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_stable_for_ordering() {
        let a = CipherBatchMessage {
            msg_id: "b".into(),
            sender_id: "s".into(),
            recipient_id: "r".into(),
            ciphertext_hash: [1u8; 32],
            stored_at: 1,
        };
        let b = CipherBatchMessage {
            msg_id: "a".into(),
            sender_id: "s".into(),
            recipient_id: "r".into(),
            ciphertext_hash: [2u8; 32],
            stored_at: 2,
        };
        assert_eq!(compute_dag_root(&[a.clone(), b.clone()]), compute_dag_root(&[b, a]));
    }
}
