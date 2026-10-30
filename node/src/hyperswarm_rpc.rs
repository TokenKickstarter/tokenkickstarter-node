//! JSON-RPC: on-chain HyperSwarm validator discovery for Cipher clients.

use std::sync::Arc;

use codec::Decode;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sc_client_api::backend::Backend;
use sc_client_api::StorageProvider;
use sp_blockchain::HeaderBackend;
use sp_core::twox_128;
use sp_storage::StorageKey;
use sp_runtime::traits::{Block as BlockT, UniqueSaturatedInto};

use tks_runtime::{AccountId, Runtime};

type SwarmValidator = pallet_shard_registry::SwarmValidator<Runtime>;

#[derive(Clone, serde::Serialize)]
pub struct SwarmValidatorInfo {
    pub account_id: String,
    pub region: String,
    pub endpoint: String,
    pub active: bool,
    pub registered_at: u32,
}

#[derive(Clone, serde::Serialize)]
pub struct HyperswarmNetworkInfo {
    pub active_shard_count: u16,
    pub current_epoch: u32,
}

#[derive(Clone, serde::Serialize)]
pub struct AnchorStats {
    pub total_anchors: u64,
    pub total_messages: u64,
}

fn validators_key_prefix() -> Vec<u8> {
    let mut prefix = Vec::with_capacity(32);
    prefix.extend_from_slice(&twox_128(b"ShardRegistry"));
    prefix.extend_from_slice(&twox_128(b"Validators"));
    prefix
}

fn region_label(r: &pallet_shard_registry::GeoRegion) -> &'static str {
    use pallet_shard_registry::GeoRegion::*;
    match r {
        NorthAmerica => "north_america",
        Europe => "europe",
        Asia => "asia",
        SouthAmerica => "south_america",
        Africa => "africa",
        Oceania => "oceania",
    }
}

#[rpc(server)]
pub trait HyperswarmApi {
    #[method(name = "tks_listSwarmValidators")]
    fn list_swarm_validators(&self) -> RpcResult<Vec<SwarmValidatorInfo>>;

    #[method(name = "tks_hyperswarmNetworkInfo")]
    fn network_info(&self) -> RpcResult<HyperswarmNetworkInfo>;

    #[method(name = "tks_anchorStats")]
    fn anchor_stats(&self) -> RpcResult<AnchorStats>;
}

pub struct HyperswarmRpc<C, B, BE> {
    client: Arc<C>,
    _marker: std::marker::PhantomData<(B, BE)>,
}

impl<C, B, BE> HyperswarmRpc<C, B, BE> {
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<C, B, BE> HyperswarmApiServer for HyperswarmRpc<C, B, BE>
where
    B: BlockT,
    BE: Backend<B> + 'static,
    C: HeaderBackend<B> + StorageProvider<B, BE> + Send + Sync + 'static,
{
    fn list_swarm_validators(&self) -> RpcResult<Vec<SwarmValidatorInfo>> {
        let at = self.client.info().best_hash;
        let prefix = StorageKey(validators_key_prefix());
        let keys = self
            .client
            .storage_keys(at, Some(&prefix), None)
            .map_err(|e| jsonrpsee::types::ErrorObject::owned(-32000, e.to_string(), None::<()>))?;

        let mut out = Vec::new();
        for key in keys {
            let Some(raw) = self
                .client
                .storage(at, &key)
                .map_err(|e| jsonrpsee::types::ErrorObject::owned(-32000, e.to_string(), None::<()>))?
            else {
                continue;
            };
            let Ok(val) = SwarmValidator::decode(&mut &raw.0[..]) else {
                continue;
            };
            if !val.active {
                continue;
            }
            let endpoint = String::from_utf8_lossy(&val.endpoint).into_owned();
            let account_bytes = if key.0.len() > 48 {
                &key.0[48..]
            } else {
                continue;
            };
            let Ok(account_id) = AccountId::decode(&mut &account_bytes[..]) else {
                continue;
            };
            out.push(SwarmValidatorInfo {
                account_id: format!("{account_id:?}"),
                region: region_label(&val.region).into(),
                endpoint,
                active: val.active,
                registered_at: val.registered_at.unique_saturated_into(),
            });
        }
        Ok(out)
    }

    fn network_info(&self) -> RpcResult<HyperswarmNetworkInfo> {
        let at = self.client.info().best_hash;
        let shard_key = {
            let mut k = twox_128(b"ShardRegistry").to_vec();
            k.extend_from_slice(&twox_128(b"ActiveShardCount"));
            k
        };
        let epoch_key = {
            let mut k = twox_128(b"ShardRegistry").to_vec();
            k.extend_from_slice(&twox_128(b"CurrentEpoch"));
            k
        };

        let active_shard_count = self
            .client
            .storage(at, &StorageKey(shard_key))
            .ok()
            .flatten()
            .and_then(|v| u16::decode(&mut &v.0[..]).ok())
            .unwrap_or(1);

        let current_epoch = self
            .client
            .storage(at, &StorageKey(epoch_key))
            .ok()
            .flatten()
            .and_then(|v| u32::decode(&mut &v.0[..]).ok())
            .unwrap_or(0);

        Ok(HyperswarmNetworkInfo {
            active_shard_count,
            current_epoch,
        })
    }

    fn anchor_stats(&self) -> RpcResult<AnchorStats> {
        let at = self.client.info().best_hash;
        let total_anchors = read_storage_u64(
            &self.client,
            at,
            b"HyperSwarmAnchor",
            b"TotalAnchors",
        );
        let total_messages = read_storage_u64(
            &self.client,
            at,
            b"HyperSwarmAnchor",
            b"TotalMessageCount",
        );
        Ok(AnchorStats {
            total_anchors,
            total_messages,
        })
    }
}

fn read_storage_u64<C, B, BE>(
    client: &Arc<C>,
    at: B::Hash,
    pallet: &[u8],
    item: &[u8],
) -> u64
where
    B: BlockT,
    BE: Backend<B> + 'static,
    C: StorageProvider<B, BE> + Send + Sync + 'static,
{
    let mut key = twox_128(pallet).to_vec();
    key.extend_from_slice(&twox_128(item));
    client
        .storage(at, &StorageKey(key))
        .ok()
        .flatten()
        .and_then(|v| u64::decode(&mut &v.0[..]).ok())
        .unwrap_or(0)
}
