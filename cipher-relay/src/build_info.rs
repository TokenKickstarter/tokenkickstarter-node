//! Node / chain metadata exposed on relay HTTP (GET /version).

use std::sync::Arc;

#[derive(Clone, Debug, serde::Serialize)]
pub struct ChainBuildInfo {
    pub node_version: String,
    pub spec_version: u32,
    pub chain_name: String,
    pub genesis_hash: String,
}

pub type SharedChainBuildInfo = Arc<ChainBuildInfo>;
