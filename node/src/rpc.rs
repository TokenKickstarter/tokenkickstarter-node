//! # TKS RPC Extensions
//!
//! Expose system + transaction payment RPC endpoints.

use std::sync::Arc;

use jsonrpsee::RpcModule;
use sc_transaction_pool_api::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_runtime::traits::Block as BlockT;
use tks_runtime::{AccountId, Balance, Nonce};

/// Full client dependencies for RPC.
pub struct FullDeps<C, P> {
    /// The client instance.
    pub client: Arc<C>,
    /// Transaction pool instance.
    pub pool: Arc<P>,
}

/// Instantiate all full RPC extensions.
pub fn create_full<C, P, Block>(
    deps: FullDeps<C, P>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
    Block: BlockT,
    C: ProvideRuntimeApi<Block>,
    C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
    C: Send + Sync + 'static,
    C::Api: frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce>,
    C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
    C::Api: sp_block_builder::BlockBuilder<Block>,
    P: TransactionPool + 'static,
{
    use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
    use substrate_frame_rpc_system::{System, SystemApiServer};

    let mut module = RpcModule::new(());
    let FullDeps { client, pool } = deps;

    module.merge(System::new(client.clone(), pool).into_rpc())?;
    module.merge(TransactionPayment::new(client).into_rpc())?;

    Ok(module)
}
