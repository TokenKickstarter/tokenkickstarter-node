//! Keep embedded shard executor epoch aligned with on-chain `ShardRegistry::CurrentEpoch`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use codec::Decode;
use futures::FutureExt;
use sc_client_api::SpawnNamed;
use sc_client_api::StorageProvider;
use sp_core::twox_128;
use sp_storage::StorageKey;

use crate::service::FullClient;

type Block = tks_runtime::Block;

pub fn new_epoch_handle(initial: u32) -> Arc<AtomicU32> {
    Arc::new(AtomicU32::new(initial))
}

pub fn spawn_epoch_sync(
    spawn: impl SpawnNamed + Send + Sync + 'static,
    client: Arc<FullClient>,
    epoch: Arc<AtomicU32>,
) {
    spawn.spawn(
        "cipher-epoch-sync",
        Some("network"),
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let at = client.info().best_hash;
                if let Some(v) = read_current_epoch(&client, at) {
                    epoch.store(v, Ordering::Relaxed);
                }
            }
        }
        .boxed(),
    );
}

fn shard_registry_epoch_key() -> StorageKey {
    let mut k = twox_128(b"ShardRegistry").to_vec();
    k.extend_from_slice(&twox_128(b"CurrentEpoch"));
    StorageKey(k)
}

fn read_current_epoch(
    client: &FullClient,
    at: <Block as sp_runtime::traits::Block>::Hash,
) -> Option<u32> {
    let key = shard_registry_epoch_key();
    let raw = client.storage(at, &key).ok()??;
    u32::decode(&mut &raw.0[..]).ok()
}
