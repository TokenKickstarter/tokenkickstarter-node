//! L1 anchor submission payload (sign/submit via local node or Polkadot.js).

use serde::{Deserialize, Serialize};

/// JSON artifact for operators to submit `HyperSwarmAnchor::post_anchor`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnchorSubmitPayload {
    pub pallet: String,
    pub call: String,
    pub shard_id: u16,
    pub dag_root_hash: String,
    pub message_count: u32,
    pub epoch: u32,
    pub da_path: String,
    pub note: String,
}

impl AnchorSubmitPayload {
    pub fn new(
        shard_id: u16,
        dag_root_hash: [u8; 32],
        message_count: u32,
        epoch: u32,
        da_path: String,
    ) -> Self {
        Self {
            pallet: "HyperSwarmAnchor".into(),
            call: "post_anchor".into(),
            shard_id,
            dag_root_hash: format!("0x{}", hex::encode(dag_root_hash)),
            message_count,
            epoch,
            da_path,
            note: "Caller must be in ShardValidators for shard_id (pallet-shard-registry). Submit via author_submitExtrinsic on a validator or Polkadot.js.".into(),
        }
    }
}
