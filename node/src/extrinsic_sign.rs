//! Build signed extrinsics for Ethereum-account (AccountId20) runtime calls.

use codec::Encode;
use fp_account::AccountId20;
use sp_core::{ecdsa, Pair};
use sp_runtime::{
    generic::{Era, SignedPayload},
    traits::Block as BlockT,
};
use tks_runtime::{Hash, RuntimeCall, Signature, SignedExtra, UncheckedExtrinsic};

type Block = tks_runtime::Block;

/// Encode a signed extrinsic for `call` using an ECDSA dev/prod seed pair.
pub fn encode_signed_extrinsic(
    call: RuntimeCall,
    signer: &ecdsa::Pair,
    nonce: u32,
    genesis_hash: Hash,
) -> Result<Vec<u8>, String> {
    let account = AccountId20::from(signer.public());
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
            tks_runtime::VERSION.spec_version,
            tks_runtime::VERSION.transaction_version,
            genesis_hash,
            genesis_hash,
            (),
            (),
            (),
        ),
    );

    let ecdsa_sig = raw_payload.using_encoded(|payload| signer.sign(payload));
    let signature = Signature::new(ecdsa_sig);
    let ext = UncheckedExtrinsic::new_signed(call, account, signature, extra);
    Ok(ext.encode())
}
