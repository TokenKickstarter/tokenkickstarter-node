//! # TKS Node Service
//!
//! Constructs and starts the full TKS blockchain node with Frontier EVM backend.

use std::sync::Arc;
use std::time::Duration;
use std::collections::BTreeMap;

use futures::{FutureExt, StreamExt};
use sc_client_api::{Backend, BlockBackend, BlockchainEvents};
use sc_consensus_aura::{ImportQueueParams, SlotProportion, StartAuraParams};
use sc_consensus_grandpa::SharedVoterState;
use sc_service::{error::Error as ServiceError, Configuration, TaskManager, WarpSyncConfig};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use sp_runtime::traits::Block as BlockT;

// Frontier imports
use fc_mapping_sync::{kv::MappingSyncWorker, SyncStrategy};
use fc_rpc_core::types::{FeeHistoryCache, FilterPool};

/// Alias for the network backend type.
type NetworkBackend = sc_network::NetworkWorker<
    tks_runtime::Block,
    <tks_runtime::Block as BlockT>::Hash,
>;

pub type HostFunctions = (
    sp_io::SubstrateHostFunctions,
    cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

pub(crate) type FullClient = sc_service::TFullClient<
    tks_runtime::Block,
    tks_runtime::RuntimeApi,
    sc_executor::WasmExecutor<HostFunctions>,
>;

type FullBackend = sc_service::TFullBackend<tks_runtime::Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, tks_runtime::Block>;
type FullGrandpaBlockImport = sc_consensus_grandpa::GrandpaBlockImport<
    FullBackend,
    tks_runtime::Block,
    FullClient,
    FullSelectChain,
>;
type TransactionPool = sc_transaction_pool::TransactionPoolHandle<tks_runtime::Block, FullClient>;

/// Frontier KV backend type (2 generic args for this version).
pub type FrontierBackend = fc_db::kv::Backend<tks_runtime::Block, FullClient>;

/// Get the Frontier database directory.
pub fn frontier_database_dir(config: &Configuration) -> std::path::PathBuf {
    config.base_path.config_dir(config.chain_spec.id()).join("frontier").join("db")
}

/// Open the Frontier KV backend.
pub fn open_frontier_backend(
    client: Arc<FullClient>,
    config: &Configuration,
) -> Result<Arc<FrontierBackend>, String> {
    let db_config_dir = config.base_path.config_dir(config.chain_spec.id());
    let database = match &config.database {
        sc_service::DatabaseSource::RocksDb { .. } => fc_db::kv::DatabaseSource::RocksDb {
            path: "".into(),
            cache_size: 0,
        },
        sc_service::DatabaseSource::ParityDb { .. } => fc_db::kv::DatabaseSource::ParityDb {
            path: "".into(),
        },
        _ => fc_db::kv::DatabaseSource::Auto {
            rocksdb_path: "".into(),
            paritydb_path: "".into(),
            cache_size: 0,
        },
    };

    Ok(Arc::new(fc_db::kv::Backend::open(
        client,
        &database,
        &db_config_dir,
    )?))
}



/// Build the partial components required for the node.
pub fn new_partial(
    config: &Configuration,
) -> Result<
    sc_service::PartialComponents<
        FullClient,
        FullBackend,
        FullSelectChain,
        sc_consensus::DefaultImportQueue<tks_runtime::Block>,
        TransactionPool,
        (
            FullGrandpaBlockImport,
            sc_consensus_grandpa::LinkHalf<tks_runtime::Block, FullClient, FullSelectChain>,
            Option<Telemetry>,
            Arc<FrontierBackend>,
        ),
    >,
    ServiceError,
> {
    let telemetry = config
        .telemetry_endpoints
        .clone()
        .filter(|x| !x.is_empty())
        .map(|endpoints| -> Result<_, sc_telemetry::Error> {
            let worker = TelemetryWorker::new(16)?;
            let telemetry = worker.handle().new_telemetry(endpoints);
            Ok((worker, telemetry))
        })
        .transpose()?;

    let executor = sc_service::new_wasm_executor::<HostFunctions>(&config.executor);

    let (client, backend, keystore_container, task_manager) =
        sc_service::new_full_parts::<tks_runtime::Block, tks_runtime::RuntimeApi, _>(
            config,
            telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
            executor,
        )?;
    let client = Arc::new(client);

    let telemetry = telemetry.map(|(worker, telemetry)| {
        task_manager
            .spawn_handle()
            .spawn("telemetry", None, worker.run());
        telemetry
    });

    let select_chain = sc_consensus::LongestChain::new(backend.clone());

    let transaction_pool = sc_transaction_pool::Builder::new(
        task_manager.spawn_essential_handle(),
        client.clone(),
        config.role.is_authority().into(),
    )
    .with_options(config.transaction_pool.clone())
    .with_prometheus(config.prometheus_registry())
    .build();

    let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
        client.clone(),
        512,
        &client,
        select_chain.clone(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let slot_duration = sc_consensus_aura::slot_duration(&*client)?;

    let import_queue =
        sc_consensus_aura::import_queue::<AuraPair, _, _, _, _, _>(ImportQueueParams {
            block_import: grandpa_block_import.clone(),
            justification_import: Some(Box::new(grandpa_block_import.clone())),
            client: client.clone(),
            create_inherent_data_providers: move |_, ()| async move {
                let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
                let slot =
                    sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
                        *timestamp,
                        slot_duration,
                    );
                Ok((slot, timestamp))
            },
            spawner: &task_manager.spawn_essential_handle(),
            registry: config.prometheus_registry(),
            check_for_equivocation: Default::default(),
            telemetry: telemetry.as_ref().map(|x| x.handle()),
            compatibility_mode: Default::default(),
        })?;

    // Open Frontier KV backend
    let frontier_backend = open_frontier_backend(client.clone(), config)?;

    Ok(sc_service::PartialComponents {
        client,
        backend,
        task_manager,
        import_queue,
        keystore_container,
        select_chain,
        transaction_pool: Arc::new(transaction_pool),
        other: (grandpa_block_import, grandpa_link, telemetry, frontier_backend),
    })
}

/// Build and start the full TKS node.
pub fn new_full(
    config: Configuration,
    enable_cipher_relay: bool,
    cipher_relay_port: u16,
) -> Result<TaskManager, ServiceError> {
    let sc_service::PartialComponents {
        client,
        backend,
        mut task_manager,
        import_queue,
        keystore_container,
        select_chain,
        transaction_pool,
        other: (block_import, grandpa_link, mut telemetry, frontier_backend),
    } = new_partial(&config)?;

    let genesis_hash = client
        .block_hash(0u32.into())
        .ok()
        .flatten()
        .expect("Genesis block exists; qed");

    let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
        &genesis_hash,
        &config.chain_spec,
    );

    let metrics =
        sc_network::NotificationMetrics::new(config.prometheus_registry());

    let mut net_config = sc_network::config::FullNetworkConfiguration::<
        tks_runtime::Block,
        <tks_runtime::Block as BlockT>::Hash,
        NetworkBackend,
    >::new(&config.network, config.prometheus_registry().cloned());

    let peer_store_handle = net_config.peer_store_handle();

    let (grandpa_protocol_config, grandpa_notification_service) =
        sc_consensus_grandpa::grandpa_peers_set_config::<_, NetworkBackend>(
            grandpa_protocol_name.clone(),
            metrics.clone(),
            peer_store_handle,
        );

    net_config.add_notification_protocol(grandpa_protocol_config);

    let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        grandpa_link.shared_authority_set().clone(),
        Vec::default(),
    ));

    let (network, system_rpc_tx, tx_handler_controller, sync_service) =
        sc_service::build_network(sc_service::BuildNetworkParams {
            config: &config,
            net_config,
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            import_queue,
            block_announce_validator_builder: None,
            warp_sync_config: Some(WarpSyncConfig::WithProvider(warp_sync)),
            block_relay: None,
            metrics,
        })?;

    if config.offchain_worker.enabled {
        let offchain_workers = sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
            runtime_api_provider: client.clone(),
            is_validator: config.role.is_authority(),
            keystore: Some(keystore_container.keystore()),
            offchain_db: backend.offchain_storage(),
            transaction_pool: Some(OffchainTransactionPoolFactory::new(
                transaction_pool.clone(),
            )),
            network_provider: Arc::new(network.clone()),
            enable_http_requests: true,
            custom_extensions: |_| vec![],
        })?;

        task_manager.spawn_handle().spawn(
            "offchain-workers-runner",
            "offchain-worker",
            offchain_workers
                .run(client.clone(), task_manager.spawn_handle())
                .boxed(),
        );
    }

    let role = config.role;
    let force_authoring = config.force_authoring;
    let backoff_authoring_blocks: Option<()> = None;
    let name = config.network.node_name.clone();
    let enable_grandpa = !config.disable_grandpa;
    let prometheus_registry = config.prometheus_registry().cloned();

    // Frontier: Ethereum block notification sinks for pubsub
    let pubsub_notification_sinks: fc_mapping_sync::EthereumBlockNotificationSinks<
        fc_mapping_sync::EthereumBlockNotification<tks_runtime::Block>,
    > = Default::default();
    let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

    // Frontier: Filter pool for eth_newFilter / eth_getFilterChanges
    let filter_pool: Option<FilterPool> = Some(Arc::new(std::sync::Mutex::new(
        BTreeMap::new(),
    )));

    // Frontier: Fee history cache
    let fee_history_cache: FeeHistoryCache = Arc::new(std::sync::Mutex::new(BTreeMap::new()));

    // Frontier: storage override for mapping sync and RPC
    let storage_override: Arc<dyn fc_storage::StorageOverride<tks_runtime::Block>> =
        Arc::new(fc_rpc::StorageOverrideHandler::new(client.clone()));

    // Frontier: Spawn mapping sync worker
    {
        let client_for_sync = client.clone();
        let backend_for_sync = backend.clone();
        let storage_override_for_sync = storage_override.clone();
        let frontier_backend_for_sync = frontier_backend.clone();
        let sync_service_for_sync = sync_service.clone();
        let pubsub_for_sync = pubsub_notification_sinks.clone();

        task_manager.spawn_essential_handle().spawn(
            "frontier-mapping-sync-worker",
            Some("frontier"),
            MappingSyncWorker::new(
                client_for_sync.import_notification_stream(),
                Duration::new(6, 0),
                client_for_sync.clone(),
                backend_for_sync,
                storage_override_for_sync,
                frontier_backend_for_sync,
                100,    // sync batch size: index up to 100 blocks per tick for fast backfill
                0u32.into(),
                SyncStrategy::Normal,
                sync_service_for_sync,
                pubsub_for_sync,
            )
            .for_each(|()| futures::future::ready(())),
        );
    }

    // RPC
    let rpc_extensions_builder = {
        let client = client.clone();
        let pool = transaction_pool.clone();
        let is_authority = role.is_authority();
        let enable_dev_signer = true; // dev mode
        let network = network.clone();
        let sync = sync_service.clone();
        let frontier_backend = frontier_backend.clone();
        let filter_pool = filter_pool.clone();
        let fee_history_cache = fee_history_cache.clone();
        let pubsub_notification_sinks = pubsub_notification_sinks.clone();
        let storage_override = storage_override.clone();

        let block_data_cache = Arc::new(fc_rpc::EthBlockDataCacheTask::new(
            task_manager.spawn_handle(),
            storage_override.clone(),
            50,
            50,
            prometheus_registry.clone(),
        ));

        Box::new(move |subscription_task_executor| {
            let eth = crate::rpc::EthDeps {
                client: client.clone(),
                pool: pool.clone(),
                converter: Some(tks_runtime::TransactionConverter),
                is_authority,
                enable_dev_signer,
                network: network.clone(),
                sync: sync.clone(),
                frontier_backend: frontier_backend.clone() as Arc<dyn fc_api::Backend<tks_runtime::Block>>,
                storage_override: storage_override.clone(),
                block_data_cache: block_data_cache.clone(),
                filter_pool: filter_pool.clone(),
                max_past_logs: 10_000,
                max_block_range: 10_000,
                fee_history_cache: fee_history_cache.clone(),
                fee_history_cache_limit: 2048,
                execute_gas_limit_multiplier: 10,
                forced_parent_hashes: None,
                pending_create_inherent_data_providers: move |_, ()| async move {
                    let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
                    Ok(timestamp)
                },
            };

            let deps = crate::rpc::FullDeps {
                client: client.clone(),
                pool: pool.clone(),
                eth,
            };

            crate::rpc::create_full(
                deps,
                subscription_task_executor,
                pubsub_notification_sinks.clone(),
            )
            .map_err(Into::into)
        })
    };

    let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        network: network.clone(),
        client: client.clone(),
        keystore: keystore_container.keystore(),
        task_manager: &mut task_manager,
        transaction_pool: transaction_pool.clone(),
        rpc_builder: rpc_extensions_builder,
        backend,
        system_rpc_tx,
        tx_handler_controller,
        sync_service: sync_service.clone(),
        config,
        telemetry: telemetry.as_mut(),
        tracing_execute_block: None,
    })?;

    // Start Aura block authoring (if authority)
    if role.is_authority() {
        let proposer_factory = sc_basic_authorship::ProposerFactory::new(
            task_manager.spawn_handle(),
            client.clone(),
            transaction_pool.clone(),
            prometheus_registry.as_ref(),
            telemetry.as_ref().map(|x| x.handle()),
        );

        let slot_duration = sc_consensus_aura::slot_duration(&*client)?;

        let aura = sc_consensus_aura::start_aura::<AuraPair, _, _, _, _, _, _, _, _, _, _>(
            StartAuraParams {
                slot_duration,
                client: client.clone(),
                select_chain,
                block_import,
                proposer_factory,
                create_inherent_data_providers: move |_, ()| async move {
                    let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
                    let slot =
                        sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
                            *timestamp,
                            slot_duration,
                        );
                    Ok((slot, timestamp))
                },
                force_authoring,
                backoff_authoring_blocks,
                keystore: keystore_container.keystore(),
                sync_oracle: sync_service.clone(),
                justification_sync_link: sync_service.clone(),
                block_proposal_slot_portion: SlotProportion::new(2f32 / 3f32),
                max_block_proposal_slot_portion: None,
                telemetry: telemetry.as_ref().map(|x| x.handle()),
                compatibility_mode: Default::default(),
            },
        )?;

        task_manager
            .spawn_essential_handle()
            .spawn_blocking("aura", Some("block-authoring"), aura);
    }

    // Start GRANDPA finality (if not disabled)
    if enable_grandpa {
        let grandpa_config = sc_consensus_grandpa::Config {
            gossip_duration: std::time::Duration::from_millis(333),
            justification_generation_period: 512,
            name: Some(name),
            observer_enabled: false,
            keystore: if role.is_authority() {
                Some(keystore_container.keystore())
            } else {
                None
            },
            local_role: role,
            telemetry: telemetry.as_ref().map(|x| x.handle()),
            protocol_name: grandpa_protocol_name,
        };

        let grandpa = sc_consensus_grandpa::run_grandpa_voter(
            sc_consensus_grandpa::GrandpaParams {
                config: grandpa_config,
                link: grandpa_link,
                network,
                sync: sync_service,
                notification_service: grandpa_notification_service,
                voting_rule: sc_consensus_grandpa::VotingRulesBuilder::default().build(),
                prometheus_registry,
                shared_voter_state: SharedVoterState::empty(),
                telemetry: telemetry.as_ref().map(|x| x.handle()),
                offchain_tx_pool_factory: OffchainTransactionPoolFactory::new(transaction_pool),
            },
        )?;

        task_manager
            .spawn_essential_handle()
            .spawn_blocking("grandpa-voter", None, grandpa);
    }

    // Start Cipher Relay Messenger task (if enabled)
    if enable_cipher_relay {
        task_manager.spawn_essential_handle().spawn("cipher-relay", Some("network"), async move {
            log::info!("🚀 Starting Embedded Cipher Relay on port {}...", cipher_relay_port);
            if let Err(e) = cipher_relay::run_relay_server(cipher_relay_port).await {
                log::error!("Cipher Relay exited with error: {:?}", e);
            }
        });
    }

    Ok(task_manager)
}
