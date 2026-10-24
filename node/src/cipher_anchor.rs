//! Watch `{base-path}/cipher/anchors/` and submit `HyperSwarmAnchor::post_anchor` extrinsics.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use codec::Encode;
use cipher_shard_executor::anchor::AnchorSubmitPayload;
use fp_account::AccountId20;
use frame_system_rpc_runtime_api::AccountNonceApi;
use futures::FutureExt;
use pallet_hyperswarm_anchor::Call as HyperSwarmAnchorCall;
use sc_client_api::BlockBackend;
use sc_transaction_pool_api::{TransactionPool, TransactionSource};
use sp_blockchain::HeaderBackend;
use sp_api::ProvideRuntimeApi;
use sp_core::traits::SpawnNamed;
use sp_core::{ecdsa, Pair};
use sp_runtime::{
    generic::{Era, SignedPayload},
    traits::Block as BlockT,
};
use tks_runtime::{RuntimeCall, Signature, SignedExtra, UncheckedExtrinsic};

use crate::service::{FullClient, FullTransactionPool as Pool};

type Block = tks_runtime::Block;

/// Submit signed extrinsics for new anchor JSON files (idempotent via `.posted` marker).
pub fn spawn_anchor_submitter(
    spawn: impl SpawnNamed + Send + Sync + 'static,
    client: Arc<FullClient>,
    pool: Arc<Pool>,
    anchors_dir: PathBuf,
    signer: ecdsa::Pair,
) {
    let account = AccountId20::from(signer.public());
    log::info!(
        target: "cipher-anchor",
        "Auto post_anchor enabled for account {account:?}, watching {}",
        anchors_dir.display()
    );
    spawn.spawn(
        "cipher-anchor-submitter",
        Some("network"),
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                interval.tick().await;
                if let Err(e) =
                    process_anchor_dir(&client, &pool, &anchors_dir, &signer, account).await
                {
                    log::warn!(target: "cipher-anchor", "anchor submit pass failed: {e}");
                }
            }
        }
        .boxed(),
    );
}

async fn process_anchor_dir(
    client: &Arc<FullClient>,
    pool: &Arc<Pool>,
    dir: &Path,
    signer: &ecdsa::Pair,
    account: AccountId20,
) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    let posted_dir = dir.join(".posted");
    std::fs::create_dir_all(&posted_dir).map_err(|e| e.to_string())?;

    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.starts_with("anchor_") || !name.ends_with(".json") {
            continue;
        }
        let marker = posted_dir.join(name);
        if marker.exists() {
            continue;
        }

        let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let payload: AnchorSubmitPayload = match serde_json::from_str(&raw) {
            Ok(p) => p,
            Err(e) => {
                return Err(format!("parse {}: {e}", path.display()));
            }
        };

        let dag_root = parse_hash_hex(&payload.dag_root_hash)?;
        match submit_post_anchor(
            client,
            pool,
            signer,
            account,
            payload.shard_id,
            dag_root,
            payload.message_count,
            payload.epoch,
        )
        .await
        {
            Ok(hash) => {
                log::info!(
                    target: "cipher-anchor",
                    "Submitted post_anchor for {} (tx {:?})",
                    path.display(),
                    hash
                );
                std::fs::write(&marker, hash.encode()).map_err(|e| e.to_string())?;
            }
            Err(e) => {
                log::warn!(
                    target: "cipher-anchor",
                    "post_anchor failed for {}: {e}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

fn parse_hash_hex(s: &str) -> Result<[u8; 32], String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|e| e.to_string())?;
    bytes
        .try_into()
        .map_err(|_| "dag_root_hash must be 32 bytes".to_string())
}

async fn submit_post_anchor(
    client: &Arc<FullClient>,
    pool: &Arc<Pool>,
    signer: &ecdsa::Pair,
    account: AccountId20,
    shard_id: u16,
    dag_root_hash: [u8; 32],
    message_count: u32,
    epoch: u32,
) -> Result<<Block as BlockT>::Hash, String> {
    let call = RuntimeCall::HyperSwarmAnchor(HyperSwarmAnchorCall::post_anchor {
        shard_id,
        dag_root_hash,
        message_count,
        epoch,
    });

    let at = client.info().best_hash;
    let genesis_hash = client
        .block_hash(0u32.into())
        .map_err(|e| e.to_string())?
        .ok_or("genesis hash missing")?;
    let nonce = client
        .runtime_api()
        .account_nonce(at, account)
        .map_err(|e| e.to_string())?;

    let extra: SignedExtra = (
        frame_system::CheckNonZeroSender::<tks_runtime::Runtime>::new(),
        frame_system::CheckSpecVersion::<tks_runtime::Runtime>::new(),
        frame_system::CheckTxVersion::<tks_runtime::Runtime>::new(),
        frame_system::CheckGenesis::<tks_runtime::Runtime>::new(),
        frame_system::CheckEra::<tks_runtime::Runtime>::from(Era::Immortal),
        frame_system::CheckNonce::<tks_runtime::Runtime>::from(nonce),
        frame_system::CheckWeight::<tks_runtime::Runtime>::new(),
        pallet_transaction_payment::ChargeTransactionPayment::<tks_runtime::Runtime>::from(0),
    );

    // `SignedPayload::new` reads implicit extensions via host functions and must not run
    // on the async executor; provide explicit implicit data instead.
    let raw_payload = SignedPayload::from_raw(
        call.clone(),
        extra.clone(),
        (
            (),
            tks_runtime::VERSION.spec_version,
            tks_runtime::VERSION.transaction_version,
            genesis_hash,
            genesis_hash,
            (),
            (),
            (),
        ),
    );

    let ecdsa_sig = raw_payload.using_encoded(|payload| signer.sign(payload));
    let signature = Signature::new(ecdsa_sig);

    let ext = UncheckedExtrinsic::new_signed(call, account, signature, extra);

    let hash = pool
        .submit_one(at, TransactionSource::Local, ext)
        .await
        .map_err(|e| e.to_string())?;
    Ok(hash)
}
