//! Transaction pool sizing for burst EVM load (many nonce-sequenced txs per account).

use sc_cli::TransactionPoolParams;
use sc_service::Configuration;
use sc_transaction_pool::TransactionPoolOptions;

/// Substrate sets `future.count = pool_limit / 10`. For burst tests (e.g. 10 senders × 2k
/// nonces), the future queue must hold all in-flight txs; scale limits before building the pool.
pub fn apply_burst_pool_scaling(
    config: &mut Configuration,
    params: &TransactionPoolParams,
    future_divisor: usize,
) {
    if future_divisor <= 1 {
        return;
    }
    let base_limit = params.pool_limit;
    let base_kbytes = params.pool_kbytes;
    let scaled_limit = base_limit.saturating_mul(future_divisor);
    let scaled_kbytes = base_kbytes.saturating_mul(future_divisor);

    config.transaction_pool = TransactionPoolOptions::new_with_params(
        scaled_limit,
        scaled_kbytes * 1024,
        params.tx_ban_seconds,
        params.pool_type.into(),
        config.dev_key_seed.is_some(),
    );

    log::info!(
        target: "txpool",
        "Burst pool scaling: nominal pool_limit={} future_divisor={} → ready.limit={} future.limit≈{}",
        base_limit,
        future_divisor,
        scaled_limit,
        scaled_limit / future_divisor.max(1),
    );
}
