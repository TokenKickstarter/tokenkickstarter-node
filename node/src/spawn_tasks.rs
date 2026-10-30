//! Fork of `sc_service::builder::spawn_tasks` — strips P2P identity RPC after `gen_rpc_module`.

use std::sync::Arc;

use log::info;
use sc_client_api::{
    BlockBackend, BlockIdTo, BlockchainEvents, Chain, ExecutorProvider, HeaderBackend,
    ProofProvider, StorageProvider, UsageProvider,
};
use sc_rpc::DenyUnsafe;
use sc_service::config::PrometheusConfig;
use sc_service::error::Error;
use sc_service::{
    gen_rpc_module, init_telemetry, propagate_transaction_notifications, start_rpc_servers,
    GenRpcModuleParams, MetricsService, RpcHandlers, SpawnTasksParams,
};
use prometheus_endpoint;
use sc_transaction_pool_api::MaintainedTransactionPool;
use sp_api::{CallApiAt, Metadata, ProvideRuntimeApi};
use sp_blockchain::HeaderMetadata;
use sp_runtime::traits::Block as BlockT;

/// Like [`sc_service::spawn_tasks`], but removes `system_localPeerId` / listen addresses when requested.
pub fn spawn_tasks<TBl, TBackend, TExPool, TRpc, TCl>(
    SpawnTasksParams {
        mut config,
        task_manager,
        client,
        backend,
        keystore,
        transaction_pool,
        rpc_builder,
        network,
        system_rpc_tx,
        tx_handler_controller,
        sync_service,
        telemetry,
        tracing_execute_block: execute_block,
    }: SpawnTasksParams<TBl, TCl, TExPool, TRpc, TBackend>,
    hide_node_identity: bool,
) -> Result<RpcHandlers, Error>
where
    TCl: ProvideRuntimeApi<TBl>
        + HeaderMetadata<TBl, Error = sp_blockchain::Error>
        + Chain<TBl>
        + BlockBackend<TBl>
        + BlockIdTo<TBl, Error = sp_blockchain::Error>
        + ProofProvider<TBl>
        + HeaderBackend<TBl>
        + BlockchainEvents<TBl>
        + ExecutorProvider<TBl>
        + UsageProvider<TBl>
        + StorageProvider<TBl, TBackend>
        + CallApiAt<TBl>
        + Send
        + 'static,
    <TCl as ProvideRuntimeApi<TBl>>::Api: Metadata<TBl>
        + sp_transaction_pool::runtime_api::TaggedTransactionQueue<TBl>
        + sp_session::SessionKeys<TBl>
        + sp_api::ApiExt<TBl>,
    TBl: BlockT,
    TBl::Hash: Unpin,
    TBl::Header: Unpin,
    TBackend: 'static + sc_client_api::backend::Backend<TBl> + Send,
    TExPool: MaintainedTransactionPool<Block = TBl, Hash = <TBl as BlockT>::Hash> + 'static,
{
    use libp2p::multiaddr::{Multiaddr, Protocol};

    let chain_info = client.usage_info().chain;

    sp_session::generate_initial_session_keys(
        client.clone(),
        chain_info.best_hash,
        config
            .dev_key_seed
            .clone()
            .map(|s| vec![s])
            .unwrap_or_default(),
        keystore.clone(),
    )
    .map_err(|e| Error::Application(Box::new(e)))?;

    let sysinfo = sc_sysinfo::gather_sysinfo();
    sc_sysinfo::print_sysinfo(&sysinfo);

    let telemetry = telemetry
        .map(|telemetry| {
            init_telemetry(
                config.network.node_name.clone(),
                config.impl_name.clone(),
                config.impl_version.clone(),
                config.chain_spec.name().to_string(),
                config.role.is_authority(),
                network.clone(),
                client.clone(),
                telemetry,
                Some(sysinfo),
            )
        })
        .transpose()?;

    info!("📦 Highest known block at #{}", chain_info.best_number);

    let spawn_handle = task_manager.spawn_handle();

    spawn_handle.spawn(
        "txpool-notifications",
        Some("transaction-pool"),
        sc_transaction_pool::notification_future(client.clone(), transaction_pool.clone()),
    );

    spawn_handle.spawn(
        "on-transaction-imported",
        Some("transaction-pool"),
        propagate_transaction_notifications(
            transaction_pool.clone(),
            tx_handler_controller,
            telemetry.clone(),
        ),
    );

    let metrics_service = if let Some(PrometheusConfig { port, registry }) =
        config.prometheus_config.clone()
    {
        let metrics = MetricsService::with_prometheus(
            telemetry,
            &registry,
            config.role,
            &config.network.node_name,
            &config.impl_version,
        )?;
        spawn_handle.spawn(
            "prometheus-endpoint",
            None,
            prometheus_endpoint::init_prometheus(port, registry).map(drop),
        );
        metrics
    } else {
        MetricsService::new(telemetry)
    };

    spawn_handle.spawn(
        "telemetry-periodic-send",
        None,
        metrics_service.run(
            client.clone(),
            transaction_pool.clone(),
            network.clone(),
            sync_service.clone(),
        ),
    );

    let rpc_id_provider = config.rpc.id_provider.take();

    let rpc_v2_metrics = config
        .prometheus_registry()
        .map(|registry| sc_rpc_spec_v2::transaction::TransactionMetrics::new(registry))
        .transpose()?;

    let build_rpc_module = || {
        let mut module = gen_rpc_module(GenRpcModuleParams {
            spawn_handle: task_manager.spawn_handle(),
            client: client.clone(),
            transaction_pool: transaction_pool.clone(),
            keystore: keystore.clone(),
            system_rpc_tx: system_rpc_tx.clone(),
            impl_name: config.impl_name.clone(),
            impl_version: config.impl_version.clone(),
            chain_spec: config.chain_spec.as_ref(),
            state_pruning: &config.state_pruning,
            blocks_pruning: config.blocks_pruning,
            backend: backend.clone(),
            rpc_builder: &*rpc_builder,
            metrics: rpc_v2_metrics.clone(),
            tracing_execute_block: execute_block.clone(),
        })?;
        if hide_node_identity {
            crate::rpc_public::strip_sensitive_system_methods(&mut module);
        }
        Ok(module)
    };

    let rpc_server_handle = start_rpc_servers(
        &config.rpc,
        config.prometheus_registry(),
        &config.tokio_handle,
        build_rpc_module,
        rpc_id_provider,
    )?;

    let listen_addrs = rpc_server_handle
        .listen_addrs()
        .into_iter()
        .map(|socket_addr| {
            let mut multiaddr: Multiaddr = socket_addr.ip().into();
            multiaddr.push(Protocol::Tcp(socket_addr.port()));
            multiaddr
        })
        .collect();

    let in_memory_rpc = {
        let mut module = build_rpc_module()?;
        module.extensions_mut().insert(DenyUnsafe::No);
        module
    };

    let in_memory_rpc_handle = RpcHandlers::new(Arc::new(in_memory_rpc), listen_addrs);

    spawn_handle.spawn(
        "informant",
        None,
        sc_informant::build(client.clone(), network, sync_service.clone()),
    );

    task_manager.keep_alive((config.base_path, rpc_server_handle));

    Ok(in_memory_rpc_handle)
}
