//! # TKS RPC Extensions
//!
//! Expose system, transaction payment, and Ethereum-compatible RPC endpoints.

use std::sync::Arc;

use jsonrpsee::RpcModule;
use sc_client_api::backend::{Backend, StorageProvider};
use sc_client_api::{AuxStore, BlockchainEvents, UsageProvider};
use sc_network::service::traits::NetworkService;
use sc_network_sync::SyncingService;
use sc_transaction_pool_api::TransactionPool;
use sp_api::{CallApiAt, ProvideRuntimeApi};
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;

use fc_rpc::pending::AuraConsensusDataProvider;
use fc_rpc_core::types::{FeeHistoryCache, FilterPool};

use tks_runtime::{AccountId, Balance, Nonce};

/// Extra dependencies for Ethereum-compatible RPC.
pub struct EthDeps<B: BlockT, C, P, CT, CIDP> {
    /// The client instance.
    pub client: Arc<C>,
    /// Transaction pool instance.
    pub pool: Arc<P>,
    /// Ethereum transaction converter.
    pub converter: Option<CT>,
    /// The Node authority flag.
    pub is_authority: bool,
    /// Whether to enable dev signer.
    pub enable_dev_signer: bool,
    /// Network service.
    pub network: Arc<dyn NetworkService>,
    /// Chain syncing service.
    pub sync: Arc<SyncingService<B>>,
    /// Frontier Backend.
    pub frontier_backend: Arc<dyn fc_api::Backend<B>>,
    /// Ethereum data access overrides.
    pub storage_override: Arc<dyn fc_storage::StorageOverride<B>>,
    /// Cache for Ethereum block data.
    pub block_data_cache: Arc<fc_rpc::EthBlockDataCacheTask<B>>,
    /// EthFilterApi pool.
    pub filter_pool: Option<FilterPool>,
    /// Maximum number of logs in a query.
    pub max_past_logs: u32,
    /// Maximum block range for eth_getLogs.
    pub max_block_range: u32,
    /// Fee history cache.
    pub fee_history_cache: FeeHistoryCache,
    /// Maximum fee history cache size.
    pub fee_history_cache_limit: fc_rpc_core::types::FeeHistoryCacheLimit,
    /// Maximum allowed gas limit multiplier.
    pub execute_gas_limit_multiplier: u64,
    /// Mandated parent hashes for a given block hash.
    pub forced_parent_hashes: Option<std::collections::BTreeMap<H256, H256>>,
    /// Pending state inherent data providers.
    pub pending_create_inherent_data_providers: CIDP,
}

/// Full client dependencies for RPC.
pub struct FullDeps<B: BlockT, C, P, CT, CIDP> {
    /// The client instance.
    pub client: Arc<C>,
    /// Transaction pool instance.
    pub pool: Arc<P>,
    /// Ethereum-compatibility specific dependencies.
    pub eth: EthDeps<B, C, P, CT, CIDP>,
}

/// Default Ethereum config for TKS chain.
pub struct DefaultEthConfig<C, BE>(std::marker::PhantomData<(C, BE)>);

impl<B, C, BE> fc_rpc::EthConfig<B, C> for DefaultEthConfig<C, BE>
where
    B: BlockT,
    C: StorageProvider<B, BE> + Sync + Send + 'static,
    BE: Backend<B> + 'static,
{
    type EstimateGasAdapter = ();
    type RuntimeStorageOverride =
        fc_rpc::frontier_backend_client::SystemAccountId20StorageOverride<B, C, BE>;
}

/// Instantiate all full RPC extensions.
pub fn create_full<B, C, P, BE, CT, CIDP>(
    deps: FullDeps<B, C, P, CT, CIDP>,
    subscription_task_executor: sc_rpc::SubscriptionTaskExecutor,
    pubsub_notification_sinks: Arc<
        fc_mapping_sync::EthereumBlockNotificationSinks<
            fc_mapping_sync::EthereumBlockNotification<B>,
        >,
    >,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
    B: BlockT<Hash = H256>,
    C: CallApiAt<B> + ProvideRuntimeApi<B>,
    C::Api: sp_block_builder::BlockBuilder<B>,
    C::Api: sp_consensus_aura::AuraApi<B, AuraId>,
    C::Api: frame_system_rpc_runtime_api::AccountNonceApi<B, AccountId, Nonce>,
    C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<B, Balance>,
    C::Api: fp_rpc::ConvertTransactionRuntimeApi<B>,
    C::Api: fp_rpc::EthereumRuntimeRPCApi<B>,
    C: HeaderBackend<B> + HeaderMetadata<B, Error = BlockChainError> + 'static,
    C: BlockchainEvents<B> + AuxStore + UsageProvider<B> + StorageProvider<B, BE>,
    C: Send + Sync + 'static,
    BE: Backend<B> + 'static,
    P: TransactionPool<Block = B, Hash = B::Hash> + 'static,
    CT: fp_rpc::ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
    CIDP: sp_inherents::CreateInherentDataProviders<B, ()> + Send + 'static,
{
    use fc_rpc::{
        Eth, EthApiServer, EthDevSigner, EthFilter, EthFilterApiServer,
        EthPubSub, EthPubSubApiServer, EthSigner, Net, NetApiServer, Web3, Web3ApiServer,
    };
    use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
    use substrate_frame_rpc_system::{System, SystemApiServer};

    let mut io = RpcModule::new(());

    let FullDeps {
        client,
        pool,
        eth,
    } = deps;

    // System + TransactionPayment (Substrate standard)
    io.merge(System::new(client.clone(), pool.clone()).into_rpc())?;
    io.merge(TransactionPayment::new(client.clone()).into_rpc())?;

    // Ethereum RPCs
    let EthDeps {
        client,
        pool,
        converter,
        is_authority,
        enable_dev_signer,
        network,
        sync,
        frontier_backend,
        storage_override,
        block_data_cache,
        filter_pool,
        max_past_logs,
        max_block_range,
        fee_history_cache,
        fee_history_cache_limit,
        execute_gas_limit_multiplier,
        forced_parent_hashes,
        pending_create_inherent_data_providers,
    } = eth;

    let mut signers = Vec::new();
    if enable_dev_signer {
        signers.push(Box::new(EthDevSigner::new()) as Box<dyn EthSigner>);
    }

    io.merge(
        Eth::<_, _, _, _, _, _, DefaultEthConfig<_, _>>::new(
            client.clone(),
            pool.clone(),
            converter,
            sync.clone(),
            signers,
            storage_override.clone(),
            frontier_backend.clone(),
            is_authority,
            block_data_cache.clone(),
            fee_history_cache,
            fee_history_cache_limit,
            execute_gas_limit_multiplier,
            false, // allow_unprotected_txs
            forced_parent_hashes,
            pending_create_inherent_data_providers,
            Some(Box::new(AuraConsensusDataProvider::new(client.clone()))),
        )
        .replace_config::<DefaultEthConfig<_, _>>()
        .into_rpc(),
    )?;

    if let Some(filter_pool) = filter_pool {
        io.merge(
            EthFilter::new(
                client.clone(),
                frontier_backend.clone(),
                pool.clone(),
                filter_pool,
                500_usize,
                max_past_logs,
                max_block_range,
                block_data_cache.clone(),
            )
            .into_rpc(),
        )?;
    }

    io.merge(
        EthPubSub::new(
            pool,
            client.clone(),
            sync,
            subscription_task_executor,
            storage_override,
            pubsub_notification_sinks,
        )
        .into_rpc(),
    )?;

    io.merge(
        Net::new(client.clone(), network, true).into_rpc(),
    )?;

    io.merge(Web3::new(client).into_rpc())?;

    Ok(io)
}
