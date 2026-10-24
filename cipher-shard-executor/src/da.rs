//! Data availability blob persistence.

use std::path::{Path, PathBuf};

use crate::batch::DataAvailabilityBlob;

/// Writes `blob` under `{base}/shard_{id}/epoch_{epoch}/{root_hex}.json`.
pub fn write_blob(
    base: &Path,
    root: &[u8; 32],
    blob: &DataAvailabilityBlob,
) -> Result<PathBuf, String> {
    let dir = base
        .join(format!("shard_{}", blob.shard_id))
        .join(format!("epoch_{}", blob.epoch));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("0x{}.json", hex::encode(root)));
    let data = serde_json::to_vec_pretty(blob).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(path)
}
