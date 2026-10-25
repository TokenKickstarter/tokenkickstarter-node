//! Sign Substrate extrinsic payloads the way `EthereumSignature` verifies (Keccak-256).

use fp_account::AccountId20;
use sp_core::{ecdsa, Pair};
use tks_runtime::Signature;

/// Keccak-256 sign the SCALE-encoded signing payload (65-byte recoverable secp256k1).
pub fn sign_ethereum_payload(signer: &ecdsa::Pair, payload: &[u8]) -> Signature {
    let digest = sp_core::hashing::keccak_256(payload);
    let sig = signer.sign_prehashed(&digest);
    Signature::new(sig)
}

#[allow(dead_code)]
pub fn account_id20_from_pair(pair: &ecdsa::Pair) -> AccountId20 {
    AccountId20::from(pair.public())
}
