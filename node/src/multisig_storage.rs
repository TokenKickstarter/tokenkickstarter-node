//! Read `pallet-multisig` pending calls via JSON-RPC `state_getStorage`.

use codec::{Decode, Encode};
use fp_account::AccountId20;
use pallet_multisig::Timepoint;
use sp_core::hashing::{blake2_128_concat, twox_128};

use crate::shard_setup_cmd::rpc_call;

#[derive(Debug, Decode)]
pub struct MultisigState {
    pub when: Timepoint,
    pub deposit: u128,
    pub depositor: AccountId20,
    pub approvals: Vec<AccountId20>,
}

pub fn multisigs_storage_key(multisig: AccountId20, call_hash: &[u8; 32]) -> Vec<u8> {
    let mut key = twox_128(b"Multisig").to_vec();
    key.extend_from_slice(&twox_128(b"Multisigs"));
    key.extend_from_slice(&blake2_128_concat(&multisig.encode()));
    key.extend_from_slice(&blake2_128_concat(call_hash));
    key
}

pub fn multisigs_prefix_key(multisig: AccountId20) -> Vec<u8> {
    let mut key = twox_128(b"Multisig").to_vec();
    key.extend_from_slice(&twox_128(b"Multisigs"));
    key.extend_from_slice(&blake2_128_concat(&multisig.encode()));
    key
}

pub fn fetch_multisig_state(
    rpc_url: &str,
    multisig: AccountId20,
    call_hash: &[u8; 32],
) -> Result<Option<MultisigState>, sc_cli::Error> {
    let key = multisigs_storage_key(multisig, call_hash);
    let hex_key = format!("0x{}", hex::encode(key));
    let result = rpc_call(rpc_url, "state_getStorage", serde_json::json!([hex_key]))?;
    if result.is_null() {
        return Ok(None);
    }
    let hex = result
        .as_str()
        .ok_or_else(|| sc_cli::Error::Input("bad multisig storage hex".into()))?;
    let bytes = hex::decode(hex.trim_start_matches("0x"))
        .map_err(|e| sc_cli::Error::Input(e.to_string()))?;
    MultisigState::decode(&mut &bytes[..])
        .map(|s| Some(s))
        .map_err(|e| sc_cli::Error::Input(format!("decode Multisig: {e}")))
}

pub fn list_pending_call_hashes(
    rpc_url: &str,
    multisig: AccountId20,
) -> Result<Vec<[u8; 32]>, sc_cli::Error> {
    let prefix = multisigs_prefix_key(multisig);
    let hex_prefix = format!("0x{}", hex::encode(prefix));
    let keys = rpc_call(
        rpc_url,
        "state_getKeys",
        serde_json::json!([hex_prefix]),
    )?;
    let arr = keys
        .as_array()
        .ok_or_else(|| sc_cli::Error::Input("state_getKeys: expected array".into()))?;
    let mut out = Vec::new();
    for k in arr {
        let hex = k
            .as_str()
            .ok_or_else(|| sc_cli::Error::Input("bad key hex".into()))?;
        let bytes = hex::decode(hex.trim_start_matches("0x"))
            .map_err(|e| sc_cli::Error::Input(e.to_string()))?;
        // suffix: blake128(32) + call_hash(32)
        if bytes.len() < 16 + 32 {
            continue;
        }
        let hash: [u8; 32] = bytes[bytes.len() - 32..]
            .try_into()
            .map_err(|_| sc_cli::Error::Input("short multisig key".into()))?;
        out.push(hash);
    }
    Ok(out)
}

pub fn wait_for_multisig_timepoint(
    rpc_url: &str,
    multisig: AccountId20,
    call_hash: &[u8; 32],
    max_wait_secs: u64,
) -> Result<Timepoint, sc_cli::Error> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(max_wait_secs);
    loop {
        if let Some(state) = fetch_multisig_state(rpc_url, multisig, call_hash)? {
            return Ok(state.when);
        }
        if std::time::Instant::now() >= deadline {
            return Err(sc_cli::Error::Application(
                "timed out waiting for multisig pending state (as_multi not included?)".into(),
            ));
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

pub fn parse_account_id20(hex: &str) -> Result<AccountId20, sc_cli::Error> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).map_err(|e| sc_cli::Error::Input(e.to_string()))?;
    let bytes: [u8; 20] = bytes
        .try_into()
        .map_err(|_| sc_cli::Error::Input("H160 must be 20 bytes".into()))?;
    Ok(AccountId20::from(bytes))
}
