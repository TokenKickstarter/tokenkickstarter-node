//! List or cancel stuck `pallet-multisig` pending calls for the sudo account.

use codec::Encode;
use fp_account::AccountId20;
use pallet_multisig::Call as MultisigCall;
use sp_core::{ecdsa, Pair};
use tks_runtime::RuntimeCall;

use crate::extrinsic_sign::encode_signed_extrinsic;
use crate::genesis_util::multisig_account_id;
use crate::multisig_storage::{list_pending_call_hashes, parse_account_id20};
use crate::shard_setup_cmd::{
    fetch_account_nonce, fetch_genesis_hash, fetch_runtime_version, rpc_call,
};
use pallet_multisig::Timepoint;
use tks_runtime::BlockNumber;

#[derive(Debug, clap::Parser)]
pub struct MultisigListPendingCmd {
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    pub rpc_url: String,

    #[arg(long)]
    pub multisig: String,
}

impl MultisigListPendingCmd {
    pub fn run(&self) -> sc_cli::Result<()> {
        let multisig = parse_account_id20(&self.multisig)?;
        let hashes = list_pending_call_hashes(&self.rpc_url, multisig)?;
        if hashes.is_empty() {
            println!("No pending multisig calls for {multisig:?}");
            return Ok(());
        }
        for h in hashes {
            println!("0x{}", hex::encode(h));
        }
        Ok(())
    }
}

#[derive(Debug, clap::Parser)]
pub struct MultisigCancelPendingCmd {
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    pub rpc_url: String,

    #[arg(long)]
    pub member_seed_file: std::path::PathBuf,

    #[arg(long)]
    pub multisig: String,

    /// Cancel one pending call; omit to cancel all listed pending calls.
    #[arg(long)]
    pub call_hash: Option<String>,

    #[arg(long, default_value_t = 2)]
    pub threshold: u16,

    #[arg(long, value_delimiter = ',')]
    pub signatories: Vec<String>,
}

impl MultisigCancelPendingCmd {
    pub fn run(&self) -> sc_cli::Result<()> {
        let multisig = parse_account_id20(&self.multisig)?;
        let mut signers: Vec<AccountId20> = self
            .signatories
            .iter()
            .map(|s| parse_account_id20(s))
            .collect::<Result<_, _>>()?;
        signers.sort_by(|a, b| a.0.cmp(&b.0));

        let member_seed = std::fs::read_to_string(&self.member_seed_file)
            .map_err(|e| sc_cli::Error::Input(format!("read seed: {e}")))?;
        let (pair, account) = crate::ecdsa_seed::load_ecdsa_from_seed_text(member_seed.trim())
            .map_err(|e| sc_cli::Error::Input(e.to_string()))?;

        let expected = multisig_account_id(&signers, self.threshold);
        if expected != multisig {
            return Err(sc_cli::Error::Input(format!(
                "multisig {multisig:?} != derived {expected:?} for given signatories"
            )));
        }

        let call_hashes: Vec<[u8; 32]> = if let Some(h) = &self.call_hash {
            vec![parse_hash32(h)?]
        } else {
            list_pending_call_hashes(&self.rpc_url, multisig)?
        };

        if call_hashes.is_empty() {
            println!("Nothing to cancel.");
            return Ok(());
        }

        let genesis_hash = fetch_genesis_hash(&self.rpc_url)?;
        let (spec_version, transaction_version) = fetch_runtime_version(&self.rpc_url)?;
        let max_weight = frame_support::weights::Weight::from_parts(5_000_000_000, 0);

        let other_for = |who: AccountId20| -> Vec<AccountId20> {
            signers.iter().copied().filter(|a| *a != who).collect()
        };

        for call_hash in call_hashes {
            let timepoint = fetch_timepoint(&self.rpc_url, multisig, &call_hash)?;
            let mut nonce = fetch_account_nonce(&self.rpc_url, account)?;
            let call = RuntimeCall::Multisig(MultisigCall::cancel_as_multi {
                threshold: self.threshold,
                other_signatories: other_for(account),
                timepoint,
                call_hash,
            });
            let ext = encode_signed_extrinsic(
                call,
                &pair,
                account,
                nonce,
                genesis_hash,
                spec_version,
                transaction_version,
            )
            .map_err(|e| sc_cli::Error::Application(e.into()))?;
            let hex_ext = format!("0x{}", hex::encode(ext));
            let tx = rpc_call(
                &self.rpc_url,
                "author_submitExtrinsic",
                serde_json::json!([hex_ext]),
            )?;
            println!(
                "cancel_as_multi call_hash=0x{} nonce={nonce} tx={tx}",
                hex::encode(call_hash)
            );
            nonce += 1;
            std::thread::sleep(std::time::Duration::from_secs(6));
        }
        Ok(())
    }
}

fn fetch_timepoint(
    rpc_url: &str,
    multisig: AccountId20,
    call_hash: &[u8; 32],
) -> Result<Timepoint<BlockNumber>, sc_cli::Error> {
    use crate::multisig_storage::fetch_multisig_state;
    let state = fetch_multisig_state(rpc_url, multisig, call_hash)?
        .ok_or_else(|| sc_cli::Error::Input("pending multisig not found for call_hash".into()))?;
    Ok(state.when)
}

fn parse_hash32(hex: &str) -> Result<[u8; 32], sc_cli::Error> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).map_err(|e| sc_cli::Error::Input(e.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| sc_cli::Error::Input("call_hash must be 32 bytes".into()))
}
