//! Canonical Cipher batch Merkle root.
//!
//! `prove_false_anchor` recomputes this exact root. Leaves are length-prefixed
//! ids plus the ciphertext hash and timestamp. This is a binary Merkle tree
//! over one batch, not a general-purpose DAG.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use sha2::{Digest, Sha256};

/// Maximum leaves in one anchor so a fraud proof fits in one extrinsic.
pub const MAX_ANCHOR_LEAVES: usize = 512;

/// Max bytes for `msg_id`, `sender_id`, and `recipient_id`.
pub const MAX_LEAF_FIELD_LEN: usize = 128;

/// One message leaf committed in `dag_root_hash`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleLeaf {
    pub msg_id: Vec<u8>,
    pub sender_id: Vec<u8>,
    pub recipient_id: Vec<u8>,
    pub ciphertext_hash: [u8; 32],
    pub stored_at: u64,
}

fn push_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    let n = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
    buf.extend_from_slice(&n.to_le_bytes());
    buf.extend_from_slice(bytes);
}

/// SHA-256 of `tks-leaf-v1` || length-prefixed fields.
pub fn leaf_hash(leaf: &MerkleLeaf) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"tks-leaf-v1");
    push_lp(&mut buf, &leaf.msg_id);
    push_lp(&mut buf, &leaf.sender_id);
    push_lp(&mut buf, &leaf.recipient_id);
    buf.extend_from_slice(&leaf.ciphertext_hash);
    buf.extend_from_slice(&leaf.stored_at.to_le_bytes());
    Sha256::digest(&buf).into()
}

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// Merkle root over `leaves`, sorted by `msg_id`. Empty input is the zero root.
pub fn merkle_root(leaves: &[MerkleLeaf]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    let mut order: Vec<usize> = (0..leaves.len()).collect();
    order.sort_by(|&a, &b| leaves[a].msg_id.cmp(&leaves[b].msg_id));
    let mut level: Vec<[u8; 32]> = order.into_iter().map(|i| leaf_hash(&leaves[i])).collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            if i + 1 < level.len() {
                next.push(hash_pair(&level[i], &level[i + 1]));
                i += 2;
            } else {
                next.push(hash_pair(&level[i], &level[i]));
                i += 1;
            }
        }
        level = next;
    }
    level[0]
}

/// Declared `message_count` disagrees with the leaf set that reproduces the root.
pub fn count_is_false(declared: u32, proven_leaves: usize) -> bool {
    declared != proven_leaves as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(id: &str, n: u8) -> MerkleLeaf {
        MerkleLeaf {
            msg_id: id.as_bytes().to_vec(),
            sender_id: b"s".to_vec(),
            recipient_id: b"r".to_vec(),
            ciphertext_hash: [n; 32],
            stored_at: n as u64,
        }
    }

    #[test]
    fn root_independent_of_input_order() {
        let a = leaf("b", 1);
        let b = leaf("a", 2);
        assert_eq!(merkle_root(&[a.clone(), b.clone()]), merkle_root(&[b, a]));
    }

    #[test]
    fn duplicate_ids_change_the_root_and_count() {
        let a = leaf("m", 1);
        assert_ne!(merkle_root(&[a.clone()]), merkle_root(&[a.clone(), a]));
        assert!(count_is_false(2, 1));
        assert!(!count_is_false(1, 1));
    }
}
