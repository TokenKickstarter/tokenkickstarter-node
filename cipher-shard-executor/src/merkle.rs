//! Merkle root over batch messages (sorted by `msg_id`).

use sha2::{Digest, Sha256};

use crate::batch::CipherBatchMessage;

fn leaf_hash(msg: &CipherBatchMessage) -> [u8; 32] {
    let json = serde_json::to_vec(msg).expect("batch message must serialize");
    Sha256::digest(json).into()
}

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// Computes the DAG / batch Merkle root used in `HyperSwarmAnchor::post_anchor`.
pub fn compute_dag_root(messages: &[CipherBatchMessage]) -> [u8; 32] {
    if messages.is_empty() {
        return [0u8; 32];
    }

    let mut leaves: Vec<[u8; 32]> = {
        let mut sorted: Vec<_> = messages.to_vec();
        sorted.sort_by(|a, b| a.msg_id.cmp(&b.msg_id));
        sorted.iter().map(leaf_hash).collect()
    };

    while leaves.len() > 1 {
        let mut next = Vec::with_capacity((leaves.len() + 1) / 2);
        let mut i = 0;
        while i < leaves.len() {
            if i + 1 < leaves.len() {
                next.push(hash_pair(&leaves[i], &leaves[i + 1]));
                i += 2;
            } else {
                next.push(hash_pair(&leaves[i], &leaves[i]));
                i += 1;
            }
        }
        leaves = next;
    }
    leaves[0]
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
        let r1 = compute_dag_root(&[a.clone(), b.clone()]);
        let r2 = compute_dag_root(&[b, a]);
        assert_eq!(r1, r2);
    }
}
