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
//! ```text
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
// Substrate `#[frame_support::pallet]` (`CloneNoBound`) generates `.clone()` on `Copy` fields in Clippy >= 1.100.
#![allow(clippy::clone_on_copy)]

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

/// Gas the author may execute in one 1-second slot.
///
/// r4 stayed near 1 s around 1,600 simple transfers (~34M gas); under r8 stress,
/// `~942` tx/block (`~20M` gas) still spiked block intervals to 14–18 s when slot
/// headroom ran dry. Capping per-slot execution at `25M` gas (`50%` of the 50M
/// header floor, `~1,190` 21k-gas transfers/block) sustains `>1,000` EVM TPS
/// while leaving P2P gossip and GRANDPA headroom inside every 1,000 ms Aura slot.
pub const SLOT_GAS_CAP: u64 = 25_000_000;

/// Never above the header limit, never above [`SLOT_GAS_CAP`].
pub fn executable_gas(header_gas_limit: u64, slot_gas_cap: u64) -> u64 {
    header_gas_limit.max(1).min(slot_gas_cap.max(1))
}

/// Ref-time per gas so the normal-class budget is exhausted at `executable_gas`.
/// Base extrinsic weight then stops the builder slightly earlier, so
/// `gasUsed <= gasLimit`.
pub fn weight_ref_time_per_gas(
    normal_max_ref_time: u64,
    header_gas_limit: u64,
    slot_gas_cap: u64,
) -> u64 {
    let executable = executable_gas(header_gas_limit, slot_gas_cap);
    (normal_max_ref_time / executable).max(1)
}

/// Largest gas total that fits in `normal_max_ref_time` at `weight_per_gas`.
pub fn max_gas_within_weight(normal_max_ref_time: u64, weight_per_gas: u64) -> u64 {
    normal_max_ref_time.checked_div(weight_per_gas).unwrap_or(0)
}

/// Computes a clamped proportional gas-limit adjustment step (`delta * current_limit / target / 8`).
fn proportional_gas_step(delta: u64, current_limit: u64, target: u64, max_step: u64) -> u64 {
    let proportional = delta
        .saturating_mul(current_limit)
        .checked_div(target)
        .map_or(max_step, |scaled| scaled / 8);
    proportional.min(max_step)
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // ─── Config ────────────────────────────────────────────────────────

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
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
    pub type CurrentGasLimit<T: Config> = StorageValue<_, u64, ValueQuery, T::InitialGasLimit>;

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
        fn on_finalize(_n: BlockNumberFor<T>) {
            let used = LastBlockGasUsed::<T>::get();
            let limit = CurrentGasLimit::<T>::get();
            if used > limit {
                log::error!(
                    target: "adaptive-gas",
                    "Block gas used ({used}) exceeded limit ({limit}); clamping metrics"
                );
                LastBlockGasUsed::<T>::put(limit);
            }
        }

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
                let increase = proportional_gas_step(excess, current_limit, target, max_increase);
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
                let decrease = proportional_gas_step(deficit, current_limit, target, max_decrease);
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

#[cfg(test)]
mod budget_tests {
    use super::{executable_gas, max_gas_within_weight, weight_ref_time_per_gas, SLOT_GAS_CAP};

    /// Fill a block with 21,000-gas transfers until the weight budget is gone.
    #[test]
    fn filled_block_never_exceeds_header_gas_limit() {
        // 75% of a 7s ref-time block — same shape as runtime `BlockWeights`.
        let normal_max = (7_000_000_000_000u64 / 4) * 3;
        let header_limits = [50_000_000u64, 56_250_000, 35_000_000, 4_000_000_000];
        for header in header_limits {
            let wpg = weight_ref_time_per_gas(normal_max, header, SLOT_GAS_CAP);
            let mut used = 0u64;
            let mut weight = 0u64;
            let tx_weight = 21_000u64.saturating_mul(wpg);
            while weight.saturating_add(tx_weight) <= normal_max {
                used = used.saturating_add(21_000);
                weight = weight.saturating_add(tx_weight);
            }
            let cap = max_gas_within_weight(normal_max, wpg);
            assert!(used <= header, "header {header}: filled {used} > gasLimit");
            assert!(
                used <= SLOT_GAS_CAP,
                "header {header}: filled {used} > slot cap"
            );
            assert!(cap <= header.min(SLOT_GAS_CAP));
            assert!(executable_gas(header, SLOT_GAS_CAP) <= header);
        }
    }

    #[test]
    fn mismatched_700ms_weight_would_overshoot() {
        // Old bug: weight-per-gas used ~525ms of ref time while the normal
        // class allowed ~5.25s, so about 10× the gas limit fit.
        let wrongly_small = (525_000_000_000u64 / 50_000_000).max(1);
        let real_budget = (7_000_000_000_000u64 / 4) * 3;
        let fits = max_gas_within_weight(real_budget, wrongly_small);
        assert!(fits > 50_000_000, "fixture should show the old overshoot");
    }
}
