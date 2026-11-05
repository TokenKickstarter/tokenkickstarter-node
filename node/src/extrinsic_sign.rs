//! Build signed extrinsics for Ethereum-account (AccountId20) runtime calls.

use codec::Encode;
use fp_account::AccountId20;
use sp_core::ecdsa;
use sp_runtime::{
    generic::{Era, SignedPayload},
    traits::Block as BlockT,
};
use tks_runtime::{Hash, RuntimeCall, SignedExtra, UncheckedExtrinsic};

use crate::ethereum_sign::sign_ethereum_payload;

type Block = tks_runtime::Block;

/// Encode a signed extrinsic for `call` using an ECDSA dev/prod seed pair.
/// `spec_version` / `transaction_version` must match **on-chain** runtime (from RPC), not the
/// binary's compiled `VERSION`, so extrinsics validate before a `set_code` upgrade lands.
pub fn encode_signed_extrinsic(
    call: RuntimeCall,
    signer: &ecdsa::Pair,
    account: AccountId20,
    nonce: u32,
    genesis_hash: Hash,
    spec_version: u32,
    transaction_version: u32,
) -> Result<Vec<u8>, String> {
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

    let raw_payload = SignedPayload::from_raw(
        call.clone(),
        extra.clone(),
        (
            (),
            spec_version,
            transaction_version,
            genesis_hash,
            genesis_hash,
            (),
            (),
            (),
        ),
    );

    let signature = raw_payload.using_encoded(|payload| sign_ethereum_payload(signer, payload));
    let ext = UncheckedExtrinsic::new_signed(call, account, signature, extra);
    Ok(ext.encode())
}
