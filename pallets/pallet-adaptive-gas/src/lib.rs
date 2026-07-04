//! # TKS Adaptive Gas Pallet
//!
//! Automatically adjusts the **block gas limit** (and therefore effective TPS)
//! every block based on actual network load, using EIP-1559-style math.
//!
//! ## Algorithm
//!
//! Every block:
//! 1. Read how full the PREVIOUS block was (`last_gas_used / current_limit`)
//! 2. Compare to the target fill ratio (default 50%)
//! 3. Adjust the gas limit by up to ±12.5% toward the target
//! 4. Clamp between `MinGasLimit` and `MaxGasLimit`
//!
//! ```
//! if last_used > target:
//!     new_limit = current_limit × (1 + Δ/8)   ← scale UP, max +12.5%/block
//! else:
//!     new_limit = current_limit × (1 - Δ/8)   ← scale DOWN, max -12.5%/block
//! ```
//!
//! ## TPS Range (at 1-second block time)
//!
//! | Gas Limit | Avg tx (50k gas) | Effective TPS |
//! |-----------|-----------------|---------------|
//! | 75M (min) | 50,000          | ~1,500        |
//! | 500M (start) | 50,000       | ~10,000       |
//! | 4B (max)  | 50,000          | ~80,000       |
//!
//! ## Scaling Speed
//!
//! - Idle → full load: ~8 blocks (~8 seconds) to double the gas limit
//! - Full → idle: same rate downward
//! - Sustained load at 50%: limit stays stable
//!
//! ## Integration
//!
//! The runtime reads `CurrentGasLimit` from storage via `AdaptiveGasLimitGet`
//! which implements `Get<U256>` for `pallet_evm::Config::BlockGasLimit`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

/// The gas limit adapter that reads dynamically from pallet storage.
/// Plug this into `pallet_evm::Config::BlockGasLimit`.
pub struct AdaptiveGasLimitGet<T>(core::marker::PhantomData<T>);

impl<T: pallet::Config> frame_support::traits::Get<sp_core::U256> for AdaptiveGasLimitGet<T> {
    fn get() -> sp_core::U256 {
        sp_core::U256::from(pallet::CurrentGasLimit::<T>::get())
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::Perbill;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // ─── Config ────────────────────────────────────────────────────────

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Minimum gas limit (floor). ~1,500 TPS at 50k gas/tx.
        #[pallet::constant]
        type MinGasLimit: Get<u64>;

        /// Maximum gas limit (ceiling). ~80,000 TPS at 50k gas/tx.
        #[pallet::constant]
        type MaxGasLimit: Get<u64>;

        /// Initial gas limit on genesis. ~10,000 TPS at 50k gas/tx.
        #[pallet::constant]
        type InitialGasLimit: Get<u64>;

        /// Target block fullness (50% = blocks should be half full for stable limit).
        /// Below this → limit shrinks. Above this → limit grows.
        #[pallet::constant]
        type TargetFillPercent: Get<u32>;

        /// Maximum adjustment per block as a fraction of current limit (numerator/1000).
        /// 125 = 12.5%, matching EIP-1559's elasticity.
        #[pallet::constant]
        type MaxAdjustmentPermill: Get<u32>;
    }

    // ─── Storage ───────────────────────────────────────────────────────

    /// Current gas limit — changes every block based on network load.
    /// This is the single source of truth for how many transactions fit per block.
    #[pallet::storage]
    #[pallet::getter(fn current_gas_limit)]
    pub type CurrentGasLimit<T: Config> = StorageValue<
        _,
        u64,
        ValueQuery,
        T::InitialGasLimit,
    >;

    /// Gas consumed by the LAST finalized block (set in on_finalize).
    #[pallet::storage]
    #[pallet::getter(fn last_block_gas_used)]
    pub type LastBlockGasUsed<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// All-time high gas limit ever reached.
    #[pallet::storage]
    #[pallet::getter(fn peak_gas_limit)]
    pub type PeakGasLimit<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// Number of consecutive blocks above target (for analytics).
    #[pallet::storage]
    pub type ConsecutiveFullBlocks<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// Number of consecutive blocks below target (for analytics).
    #[pallet::storage]
    pub type ConsecutiveEmptyBlocks<T: Config> = StorageValue<_, u32, ValueQuery>;

    // ─── Events ────────────────────────────────────────────────────────

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Gas limit was automatically adjusted.
        GasLimitAdjusted {
            /// Previous gas limit.
            old_limit: u64,
            /// New gas limit for this block.
            new_limit: u64,
            /// Gas used in the previous block.
            last_used: u64,
            /// Target gas usage (fill% × old_limit).
            target: u64,
            /// Direction: true = scaled up, false = scaled down.
            scaled_up: bool,
        },
        /// Gas limit hit the configured ceiling.
        ReachedMaxLimit { limit: u64 },
        /// Gas limit hit the configured floor.
        ReachedMinLimit { limit: u64 },
    }

    // ─── Hooks ─────────────────────────────────────────────────────────

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Called at the START of every block.
        /// Reads last block's gas usage and adjusts the limit for this block.
        fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
            let current_limit = CurrentGasLimit::<T>::get();
            let last_used = LastBlockGasUsed::<T>::get();

            // target = fill_percent% of current limit
            let fill_percent = T::TargetFillPercent::get() as u64;
            let target = current_limit.saturating_mul(fill_percent) / 100;

            // How far off from target?
            let max_adj_permill = T::MaxAdjustmentPermill::get() as u64;

            let (new_limit, scaled_up) = if last_used > target {
                // Block was OVER target → scale up
                let excess = last_used.saturating_sub(target);
                // adjustment = excess/target × current_limit × max_adj
                // Capped at MaxAdjustmentPermill/1000 of current_limit
                let max_increase = current_limit.saturating_mul(max_adj_permill) / 1000;
                let proportional = if target > 0 {
                    excess.saturating_mul(current_limit) / target / 8
                } else {
                    max_increase
                };
                let increase = proportional.min(max_increase);
                let nl = current_limit
                    .saturating_add(increase)
                    .min(T::MaxGasLimit::get());

                ConsecutiveFullBlocks::<T>::mutate(|c| *c = c.saturating_add(1));
                ConsecutiveEmptyBlocks::<T>::put(0);

                if nl == T::MaxGasLimit::get() {
                    Self::deposit_event(Event::ReachedMaxLimit { limit: nl });
                }
                (nl, true)
            } else {
                // Block was UNDER target → scale down
                let deficit = target.saturating_sub(last_used);
                let max_decrease = current_limit.saturating_mul(max_adj_permill) / 1000;
                let proportional = if target > 0 {
                    deficit.saturating_mul(current_limit) / target / 8
                } else {
                    max_decrease
                };
                let decrease = proportional.min(max_decrease);
                let nl = current_limit
                    .saturating_sub(decrease)
                    .max(T::MinGasLimit::get());

                ConsecutiveEmptyBlocks::<T>::mutate(|c| *c = c.saturating_add(1));
                ConsecutiveFullBlocks::<T>::put(0);

                if nl == T::MinGasLimit::get() {
                    Self::deposit_event(Event::ReachedMinLimit { limit: nl });
                }
                (nl, false)
            };

            if new_limit != current_limit {
                CurrentGasLimit::<T>::put(new_limit);

                // Update peak
                if new_limit > PeakGasLimit::<T>::get() {
                    PeakGasLimit::<T>::put(new_limit);
                }

                Self::deposit_event(Event::GasLimitAdjusted {
                    old_limit: current_limit,
                    new_limit,
                    last_used,
                    target,
                    scaled_up,
                });

                log::debug!(
                    target: "adaptive-gas",
                    "Block gas limit adjusted: {} → {} (last_used={}, target={}, {})",
                    current_limit, new_limit, last_used, target,
                    if scaled_up { "↑" } else { "↓" }
                );
            }

            // Reset for this block
            LastBlockGasUsed::<T>::put(0);

            T::DbWeight::get().reads_writes(4, 4)
        }
    }

    // ─── View Functions ────────────────────────────────────────────────

    impl<T: Config> Pallet<T> {
        /// Record gas used by a transaction. Called by EVM hook.
        pub fn record_gas_used(gas: u64) {
            LastBlockGasUsed::<T>::mutate(|used| {
                *used = used.saturating_add(gas);
            });
        }

        /// Returns current effective TPS estimate (assuming 50k gas avg per tx).
        pub fn estimated_tps() -> u64 {
            CurrentGasLimit::<T>::get() / 50_000
        }

        /// Returns fill percentage of the last block (0–100).
        pub fn last_block_fill_percent() -> u32 {
            let limit = CurrentGasLimit::<T>::get();
            if limit == 0 {
                return 0;
            }
            let used = LastBlockGasUsed::<T>::get();
            ((used.saturating_mul(100)) / limit) as u32
        }

        /// Returns a snapshot of all adaptive stats.
        pub fn stats() -> AdaptiveStats {
            AdaptiveStats {
                current_gas_limit: CurrentGasLimit::<T>::get(),
                last_block_gas_used: LastBlockGasUsed::<T>::get(),
                peak_gas_limit: PeakGasLimit::<T>::get(),
                min_gas_limit: T::MinGasLimit::get(),
                max_gas_limit: T::MaxGasLimit::get(),
                target_fill_percent: T::TargetFillPercent::get(),
                estimated_tps: Self::estimated_tps(),
                consecutive_full_blocks: ConsecutiveFullBlocks::<T>::get(),
                consecutive_empty_blocks: ConsecutiveEmptyBlocks::<T>::get(),
            }
        }
    }

    // ─── Genesis ───────────────────────────────────────────────────────

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub initial_gas_limit: Option<u64>,
        #[serde(skip)]
        pub _phantom: core::marker::PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            let limit = self
                .initial_gas_limit
                .unwrap_or_else(T::InitialGasLimit::get);
            CurrentGasLimit::<T>::put(limit);
            PeakGasLimit::<T>::put(limit);
        }
    }
}

/// Snapshot of all adaptive gas stats (returned by RPC/tks-cli).
#[derive(Debug, Clone, codec::Encode, codec::Decode)]
pub struct AdaptiveStats {
    pub current_gas_limit: u64,
    pub last_block_gas_used: u64,
    pub peak_gas_limit: u64,
    pub min_gas_limit: u64,
    pub max_gas_limit: u64,
    pub target_fill_percent: u32,
    pub estimated_tps: u64,
    pub consecutive_full_blocks: u32,
    pub consecutive_empty_blocks: u32,
}
