//! Records effective EVM gas per transaction so `pallet-adaptive-gas` can scale
//! block gas limits from real utilization (not stuck at the 75M floor).

use core::marker::PhantomData;

use fp_evm::{Error, OnChargeEVMTransaction};
use pallet_evm::Config as EvmConfig;
use sp_core::{H160, U256};

/// No-op fee handler that feeds block gas usage into [`pallet_adaptive_gas`].
pub struct AdaptiveGasOnCharge<T>(PhantomData<T>);

impl<T> OnChargeEVMTransaction<T> for AdaptiveGasOnCharge<T>
where
    T: EvmConfig + pallet_adaptive_gas::Config,
{
    type LiquidityInfo = ();

    fn withdraw_fee(_who: &H160, _fee: U256) -> Result<Self::LiquidityInfo, Error<T>> {
        Ok(())
    }

    fn correct_and_deposit_fee(
        _who: &H160,
        corrected_fee: U256,
        actual_base_fee_amount: U256,
        _already_withdrawn: Self::LiquidityInfo,
    ) -> Self::LiquidityInfo {
        let effective_gas = effective_gas_from_fees::<T>(corrected_fee, actual_base_fee_amount);
        if effective_gas > 0 {
            pallet_adaptive_gas::Pallet::<T>::record_gas_used(effective_gas);
        }
    }

    fn pay_priority_fee(_tip: Self::LiquidityInfo) {}
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
