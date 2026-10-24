//! Records effective EVM gas per transaction so `pallet-adaptive-gas` can scale
//! block gas limits from real utilization (not stuck at the 75M floor).

use core::marker::PhantomData;

use fp_evm::FeeCalculator;
use frame_support::traits::fungible::{Balanced, Credit, Inspect};
use pallet_evm::{
    AccountIdOf, Config as EvmConfig, Error, EVMFungibleAdapter, OnChargeEVMTransaction,
};
use sp_core::{H160, U256};
use sp_runtime::traits::UniqueSaturatedInto;

/// Wraps the default EVM fee adapter and feeds block gas usage into [`pallet_adaptive_gas`].
pub struct AdaptiveGasOnCharge<T>(PhantomData<T>);

impl<T> OnChargeEVMTransaction<T> for AdaptiveGasOnCharge<T>
where
    T: EvmConfig + pallet_adaptive_gas::Config,
    T::Currency: Balanced<AccountIdOf<T>>,
    U256: UniqueSaturatedInto<<T::Currency as Inspect<AccountIdOf<T>>>::Balance>,
{
    type LiquidityInfo = Option<Credit<AccountIdOf<T>, T::Currency>>;

    fn withdraw_fee(who: &H160, fee: U256) -> Result<Self::LiquidityInfo, Error<T>> {
        <EVMFungibleAdapter<T::Currency, ()> as OnChargeEVMTransaction<T>>::withdraw_fee(
            who, fee,
        )
    }

    fn correct_and_deposit_fee(
        who: &H160,
        corrected_fee: U256,
        base_fee: U256,
        already_withdrawn: Self::LiquidityInfo,
    ) -> Self::LiquidityInfo {
        let effective_gas = effective_gas_from_fees::<T>(corrected_fee, base_fee);
        if effective_gas > 0 {
            pallet_adaptive_gas::Pallet::<T>::record_gas_used(effective_gas);
        }
        <EVMFungibleAdapter<T::Currency, ()> as OnChargeEVMTransaction<T>>::correct_and_deposit_fee(
            who,
            corrected_fee,
            base_fee,
            already_withdrawn,
        )
    }

    fn pay_priority_fee(tip: Self::LiquidityInfo) {
        <EVMFungibleAdapter<T::Currency, ()> as OnChargeEVMTransaction<T>>::pay_priority_fee(tip);
    }
}

fn effective_gas_from_fees<T: EvmConfig>(corrected_fee: U256, actual_base_fee_amount: U256) -> u64 {
    let (base_fee_rate, _) = T::FeeCalculator::min_gas_price();
    if !actual_base_fee_amount.is_zero() && !base_fee_rate.is_zero() {
        return actual_base_fee_amount
            .checked_div(base_fee_rate)
            .unwrap_or(U256::zero())
            .min(U256::from(u64::MAX))
            .low_u64();
    }
    if !corrected_fee.is_zero() && !base_fee_rate.is_zero() {
        return corrected_fee
            .checked_div(base_fee_rate)
            .unwrap_or(U256::zero())
            .min(U256::from(u64::MAX))
            .low_u64();
    }
    0
}
